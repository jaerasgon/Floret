//! Retained-mode scene description — a port of `src/runtime/scene.ts`.
//!
//! Draw nodes return `SceneNode`s (plain data); the GPU renderer
//! walks the tree. Every shape carries its own `ResolvedStyle`.
//!
//! Differences from TS, by design:
//! - A `Style` is data (a list of [`StyleOp`]s applied in order), not a JS
//!   closure, so it can be compared, hashed, sent across threads and saved.
//! - Colours are [`Paint`]s: the exact CSS string TS would have produced
//!   (for parity with the TS traces) plus parsed RGBA for the renderer.
//! - Mode / font fields stay strings, like TS: nodes can pass any string
//!   and the renderer interprets it (unknown rect modes behave as `corner`).

use crate::js;
use crate::value::{Color, Value, Vec2};
use indexmap::IndexMap;
use std::sync::{Arc, OnceLock};

// ─── Paint ───────────────────────────────────────────────────────────────────

/// A fill / stroke colour: the CSS string TS stores plus RGBA (0..1).
#[derive(Clone, Debug, PartialEq)]
pub struct Paint {
    pub css: Arc<str>,
    pub rgba: [f64; 4],
}

impl Paint {
    /// `colorToCss`: 8-bit channels, `rgb()` when opaque, else `rgba()` with
    /// alpha to 3 decimals. NaN channels format as "NaN", like JS.
    pub fn from_color(c: Color) -> Paint {
        // f64::clamp keeps NaN as NaN, like the TS clamp01.
        let clamp01 = |v: f64| v.clamp(0.0, 1.0);
        let ch = |v: f64| js::round(clamp01(v) * 255.0);
        let (r, g, b, a) = (ch(c.r), ch(c.g), ch(c.b), clamp01(c.a));
        let s = js::num_to_string;
        let css = if a >= 1.0 {
            format!("rgb({},{},{})", s(r), s(g), s(b))
        } else {
            format!("rgba({},{},{},{})", s(r), s(g), s(b), js::to_fixed(a, 3))
        };
        Paint {
            css: Arc::from(css),
            rgba: [r / 255.0, g / 255.0, b / 255.0, a],
        }
    }

    /// A literal CSS colour (`#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`).
    /// Unparseable strings keep their text and render as opaque black.
    pub fn css(s: &str) -> Paint {
        Paint {
            css: Arc::from(s),
            rgba: parse_css(s).unwrap_or([0.0, 0.0, 0.0, 1.0]),
        }
    }
}

/// Parse the CSS colour forms Floret produces.
pub fn parse_css(s: &str) -> Option<[f64; 4]> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix('#') {
        let n = |i: usize, w: usize| u8::from_str_radix(&hex[i..i + w], 16).ok().map(f64::from);
        return match hex.len() {
            3 => Some([
                n(0, 1)? * 17.0 / 255.0,
                n(1, 1)? * 17.0 / 255.0,
                n(2, 1)? * 17.0 / 255.0,
                1.0,
            ]),
            6 => Some([n(0, 2)? / 255.0, n(2, 2)? / 255.0, n(4, 2)? / 255.0, 1.0]),
            8 => Some([
                n(0, 2)? / 255.0,
                n(2, 2)? / 255.0,
                n(4, 2)? / 255.0,
                n(6, 2)? / 255.0,
            ]),
            _ => None,
        };
    }
    let inner = t
        .strip_prefix("rgba(")
        .or_else(|| t.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<f64> = inner
        .split(',')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .ok()?;
    match parts.as_slice() {
        [r, g, b] => Some([r / 255.0, g / 255.0, b / 255.0, 1.0]),
        [r, g, b, a] => Some([r / 255.0, g / 255.0, b / 255.0, *a]),
        _ => None,
    }
}

// ─── Resolved style ──────────────────────────────────────────────────────────

