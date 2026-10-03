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
/// A gradient paint (`style/gradient`) also carries its [`Gradient`]; `rgba`
/// is then its first stop, for consumers that only draw flat colours.
#[derive(Clone, Debug, PartialEq)]
pub struct Paint {
    pub css: Arc<str>,
    pub rgba: [f64; 4],
    pub gradient: Option<Arc<Gradient>>,
}

impl Paint {
    /// `colorToCss`: 8-bit channels, `rgb()` when opaque, else `rgba()` with
    /// alpha to 3 decimals. NaN channels format as "NaN", like JS.
    pub fn from_color(c: Color) -> Paint {
        // f64::clamp keeps NaN as NaN, like the TS clamp01.
        let clamp01 = |v: f64| v.clamp(0.0, 1.0);
        let ch = |v: f64| js::round(clamp01(v) * 255.0);
        let (r, g, b, a) = (ch(c.r), ch(c.g), ch(c.b), clamp01(c.a));
        // A channel is NaN or an integer in 0..=255.
        let push = |css: &mut String, v: f64| {
            if v.is_nan() {
                css.push_str("NaN");
                return;
            }
            let n = v as u8;
            if n >= 100 {
                css.push(char::from(b'0' + n / 100));
            }
            if n >= 10 {
                css.push(char::from(b'0' + n / 10 % 10));
            }
            css.push(char::from(b'0' + n % 10));
        };
        let mut css = String::with_capacity(24);
        css.push_str(if a >= 1.0 { "rgb(" } else { "rgba(" });
        push(&mut css, r);
        css.push(',');
        push(&mut css, g);
        css.push(',');
        push(&mut css, b);
        if a < 1.0 || a.is_nan() {
            css.push(',');
            css.push_str(&js::to_fixed(a, 3));
        }
        css.push(')');
        Paint {
            css: Arc::from(css),
            rgba: [r / 255.0, g / 255.0, b / 255.0, a],
            gradient: None,
        }
    }

    /// A literal CSS colour (`#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`).
    /// Unparseable strings keep their text and render as opaque black.
    pub fn css(s: &str) -> Paint {
        Paint {
            css: Arc::from(s),
            rgba: parse_css(s).unwrap_or([0.0, 0.0, 0.0, 1.0]),
            gradient: None,
        }
    }

    /// A gradient paint. Its CSS form names the kind and the stops, e.g.
    /// `linear-gradient(rgb(255,0,0) 0, rgb(0,0,255) 1)`.
    pub fn gradient(g: Gradient) -> Paint {
        let stops: Vec<String> = g
            .stops
            .iter()
            .map(|s| {
                let [r, gr, b, a] = s.rgba;
                let css = Paint::from_color(Color { r, g: gr, b, a }).css;
                format!("{css} {}", js::num_to_string(s.pos))
            })
            .collect();
        let kind = match g.shape {
            GradientShape::Linear { .. } => "linear",
            GradientShape::Radial { .. } => "radial",
            GradientShape::Conic { .. } => "conic",
        };
        Paint {
            css: Arc::from(format!("{kind}-gradient({})", stops.join(", "))),
            rgba: g.stops.first().map_or([0.0; 4], |s| s.rgba),
            gradient: Some(Arc::new(g)),
        }
    }
}

// ─── Gradients ───────────────────────────────────────────────────────────────

/// A gradient paint (`style/gradient`), drawn per pixel by the renderer.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub shape: GradientShape,
    /// At least one stop, positions in 0..1 ascending; straight RGBA 0..1.
    pub stops: Arc<[GradientStop]>,
    pub units: GradientUnits,
    pub extend: GradientExtend,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub pos: f64,
    pub rgba: [f64; 4],
}

/// Where the gradient lies, in [`GradientUnits`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientShape {
    /// 0 at `start`, 1 at `end`, constant across.
    Linear { start: Vec2, end: Vec2 },
    /// 0 at `center`, 1 at `radius`.
    Radial { center: Vec2, radius: f64 },
    /// A sweep around `center`, clockwise on screen from `angle` (radians,
    /// 0 = pointing right, like `transform/rotate`).
    Conic { center: Vec2, angle: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientUnits {
    /// Relative to each shape's bounds: −1..1 across it, centre origin
    /// (y down). Points outside the box are fine.
    Box,
    /// Scene units, like shape coordinates (group transforms apply).
    Scene,
}

/// What the gradient does past its ends (t outside 0..1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientExtend {
    /// Hold the end colours.
    Pad,
    Repeat,
    /// Repeat, every other copy reversed.
    Mirror,
}

