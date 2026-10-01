//! Video files the app lends the engine (`video/file`, `video/frame-at`).
//!
//! The engine never decodes video. `floret-video` implements [`Videos`]
//! (MP4 demux + H.264 decoding, AVFoundation on macOS) and the app installs
//! it with [`install`]; with nothing installed the nodes report that video
//! isn't available.
//!
//! Like camera sessions (`crate::devices`), playback sessions are keyed by
//! node id and source and live in the provider, not the node: a node is
//! rebuilt on every recompile (any wire or pin edit), and the playhead has
//! to survive that. The provider advances each session's [`Playhead`] with
//! the tick's `dt`, decodes ahead on its own thread and answers with the
//! newest decoded frame for the playhead, so a tick never waits on a
//! decoder (except under [`wait_for_frames`], for offline renders).

use crate::image::Image;
use std::cell::Cell;
use std::sync::{Arc, RwLock};

/// An explicit position that overrides playback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seek {
    /// 0..1 of the duration (`video/file`'s `scrub`).
    Position(f64),
    /// Seconds from the start (`video/frame-at`).
    Seconds(f64),
}

/// What a video node asks for this tick.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoRequest<'a> {
    /// A file path, `file://` URL or `data:` URL.
    pub source: &'a str,
    pub play: bool,
    pub looping: bool,
    /// Playback rate; negative plays backwards.
    pub speed: f64,
    pub seek: Option<Seek>,
    /// Seconds since the previous tick.
    pub dt: f64,
    /// Jump back to the start (the first tick after a transport restart).
    pub restart: bool,
    /// Block until the playhead's frame is decoded ([`wait_for_frames`]).
    pub wait: bool,
}

#[derive(Clone, Debug)]
pub struct VideoFrame {
    /// The frame for the playhead (or the newest one decoded while a seek
    /// catches up); a 1×1 placeholder until the first frame arrives.
    pub image: Arc<Image>,
    /// The playhead, seconds.
    pub time: f64,
    /// Seconds; 0 until the file is open.
    pub duration: f64,
    pub ready: bool,
}

impl VideoFrame {
    /// Nothing to show yet (no source, still opening).
    pub fn empty() -> VideoFrame {
        VideoFrame {
            image: Image::placeholder(),
            time: 0.0,
            duration: 0.0,
            ready: false,
        }
    }
}

pub trait Videos: Send + Sync {
    /// Drive `node`'s playback of `req.source`. `Err` explains why there's
    /// no picture (unreadable file, unsupported codec).
    fn video(&self, node: &str, req: &VideoRequest) -> Result<VideoFrame, String>;
}

/// A session's playback clock: `play` / `loop` / `speed` / `seek` semantics,
/// shared by every provider so they all behave the same.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Playhead {
    pub time: f64,
    was_playing: bool,
}

impl Playhead {
    /// Advance for one tick of a `duration`-second video (0 while it's
    /// still opening: the playhead holds at the start until then). A
    /// restart shows the first frame before playing on.
    pub fn advance(&mut self, req: &VideoRequest, duration: f64) -> f64 {
        let started = req.play && !self.was_playing;
        self.was_playing = req.play;
        if req.restart {
            self.time = 0.0;
        }
        if duration <= 0.0 || !duration.is_finite() || (req.restart && req.seek.is_none()) {
            self.time = 0.0;
            return 0.0;
        }
        let clamp = |t: f64| {
            if t.is_finite() {
                t.clamp(0.0, duration)
            } else {
                0.0
            }
        };
        match req.seek {
            Some(Seek::Position(p)) => self.time = clamp(p * duration),
            Some(Seek::Seconds(s)) => self.time = clamp(s),
            None if req.play => {
                let speed = if req.speed.is_finite() {
                    req.speed
                } else {
                    1.0
                };
                // Play from an end it stopped at: start over.
                if started && !req.looping {
                    if speed > 0.0 && self.time >= duration {
                        self.time = 0.0;
                    } else if speed < 0.0 && self.time <= 0.0 {
                        self.time = duration;
                    }
                }
                let t = self.time + req.dt.max(0.0) * speed;
                self.time = if req.looping {
                    t.rem_euclid(duration)
                } else {
                    clamp(t)
                };
            }
            None => self.time = clamp(self.time),
        }
        self.time
    }
}