/// A fully resolved drawing style (`ResolvedStyle` in TS). `fill: None`
/// means "don't fill"; `stroke: None` means "don't stroke".
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedStyle {
    pub fill: Option<Paint>,
    pub stroke: Option<Paint>,
    pub stroke_weight: f64,
    /// "butt" | "round" | "square"
    pub stroke_cap: Arc<str>,
    /// "miter" | "bevel" | "round"
    pub stroke_join: Arc<str>,
    /// "center" | "inside" | "outside"
    pub stroke_align: Arc<str>,
    /// Gaussian blur radius in pixels.
    pub blur: f64,
    pub font_size: f64,
    pub font_family: Arc<str>,
    pub font_weight: Arc<str>,
    pub font_style: Arc<str>,
    pub letter_spacing: f64,
    pub line_height: f64,
    /// "left" | "center" | "right"
    pub text_align: Arc<str>,
    /// "alphabetic" | "top" | "middle" | "bottom"
    pub text_baseline: Arc<str>,
    /// "corner" | "center" | "corners" | "radius"
    pub rect_mode: Arc<str>,
    pub ellipse_mode: Arc<str>,
}

fn s(v: &str) -> Arc<str> {
    Arc::from(v)
}

/// What an unstyled shape uses: white fill, no stroke.
pub fn default_style() -> &'static Arc<ResolvedStyle> {
    static S: OnceLock<Arc<ResolvedStyle>> = OnceLock::new();
    S.get_or_init(|| {
        Arc::new(ResolvedStyle {
            fill: Some(Paint::css("#ffffff")),
            stroke: None,
            stroke_weight: 1.0,
            stroke_cap: s("butt"),
            stroke_join: s("miter"),
            stroke_align: s("center"),
            blur: 0.0,
            font_size: 14.0,
            font_family: s("sans-serif"),
            font_weight: s("normal"),
            font_style: s("normal"),
            letter_spacing: 0.0,
            line_height: 1.2,
            text_align: s("left"),
            text_baseline: s("alphabetic"),
            rect_mode: s("corner"),
            ellipse_mode: s("center"),
        })
    })
}

/// Lines / open polygons: thin black stroke, no fill.
pub fn line_default_style() -> &'static Arc<ResolvedStyle> {
    static S: OnceLock<Arc<ResolvedStyle>> = OnceLock::new();
    S.get_or_init(|| {
        Arc::new(ResolvedStyle {
            fill: None,
            stroke: Some(Paint::css("#000000")),
            ..(**default_style()).clone()
        })
    })
}

/// What a WIRED style chain resolves against: no paint, structural defaults.
pub fn neutral_style() -> &'static Arc<ResolvedStyle> {
    static S: OnceLock<Arc<ResolvedStyle>> = OnceLock::new();
    S.get_or_init(|| {
        Arc::new(ResolvedStyle {
            fill: None,
            stroke: None,
            ..(**default_style()).clone()
        })
    })
}

// ─── Styles as data ──────────────────────────────────────────────────────────

/// One style override. A style node's TS closure `(s) => ({...base(s), …})`
/// becomes "the base's ops, then mine".
#[derive(Clone, Debug, PartialEq)]
pub enum StyleOp {
    Fill(Option<Paint>),
    Stroke(Option<Paint>),
    StrokeWeight(f64),
    /// Set the weight only when > 0 (`style/stroke` keeps the inherited one).
    StrokeWeightIfPositive(f64),
    StrokeCap(Arc<str>),
    StrokeJoin(Arc<str>),
    StrokeAlign(Arc<str>),
    Blur(f64),
    FontSize(f64),
    FontFamily(Arc<str>),
    FontWeight(Arc<str>),
    FontStyle(Arc<str>),
    LetterSpacing(f64),
    LineHeight(f64),
    TextAlign(Arc<str>),
    TextBaseline(Arc<str>),
    RectMode(Arc<str>),
    EllipseMode(Arc<str>),
}

