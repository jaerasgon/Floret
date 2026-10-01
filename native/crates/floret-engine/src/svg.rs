//! SVG export of a scene — a port of `src/runtime/scene-svg.ts`.
//!
//! Walks a [`SceneNode`] tree and writes a self-contained SVG document. The
//! output is byte-identical to the TypeScript exporter for the same scene
//! (checked against goldens in `native/fixtures/svg/`), so every quirk of the
//! TS code is kept on purpose:
//!
//! - Scene coordinates are centre-origin, so the content sits inside a
//!   `<g transform="translate(w/2,h/2)">`.
//! - Numbers go through [`n`]: rounded to 3 decimals, printed like JS
//!   `Number#toString`, non-finite values collapse to `0`.
//! - `rgba(r,g,b,a)` paints are split into `rgb(r,g,b)` plus a
//!   `fill-opacity` / `stroke-opacity` attribute (strict SVG parsers render
//!   `rgba()` black); every other colour string passes through escaped.
//! - A group's affine is SVG's `matrix(a b c d e f)` as is; CSS blend
//!   keywords become `mix-blend-mode`, compositing operators are dropped;
//!   clip rects become numbered `<clipPath>`s.
//! - Polygons with `tension > 0` (and ≥ 3 points) become Catmull–Rom cubic
//!   Bézier `<path>`s, using the same control points as the renderer.
//!
//! Not ported (no Rust scene kind yet): `image`, `particle-batch`,
//! `particle-trail-batch`. The TS exporter emits nothing for `shader`.

use crate::js;
use crate::scene::{catmull_rom_beziers, resolve_box, ResolvedStyle, SceneNode};
use std::fmt::Write as _;
use std::sync::Arc;

/// Canvas size and background for [`scene_to_svg`] (`SvgExportOptions`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SvgExportOptions<'a> {
    /// Canvas width in pixels; floored and clamped to ≥ 1.
    pub width: f64,
    /// Canvas height in pixels; floored and clamped to ≥ 1.
    pub height: f64,
    /// CSS colour for a full-canvas background rect. `None` (or an empty
    /// string, which JS treats as falsy) leaves the background transparent.
    pub background: Option<&'a str>,
}

/// `sceneToSvg`: render top-level layers to a complete SVG document.
///
/// `layers` are the renderer's flattened layers (what the canvas driver
/// draws). Scene coordinates are centre-origin; the document translates by
/// half the canvas size so they land where the renderer draws them.
pub fn scene_to_svg(layers: &[Arc<SceneNode>], opts: &SvgExportOptions) -> String {
    let mut s = Emit {
        out: String::new(),
        clip_counter: 0,
    };
    let w = js::max(1.0, opts.width.floor());
    let h = js::max(1.0, opts.height.floor());
    let (ws, hs) = (js::num_to_string(w), js::num_to_string(h));
    s.out.push_str(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" "#,
    );
    let _ = write!(
        s.out,
        r#"viewBox="0 0 {ws} {hs}" width="{ws}" height="{hs}">"#
    );
    if let Some(bg) = opts.background.filter(|b| !b.is_empty()) {
        let _ = write!(
            s.out,
            r#"<rect width="{ws}" height="{hs}" fill="{}"/>"#,
            esc(bg)
        );
    }
    let _ = write!(
        s.out,
        r#"<g transform="translate({},{})">"#,
        n(w / 2.0),
        n(h / 2.0)
    );
    for layer in layers {
        s.node(layer);
    }
    s.out.push_str("</g></svg>");
    s.out
}

struct Emit {
    out: String,
    clip_counter: usize,
}

/// Blend keywords that map 1:1 to CSS `mix-blend-mode` (`SVG_BLEND_MODES`).
const SVG_BLEND_MODES: &[&str] = &[
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "color-dodge",
    "color-burn",
    "hard-light",
    "soft-light",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "plus-lighter",
];