impl Gradient {
    /// The colour at `t` (straight RGBA), interpolated in premultiplied
    /// space like CSS, after applying `extend`. The renderer does the same
    /// per pixel; this is the reference.
    pub fn sample(&self, t: f64) -> [f64; 4] {
        let t = match self.extend {
            GradientExtend::Pad => t.clamp(0.0, 1.0),
            GradientExtend::Repeat => t - t.floor(),
            GradientExtend::Mirror => 1.0 - ((t / 2.0 - (t / 2.0).floor()) * 2.0 - 1.0).abs(),
        };
        let stops = &self.stops;
        let Some(first) = stops.first() else {
            return [0.0; 4];
        };
        if t <= first.pos {
            return first.rgba;
        }
        for w in stops.windows(2) {
            let (a, b) = (w[0], w[1]);
            if t <= b.pos {
                let f = if b.pos > a.pos {
                    (t - a.pos) / (b.pos - a.pos)
                } else {
                    1.0
                };
                let pa = a.rgba.map(|c| c * a.rgba[3]);
                let pb = b.rgba.map(|c| c * b.rgba[3]);
                let alpha = a.rgba[3] + (b.rgba[3] - a.rgba[3]) * f;
                let ch = |i: usize| {
                    let p = pa[i] + (pb[i] - pa[i]) * f;
                    if alpha > 0.0 {
                        p / alpha
                    } else {
                        0.0
                    }
                };
                return [ch(0), ch(1), ch(2), alpha];
            }
        }
        stops[stops.len() - 1].rgba
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
    /// Variable-font axis settings (`font-variation-settings`), in order
    /// set; empty for the font's defaults. See `crate::text`.
    pub font_variations: Arc<[([u8; 4], f32)]>,
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
            font_variations: Arc::from([]),
        })
    })
}

/// What an unstyled line, point, path or open polygon uses: a thin white
/// stroke, no fill.
///
/// Known divergence from the web app, which stroked them black (invisible
/// on the default black canvas). Code replaying TS reference data turns
/// the old default back on with [`ts_reference_line_default`].
pub fn line_default_style() -> &'static Arc<ResolvedStyle> {
    static WHITE: OnceLock<Arc<ResolvedStyle>> = OnceLock::new();
    static TS_BLACK: OnceLock<Arc<ResolvedStyle>> = OnceLock::new();
    let (cell, css) = if TS_LINE_DEFAULT.with(std::cell::Cell::get) {
        (&TS_BLACK, TS_LINE_STROKE)
    } else {
        (&WHITE, LINE_STROKE)
    };
    cell.get_or_init(|| {
        Arc::new(ResolvedStyle {
            fill: None,
            stroke: Some(Paint::css(css)),
            ..(**default_style()).clone()
        })
    })
}

/// [`line_default_style`]'s stroke.
pub const LINE_STROKE: &str = "#ffffff";
/// The web app's stroke for unstyled lines, which its recorded traces and
/// `ts_cases.json` files hold.
pub const TS_LINE_STROKE: &str = "#000000";