fn slot() -> &'static RwLock<Option<Arc<dyn Videos>>> {
    static VIDEOS: RwLock<Option<Arc<dyn Videos>>> = RwLock::new(None);
    &VIDEOS
}

/// Install (or remove) the process-wide video provider.
pub fn install(videos: Option<Arc<dyn Videos>>) {
    *slot().write().unwrap_or_else(|e| e.into_inner()) = videos;
}

/// The installed provider, if any.
pub fn installed() -> Option<Arc<dyn Videos>> {
    slot().read().unwrap_or_else(|e| e.into_inner()).clone()
}

thread_local! {
    static WAIT: Cell<bool> = const { Cell::new(false) };
}

/// While the guard lives, video nodes ticked on this thread block until
/// their frame is decoded (thumbnails, offline renders), instead of
/// showing whatever is ready.
pub fn wait_for_frames() -> WaitGuard {
    WaitGuard(WAIT.with(|w| w.replace(true)))
}

pub struct WaitGuard(bool);

impl Drop for WaitGuard {
    fn drop(&mut self) {
        WAIT.with(|w| w.set(self.0));
    }
}

/// Whether ticks on this thread wait for frames.
pub fn waiting() -> bool {
    WAIT.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(play: bool, looping: bool, speed: f64, dt: f64) -> VideoRequest<'static> {
        VideoRequest {
            source: "",
            play,
            looping,
            speed,
            seek: None,
            dt,
            restart: false,
            wait: false,
        }
    }

    #[test]
    fn plays_loops_and_clamps() {
        let mut p = Playhead::default();
        assert_eq!(
            p.advance(&req(true, true, 1.0, 0.5), 0.0),
            0.0,
            "holds while opening"
        );
        assert_eq!(p.advance(&req(true, true, 1.0, 0.5), 2.0), 0.5);
        assert_eq!(p.advance(&req(true, true, 2.0, 0.5), 2.0), 1.5);
        assert_eq!(p.advance(&req(true, true, 1.0, 1.0), 2.0), 0.5, "wraps");
        assert_eq!(
            p.advance(&req(true, true, -1.0, 1.0), 2.0),
            1.5,
            "wraps backwards"
        );
        assert_eq!(p.advance(&req(false, true, 1.0, 1.0), 2.0), 1.5, "paused");
        assert_eq!(
            p.advance(&req(true, false, 1.0, 5.0), 2.0),
            2.0,
            "stops at the end"
        );
        assert_eq!(p.advance(&req(true, false, 1.0, 5.0), 2.0), 2.0);
        p.advance(&req(false, false, 1.0, 0.1), 2.0);
        assert_eq!(
            p.advance(&req(true, false, 1.0, 0.25), 2.0),
            0.25,
            "replays from the start"
        );
    }

    #[test]
    fn seeks_and_restarts() {
        let mut p = Playhead::default();
        let mut r = req(true, true, 1.0, 0.1);
        r.seek = Some(Seek::Position(0.5));
        assert_eq!(p.advance(&r, 4.0), 2.0);
        r.seek = Some(Seek::Position(1.5));
        assert_eq!(p.advance(&r, 4.0), 4.0, "position clamps, never wraps");
        r.seek = Some(Seek::Seconds(-1.0));
        assert_eq!(p.advance(&r, 4.0), 0.0);
        r.seek = Some(Seek::Seconds(f64::NAN));
        assert_eq!(p.advance(&r, 4.0), 0.0);
        r.seek = None;
        r.restart = true;
        p.time = 3.0;
        assert_eq!(p.advance(&r, 4.0), 0.0, "restarts on the first frame");
        r.restart = false;
        assert!((p.advance(&r, 4.0) - 0.1).abs() < 1e-12);
    }

    #[test]
    fn wait_guard_nests() {
        assert!(!waiting());
        {
            let _a = wait_for_frames();
            {
                let _b = wait_for_frames();
                assert!(waiting());
            }
            assert!(waiting());
        }
        assert!(!waiting());
    }
}
