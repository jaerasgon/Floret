//! Services the host app lends the engine.
//!
//! The engine has no GPU and no network, but a few nodes need one:
//! `image/from-layer` and `transform/cache` rasterise a scene, and
//! `io/image-url` fetches `http(s)` URLs. The app implements [`Host`]
//! (floret-render rasterises, an HTTP client fetches) and installs it with
//! [`Engine::set_host`](crate::Engine::set_host). Without a host those
//! nodes still produce well-formed values: a transparent image of the
//! requested size (what the TS fixtures recorded, where the canvas was a
//! no-op mock) and a failed load.
//!
//! Plumbing: `Engine::tick` installs its host in a thread-local for the
//! duration of the tick, and nodes read it through [`EvalCtx::host`]
//! (`crate::node::EvalCtx`). That keeps `EvalCtx` / `FrameCtx` literals
//! unchanged for every node and test that builds them.

use crate::image::Image;
use crate::scene::SceneNode;
use std::cell::RefCell;
use std::sync::Arc;

pub trait Host: Send + Sync {
    /// Render `layers` (centre origin, painter's order) into a
    /// `width × height` image on a transparent background, like the web
    /// app's `renderLayersToCanvas(layers, w, h, null, true)`. `None` when
    /// rendering isn't available.
    fn rasterize(&self, layers: &[Arc<SceneNode>], width: u32, height: u32) -> Option<Image> {
        let _ = (layers, width, height);
        None
    }

    /// Fetch a remote (`http:` / `https:`) URL's bytes. Called from a
    /// loader thread, never from a tick.
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        Err(format!("no network access to load {url}"))
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<dyn Host>>> = const { RefCell::new(None) };
}

/// The host installed for the tick running on this thread.
pub fn current() -> Option<Arc<dyn Host>> {
    CURRENT.with(|c| c.borrow().clone())
}

/// Installs a host for the current thread until the guard drops (restoring
/// whatever was installed before, so nested engines are fine).
pub struct HostGuard(Option<Arc<dyn Host>>);

pub fn install(host: Option<Arc<dyn Host>>) -> HostGuard {
    HostGuard(CURRENT.with(|c| c.replace(host)))
}

impl Drop for HostGuard {
    fn drop(&mut self) {
        let prev = self.0.take();
        CURRENT.with(|c| *c.borrow_mut() = prev);
    }
}