thread_local! {
    static TS_LINE_DEFAULT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// **For comparing against TypeScript reference data only** (the parity
/// traces, the `ts_cases.json` node cases). While the returned guard lives,
/// [`line_default_style`] on this thread strokes black ([`TS_LINE_STROKE`])
/// as the web app did. The engine evaluates on the calling thread, so
/// this covers a whole `Engine::tick`. Never use it in the app.
#[must_use = "the TS default is on only while the guard lives"]
pub fn ts_reference_line_default() -> TsLineDefault {
    let was = TS_LINE_DEFAULT.with(|c| c.replace(true));
    TsLineDefault { was }
}

/// Guard from [`ts_reference_line_default`]; dropping it restores the
/// previous default.
pub struct TsLineDefault {
    was: bool,
}

impl Drop for TsLineDefault {
    fn drop(&mut self) {
        TS_LINE_DEFAULT.with(|c| c.set(self.was));
    }
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
    /// Set one variation axis (replacing an earlier value for the tag).
    FontVariation([u8; 4], f32),
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
            StyleOp::FontVariation(tag, v) => {
                let mut all: Vec<([u8; 4], f32)> = st
                    .font_variations
                    .iter()
                    .filter(|(t, _)| t != tag)
                    .copied()
                    .collect();
                all.push((*tag, *v));
                st.font_variations = Arc::from(all);
            }
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
    /// `particles/render-as`: draw every live particle of a buffer with one
    /// template (CPU instancing). Layout for renderers:
    ///
    /// - Read-lock `buffer` (`buffer.read()`) and walk `0..capacity`; draw
    ///   slot `i` only when `alive[i] != 0` (dead slots hold stale data).
    /// - Position `(pos_x[i], pos_y[i])`, size `size[i]`: the web app's GL
    ///   renderer (and floret-render) draw it as the half extent — circle
    ///   radius / half the square's side, at least 0.4; TS's SVG export used
    ///   it as the diameter. Colour `(color_r, color_g, color_b, color_a)[i]` in
    ///   0..1 when `use_buffer_color`, else `style.fill`.
    /// - `template`: [`ParticleTemplate::Circle`] / `Rect` centred on the
    ///   particle, or `Layer(node)` translated to it.
    /// - `count` is the alive count when the batch was BUILT (TS memoises
    ///   render-as, so it can be stale); the buffer itself is live — use
    ///   `alive`, not `count`, to decide what to draw.
    ///
    /// [`ParticleTemplate::Circle`]: crate::particles::ParticleTemplate::Circle
    ParticleBatch {
        buffer: crate::particles::ParticleHandle,
        count: f64,
        template: crate::particles::ParticleTemplate,
        style: Arc<ResolvedStyle>,
        use_buffer_color: bool,
    },
    /// `particles/trail`: a polyline behind every live particle.
    ///
    /// - `history` is a ring buffer of past positions, `segments` per slot:
    ///   `history[(slot * segments + seg) * 2 + {0, 1}]` = (x, y). NaN marks
    ///   "no sample" (dead slot / not yet written): stop the line there.
    /// - `head_idx` is the newest sample; walk `seg = 0..segments` with
    ///   `idx = (head_idx + segments - seg) % segments` (newest → oldest).
    /// - Draw only slots with `buffer.alive[slot] != 0`. Stroke colour is
    ///   the particle's buffer colour (rgb + alpha) when `use_buffer_color`,
    ///   else `style.stroke`; width `style.stroke_weight`.
    /// - `fade_out`: per segment k (1-based, between samples k-1 and k),
    ///   alpha × (1 − (k − 0.5)/segments). `tapered`: width ×
    ///   (1 − (k − 0.5)/segments × (1 − tail_width)), min 0.001.
    ParticleTrailBatch {
        buffer: crate::particles::ParticleHandle,
        history: Arc<Vec<f32>>,
        segments: usize,
        head_idx: usize,
        style: Arc<ResolvedStyle>,
        fade_out: bool,
        use_buffer_color: bool,
        tapered: bool,
        tail_width: f64,
    },
    /// `draw/image*`, `transform/cache`: an image drawn as a textured quad.
    /// `src` is a source rect in image pixels (`None` = the whole image),
    /// `dst` the destination rect in scene units (corner mode). `tint`
    /// multiplies the texture (RGB × tint, then × tint alpha).
    Image {
        image: Arc<crate::image::Image>,
        src: Option<RectF>,
        dst: RectF,
        tint: Option<Paint>,
    },
    /// `gpu/shader`: a GLSL fragment shader (plus optional buffer passes)
    /// drawn over the whole canvas. See [`crate::shader`].
    Shader(Arc<crate::shader::ShaderShape>),
    /// `text/*`: pre-shaped glyphs, each placed and turned on its own.
    Glyphs(Arc<GlyphRun>),
    /// `transform/mask`: `content` shown through `mask`. The mask's alpha
    /// (`luminance: false`) or its luminance × alpha is the coverage;
    /// `invert` uses 1 − coverage.
    Mask {
        content: Arc<SceneNode>,
        mask: Arc<SceneNode>,
        luminance: bool,
        invert: bool,
    },
}

/// Glyphs shaped by `crate::text`, drawn with `style` (fill / stroke,
/// `font_variations` plus the CSS weight as axis coordinates).
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    pub glyphs: Arc<[PlacedGlyph]>,
    /// Font size in scene units.
    pub size: f64,
    pub style: Arc<ResolvedStyle>,
}

