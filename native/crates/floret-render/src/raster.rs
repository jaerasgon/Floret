//! Layers → engine images, for `image/from-layer` and `transform/cache`
//! (the engine's `Host::rasterize`, see `floret_engine::host`).

use crate::headless::Headless;
use floret_engine::host::Host;
use floret_engine::image::Image;
use floret_engine::scene::SceneNode;
use std::sync::{Arc, Mutex};

enum Gpu {
    Untried,
    Ready(Box<Headless>),
    Unavailable,
}

/// Renders layers offscreen on a device of its own, created on first use
/// (so patches without rasterising nodes never pay for it). Install it as
/// (part of) the engine's host; it's `Send + Sync`.
pub struct LayerRasterizer {
    gpu: Mutex<Gpu>,
}

impl Default for LayerRasterizer {
    fn default() -> Self {
        LayerRasterizer {
            gpu: Mutex::new(Gpu::Untried),
        }
    }
}

impl LayerRasterizer {
    pub fn new() -> LayerRasterizer {
        LayerRasterizer::default()
    }

    /// `width × height` RGBA (straight alpha), centre origin, transparent
    /// background. `None` without a GPU or past the device's size limit.
    pub fn rasterize(&self, layers: &[Arc<SceneNode>], width: u32, height: u32) -> Option<Image> {
        let mut gpu = self.gpu.lock().unwrap_or_else(|p| p.into_inner());
        if matches!(*gpu, Gpu::Untried) {
            *gpu = match Headless::new() {
                Some(mut h) => {
                    h.renderer.set_transparent_background(true);
                    Gpu::Ready(Box::new(h))
                }
                None => Gpu::Unavailable,
            };
        }
        let Gpu::Ready(h) = &mut *gpu else {
            return None;
        };
        let max = h.device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > max || height > max {
            return None;
        }
        let rgba = h.render(layers, width, height, [0.0; 4]);
        Some(Image::from_premultiplied(width, height, rgba))
    }
}

impl Host for LayerRasterizer {
    fn rasterize(&self, layers: &[Arc<SceneNode>], width: u32, height: u32) -> Option<Image> {
        LayerRasterizer::rasterize(self, layers, width, height)
    }
}