impl StyleOp {
    pub fn apply(&self, st: &mut ResolvedStyle) {
        match self {
            StyleOp::Fill(p) => st.fill = p.clone(),
            StyleOp::Stroke(p) => st.stroke = p.clone(),
            StyleOp::StrokeWeight(w) => st.stroke_weight = *w,
            StyleOp::StrokeWeightIfPositive(w) => {
                if *w > 0.0 {
                    st.stroke_weight = *w;
                }
            }
            StyleOp::StrokeCap(v) => st.stroke_cap = v.clone(),
            StyleOp::StrokeJoin(v) => st.stroke_join = v.clone(),
            StyleOp::StrokeAlign(v) => st.stroke_align = v.clone(),
            StyleOp::Blur(v) => st.blur = *v,
            StyleOp::FontSize(v) => st.font_size = *v,
            StyleOp::FontFamily(v) => st.font_family = v.clone(),
            StyleOp::FontWeight(v) => st.font_weight = v.clone(),
            StyleOp::FontStyle(v) => st.font_style = v.clone(),
            StyleOp::LetterSpacing(v) => st.letter_spacing = *v,
            StyleOp::LineHeight(v) => st.line_height = *v,
            StyleOp::TextAlign(v) => st.text_align = v.clone(),
            StyleOp::TextBaseline(v) => st.text_baseline = v.clone(),
            StyleOp::RectMode(v) => st.rect_mode = v.clone(),
            StyleOp::EllipseMode(v) => st.ellipse_mode = v.clone(),
        }
    }
}

/// A style chain: ops applied in order to a base style.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Style(pub Arc<[StyleOp]>);

impl Style {
    /// `chainStyle(base, mine)`: the base's ops, then `ops`.
    pub fn chain(base: Option<&Style>, ops: impl IntoIterator<Item = StyleOp>) -> Style {
        let mut all: Vec<StyleOp> = base.map(|b| b.0.to_vec()).unwrap_or_default();
        all.extend(ops);
        Style(Arc::from(all))
    }

    pub fn apply(&self, base: &ResolvedStyle) -> ResolvedStyle {
        let mut st = base.clone();
        for op in self.0.iter() {
            op.apply(&mut st);
        }
        st
    }
}

/// `resolveStyle`: a wired style resolves against NEUTRAL; with nothing
/// wired the shape's own `fallback` applies.
pub fn resolve_style(style: Option<&Style>, fallback: &Arc<ResolvedStyle>) -> Arc<ResolvedStyle> {
    match style {
        None => fallback.clone(),
        Some(st) => Arc::new(st.apply(neutral_style())),
    }
}

// ─── Scene nodes ─────────────────────────────────────────────────────────────

/// CSS / SVG `matrix(a,b,c,d,e,f)`: x' = a·x + c·y + e, y' = b·x + d·y + f.
pub type Affine = [f64; 6];
pub const IDENTITY: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectF {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SceneNode {
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        mode: Arc<str>,
        style: Arc<ResolvedStyle>,
    },
    Ellipse {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        mode: Arc<str>,
        style: Arc<ResolvedStyle>,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        style: Arc<ResolvedStyle>,
    },
    Arc {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        start: f64,
        stop: f64,
        mode: Arc<str>,
        style: Arc<ResolvedStyle>,
    },
    Triangle {
        pts: [f64; 6],
        style: Arc<ResolvedStyle>,
    },
    Quad {
        pts: [f64; 8],
        style: Arc<ResolvedStyle>,
    },
    Point {
        x: f64,
        y: f64,
        style: Arc<ResolvedStyle>,
    },
    /// `tension`: `None` = key absent in TS; `Some(t)` = present.
    Polygon {
        points: Arc<[Vec2]>,
        closed: bool,
        style: Arc<ResolvedStyle>,
        tension: Option<f64>,
    },
    Text {
        text: Arc<str>,
        x: f64,
        y: f64,
        style: Arc<ResolvedStyle>,
    },
    Group {
        children: Arc<[Arc<SceneNode>]>,
        transform: Option<Affine>,
        blend_mode: Option<Arc<str>>,
        clip: Option<RectF>,
    },
}