impl Emit {
    fn node(&mut self, node: &SceneNode) {
        match node {
            SceneNode::Group {
                children,
                transform,
                blend_mode,
                clip,
            } => {
                let mut attrs: Vec<String> = Vec::new();
                if let Some(m) = transform {
                    attrs.push(format!(
                        r#"transform="matrix({} {} {} {} {} {})""#,
                        n(m[0]),
                        n(m[1]),
                        n(m[2]),
                        n(m[3]),
                        n(m[4]),
                        n(m[5])
                    ));
                }
                if let Some(b) = blend_mode.as_deref() {
                    if SVG_BLEND_MODES.contains(&b) {
                        attrs.push(format!(r#"style="mix-blend-mode:{b}""#));
                    }
                }
                if let Some(c) = clip {
                    let id = format!("clip{}", self.clip_counter);
                    self.clip_counter += 1;
                    let _ = write!(
                        self.out,
                        r#"<defs><clipPath id="{id}"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath></defs>"#,
                        n(c.x),
                        n(c.y),
                        n(c.w),
                        n(c.h)
                    );
                    attrs.push(format!(r#"clip-path="url(#{id})""#));
                }
                if attrs.is_empty() {
                    self.out.push_str("<g>");
                } else {
                    let _ = write!(self.out, "<g {}>", attrs.join(" "));
                }
                for child in children.iter() {
                    self.node(child);
                }
                self.out.push_str("</g>");
            }
            SceneNode::Rect {
                x,
                y,
                w,
                h,
                mode,
                style,
            } => {
                let [x, y, w, h] = resolve_box(*x, *y, *w, *h, mode);
                let _ = write!(
                    self.out,
                    r#"<rect x="{}" y="{}" width="{}" height="{}" {}/>"#,
                    n(x),
                    n(y),
                    n(w),
                    n(h),
                    style_attrs(style)
                );
            }
            SceneNode::Ellipse {
                x,
                y,
                w,
                h,
                mode,
                style,
            } => {
                let [x, y, w, h] = resolve_box(*x, *y, *w, *h, mode);
                let _ = write!(
                    self.out,
                    r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" {}/>"#,
                    n(x + w / 2.0),
                    n(y + h / 2.0),
                    n((w / 2.0).abs()),
                    n((h / 2.0).abs()),
                    style_attrs(style)
                );
            }
            SceneNode::Line {
                x1,
                y1,
                x2,
                y2,
                style,
            } => {
                let Some(stroke) = style.stroke.as_ref().filter(|p| !p.css.is_empty()) else {
                    return;
                };
                let _ = write!(
                    self.out,
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" {} fill="none"/>"#,
                    n(*x1),
                    n(*y1),
                    n(*x2),
                    n(*y2),
                    stroke_attrs(&stroke.css, style)
                );
            }
            SceneNode::Arc {
                x,
                y,
                w,
                h,
                start,
                stop,
                mode,
                style,
            } => {
                let [x, y, w, h] = resolve_box(*x, *y, *w, *h, mode);
                let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                let (rx, ry) = ((w / 2.0).abs(), (h / 2.0).abs());
                let (sx, sy) = (cx + rx * start.cos(), cy + ry * start.sin());
                let (ex, ey) = (cx + rx * stop.cos(), cy + ry * stop.sin());
                let span = stop - start;
                let large = u8::from(span.abs() > std::f64::consts::PI);
                let sweep = u8::from(span >= 0.0);
                let _ = write!(
                    self.out,
                    r#"<path d="M {} {} A {} {} 0 {large} {sweep} {} {}" {}/>"#,
                    n(sx),
                    n(sy),
                    n(rx),
                    n(ry),
                    n(ex),
                    n(ey),
                    style_attrs(style)
                );
            }
            SceneNode::Triangle { pts, style } => self.polygon_pts(pts, style),
            SceneNode::Quad { pts, style } => self.polygon_pts(pts, style),
            SceneNode::Point { x, y, style } => {
                // Square in the stroke colour, falling back to fill. Like the
                // TS `stroke ?? fill`, an empty stroke string does NOT fall
                // back — it is falsy and draws nothing.
                let Some(colour) = style.stroke.as_ref().or(style.fill.as_ref()) else {
                    return;
                };
                if colour.css.is_empty() {
                    return;
                }
                let sz = js::max(1.0, style.stroke_weight);
                let _ = write!(
                    self.out,
                    r#"<rect x="{}" y="{}" width="{}" height="{}" {}/>"#,
                    n(x - sz / 2.0),
                    n(y - sz / 2.0),
                    n(sz),
                    n(sz),
                    paint("fill", &colour.css)
                );
            }
            SceneNode::Polygon {
                points,
                closed,
                style,
                tension,
            } => {
                if points.is_empty() {
                    return;
                }
                let tension = tension.unwrap_or(0.0);
                let extra_fill = if *closed { "" } else { r#" fill="none""# };
                if tension <= 0.0 || points.len() < 3 {
                    let pts: Vec<String> = points
                        .iter()
                        .map(|p| format!("{},{}", n(p.x), n(p.y)))
                        .collect();
                    let tag = if *closed { "polygon" } else { "polyline" };
                    let _ = write!(
                        self.out,
                        r#"<{tag} points="{}" {}{extra_fill}/>"#,
                        pts.join(" "),
                        style_attrs(style)
                    );
                    return;
                }
                let mut parts = vec![format!("M{},{}", n(points[0].x), n(points[0].y))];
                for seg in catmull_rom_beziers(points, *closed, tension) {
                    parts.push(format!(
                        "C{},{} {},{} {},{}",
                        n(seg.c1.x),
                        n(seg.c1.y),
                        n(seg.c2.x),
                        n(seg.c2.y),
                        n(seg.end.x),
                        n(seg.end.y)
                    ));
                }
                if *closed {
                    parts.push("Z".into());
                }
                let _ = write!(
                    self.out,
                    r#"<path d="{}" {}{extra_fill}/>"#,
                    parts.join(" "),
                    style_attrs(style)
                );
            }
            SceneNode::Text { text, x, y, style } => self.text(text, *x, *y, style),
            // Kinds added to the scene model later emit nothing until ported
            // (like the TS visitor's switch for kinds it doesn't handle).
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    /// Triangles and quads: `<polygon points="x,y …">`.
    fn polygon_pts(&mut self, pts: &[f64], style: &ResolvedStyle) {
        let pts: Vec<String> = pts
            .chunks_exact(2)
            .map(|p| format!("{},{}", n(p[0]), n(p[1])))
            .collect();
        let _ = write!(
            self.out,
            r#"<polygon points="{}" {}/>"#,
            pts.join(" "),
            style_attrs(style)
        );
    }

    fn text(&mut self, text: &str, x: f64, y: f64, st: &ResolvedStyle) {
        let anchor = match &*st.text_align {
            "center" => "middle",
            "right" => "end",
            _ => "start",
        };
        let baseline = match &*st.text_baseline {
            "top" => "hanging",
            "middle" => "middle",
            "bottom" => "text-after-edge",
            _ => "alphabetic",
        };
        // Explicit font-* attributes rather than the CSS `font` shorthand,
        // which many SVG tools ignore.
        let family = if st.font_family.is_empty() {
            "sans-serif"
        } else {
            &st.font_family
        };
        let mut font = vec![
            format!(r#"font-family="{}""#, esc(family)),
            format!(r#"font-size="{}""#, n(st.font_size)),
        ];
        if !st.font_weight.is_empty() && &*st.font_weight != "normal" {
            font.push(format!(r#"font-weight="{}""#, esc(&st.font_weight)));
        }
        if !st.font_style.is_empty() && &*st.font_style != "normal" {
            font.push(format!(r#"font-style="{}""#, esc(&st.font_style)));
        }
        // JS truthiness: 0 and NaN are both falsy.
        if st.letter_spacing != 0.0 && !st.letter_spacing.is_nan() {
            font.push(format!(r#"letter-spacing="{}""#, n(st.letter_spacing)));
        }
        let _ = write!(
            self.out,
            r#"<text x="{}" y="{}" text-anchor="{anchor}" dominant-baseline="{baseline}" {} {}>"#,
            n(x),
            n(y),
            font.join(" "),
            style_attrs(st)
        );
        if !text.contains('\n') {
            let _ = write!(self.out, "{}</text>", esc_text(text));
            return;
        }
        let dy = st.font_size * st.line_height;
        for (i, line) in text.split('\n').enumerate() {
            let y_off = if i == 0 { 0.0 } else { dy };
            let _ = write!(
                self.out,
                r#"<tspan x="{}" dy="{}">{}</tspan>"#,
                n(x),
                n(y_off),
                esc_text(line)
            );
        }
        self.out.push_str("</text>");
    }
}

// ─── Style helpers ───────────────────────────────────────────────────────────

/// `styleAttrs`: fill (or `fill="none"`) plus stroke attributes. Empty
/// colour strings count as unset, like JS truthiness.
fn style_attrs(st: &ResolvedStyle) -> String {
    let mut out = match st.fill.as_ref().filter(|p| !p.css.is_empty()) {
        Some(f) => paint("fill", &f.css),
        None => r#"fill="none""#.to_string(),
    };
    if let Some(stroke) = st.stroke.as_ref().filter(|p| !p.css.is_empty()) {
        out.push(' ');
        out.push_str(&stroke_attrs(&stroke.css, st));
    }
    out
}

/// `strokeAttrs`: paint, width, and non-default cap / join (unescaped, as
/// in TS).
fn stroke_attrs(stroke: &str, st: &ResolvedStyle) -> String {
    let mut out = format!(
        r#"{} stroke-width="{}""#,
        paint("stroke", stroke),
        n(st.stroke_weight)
    );
    if &*st.stroke_cap != "butt" {
        let _ = write!(out, r#" stroke-linecap="{}""#, st.stroke_cap);
    }
    if &*st.stroke_join != "miter" {
        let _ = write!(out, r#" stroke-linejoin="{}""#, st.stroke_join);
    }
    out
}

/// `paint`: a `fill` / `stroke` attribute, splitting `rgba()` into `rgb()`
/// plus an opacity attribute (the channels are copied as written).
fn paint(role: &str, css: &str) -> String {
    match split_rgba(css) {
        Some([r, g, b, a]) => format!(r#"{role}="rgb({r},{g},{b})" {role}-opacity="{a}""#),
        None => format!(r#"{role}="{}""#, esc(css)),
    }
}

/// Match `/^rgba\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*\)$/`
/// and return the four captures. Digits/dots, whitespace and the
/// punctuation are disjoint, so greedy scanning needs no backtracking.
fn split_rgba(css: &str) -> Option<[&str; 4]> {
    let mut rest = css.strip_prefix("rgba(")?;
    let mut caps = [""; 4];
    for (i, cap) in caps.iter_mut().enumerate() {
        rest = rest.trim_start_matches(is_js_space);
        let len = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        if len == 0 {
            return None;
        }
        *cap = &rest[..len];
        rest = rest[len..].trim_start_matches(is_js_space);
        let close = if i < 3 { ',' } else { ')' };
        rest = rest.strip_prefix(close)?;
    }
    rest.is_empty().then_some(caps)
}

/// JS RegExp `\s` (WhiteSpace + LineTerminator).
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `n`: a coordinate / length rounded to 3 decimals and printed like JS;
/// NaN / ±Infinity become `0`.
fn n(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    js::num_to_string(js::round(v * 1000.0) / 1000.0)
}

/// Escape for attribute values.
fn esc(s: &str) -> String {
    esc_text(s).replace('"', "&quot;")
}

/// Escape for text content (quotes stay).
fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_match_js() {
        assert_eq!(n(1.347e-14), "0");
        assert_eq!(n(12.3456789), "12.346");
        assert_eq!(n(-0.0004), "0");
        assert_eq!(n(-1.0005), "-1");
        assert_eq!(n(2.5e-3), "0.003");
        assert_eq!(n(f64::NAN), "0");
        assert_eq!(n(f64::NEG_INFINITY), "0");
        assert_eq!(n(1e21), "1e+21");
        assert_eq!(n(1e306), "Infinity");
    }

    #[test]
    fn rgba_split_follows_the_regex() {
        assert_eq!(
            split_rgba("rgba(244,71,61,0.500)"),
            Some(["244", "71", "61", "0.500"])
        );
        assert_eq!(
            split_rgba("rgba( 1 , 2.5,3,\u{a0}0.1 )"),
            Some(["1", "2.5", "3", "0.1"])
        );
        assert_eq!(split_rgba("rgba(1,2,3,0.5) "), None);
        assert_eq!(split_rgba("rgba(1,2,3)"), None);
        assert_eq!(split_rgba("rgba(-1,2,3,0.5)"), None);
        assert_eq!(split_rgba("rgba(NaN,2,3,0.5)"), None);
        assert_eq!(split_rgba("rgba(1,2,3,0.5\u{85})"), None);
        assert_eq!(split_rgba("RGBA(1,2,3,0.5)"), None);
        assert_eq!(
            split_rgba("rgba(...,.,1.2.3,0)"),
            Some(["...", ".", "1.2.3", "0"])
        );
    }

    #[test]
    fn escapes() {
        assert_eq!(esc(r#"a&b<c>"d'"#), "a&amp;b&lt;c&gt;&quot;d'");
        assert_eq!(esc_text(r#"<"&">"#), r#"&lt;"&amp;"&gt;"#);
    }
}
