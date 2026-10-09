//! One node's playback: the playhead (advanced by the tick) and a decoder
//! thread that keeps a small cache of frames around it.
//!
//! Every tick turns the playhead into a [`Want`]: the frame on screen plus
//! the next few in the playing direction (wrapping when looping). The
//! decoder thread fills the first wanted frame that isn't cached, carrying
//! on from where it is when that's ahead within the same GOP and seeking
//! (back to the keyframe) otherwise, and drops cached frames that are no
//! longer wanted. It re-reads the want after every frame, so a scrub
//! redirects it at once. Frames decoded on the way to a target are only
//! converted to RGBA when wanted.
//!
//! The tick never waits: it shows the wanted frame if it's cached and
//! otherwise keeps showing the last one (a seek catching up).

use crate::demux::Frames;
use floret_engine::image::Image;
use floret_engine::video::{Playhead, VideoFrame, VideoRequest};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// A session no tick has asked for in this long shuts its decoder down.
pub const IDLE: Duration = Duration::from_secs(3);
/// Decoded frames kept per session.
const CACHE_BYTES: usize = 160 << 20;
const MAX_WINDOW: usize = 32;
/// Frames prefetched past the playhead while paused or scrubbing.
const PAUSED_AHEAD: usize = 2;
/// How long a blocking ([`VideoRequest::wait`]) poll waits.
const WAIT_LIMIT: Duration = Duration::from_secs(10);

/// The frames a tick wants cached: `target`, then `ahead` more in the
/// playing direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Want {
    pub target: usize,
    pub ahead: usize,
    pub reverse: bool,
    pub looping: bool,
}

impl Want {
    /// Wanted display indices among `n` frames, most urgent first.
    pub fn indices(&self, n: usize) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.ahead + 1);
        if n == 0 {
            return out;
        }
        let target = self.target.min(n - 1);
        for k in 0..=self.ahead.min(n - 1) {
            let i = match (self.reverse, self.looping) {
                (false, true) => (target + k) % n,
                (true, true) => (target + n - k % n) % n,
                (false, false) if target + k < n => target + k,
                (true, false) if k <= target => target - k,
                _ => break,
            };
            out.push(i);
        }
        out
    }
}

struct Opened {
    frames: Arc<Frames>,
    window: usize,
}

struct State {
    playhead: Playhead,
    want: Option<Want>,
    last_poll: Instant,
    cancelled: bool,
    /// The decoder thread has stopped (idle, failed or cancelled).
    done: bool,
    opened: Option<Opened>,
    error: Option<String>,
    cache: BTreeMap<usize, Arc<Image>>,
    /// Frames the decoder couldn't produce (damaged); not retried.
    dead: HashSet<usize>,
    shown: Option<Arc<Image>>,
}

struct Shared {
    state: Mutex<State>,
    cv: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn wait<'a>(&self, st: MutexGuard<'a, State>, d: Duration) -> MutexGuard<'a, State> {
        self.cv
            .wait_timeout(st, d)
            .map(|(g, _)| g)
            .unwrap_or_else(|e| e.into_inner().0)
    }

    fn fail(&self, msg: String) {
        let mut st = self.lock();
        st.error = Some(msg);
        st.done = true;
        self.cv.notify_all();
    }
}

pub struct Session {
    shared: Arc<Shared>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shared.lock().cancelled = true;
        self.shared.cv.notify_all();
    }
}

