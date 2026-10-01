//! Floret's GPU scene renderer.
//!
//! Turns the engine's retained-mode scene (`floret_engine::scene::SceneNode`
//! trees) into pixels with wgpu, matching the web app's WebGL2 renderer
//! (`src/runtime/gl/`): SDF rects / ellipses / capsules, stencil-winding
//! polygon fills, CPU-rasterised text quads, blend-group layers, per-shape
//! Gaussian blur, scissor clips, MSAA, premultiplied alpha.
//!
//! ```no_run
//! # fn demo(layers: &[std::sync::Arc<floret_engine::scene::SceneNode>]) {
//! // Headless: render to RGBA8 bytes (None when there's no GPU adapter).
//! let pixels = floret_render::render_to_rgba(layers, 400, 400, [0.0, 0.0, 0.0, 1.0]);
//! # }
//! ```
//!
//! For live rendering create a [`Renderer`] on your device and call
//! [`Renderer::render`] each frame into a texture of the canvas size.

mod batch;
mod gpu;
mod headless;
pub mod shader;
mod text;

pub use batch::{Blend, Cmd, Emitter, FrameBuilder};
pub use gpu::{RenderStats, Renderer, STENCIL_FORMAT};
pub use headless::{render_and_read, render_to_rgba, Headless, FORMAT as OFFSCREEN_FORMAT};
pub use shader::{RenderClock, ShaderError};
pub use text::{TextRasterizer, TextRun};

use floret_engine::scene::{Paint, SceneNode};
use floret_engine::Value;
use std::sync::Arc;

/// Canvas size limits (the web app's driver clamps to the same range).
pub const MIN_CANVAS: u32 = 32;
pub const MAX_CANVAS: u32 = 4096;
/// Default canvas size when the renderer has no usable width / height.
pub const DEFAULT_CANVAS: (u32, u32) = (1080, 1920);

/// The scene a renderer node shows: its layer spread (first output)
/// flattened into top-level nodes, painter's order. Lists (e.g. one per
/// ForEach iteration) are flattened; null / undefined / non-scene values
/// are dropped — like the TS canvas driver.
pub fn flatten_layers(spread: &[Value]) -> Vec<Arc<SceneNode>> {
    fn walk(v: &Value, out: &mut Vec<Arc<SceneNode>>) {
        match v {
            Value::Scene(n) => out.push(n.clone()),
            Value::List(items) => items.iter().for_each(|i| walk(i, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for v in spread {
        walk(v, &mut out);
    }
    out
}

/// The renderer's background (second output, slot 0) as RGBA 0..1. Like the
/// TS driver, only a `Color` counts (quantised to 8-bit through its CSS
/// form); anything else is black.
pub fn background(spread: &[Value]) -> [f64; 4] {
    match spread.first() {
        Some(Value::Color(c)) => Paint::from_color(*c).rgba,
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}

/// Canvas size from a renderer's `width` / `height` params: floored and
/// clamped to [`MIN_CANVAS`]..=[`MAX_CANVAS`], [`DEFAULT_CANVAS`] when not
/// a finite number (the TS driver's rule).
pub fn canvas_size(
    width: Option<&serde_json::Value>,
    height: Option<&serde_json::Value>,
) -> (u32, u32) {
    let dim = |v: Option<&serde_json::Value>, d: u32| {
        v.and_then(|v| v.as_f64())
            .filter(|n| n.is_finite())
            .map_or(d, |n| {
                n.floor()
                    .clamp(f64::from(MIN_CANVAS), f64::from(MAX_CANVAS)) as u32
            })
    };
    (dim(width, DEFAULT_CANVAS.0), dim(height, DEFAULT_CANVAS.1))
}

/// The renderer can live in shared state (e.g. egui callback resources).
const _: () = {
    const fn check<T: Send + Sync>() {}
    check::<Renderer>();
    check::<Headless>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use floret_engine::scene::{default_style, SceneNode};
    use floret_engine::Color;
    use serde_json::json;

    #[test]
    fn flattens_nested_layer_lists() {
        let n = Arc::new(SceneNode::Point {
            x: 0.0,
            y: 0.0,
            style: default_style().clone(),
        });
        let spread = vec![
            Value::Scene(n.clone()),
            Value::Null,
            Value::list(vec![Value::Scene(n.clone()), Value::Undefined]),
            Value::list(vec![Value::list(vec![Value::Scene(n)])]),
            Value::Num(3.0),
        ];
        assert_eq!(flatten_layers(&spread).len(), 3);
    }

    #[test]
    fn background_is_a_colour_or_black() {
        let c = Color {
            r: 1.0,
            g: 0.5,
            b: 0.0,
            a: 1.0,
        };
        assert_eq!(
            background(&[Value::Color(c)]),
            [1.0, 128.0 / 255.0, 0.0, 1.0]
        );
        // Like the TS driver, a CSS string doesn't count.
        assert_eq!(background(&[Value::str("#ff0000")]), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(background(&[]), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn canvas_size_follows_the_driver() {
        assert_eq!(
            canvas_size(Some(&json!(500)), Some(&json!(400.7))),
            (500, 400)
        );
        assert_eq!(
            canvas_size(Some(&json!(1)), Some(&json!(99999))),
            (32, 4096)
        );
        assert_eq!(canvas_size(None, Some(&json!("x"))), DEFAULT_CANVAS);
    }
}