/// One glyph: its origin (pen position on the baseline) and rotation in
/// radians about that origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub face: crate::text::FaceId,
    pub id: u16,
    pub x: f64,
    pub y: f64,
    pub angle: f64,
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
            SceneNode::ParticleBatch { .. } => "particle-batch",
            SceneNode::ParticleTrailBatch { .. } => "particle-trail-batch",
            SceneNode::Image { .. } => "image",
            SceneNode::Shader(_) => "shader",
            SceneNode::Glyphs(_) => "glyphs",
            SceneNode::Mask { .. } => "mask",
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
            SceneNode::ParticleBatch {
                buffer,
                count,
                template,
                style,
                use_buffer_color,
            } => {
                use crate::particles::Handle;
                put(
                    &mut m,
                    "buffer",
                    Value::Handle(Handle::Particles(buffer.clone())),
                );
                put(&mut m, "count", n(*count));
                put(&mut m, "template", template.to_value());
                put(&mut m, "style", style_value(style));
                put(&mut m, "useBufferColor", Value::Bool(*use_buffer_color));
            }
            SceneNode::ParticleTrailBatch {
                buffer,
                history,
                segments,
                head_idx,
                style,
                fade_out,
                use_buffer_color,
                tapered,
                tail_width,
            } => {
                use crate::particles::{Handle, TypedArray};
                put(
                    &mut m,
                    "buffer",
                    Value::Handle(Handle::Particles(buffer.clone())),
                );
                put(
                    &mut m,
                    "history",
                    Value::Handle(Handle::TypedArray(TypedArray::F32(history.clone()))),
                );
                put(&mut m, "segments", n(*segments as f64));
                put(&mut m, "headIdx", n(*head_idx as f64));
                put(&mut m, "style", style_value(style));
                put(&mut m, "fadeOut", Value::Bool(*fade_out));
                put(&mut m, "useBufferColor", Value::Bool(*use_buffer_color));
                put(&mut m, "tapered", Value::Bool(*tapered));
                put(&mut m, "tailWidth", n(*tail_width));
            }
            SceneNode::Image {
                image,
                src,
                dst,
                tint,
            } => {
                put(&mut m, "source", Value::Image(image.clone()));
                put(&mut m, "src", src.map_or(Value::Null, rect_value));
                put(&mut m, "dst", rect_value(*dst));
                put(
                    &mut m,
                    "tint",
                    tint.as_ref()
                        .map_or(Value::Null, |p| Value::Str(p.css.clone())),
                );
            }
            SceneNode::Shader(sh) => sh.fields(&mut m),
            SceneNode::Glyphs(run) => {
                let glyph = |g: &PlacedGlyph| {
                    let mut o = IndexMap::new();
                    o.insert("glyph".to_owned(), n(f64::from(g.id)));
                    o.insert("x".to_owned(), n(g.x));
                    o.insert("y".to_owned(), n(g.y));
                    o.insert("angle".to_owned(), n(g.angle));
                    Value::Object(Arc::new(o))
                };
                put(
                    &mut m,
                    "glyphs",
                    Value::list(run.glyphs.iter().map(glyph).collect()),
                );
                put(&mut m, "size", n(run.size));
                put(&mut m, "style", style_value(&run.style));
            }
            SceneNode::Mask {
                content,
                mask,
                luminance,
                invert,
            } => {
                put(&mut m, "content", Value::Scene(content.clone()));
                put(&mut m, "mask", Value::Scene(mask.clone()));
                let mode = if *luminance { "luminance" } else { "alpha" };
                put(&mut m, "mode", Value::str(mode));
                put(&mut m, "invert", Value::Bool(*invert));
            }
        }
        Value::Object(Arc::new(m))
    }
}

