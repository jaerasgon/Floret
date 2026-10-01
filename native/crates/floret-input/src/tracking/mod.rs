//! Hand / face / pose landmark tracking.
//!
//! Each `input/landmarks` node gets a session: a thread that loads the
//! models on `start`'s rising edge and tracks the most recent frame the
//! node handed over (older pending frames are dropped, so a slow tracker
//! never builds a backlog). The node reads whatever finished last. In
//! `all` mode the three trackers run in parallel on each frame.

pub mod bundle;
pub mod detect;
pub mod geom;
pub mod ids;
pub mod landmarker;
pub mod smooth;

use floret_engine::devices::{LandmarkLayer, LandmarksRequest};
use floret_engine::image::Image;
use landmarker::{Landmarker, Tracker};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// A session its node stopped polling (deleted, patch switched) shuts down.
const IDLE: Duration = Duration::from_secs(2);

#[derive(Default)]
struct State {
    /// The newest frame not yet tracked, with its time in seconds.
    pending: Option<(Arc<Image>, f64)>,
    last_image: u64,
    /// `None` until the models have loaded.
    result: Option<Vec<LandmarkLayer>>,
    error: Option<String>,
    last_poll: Option<Instant>,
    /// Set by the node's falling `start` edge or a config change.
    cancel: bool,
    /// The thread has exited.
    done: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

struct Session {
    trackers: Vec<Tracker>,
    count: usize,
    born: Instant,
    shared: Arc<Shared>,
}

impl Session {
    fn spawn(trackers: Vec<Tracker>, count: usize) -> Session {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                last_poll: Some(Instant::now()),
                ..State::default()
            }),
            wake: Condvar::new(),
        });
        let s = shared.clone();
        let t = trackers.clone();
        let spawned = std::thread::Builder::new()
            .name("floret-tracker".into())
            .spawn(move || run(&s, &t, count));
        if let Err(e) = spawned {
            let mut st = shared.lock();
            st.error = Some(format!("couldn't start tracking: {e}"));
            st.done = true;
        }
        Session {
            trackers,
            count,
            born: Instant::now(),
            shared,
        }
    }

    fn stop(&self) {
        self.shared.lock().cancel = true;
        self.shared.wake.notify_all();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

/// All tracker sessions, keyed by node id.
#[derive(Default)]
pub struct Trackers {
    sessions: Mutex<HashMap<String, Session>>,
}

impl Trackers {
    /// The node's tick: start / stop / reconfigure its session, hand over
    /// a new frame, and read the latest result.
    pub fn request(
        &self,
        node: &str,
        req: &LandmarksRequest,
    ) -> Result<Option<Vec<LandmarkLayer>>, String> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        // Idle sessions go; a failed one stays (and keeps reporting its
        // error) until its node stops or reconfigures, rather than retrying
        // every tick.
        sessions.retain(|_, s| {
            let st = s.shared.lock();
            (!st.done || st.error.is_some()) && st.last_poll.is_some_and(|t| t.elapsed() < IDLE)
        });
        if !req.start {
            sessions.remove(node);
            return Ok(None);
        }
        let stale = sessions
            .get(node)
            .is_some_and(|s| s.trackers != req.trackers || s.count != req.count);
        if stale {
            sessions.remove(node);
        }
        let s = sessions
            .entry(node.to_owned())
            .or_insert_with(|| Session::spawn(req.trackers.clone(), req.count));
        let mut st = s.shared.lock();
        st.last_poll = Some(Instant::now());
        if let Some(img) = &req.image {
            if img.id() != st.last_image {
                st.last_image = img.id();
                st.pending = Some((img.clone(), s.born.elapsed().as_secs_f64()));
                s.shared.wake.notify_all();
            }
        }
        match &st.error {
            Some(e) => Err(e.clone()),
            None => Ok(st.result.clone()),
        }
    }

    /// Number of running sessions (for tests and diagnostics).
    pub fn active(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

/// The session thread.
fn run(shared: &Shared, trackers: &[Tracker], count: usize) {
    let finish = |error: Option<String>| {
        let mut st = shared.lock();
        st.error = error;
        st.done = true;
    };
    let mut lms = match trackers
        .iter()
        .map(|&t| Landmarker::new(t, count))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(l) => l,
        Err(e) => return finish(Some(format!("tracking models failed to load: {e}"))),
    };
    let mut id_trackers: Vec<ids::IdTracker> =
        trackers.iter().map(|_| Default::default()).collect();
    shared.lock().result = Some(Vec::new());

    loop {
        let (img, t) = {
            let mut st = shared.lock();
            loop {
                if st.cancel || st.last_poll.is_none_or(|t| t.elapsed() >= IDLE) {
                    drop(st);
                    return finish(None);
                }
                if let Some(p) = st.pending.take() {
                    break p;
                }
                st = shared
                    .wake
                    .wait_timeout(st, Duration::from_millis(250))
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
        };
        let frame = geom::Frame {
            width: img.width(),
            height: img.height(),
            rgba: img.rgba(),
        };
        let found: Vec<Vec<Vec<geom::Point>>> = if lms.len() == 1 {
            vec![lms[0].process(&frame, t)]
        } else {
            std::thread::scope(|s| {
                let jobs: Vec<_> = lms
                    .iter_mut()
                    .map(|lm| s.spawn(|| lm.process(&frame, t)))
                    .collect();
                jobs.into_iter()
                    .map(|j| j.join().unwrap_or_default())
                    .collect()
            })
        };
        let mut layers: Vec<ids::Layer> = found
            .into_iter()
            .zip(trackers)
            .zip(&mut id_trackers)
            .map(|((dets, &tracker), idt)| ids::Layer {
                tracker,
                ids: idt.assign(&dets),
                dets,
            })
            .collect();
        if trackers.len() > 1 {
            ids::unify(&mut layers);
        }
        shared.lock().result = Some(
            layers
                .into_iter()
                .map(|l| LandmarkLayer {
                    tracker: l.tracker,
                    dets: l.dets,
                    ids: l.ids,
                })
                .collect(),
        );
    }
}