impl SceneNode {
    pub fn group(children: Vec<Arc<SceneNode>>) -> SceneNode {
        SceneNode::Group {
            children: Arc::from(children),
            transform: None,
            blend_mode: None,
            clip: None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            SceneNode::Rect { .. } => "rect",
            SceneNode::Ellipse { .. } => "ellipse",
            SceneNode::Line { .. } => "line",
            SceneNode::Arc { .. } => "arc",
            SceneNode::Triangle { .. } => "triangle",
            SceneNode::Quad { .. } => "quad",
            SceneNode::Point { .. } => "point",
            SceneNode::Polygon { .. } => "polygon",
            SceneNode::Text { .. } => "text",
            SceneNode::Group { .. } => "group",
        }
    }

    /// The TS object shape (same keys), for parity checks and debugging.
    pub fn to_value(&self) -> Value {
        let mut m: IndexMap<String, Value> = IndexMap::new();
        m.insert("kind".into(), Value::str(self.kind()));
        let n = |v: f64| Value::Num(v);
        let put = |m: &mut IndexMap<String, Value>, k: &str, v: Value| {
            m.insert(k.into(), v);
        };
        match self {
            SceneNode::Rect {
                x,
                y,
                w,
                h,
                mode,
                style,
            }
            | SceneNode::Ellipse {
                x,
                y,
                w,
                h,
                mode,
                style,
            } => {
                for (k, v) in [("x", x), ("y", y), ("w", w), ("h", h)] {
                    put(&mut m, k, n(*v));
                }
                put(&mut m, "mode", Value::Str(mode.clone()));
                put(&mut m, "style", style_value(style));
            }
            SceneNode::Line {
                x1,
                y1,
                x2,
                y2,
                style,
            } => {
                for (k, v) in [("x1", x1), ("y1", y1), ("x2", x2), ("y2", y2)] {
                    put(&mut m, k, n(*v));
                }
                put(&mut m, "style", style_value(style));
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
                for (k, v) in [
                    ("x", x),
                    ("y", y),
                    ("w", w),
                    ("h", h),
                    ("start", start),
                    ("stop", stop),
                ] {
                    put(&mut m, k, n(*v));
                }
                put(&mut m, "mode", Value::Str(mode.clone()));
                put(&mut m, "style", style_value(style));
            }
            SceneNode::Triangle { pts, style } => {
                for (i, k) in ["x1", "y1", "x2", "y2", "x3", "y3"].iter().enumerate() {
                    put(&mut m, k, n(pts[i]));
                }
                put(&mut m, "style", style_value(style));
            }
            SceneNode::Quad { pts, style } => {
                for (i, k) in ["x1", "y1", "x2", "y2", "x3", "y3", "x4", "y4"]
                    .iter()
                    .enumerate()
                {
                    put(&mut m, k, n(pts[i]));
                }
                put(&mut m, "style", style_value(style));
            }
            SceneNode::Point { x, y, style } => {
                put(&mut m, "x", n(*x));
                put(&mut m, "y", n(*y));
                put(&mut m, "style", style_value(style));
            }
            SceneNode::Polygon {
                points,
                closed,
                style,
                tension,
            } => {
                put(
                    &mut m,
                    "points",
                    Value::list(points.iter().map(|p| Value::Vec2(*p)).collect()),
                );
                put(&mut m, "closed", Value::Bool(*closed));
                put(&mut m, "style", style_value(style));
                if let Some(t) = tension {
                    put(&mut m, "tension", n(*t));
                }
            }
            SceneNode::Text { text, x, y, style } => {
                put(&mut m, "text", Value::Str(text.clone()));
                put(&mut m, "x", n(*x));
                put(&mut m, "y", n(*y));
                put(&mut m, "style", style_value(style));
            }
            SceneNode::Group {
                children,
                transform,
                blend_mode,
                clip,
            } => {
                put(
                    &mut m,
                    "children",
                    Value::list(children.iter().map(|c| Value::Scene(c.clone())).collect()),
                );
                if let Some(t) = transform {
                    put(
                        &mut m,
                        "transform",
                        Value::list(t.iter().map(|v| n(*v)).collect()),
                    );
                }
                if let Some(b) = blend_mode {
                    put(&mut m, "blendMode", Value::Str(b.clone()));
                }
                if let Some(c) = clip {
                    let mut r = IndexMap::new();
                    for (k, v) in [("x", c.x), ("y", c.y), ("w", c.w), ("h", c.h)] {
                        r.insert(k.to_string(), n(v));
                    }
                    put(&mut m, "clip", Value::Object(Arc::new(r)));
                }
            }
        }
        Value::Object(Arc::new(m))
    }
}

