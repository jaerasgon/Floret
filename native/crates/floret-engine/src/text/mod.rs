//! Typography: the font library, shaping, glyph outlines and text-on-path
//! layout behind the `text/*` nodes.
//!
//! The renderer draws `draw/text` with cosmic-text, and anything that needs
//! more (variation axes, pre-shaped glyph runs) from these same layouts:
//! glyph ids, positions and axis coordinates are computed here once, and
//! the renderer only rasterises them, so node outputs (glyph positions,
//! outline points) line up with what's drawn.
//!
//! Font data is pure-Rust: fontdb (system + bundled + loaded files),
//! harfrust for shaping (the shaper cosmic-text uses), skrifa for outlines
//! and axes, unicode-bidi for mixed-direction lines.

mod layout;
mod library;
pub mod outline;
pub mod path;

pub use layout::{
    block, face_settings, normalized_coords, shape_line, AxisSetting, BlockGlyph, FontSpec,
    ShapedGlyph, ShapedLine, TextBlock,
};
pub use library::{
    css_weight, database, face, fallback, families, generation, load_bytes, load_file, resolve,
    resolve_family, AxisInfo, FaceData, FaceId, LoadedFont, VARIABLE_FAMILY,
};

/// Parse a 4-character axis tag (`"wght"`); shorter tags are space-padded
/// like OpenType's.
pub fn parse_tag(s: &str) -> Option<[u8; 4]> {
    let s = s.trim();
    if s.is_empty() || s.len() > 4 || !s.is_ascii() {
        return None;
    }
    let mut t = [b' '; 4];
    t[..s.len()].copy_from_slice(s.as_bytes());
    Some(t)
}

#[cfg(test)]
mod tests;
