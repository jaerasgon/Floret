//! Patch files the operating system asks Floret to open.
//!
//! On macOS, double-clicking a `.flora` file in Finder (or Open With →
//! Floret, or dropping one on the Dock icon) doesn't pass the file on the
//! command line: it sends the app an "open documents" Apple Event, both at
//! launch and while it runs. winit has no API for that, so [`install`]
//! registers a handler for it (see `macos.rs`) that queues the paths here;
//! the app takes them with [`take`] and opens them like File → Open.
//!
//! Elsewhere [`install`] does nothing: Windows and Linux pass the file as a
//! command-line argument, which the app reads itself.
//!
//! Nothing here may bring the app down: if the hook can't be installed it
//! logs why and Floret runs without it.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

#[cfg(target_os = "macos")]
mod macos;

type Waker = Box<dyn Fn() + Send + Sync>;

static QUEUE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static WAKER: OnceLock<Waker> = OnceLock::new();

/// Start listening for the OS's open-file requests. Call once, on the main
/// thread, before the event loop starts (so the request that launched the
/// app is caught too).
pub fn install() {
    #[cfg(target_os = "macos")]
    macos::install();
}

/// Called after paths are queued (e.g. to wake the UI so it opens them).
/// Only the first waker set is kept.
pub fn set_waker(wake: impl Fn() + Send + Sync + 'static) {
    let _ = WAKER.set(Box::new(wake));
}

/// Queue files to open and wake the app.
pub fn push(paths: impl IntoIterator<Item = PathBuf>) {
    let added = {
        let mut q = queue();
        let n = q.len();
        q.extend(paths);
        q.len() > n
    };
    if added {
        if let Some(wake) = WAKER.get() {
            wake();
        }
    }
}

/// Every file asked for since the last call, oldest first.
pub fn take() -> Vec<PathBuf> {
    std::mem::take(&mut *queue())
}

/// The queue, even if a panic poisoned its lock (a `Vec` can't be left
/// half-updated by `extend` / `take`).
fn queue() -> MutexGuard<'static, Vec<PathBuf>> {
    QUEUE.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn queues_in_order_and_wakes() {
        static WOKEN: AtomicUsize = AtomicUsize::new(0);
        set_waker(|| {
            WOKEN.fetch_add(1, Ordering::SeqCst);
        });
        install(); // a no-op off macOS; must not panic
        assert!(take().is_empty());
        push([PathBuf::from("/a/one.flora"), PathBuf::from("/b/two.flora")]);
        push(Vec::new());
        push([PathBuf::from("three.flora")]);
        assert_eq!(
            WOKEN.load(Ordering::SeqCst),
            2,
            "an empty push doesn't wake"
        );
        assert_eq!(
            take(),
            ["/a/one.flora", "/b/two.flora", "three.flora"].map(PathBuf::from)
        );
        assert!(take().is_empty());
    }
}
