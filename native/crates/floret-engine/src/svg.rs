//! SVG export of a scene. Port of `src/runtime/scene-svg.ts` (in progress).

use crate::scene::SceneNode;
use std::sync::Arc;

/// Render layers to an SVG document of the given size and background.
pub fn scene_to_svg(
    _layers: &[Arc<SceneNode>],
    width: f64,
    height: f64,
    _background: &str,
) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}"></svg>"#)
}