/// `ResolvedStyle` as the TS object (camelCase keys, CSS colour strings).
pub fn style_value(st: &ResolvedStyle) -> Value {
    let paint = |p: &Option<Paint>| {
        p.as_ref()
            .map_or(Value::Null, |p| Value::Str(p.css.clone()))
    };
    let mut m: IndexMap<String, Value> = IndexMap::new();
    m.insert("fill".into(), paint(&st.fill));
    m.insert("stroke".into(), paint(&st.stroke));
    m.insert("strokeWeight".into(), Value::Num(st.stroke_weight));
    m.insert("strokeCap".into(), Value::Str(st.stroke_cap.clone()));
    m.insert("strokeJoin".into(), Value::Str(st.stroke_join.clone()));
    m.insert("strokeAlign".into(), Value::Str(st.stroke_align.clone()));
    m.insert("blur".into(), Value::Num(st.blur));
    m.insert("fontSize".into(), Value::Num(st.font_size));
    m.insert("fontFamily".into(), Value::Str(st.font_family.clone()));
    m.insert("fontWeight".into(), Value::Str(st.font_weight.clone()));
    m.insert("fontStyle".into(), Value::Str(st.font_style.clone()));
    m.insert("letterSpacing".into(), Value::Num(st.letter_spacing));
    m.insert("lineHeight".into(), Value::Num(st.line_height));
    m.insert("textAlign".into(), Value::Str(st.text_align.clone()));
    m.insert("textBaseline".into(), Value::Str(st.text_baseline.clone()));
    m.insert("rectMode".into(), Value::Str(st.rect_mode.clone()));
    m.insert("ellipseMode".into(), Value::Str(st.ellipse_mode.clone()));
    Value::Object(Arc::new(m))
}

// ─── Geometry helpers ────────────────────────────────────────────────────────

pub fn translate_mat(tx: f64, ty: f64) -> Affine {
    [1.0, 0.0, 0.0, 1.0, tx, ty]
}

pub fn scale_mat(sx: f64, sy: f64) -> Affine {
    [sx, 0.0, 0.0, sy, 0.0, 0.0]
}

pub fn rotate_mat(rad: f64) -> Affine {
    let (sn, c) = rad.sin_cos();
    [c, sn, -sn, c, 0.0, 0.0]
}

/// `a ∘ b`: apply `b` first, then `a`.
pub fn mul(a: &Affine, b: &Affine) -> Affine {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// `resolveBox`: mode-aware (x, y, w, h) → top-left + size.
pub fn resolve_box(x: f64, y: f64, w: f64, h: f64, mode: &str) -> [f64; 4] {
    match mode {
        "center" => [x - w / 2.0, y - h / 2.0, w, h],
        "corners" => [js::min(x, w), js::min(y, h), (w - x).abs(), (h - y).abs()],
        "radius" => [x - w, y - h, w * 2.0, h * 2.0],
        _ => [x, y, w, h],
    }
}

/// One cubic Bézier span: control points then end point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bezier {
    pub c1: Vec2,
    pub c2: Vec2,
    pub end: Vec2,
}

