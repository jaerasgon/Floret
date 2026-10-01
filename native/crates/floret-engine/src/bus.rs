//! The Send / Receive channel bus (`io/send`, `io/receive`).
//!
//! In TS the nodes POST / poll a relay inside the Vite dev server
//! (`floret-bus.vite.mjs`). Here the engine only sees the [`Bus`] trait;
//! the app installs an implementation (an in-app HTTP server over a
//! [`MemoryBus`], so the Raspberry Pi e-ink poller keeps working). With no
//! bus installed the nodes behave like the TS ones outside a browser:
//! Send reports `ok = false`, Receive outputs nothing.

use crate::scene::SceneNode;
use crate::value::Color;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

/// What `io/send` transmits.
#[derive(Clone, Debug)]
pub enum BusPayload {
    /// `JSON.stringify(data)`; travels as `application/json`.
    Json(String),
    /// Layers to rasterise to a PNG (`width × height`, opaque background).
    /// The host renders it; the engine has no GPU.
    Layers {
        layers: Vec<Arc<SceneNode>>,
        width: u32,
        height: u32,
        background: Color,
    },
}

/// The latest payload on a channel (the relay's `{ body, contentType,
/// version }`).
#[derive(Clone, Debug, PartialEq)]
pub struct BusMessage {
    /// Bumped on every publish to the channel, from 1.
    pub version: u64,
    pub content_type: String,
    pub body: Arc<[u8]>,
}

pub trait Bus: Send + Sync {
    /// Publish on `channel`. `false` when the transport refused it.
    fn send(&self, channel: &str, payload: BusPayload) -> bool;
    /// The latest message on `channel`, if anything was published.
    fn latest(&self, channel: &str) -> Option<BusMessage>;
}

fn slot() -> &'static RwLock<Option<Arc<dyn Bus>>> {
    static BUS: RwLock<Option<Arc<dyn Bus>>> = RwLock::new(None);
    &BUS
}

/// Install (or remove) the process-wide bus the Send / Receive nodes use.
pub fn install(bus: Option<Arc<dyn Bus>>) {
    *slot().write().unwrap_or_else(|e| e.into_inner()) = bus;
}

/// The installed bus, if any.
pub fn installed() -> Option<Arc<dyn Bus>> {
    slot().read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// A layers payload waiting for the host to rasterise it.
#[derive(Clone, Debug)]
pub struct LayerJob {
    pub channel: String,
    pub layers: Vec<Arc<SceneNode>>,
    pub width: u32,
    pub height: u32,
    pub background: Color,
}

/// The relay's channel store: latest payload + content type + version per
/// channel. JSON sends are stored at once; layer sends queue a
/// [`LayerJob`] that the host renders and [`MemoryBus::publish`]es as PNG.
#[derive(Default)]
pub struct MemoryBus {
    channels: Mutex<HashMap<String, BusMessage>>,
    jobs: Mutex<Vec<LayerJob>>,
}

impl MemoryBus {
    pub fn new() -> MemoryBus {
        MemoryBus::default()
    }

    /// Store a payload (the relay's POST). Returns the new version.
    pub fn publish(&self, channel: &str, content_type: &str, body: Vec<u8>) -> u64 {
        let mut ch = self.channels.lock().unwrap_or_else(|e| e.into_inner());
        let version = ch.get(channel).map_or(0, |m| m.version) + 1;
        ch.insert(
            channel.to_owned(),
            BusMessage {
                version,
                content_type: content_type.to_owned(),
                body: Arc::from(body),
            },
        );
        version
    }

    /// Layer sends waiting to be rasterised, oldest first. A channel's
    /// older jobs are superseded by its newest.
    pub fn take_layer_jobs(&self) -> Vec<LayerJob> {
        let jobs = std::mem::take(&mut *self.jobs.lock().unwrap_or_else(|e| e.into_inner()));
        let mut out: Vec<LayerJob> = Vec::new();
        for j in jobs {
            out.retain(|o| o.channel != j.channel);
            out.push(j);
        }
        out
    }
}

impl Bus for MemoryBus {
    fn send(&self, channel: &str, payload: BusPayload) -> bool {
        match payload {
            BusPayload::Json(text) => {
                self.publish(channel, "application/json", text.into_bytes());
            }
            BusPayload::Layers {
                layers,
                width,
                height,
                background,
            } => self
                .jobs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(LayerJob {
                    channel: channel.to_owned(),
                    layers,
                    width,
                    height,
                    background,
                }),
        }
        true
    }

    fn latest(&self, channel: &str) -> Option<BusMessage> {
        self.channels
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(channel)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_jobs() {
        let bus = MemoryBus::new();
        assert!(bus.latest("a").is_none());
        assert!(bus.send("a", BusPayload::Json("[1]".into())));
        assert_eq!(bus.publish("a", "text/plain", b"hi".to_vec()), 2);
        let m = bus.latest("a").unwrap();
        assert_eq!(
            (m.version, m.content_type.as_str(), &*m.body),
            (2, "text/plain", &b"hi"[..])
        );
        let job = |w| BusPayload::Layers {
            layers: vec![],
            width: w,
            height: 1,
            background: Color::WHITE,
        };
        bus.send("x", job(1));
        bus.send("y", job(2));
        bus.send("x", job(3));
        let jobs: Vec<_> = bus
            .take_layer_jobs()
            .into_iter()
            .map(|j| (j.channel, j.width))
            .collect();
        assert_eq!(jobs, vec![("y".to_owned(), 2), ("x".to_owned(), 3)]);
        assert!(bus.take_layer_jobs().is_empty());
    }
}