/// `{x, y, w, h}`.
fn rect_value(r: RectF) -> Value {
    let mut m = IndexMap::new();
    for (k, v) in [("x", r.x), ("y", r.y), ("w", r.w), ("h", r.h)] {
        m.insert(k.to_string(), Value::Num(v));
    }
    Value::Object(Arc::new(m))
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
    // Native-only, so absent unless set (keeps the TS traces comparable).
    if !st.font_variations.is_empty() {
        m.insert(
            "fontVariations".into(),
            Value::Str(Arc::from(font_variations_css(&st.font_variations))),
        );
    }
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

/// `font-variation-settings` syntax: `"wght" 650, "wdth" 80`.
pub fn font_variations_css(v: &[([u8; 4], f32)]) -> String {
    v.iter()
        .map(|(t, x)| {
            format!(
                "\"{}\" {}",
                String::from_utf8_lossy(t),
                js::num_to_string(f64::from(*x))
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
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
    fn unstyled_lines_stroke_white_and_ts_replays_black() {
        let stroke = || {
            line_default_style()
                .stroke
                .as_ref()
                .map(|p| p.css.to_string())
        };
        assert_eq!(stroke().as_deref(), Some("#ffffff"));
        assert_eq!(line_default_style().fill, None);
        {
            let _ts = ts_reference_line_default();
            assert_eq!(stroke().as_deref(), Some("#000000"));
            // Nested guards restore what was there.
            drop(ts_reference_line_default());
            assert_eq!(stroke().as_deref(), Some("#000000"));
            // Only this thread sees it.
            let other = std::thread::spawn(move || {
                line_default_style()
                    .stroke
                    .as_ref()
                    .map(|p| p.css.to_string())
            });
            assert_eq!(other.join().unwrap().as_deref(), Some("#ffffff"));
        }
        assert_eq!(stroke().as_deref(), Some("#ffffff"));
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

    fn stops(v: &[(f64, [f64; 4])]) -> Arc<[GradientStop]> {
        v.iter()
            .map(|&(pos, rgba)| GradientStop { pos, rgba })
            .collect()
    }

    #[test]
    fn gradients_interpolate_premultiplied_and_extend() {
        let mut g = Gradient {
            shape: GradientShape::Linear {
                start: Vec2 { x: -1.0, y: 0.0 },
                end: Vec2 { x: 1.0, y: 0.0 },
            },
            stops: stops(&[
                (0.0, [1.0, 0.0, 0.0, 1.0]),
                (0.5, [0.0, 0.0, 1.0, 1.0]),
                (1.0, [0.0, 0.0, 1.0, 0.0]),
            ]),
            units: GradientUnits::Box,
            extend: GradientExtend::Pad,
        };
        assert_eq!(g.sample(-1.0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(g.sample(0.25), [0.5, 0.0, 0.5, 1.0]);
        // Fading to transparent keeps the hue (premultiplied lerp).
        assert_eq!(g.sample(0.75), [0.0, 0.0, 1.0, 0.5]);
        assert_eq!(g.sample(2.0)[3], 0.0);
        g.extend = GradientExtend::Repeat;
        assert_eq!(g.sample(1.25), g.sample(0.25));
        g.extend = GradientExtend::Mirror;
        assert_eq!(g.sample(1.75), g.sample(0.25));
        let p = Paint::gradient(g);
        assert_eq!(
            &*p.css,
            "linear-gradient(rgb(255,0,0) 0, rgb(0,0,255) 0.5, rgba(0,0,255,0.000) 1)"
        );
        assert_eq!(p.rgba, [1.0, 0.0, 0.0, 1.0]);
        assert!(p.gradient.is_some());
    }

    #[test]
    fn masks_describe_themselves() {
        let dot = Arc::new(SceneNode::Point {
            x: 0.0,
            y: 0.0,
            style: default_style().clone(),
        });
        let m = SceneNode::Mask {
            content: dot.clone(),
            mask: dot,
            luminance: true,
            invert: false,
        };
        assert_eq!(m.kind(), "mask");
        assert!(matches!(m.to_value().get("mode"), Value::Str(s) if &*s == "luminance"));
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