impl Session {
    /// Start playing `source` (`fps`: an image sequence's frame rate).
    pub fn spawn(source: &str, fps: f64) -> Session {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                playhead: Playhead::default(),
                want: None,
                last_poll: Instant::now(),
                cancelled: false,
                done: false,
                opened: None,
                error: None,
                cache: BTreeMap::new(),
                dead: HashSet::new(),
                shown: None,
            }),
            cv: Condvar::new(),
        });
        let s = shared.clone();
        let src = source.to_owned();
        let spawned = std::thread::Builder::new()
            .name("floret-video".into())
            .spawn(move || {
                let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    decode_loop(&s, &src, fps)
                }));
                match run {
                    Ok(Ok(())) => {
                        let mut st = s.lock();
                        st.done = true;
                        st.cache.clear();
                    }
                    Ok(Err(e)) => s.fail(e),
                    Err(_) => s.fail("the video decoder crashed".into()),
                }
            });
        if let Err(e) = spawned {
            shared.fail(format!("couldn't start the video decoder: {e}"));
        }
        Session { shared }
    }

    /// Shut down, or idle long enough that it's about to.
    pub fn stale(&self) -> bool {
        let st = self.shared.lock();
        (st.done && st.error.is_none()) || st.last_poll.elapsed() >= IDLE
    }

    /// The node's tick: advance the playhead, point the decoder at it and
    /// return the frame to show.
    pub fn poll(&self, req: &VideoRequest) -> Result<VideoFrame, String> {
        let sh = &self.shared;
        let deadline = Instant::now() + WAIT_LIMIT;
        let mut st = sh.lock();
        st.last_poll = Instant::now();
        while req.wait && st.opened.is_none() && st.error.is_none() && Instant::now() < deadline {
            st = sh.wait(st, Duration::from_millis(20));
        }
        if let Some(e) = &st.error {
            return Err(e.clone());
        }
        let Some(open) = &st.opened else {
            st.playhead.advance(req, 0.0);
            return Ok(VideoFrame::empty());
        };
        let (frames, window) = (open.frames.clone(), open.window);
        let time = st.playhead.advance(req, frames.duration);
        let target = frames.index_at(time);
        let playing = req.play && req.seek.is_none() && req.speed != 0.0;
        let want = Want {
            target,
            ahead: if playing { window - 1 } else { PAUSED_AHEAD },
            reverse: playing && req.speed < 0.0,
            looping: req.looping,
        };
        if st.want != Some(want) {
            st.want = Some(want);
            sh.cv.notify_all();
        }
        while req.wait
            && !st.cache.contains_key(&target)
            && !st.dead.contains(&target)
            && !st.done
            && Instant::now() < deadline
        {
            st = sh.wait(st, Duration::from_millis(20));
        }
        if let Some(img) = st.cache.get(&target) {
            st.shown = Some(img.clone());
        }
        Ok(match &st.shown {
            Some(img) => VideoFrame {
                image: img.clone(),
                time,
                duration: frames.duration,
                ready: true,
            },
            None => VideoFrame {
                time,
                duration: frames.duration,
                ..VideoFrame::empty()
            },
        })
    }
}

fn decode_loop(sh: &Shared, source: &str, fps: f64) -> Result<(), String> {
    let mut media = crate::open_at(source, fps)?;
    let frames = Arc::new(media.frames.clone());
    let n = frames.len();
    if n == 0 {
        return Err("the video has no frames".into());
    }
    let frame_bytes = (media.width as usize * media.height as usize * 4).max(1);
    let window = (CACHE_BYTES / frame_bytes).clamp(PAUSED_AHEAD + 1, MAX_WINDOW);
    {
        let mut st = sh.lock();
        st.opened = Some(Opened {
            frames: frames.clone(),
            window,
        });
        sh.cv.notify_all();
    }

    // Display index the decoder outputs next, while it's mid-stream.
    let mut pos: Option<usize> = None;
    // The frame the last seek was for, until it arrives.
    let mut seeking: Option<usize> = None;
    loop {
        let (wanted, missing) = {
            let mut st = sh.lock();
            loop {
                if st.cancelled || st.last_poll.elapsed() >= IDLE {
                    return Ok(());
                }
                if let Some(want) = st.want {
                    let list = want.indices(n);
                    st.cache.retain(|k, _| list.contains(k));
                    let missing = list
                        .iter()
                        .copied()
                        .find(|i| !st.cache.contains_key(i) && !st.dead.contains(i));
                    if let Some(m) = missing {
                        break (list, m);
                    }
                }
                st = sh.wait(st, Duration::from_millis(250));
            }
        };

        let carry_on = pos.is_some_and(|p| p <= missing && frames.key[missing] <= p);
        if !carry_on {
            if seeking == Some(missing) {
                // A seek already ran past it without producing it.
                let mut st = sh.lock();
                st.dead.insert(missing);
                sh.cv.notify_all();
                seeking = None;
                pos = None;
                continue;
            }
            media.decoder.seek(missing)?;
            seeking = Some(missing);
        }
        match media.decoder.next(&|i| wanted.contains(&i)) {
            Ok(Some((i, img))) => {
                pos = Some(i + 1);
                if seeking == Some(i) {
                    seeking = None;
                }
                if let Some(img) = img {
                    let mut st = sh.lock();
                    st.cache.insert(i, Arc::new(img));
                    sh.cv.notify_all();
                }
            }
            Ok(None) | Err(_) => pos = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn want(target: usize, ahead: usize, reverse: bool, looping: bool) -> Want {
        Want {
            target,
            ahead,
            reverse,
            looping,
        }
    }

    #[test]
    fn wanted_frames_follow_direction_and_wrap() {
        assert_eq!(want(2, 3, false, false).indices(10), [2, 3, 4, 5]);
        assert_eq!(want(8, 3, false, false).indices(10), [8, 9]);
        assert_eq!(want(8, 3, false, true).indices(10), [8, 9, 0, 1]);
        assert_eq!(want(1, 3, true, false).indices(10), [1, 0]);
        assert_eq!(want(1, 3, true, true).indices(10), [1, 0, 9, 8]);
        assert_eq!(want(0, 9, false, true).indices(3), [0, 1, 2]);
        assert_eq!(want(5, 2, false, false).indices(3), [2]);
        assert!(want(0, 2, false, false).indices(0).is_empty());
    }
}
