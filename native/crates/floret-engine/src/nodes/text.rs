//! `text/*` nodes (native-only): fonts, variable-font axes, per-glyph
//! layout, text on a path and glyph outline points. See `crate::text`.
//!
//! `text/glyphs` and `text/outline` lay text out exactly like `draw/text`
//! with the same string / pos / size / align / style, so their glyphs and
//! points sit on top of the drawn text.

use super::convert::js_string_or_empty;
use super::shared::to_vec2;
use super::style::style_arg;
use super::Registrar;
use crate::node::{scalar, NodeImpl, NodeInit};
use crate::scene::{
    default_style, resolve_style, GlyphRun, PlacedGlyph, ResolvedStyle, SceneNode, Style, StyleOp,
};
use crate::text::path::Path;
use crate::text::{self, FontSpec};
use crate::value::{Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("text/font", font);
    r.node("text/glyphs", glyphs);
    r.node("text/on-path", on_path);
    r.node("text/outline", outline);
    r.node("style/font-variations", font_variations);
}

fn str_arg(v: &Value) -> String {
    js_string_or_empty(v).trim().to_owned()
}

fn num_arg(v: &Value, fallback: f64) -> f64 {
    let n = v.to_number();
    if n.is_finite() {
        n
    } else {
        fallback
    }
}

fn nums(v: impl IntoIterator<Item = f64>) -> Value {
    Value::list(v.into_iter().map(Value::Num).collect())
}

fn vecs(v: impl IntoIterator<Item = Vec2>) -> Value {
    Value::list(v.into_iter().map(Value::Vec2).collect())
}

fn layer(run: GlyphRun) -> Value {
    Value::Scene(Arc::new(SceneNode::Glyphs(Arc::new(run))))
}

/// The text style a `draw/text`-like node draws with: the wired chain,
/// with this node's size (and alignment, when given).
fn text_style(style: &Value, size: f64, align: Option<&Value>) -> Arc<ResolvedStyle> {
    let base = resolve_style(style_arg(style), default_style());
    let mut st = ResolvedStyle {
        font_size: size,
        ..(*base).clone()
    };
    if let Some(a) = align {
        st.text_align = match a {
            Value::Undefined | Value::Null => Arc::from("left"),
            other => Arc::from(js_string_or_empty(other)),
        };
    }
    Arc::new(st)
}

fn size_arg(v: &Value) -> f64 {
    crate::js::max(1.0, v.to_number())
}

/// A glyph run of one glyph at the origin (for per-glyph layers).
fn single(face: text::FaceId, id: u16, size: f64, style: &Arc<ResolvedStyle>) -> Value {
    layer(GlyphRun {
        glyphs: Arc::from([PlacedGlyph {
            face,
            id,
            x: 0.0,
            y: 0.0,
            angle: 0.0,
        }]),
        size,
        style: style.clone(),
    })
}

// ─── text/font ───────────────────────────────────────────────────────────────

/// Outputs (font, loaded, axes, axisMin, axisDefault, axisMax). `font` is a
/// family name for `style/text-params`. A file wins over the family.
fn font(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let family = str_arg(&args[0]);
        let file = str_arg(&args[1]);
        let (name, loaded) = if file.is_empty() {
            let name = if family.is_empty() {
                "sans-serif".to_owned()
            } else {
                family
            };
            let resolved = text::resolve_family(&name);
            let loaded = resolved.eq_ignore_ascii_case(&name)
                || name
                    .split(',')
                    .any(|f| resolved.eq_ignore_ascii_case(f.trim().trim_matches(['"', '\''])));
            (name, loaded)
        } else {
            match text::load_file(std::path::Path::new(&file)) {
                Ok(f) => (f.family, true),
                Err(_) => (
                    if family.is_empty() {
                        "sans-serif".to_owned()
                    } else {
                        family
                    },
                    false,
                ),
            }
        };
        let axes = text::resolve(&name, 400, false)
            .and_then(text::face)
            .map(|f| f.axes.clone())
            .unwrap_or_default();
        Ok(vec![
            Value::str(&name),
            Value::Bool(loaded),
            Value::list(axes.iter().map(|a| Value::str(&a.tag_str())).collect()),
            nums(axes.iter().map(|a| f64::from(a.min))),
            nums(axes.iter().map(|a| f64::from(a.default))),
            nums(axes.iter().map(|a| f64::from(a.max))),
        ])
    })
}

// ─── style/font-variations ───────────────────────────────────────────────────

/// Variable-font axes on a style chain. `weight`, `width` and
/// `opticalSize` apply when > 0, `slant` when non-zero; `axis` / `value`
/// set any other axis by tag (chain several nodes for more).
fn font_variations(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let mut ops = Vec::new();
        let mut set = |tag: &[u8; 4], v: f64, on: bool| {
            if on && v.is_finite() {
                ops.push(StyleOp::FontVariation(*tag, v as f32));
            }
        };
        let w = args[0].to_number();
        set(b"wght", w, w > 0.0);
        let wd = args[1].to_number();
        set(b"wdth", wd, wd > 0.0);
        let sl = args[2].to_number();
        set(b"slnt", sl, sl != 0.0);
        let op = args[3].to_number();
        set(b"opsz", op, op > 0.0);
        if let Some(tag) = text::parse_tag(&js_string_or_empty(&args[4])) {
            set(&tag, args[5].to_number(), true);
        }
        Ok(vec![Value::Style(Style::chain(style_arg(&args[6]), ops))])
    })
}

// ─── text/glyphs ─────────────────────────────────────────────────────────────