/// `catmullRomBeziers`: uniform Catmull–Rom through `points` as cubic Béziers.
pub fn catmull_rom_beziers(points: &[Vec2], closed: bool, tension: f64) -> Vec<Bezier> {
    let n = points.len();
    if n == 0 {
        return Vec::new();
    }
    let k = tension / 6.0;
    let last = if closed { n } else { n - 1 };
    (0..last)
        .map(|i| {
            let at = |j: usize| points[j];
            let p0 = at(if closed {
                (i + n - 1) % n
            } else {
                i.saturating_sub(1)
            });
            let p1 = at(i);
            let p2 = at(if closed { (i + 1) % n } else { i + 1 });
            let p3 = at(if closed {
                (i + 2) % n
            } else {
                (i + 2).min(n - 1)
            });
            Bezier {
                c1: Vec2 {
                    x: p1.x + (p2.x - p0.x) * k,
                    y: p1.y + (p2.y - p0.y) * k,
                },
                c2: Vec2 {
                    x: p2.x - (p3.x - p1.x) * k,
                    y: p2.y - (p3.y - p1.y) * k,
                },
                end: p2,
            }
        })
        .collect()
}

/// `composeFontCss`: the CSS `font` shorthand.
pub fn compose_font_css(st: &ResolvedStyle) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !st.font_style.is_empty() && &*st.font_style != "normal" {
        parts.push(st.font_style.to_string());
    }
    if !st.font_weight.is_empty() && &*st.font_weight != "normal" {
        parts.push(st.font_weight.to_string());
    }
    parts.push(format!("{}px", js::num_to_string(st.font_size)));
    parts.push(if st.font_family.is_empty() {
        "sans-serif".into()
    } else {
        st.font_family.to_string()
    });
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_to_css_matches_ts() {
        let c = |r, g, b, a| Paint::from_color(Color { r, g, b, a }).css.to_string();
        assert_eq!(c(1.0, 0.0, 0.0, 1.0), "rgb(255,0,0)");
        assert_eq!(c(0.5, 0.5, 0.5, 0.5), "rgba(128,128,128,0.500)");
        assert_eq!(c(2.0, -1.0, 0.0, 1.5), "rgb(255,0,0)");
        assert_eq!(c(f64::NAN, 0.0, 0.0, 1.0), "rgb(NaN,0,0)");
    }

    #[test]
    fn chains_apply_in_order() {
        let red = Paint::from_color(Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        });
        let fill = Style::chain(None, [StyleOp::Fill(Some(red.clone()))]);
        let no_fill = Style::chain(Some(&fill), [StyleOp::Fill(None)]);
        assert_eq!(resolve_style(Some(&fill), default_style()).fill, Some(red));
        assert_eq!(resolve_style(Some(&no_fill), default_style()).fill, None);
        // Nothing wired → the shape's own default.
        assert_eq!(
            resolve_style(None, default_style())
                .fill
                .as_ref()
                .unwrap()
                .css
                .as_ref(),
            "#ffffff"
        );
        // A wired chain resolves against NEUTRAL (no stroke).
        assert_eq!(
            resolve_style(Some(&fill), line_default_style()).stroke,
            None
        );
    }

    #[test]
    fn parses_css() {
        assert_eq!(parse_css("#ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_css("rgba(0,0,0,0.500)"), Some([0.0, 0.0, 0.0, 0.5]));
        assert_eq!(parse_css("#fff"), Some([1.0, 1.0, 1.0, 1.0]));
    }

    #[test]
    fn boxes_and_fonts() {
        assert_eq!(
            resolve_box(10.0, 10.0, 4.0, 6.0, "center"),
            [8.0, 7.0, 4.0, 6.0]
        );
        assert_eq!(
            resolve_box(5.0, 5.0, 1.0, 2.0, "corners"),
            [1.0, 2.0, 4.0, 3.0]
        );
        let mut st = (**default_style()).clone();
        st.font_weight = Arc::from("bold");
        assert_eq!(compose_font_css(&st), "bold 14px sans-serif");
    }
}