/// Inputs (string, pos, size, align, spacing, style). Outputs (shape,
/// glyphs, positions, advances, chars, charIndex): the whole run as one
/// layer, then per glyph a layer drawn at the origin (translate it to its
/// position), its origin on the baseline, advance, source text and the
/// index of its first character in the string.
fn glyphs(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let s = js_string_or_empty(&args[0]);
        let pos = to_vec2(&args[1]);
        let size = size_arg(&args[2]);
        let st = text_style(&args[5], size, Some(&args[3]));
        let tracking = st.letter_spacing + num_arg(&args[4], 0.0);
        let mut placed = Vec::new();
        let (mut layers, mut positions, mut advances, mut chars, mut index) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        if let Some(b) = text::block(&s, &st, tracking) {
            let (ox, oy) = b.origin(&st, pos.x, pos.y);
            for g in &b.glyphs {
                let (x, y) = (ox + g.glyph.x, oy + g.glyph.y);
                placed.push(PlacedGlyph {
                    face: g.glyph.face,
                    id: g.glyph.id,
                    x,
                    y,
                    angle: 0.0,
                });
                layers.push(single(g.glyph.face, g.glyph.id, size, &st));
                positions.push(Vec2 { x, y });
                advances.push(g.glyph.advance);
                chars.push(Value::str(&s[g.source.clone()]));
                index.push(s[..g.source.start].chars().count() as f64);
            }
        }
        Ok(vec![
            layer(GlyphRun {
                glyphs: Arc::from(placed),
                size,
                style: st,
            }),
            Value::list(layers),
            vecs(positions),
            nums(advances),
            Value::list(chars),
            nums(index),
        ])
    })
}

// ─── text/on-path ────────────────────────────────────────────────────────────

/// Inputs (string, points, path, closed, size, offset, spacing, align,
/// shift, style). Outputs (shape, glyphs, positions, angles). A wired
/// `path` layer (e.g. `draw/path`) wins over `points`.
fn on_path(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let s = js_string_or_empty(&args[0]).replace('\n', " ");
        let closed = args[3].truthy();
        let path = match &args[2] {
            Value::Scene(n) => Path::from_layer(n),
            _ => None,
        }
        .or_else(|| match &args[1] {
            Value::List(items) => Path::new(items.iter().map(to_vec2), closed),
            _ => None,
        });
        let size = size_arg(&args[4]);
        let st = text_style(&args[9], size, None);
        let tracking = st.letter_spacing + num_arg(&args[6], 0.0);
        let align = js_string_or_empty(&args[7]);
        let mut placed = Vec::new();
        let (mut layers, mut positions, mut angles) = (Vec::new(), Vec::new(), Vec::new());
        if let Some(path) = path {
            let line = text::shape_line(&s, &FontSpec::from_style(&st), tracking);
            for g in text::path::on_path(
                &line,
                &path,
                num_arg(&args[5], 0.0),
                &align,
                num_arg(&args[8], 0.0),
            ) {
                placed.push(PlacedGlyph {
                    face: g.glyph.face,
                    id: g.glyph.id,
                    x: g.pos.x,
                    y: g.pos.y,
                    angle: g.angle,
                });
                layers.push(single(g.glyph.face, g.glyph.id, size, &st));
                positions.push(g.pos);
                angles.push(g.angle);
            }
        }
        Ok(vec![
            layer(GlyphRun {
                glyphs: Arc::from(placed),
                size,
                style: st,
            }),
            Value::list(layers),
            vecs(positions),
            nums(angles),
        ])
    })
}

// ─── text/outline ────────────────────────────────────────────────────────────

/// Inputs (string, pos, size, align, spacing, mode, style). Outputs
/// (points, contours, glyphs): points every `spacing` units along the glyph
/// outlines (`mode` `outline`) or on a grid inside them (`fill`), with the
/// contour and glyph each came from.
fn outline(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let s = js_string_or_empty(&args[0]);
        let pos = to_vec2(&args[1]);
        let size = size_arg(&args[2]);
        let st = text_style(&args[6], size, Some(&args[3]));
        let spacing = num_arg(&args[4], 6.0).max(0.25);
        let fill = js_string_or_empty(&args[5]) == "fill";
        let font = FontSpec::from_style(&st);
        let (mut points, mut contours, mut glyph_ix) = (Vec::new(), Vec::new(), Vec::new());
        let mut contour_id = 0usize;
        if let Some(b) = text::block(&s, &st, st.letter_spacing) {
            let (ox, oy) = b.origin(&st, pos.x, pos.y);
            for (gi, g) in b.glyphs.iter().enumerate() {
                let Some(face) = text::face(g.glyph.face) else {
                    continue;
                };
                let settings = text::face_settings(&face, &font);
                let at = |p: Vec2| Vec2 {
                    x: ox + g.glyph.x + p.x,
                    y: oy + g.glyph.y + p.y,
                };
                let cs = text::outline::glyph_contours(&face, g.glyph.id, size, &settings);
                if fill {
                    for p in text::outline::fill_points(&cs, spacing) {
                        points.push(at(p));
                        contours.push(contour_id as f64);
                        glyph_ix.push(gi as f64);
                    }
                    contour_id += 1;
                } else {
                    for c in &cs {
                        for p in text::outline::sample_contour(c, spacing) {
                            points.push(at(p));
                            contours.push(contour_id as f64);
                            glyph_ix.push(gi as f64);
                        }
                        contour_id += 1;
                    }
                }
            }
        }
        Ok(vec![vecs(points), nums(contours), nums(glyph_ix)])
    })
}

#[cfg(test)]
mod tests;
