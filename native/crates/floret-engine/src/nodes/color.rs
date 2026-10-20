//! `color/*` nodes. Ported from `src/nodes/color/color.ts`,
//! `src/nodes/color/palette.ts` and `src/nodes/_shared/color-conv.ts`.
//!
//! Channel conventions on node boundaries: r, g, b, a in 0..255, h in
//! 0..360, s / l in 0..100. Internally `Color` stores 0..1 floats.

use super::shared::{first, to_color, WHITE};
use super::Registrar;
use crate::js;
use crate::node::{per_element, raw, scalar, NodeImpl, NodeInit};
use crate::spec::{Params, PortSpec, PortType};
use crate::value::{one, Color, Value};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("color/color", |i| per_element(i, color(i)));
    r.dynamic("color/color", color_inputs);
    r.node("color/split", |i| per_element(i, split(i)));
    r.node("color/set-alpha", set_alpha);
    r.node("color/lerp", |i| per_element(i, lerp(i)));
    r.node("color/gradient", |i| per_element(i, gradient(i)));
    r.node("color/rgb", rgb);
    r.node("color/hsl", hsl);
    r.node("color/hex", hex);
    r.node("color/split-rgb", split_rgb);
    r.node("color/split-hsl", split_hsl);
    r.node("color/palette", palette);
    r.dynamic("color/palette", build_stop_ports);
    // Per element over spreads (v4 patches on); Gradient per `t`, its
    // `colors` stay the one list.
    r.lifted(&[
        "color/color",
        "color/split",
        "color/set-alpha",
        "color/lerp",
        "color/gradient",
        "color/rgb",
        "color/hsl",
        "color/hex",
        "color/split-rgb",
        "color/split-hsl",
    ]);
}

// ─── color-conv.ts ───────────────────────────────────────────────────────────

/// `clamp01`: NaN passes through (both JS comparisons are false), as it
/// does with `f64::clamp`.
pub(crate) fn clamp01(n: f64) -> f64 {
    n.clamp(0.0, 1.0)
}

/// JS `String(v)` / ToString for the runtime's value shapes.
fn js_to_string(v: &Value) -> String {
    match v {
        Value::Undefined => "undefined".into(),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => js::num_to_string(*n),
        Value::Str(s) => s.to_string(),
        Value::Vec2(_) | Value::Color(_) | Value::Object(_) => "[object Object]".into(),
        Value::Scene(_) => "[object Object]".into(),
        // JS prints a function's source; there's no equivalent for a style chain.
        Value::Style(_) => "function".into(),
        Value::Handle(h) => h.js_string(),
        Value::Image(_) => "[object OffscreenCanvas]".into(),
        // Array.prototype.join: nullish elements become "".
        Value::List(items) => items
            .iter()
            .map(|x| match x {
                Value::Undefined | Value::Null => String::new(),
                other => js_to_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// `/^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i` → three bytes.
fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let body = s.strip_prefix('#').unwrap_or(s);
    let b = body.as_bytes();
    if b.len() != 6 || !b.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&body[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// `hexToColor01(hex)`. `RegExp.exec` stringifies its argument, so non-
/// string values are matched against their JS string form.
pub(crate) fn hex_to_color01(v: &Value) -> Color {
    let s = match v {
        Value::Str(s) => s.to_string(),
        other => js_to_string(other),
    };
    match parse_hex(&s) {
        Some([r, g, b]) => Color {
            r: f64::from(r) / 255.0,
            g: f64::from(g) / 255.0,
            b: f64::from(b) / 255.0,
            a: 1.0,
        },
        None => WHITE,
    }
}

/// `hslToColor01(h, s, l, a)`.
pub(crate) fn hsl_to_color01(h: f64, s: f64, l: f64, a: f64) -> Color {
    let s_n = s / 100.0;
    let l_n = l / 100.0;
    let c = (1.0 - (2.0 * l_n - 1.0).abs()) * s_n;
    let hh = (((h % 360.0) + 360.0) % 360.0) / 60.0;
    let x = c * (1.0 - ((hh % 2.0) - 1.0).abs());
    let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
    if hh < 1.0 {
        r = c;
        g = x;
    } else if hh < 2.0 {
        r = x;
        g = c;
    } else if hh < 3.0 {
        g = c;
        b = x;
    } else if hh < 4.0 {
        g = x;
        b = c;
    } else if hh < 5.0 {
        r = x;
        b = c;
    } else {
        r = c;
        b = x;
    }
    let m = l_n - c / 2.0;
    Color {
        r: r + m,
        g: g + m,
        b: b + m,
        a: clamp01(a),
    }
}

/// `color01ToHsl(col)` → (h 0..360, s 0..100, l 0..100, a 0..1).
pub(crate) fn color01_to_hsl(col: Color) -> [f64; 4] {
    let Color { r, g, b, a } = col;
    let mx = js::max_of([r, g, b]);
    let mn = js::min_of([r, g, b]);
    let l = (mx + mn) / 2.0;
    if mx == mn {
        return [0.0, 0.0, l * 100.0, a];
    }
    let d = mx - mn;
    let s = if l > 0.5 {
        d / (2.0 - mx - mn)
    } else {
        d / (mx + mn)
    };
    let h = if mx == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h * 60.0, s * 100.0, l * 100.0, a]
}

fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

// ─── Colour spaces (native-only) ─────────────────────────────────────────────
//
// OKLab (Björn Ottosson, 2020) is a perceptual space: equal steps look
// equally different, so blends through it stay bright and even where an
// RGB blend goes muddy or dark in the middle. OKLCH is the same space in
// polar form (lightness, chroma, hue), so a blend walks round the hue
// wheel instead of cutting across it through grey.

/// The space a blend is computed in (the `space` param; RGB when absent,
/// which is what every node did before).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Space {
    Rgb,
    Oklab,
    Oklch,
}

impl Space {
    pub(crate) fn parse(v: Option<&serde_json::Value>) -> Space {
        match v.and_then(serde_json::Value::as_str) {
            Some("oklab") => Space::Oklab,
            Some("oklch") => Space::Oklch,
            _ => Space::Rgb,
        }
    }
}

/// sRGB transfer function → linear light (odd-extended for negatives).
fn srgb_to_linear(c: f64) -> f64 {
    let a = c.abs();
    let l = if a <= 0.040_45 {
        a / 12.92
    } else {
        ((a + 0.055) / 1.055).powf(2.4)
    };
    l.copysign(c)
}

fn linear_to_srgb(l: f64) -> f64 {
    let a = l.abs();
    let c = if a <= 0.003_130_8 {
        a * 12.92
    } else {
        1.055 * a.powf(1.0 / 2.4) - 0.055
    };
    c.copysign(l)
}

type Mat3 = [[f64; 3]; 3];

/// Linear sRGB → LMS cone response (Ottosson's M1).
const RGB_TO_LMS: Mat3 = [
    [0.412_221_470_8, 0.536_332_536_3, 0.051_445_992_9],
    [0.211_903_498_2, 0.680_699_545_1, 0.107_396_956_6],
    [0.088_302_461_9, 0.281_718_837_6, 0.629_978_700_5],
];

/// Cube-rooted LMS → OKLab (Ottosson's M2).
const LMS_TO_LAB: Mat3 = [
    [0.210_454_255_3, 0.793_617_785_0, -0.004_072_046_8],
    [1.977_998_495_1, -2.428_592_205_0, 0.450_593_709_9],
    [0.025_904_037_1, 0.782_771_766_2, -0.808_675_766_0],
];

fn mul(m: &Mat3, v: [f64; 3]) -> [f64; 3] {
    m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

fn invert(m: &Mat3) -> Mat3 {
    let c = |r: usize, k: usize| {
        let (r1, r2) = ((r + 1) % 3, (r + 2) % 3);
        let (k1, k2) = ((k + 1) % 3, (k + 2) % 3);
        m[r1][k1] * m[r2][k2] - m[r1][k2] * m[r2][k1]
    };
    let det = m[0][0] * c(0, 0) + m[0][1] * c(0, 1) + m[0][2] * c(0, 2);
    // The inverse is the transposed cofactor matrix over the determinant.
    std::array::from_fn(|i| std::array::from_fn(|j| c(j, i) / det))
}

/// The two inverses, computed from the forward matrices (rather than
/// Ottosson's 10-digit published ones) so a round trip is exact to f64
/// precision.
fn inverses() -> &'static (Mat3, Mat3) {
    static INV: std::sync::OnceLock<(Mat3, Mat3)> = std::sync::OnceLock::new();
    INV.get_or_init(|| (invert(&LMS_TO_LAB), invert(&RGB_TO_LMS)))
}

/// sRGB (0..1) → OKLab [L, a, b].
pub(crate) fn to_oklab(c: Color) -> [f64; 3] {
    let lin = [c.r, c.g, c.b].map(srgb_to_linear);
    mul(&LMS_TO_LAB, mul(&RGB_TO_LMS, lin).map(f64::cbrt))
}

/// OKLab → sRGB (unclamped: may fall outside 0..1 when out of gamut).
pub(crate) fn from_oklab(lab: [f64; 3], alpha: f64) -> Color {
    let (lab_to_lms, lms_to_rgb) = inverses();
    let lms = mul(lab_to_lms, lab).map(|x| x * x * x);
    let [r, g, b] = mul(lms_to_rgb, lms).map(linear_to_srgb);
    Color { r, g, b, a: alpha }
}

/// sRGB → OKLCH [L, C, h in degrees 0..360].
pub(crate) fn to_oklch(c: Color) -> [f64; 3] {
    let [l, a, b] = to_oklab(c);
    let h = b.atan2(a).to_degrees();
    [l, js::hypot(a, b), if h < 0.0 { h + 360.0 } else { h }]
}

pub(crate) fn from_oklch([l, c, h]: [f64; 3], alpha: f64) -> Color {
    let (s, co) = h.to_radians().sin_cos();
    from_oklab([l, c * co, c * s], alpha)
}

/// Below this chroma a colour is grey and its hue means nothing.
const ACHROMATIC: f64 = 1e-4;

fn clamp_rgb(c: Color) -> Color {
    Color {
        r: clamp01(c.r),
        g: clamp01(c.g),
        b: clamp01(c.b),
        a: c.a,
    }
}

/// Blend `a` → `b` by `t` in `space`. RGB is the plain per-channel lerp;
/// the OK spaces convert, blend (OKLCH along the shorter way round the
/// hue wheel) and clamp back into RGB. Alpha always blends linearly.
pub(crate) fn mix(a: Color, b: Color, t: f64, space: Space) -> Color {
    let alpha = a.a + (b.a - a.a) * t;
    match space {
        Space::Rgb => lerp_color(a, b, t),
        Space::Oklab => {
            let (p, q) = (to_oklab(a), to_oklab(b));
            let at = |i: usize| p[i] + (q[i] - p[i]) * t;
            clamp_rgb(from_oklab([at(0), at(1), at(2)], alpha))
        }
        Space::Oklch => {
            let ([l1, c1, mut h1], [l2, c2, mut h2]) = (to_oklch(a), to_oklch(b));
            // A grey end takes the other end's hue, so it doesn't swing
            // through red on its way to a colour.
            if c1 < ACHROMATIC {
                h1 = h2;
            }
            if c2 < ACHROMATIC {
                h2 = h1;
            }
            let mut dh = h2 - h1;
            if dh > 180.0 {
                dh -= 360.0;
            } else if dh < -180.0 {
                dh += 360.0;
            }
            let lch = [l1 + (l2 - l1) * t, c1 + (c2 - c1) * t, h1 + dh * t];
            clamp_rgb(from_oklch(lch, alpha))
        }
    }
}

/// `inputs[i]?.[0] ?? fallback`.
fn first_or(inputs: &[crate::value::Spread], i: usize, fallback: Value) -> Value {
    let v = first(inputs, i);
    if v.is_nullish() {
        fallback
    } else {
        v
    }
}

// ─── color/color ─────────────────────────────────────────────────────────────

fn num_port(name: &str, default: f64) -> PortSpec {
    let mut p = PortSpec::new(name, PortType::Number).default(Value::Num(default));
    p.param = Some(name.to_owned());
    p
}

/// `dynamicInputs` for `color/color`: pins follow `mode` (rgb / hsl / hex).
fn color_inputs(params: &Params) -> Vec<PortSpec> {
    let mode = match params.get("mode") {
        None | Some(serde_json::Value::Null) => "rgb",
        Some(serde_json::Value::String(s)) => s.as_str(),
        // Any other mode makes the TS switch return undefined (which the
        // editor can't resolve); expose no pins.
        Some(_) => "",
    };
    match mode {
        "rgb" => vec![
            num_port("r", 255.0),
            num_port("g", 255.0),
            num_port("b", 255.0),
            num_port("a", 255.0),
        ],
        "hsl" => vec![
            num_port("h", 0.0),
            num_port("s", 100.0),
            num_port("l", 50.0),
            num_port("a", 255.0),
        ],
        "hex" => {
            let mut p = PortSpec::new("hex", PortType::String).default(Value::str("#ffffff"));
            p.param = Some("hex".into());
            vec![p]
        }
        _ => Vec::new(),
    }
}

/// `params.param<string>(key, fallback)` compared against string literals.
fn param_str(cx: &crate::node::EvalCtx, key: &str, fallback: &str) -> Option<String> {
    match cx.param(key) {
        None => Some(fallback.to_owned()),
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(_) => None,
    }
}

fn color(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let mode = param_str(cx, "mode", "rgb");
        let n = |i: usize, d: f64| first_or(inputs, i, Value::Num(d)).to_number();
        let out = match mode.as_deref() {
            Some("rgb") => Color {
                r: clamp01(n(0, 255.0) / 255.0),
                g: clamp01(n(1, 255.0) / 255.0),
                b: clamp01(n(2, 255.0) / 255.0),
                a: clamp01(n(3, 255.0) / 255.0),
            },
            Some("hsl") => {
                let a = clamp01(n(3, 255.0) / 255.0);
                hsl_to_color01(n(0, 0.0), n(1, 100.0), n(2, 50.0), a)
            }
            Some("hex") => {
                let v = first_or(inputs, 0, Value::str("#ffffff"));
                hex_to_color01(&Value::Str(Arc::from(js_to_string(&v))))
            }
            _ => WHITE,
        };
        Ok(vec![one(Value::Color(out))])
    })
}

// ─── color/split ─────────────────────────────────────────────────────────────

fn split(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let c = to_color(&first(inputs, 0));
        let mode = param_str(cx, "mode", "rgb");
        let vals = if mode.as_deref() == Some("hsl") {
            let [h, s, l, a] = color01_to_hsl(c);
            [h, s, l, a * 255.0]
        } else {
            [c.r * 255.0, c.g * 255.0, c.b * 255.0, c.a * 255.0]
        };
        Ok(vals.iter().map(|v| one(Value::Num(*v))).collect())
    })
}

// ─── Ops ─────────────────────────────────────────────────────────────────────

fn set_alpha(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let col = to_color(&args[0]);
        Ok(vec![Value::Color(Color {
            a: clamp01(args[1].to_number() / 255.0),
            ..col
        })])
    })
}

/// Raw rather than scalar so it can read the `space` param.
fn lerp(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let pa = to_color(&first(inputs, 0));
        let pb = to_color(&first(inputs, 1));
        let t = first(inputs, 2).to_number();
        let space = Space::parse(cx.param("space"));
        Ok(vec![one(Value::Color(mix(pa, pb, t, space)))])
    })
}

fn gradient(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let space = Space::parse(cx.param("space"));
        let stops: &[Value] = inputs.first().map(|s| &s[..]).unwrap_or(&[]);
        let t = first_or(inputs, 1, Value::Num(0.0)).to_number();
        let out = match stops.len() {
            0 => WHITE,
            1 => to_color(&stops[0]),
            len => {
                let tc = clamp01(t);
                let last = (len - 1) as f64;
                let scaled = tc * last;
                let lo = scaled.floor();
                let hi = js::min(last, lo + 1.0);
                let f = scaled - lo;
                // `stops[NaN]` is undefined → white.
                let at = |i: f64| {
                    if i.is_finite() && i >= 0.0 && (i as usize) < len {
                        to_color(&stops[i as usize])
                    } else {
                        WHITE
                    }
                };
                mix(at(lo), at(hi), f, space)
            }
        };
        Ok(vec![one(Value::Color(out))])
    })
}

// ─── Legacy single-mode nodes ────────────────────────────────────────────────

fn rgb(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let ch = |i: usize| clamp01(args[i].to_number() / 255.0);
        Ok(vec![Value::Color(Color {
            r: ch(0),
            g: ch(1),
            b: ch(2),
            a: ch(3),
        })])
    })
}

fn hsl(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        Ok(vec![Value::Color(hsl_to_color01(
            args[0].to_number(),
            args[1].to_number(),
            args[2].to_number(),
            args[3].to_number() / 255.0,
        ))])
    })
}

fn hex(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        Ok(vec![Value::Color(hex_to_color01(&args[0]))])
    })
}

fn split_rgb(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let c = to_color(&args[0]);
        Ok([c.r, c.g, c.b, c.a]
            .iter()
            .map(|v| Value::Num(v * 255.0))
            .collect())
    })
}

fn split_hsl(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let [h, s, l, a] = color01_to_hsl(to_color(&args[0]));
        Ok(vec![
            Value::Num(h),
            Value::Num(s),
            Value::Num(l),
            Value::Num(a * 255.0),
        ])
    })
}

// ─── color/palette ───────────────────────────────────────────────────────────

const MIN_STOPS: f64 = 1.0;
const MAX_STOPS: f64 = 64.0;
const DEFAULT_STOP_COUNT: f64 = 4.0;
const DEFAULT_STOPS: [&str; 8] = [
    "#fed000", "#fc8405", "#ed361a", "#4464a1", "#cf3895", "#6d358a", "#06b4b0", "#4b8a5f",
];

/// `clampStops(n)`: `Number.isFinite` does not coerce, so anything but a
/// finite number gives the default count.
fn clamp_stops(n: &Value) -> usize {
    match n {
        Value::Num(n) if n.is_finite() => {
            js::max(MIN_STOPS, js::min(MAX_STOPS, n.floor())) as usize
        }
        _ => DEFAULT_STOP_COUNT as usize,
    }
}

/// What the palette node makes: the authored stops (the `mode` param's
/// default, and every palette saved before modes existed), a cosine
/// palette, a colour harmony around a base colour, or the dominant
/// colours of an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaletteMode {
    Stops,
    Cosine,
    Harmony(Harmony),
    Image,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Harmony {
    Complementary,
    Analogous,
    Triadic,
    Split,
    Tetradic,
}

impl PaletteMode {
    fn parse(v: Option<&serde_json::Value>) -> PaletteMode {
        match v.and_then(serde_json::Value::as_str) {
            Some("cosine") => PaletteMode::Cosine,
            Some("complementary") => PaletteMode::Harmony(Harmony::Complementary),
            Some("analogous") => PaletteMode::Harmony(Harmony::Analogous),
            Some("triadic") => PaletteMode::Harmony(Harmony::Triadic),
            Some("split") => PaletteMode::Harmony(Harmony::Split),
            Some("tetradic") => PaletteMode::Harmony(Harmony::Tetradic),
            Some("image") => PaletteMode::Image,
            _ => PaletteMode::Stops,
        }
    }
}

impl Harmony {
    /// Whether the `angle` pin applies.
    fn has_angle(self) -> bool {
        matches!(self, Harmony::Analogous | Harmony::Split)
    }

    /// Hue offsets (degrees) from the base colour, base first.
    fn offsets(self, angle: f64) -> Vec<f64> {
        match self {
            Harmony::Complementary => vec![0.0, 180.0],
            Harmony::Analogous => vec![0.0, -angle, angle],
            Harmony::Triadic => vec![0.0, 120.0, 240.0],
            Harmony::Split => vec![0.0, 180.0 - angle, 180.0 + angle],
            Harmony::Tetradic => vec![0.0, 90.0, 180.0, 270.0],
        }
    }
}

/// The colours of a harmony: the base, then the base's hue turned round
/// the HSL colour wheel by each offset (saturation, lightness and alpha
/// kept).
pub(crate) fn harmony(base: Color, h: Harmony, angle: f64) -> Vec<Color> {
    let [hue, s, l, a] = color01_to_hsl(base);
    h.offsets(angle)
        .into_iter()
        .map(|off| {
            if off == 0.0 {
                base
            } else {
                hsl_to_color01(hue + off, s, l, a)
            }
        })
        .collect()
}

/// Inigo Quilez's cosine palette `a + b·cos(2π(c·t + d))` per channel,
/// clamped to 0..1.
pub(crate) fn cosine_palette(a: Color, b: Color, c: Color, d: Color, t: f64) -> Color {
    let ch = |a: f64, b: f64, c: f64, d: f64| {
        clamp01(a + b * (std::f64::consts::TAU * (c * t + d)).cos())
    };
    Color {
        r: ch(a.r, b.r, c.r, d.r),
        g: ch(a.g, b.g, c.g, d.g),
        b: ch(a.b, b.b, c.b, d.b),
        a: 1.0,
    }
}

fn gray(v: f64) -> Value {
    Value::Color(Color {
        r: v,
        g: v,
        b: v,
        a: 1.0,
    })
}

fn color_port(name: &str, default: Value) -> PortSpec {
    PortSpec::new(name, PortType::Color).default(default)
}

/// `stopCount`, as `buildStopPorts` reads it.
fn stop_count(params: &Params) -> usize {
    let raw = params
        .get("stopCount")
        .map(Value::from_json)
        .unwrap_or(Value::Undefined)
        .to_number();
    let count = if raw.is_nan() || raw == 0.0 {
        DEFAULT_STOP_COUNT
    } else {
        raw
    };
    clamp_stops(&Value::Num(count))
}

/// `buildStopPorts`: one colour pin per stop (`c0`, `c1`, …), or the
/// generator's pins in the other modes.
fn build_stop_ports(params: &Params) -> Vec<PortSpec> {
    match PaletteMode::parse(params.get("mode")) {
        PaletteMode::Stops => (0..stop_count(params))
            .map(|i| {
                let mut p = PortSpec::new(format!("c{i}"), PortType::Color)
                    .label(format!("{}", i + 1))
                    .default(Value::Null);
                p.param = Some(format!("c{i}"));
                p
            })
            .collect(),
        // The classic rainbow: a = b = ½ grey, c = 1, d = (0, ⅓, ⅔).
        PaletteMode::Cosine => vec![
            color_port("a", gray(0.5)),
            color_port("b", gray(0.5)),
            color_port("c", gray(1.0)),
            color_port(
                "d",
                Value::Color(Color {
                    r: 0.0,
                    g: 0.33,
                    b: 0.67,
                    a: 1.0,
                }),
            ),
        ],
        PaletteMode::Image => {
            let mut sort = PortSpec::new("sort", PortType::String).default(Value::str("lightness"));
            sort.param = Some("sort".into());
            vec![
                PortSpec::new("image", PortType::Image).default(Value::Null),
                sort,
            ]
        }
        PaletteMode::Harmony(h) => {
            let mut ports = vec![color_port(
                "base",
                Value::Color(hex_to_color01(&Value::str(DEFAULT_STOPS[2]))),
            )];
            if h.has_angle() {
                ports.push(PortSpec::new("angle", PortType::Number).default(Value::Num(30.0)));
            }
            ports
        }
    }
}

fn palette(_: &NodeInit) -> Box<dyn NodeImpl> {
    // The image mode's last answer: (image id, count, sort) → colours.
    let mut extracted: Option<((u64, usize, String), Vec<Color>)> = None;
    raw(move |inputs, cx| {
        let count = clamp_stops(
            &cx.param("stopCount")
                .map(Value::from_json)
                .unwrap_or(Value::Num(DEFAULT_STOP_COUNT)),
        );
        let colors: Vec<Color> = match PaletteMode::parse(cx.param("mode")) {
            PaletteMode::Stops => {
                return Ok(vec![Arc::from(authored_stops(inputs, cx, count))]);
            }
            PaletteMode::Cosine => {
                let [a, b, c, d] = [0, 1, 2, 3].map(|i| to_color(&first(inputs, i)));
                // Evenly round one cycle (t = i / count), so a palette
                // that repeats every cycle has no doubled colour.
                (0..count)
                    .map(|i| cosine_palette(a, b, c, d, i as f64 / count as f64))
                    .collect()
            }
            PaletteMode::Harmony(h) => {
                let base = to_color(&first(inputs, 0));
                let angle = first_or(inputs, 1, Value::Num(30.0)).to_number();
                harmony(base, h, angle)
            }
            PaletteMode::Image => {
                let Some(img) = crate::image::ready(&first(inputs, 0)).cloned() else {
                    return Ok(vec![crate::value::empty()]);
                };
                let sort = match first(inputs, 1) {
                    Value::Str(s) => PaletteSort::parse(&s),
                    _ => PaletteSort::Lightness,
                };
                let key = (img.id(), count, format!("{sort:?}"));
                match &extracted {
                    Some((k, colors)) if *k == key => colors.clone(),
                    _ => {
                        let colors = dominant_colors(&img, count, sort);
                        extracted = Some((key, colors.clone()));
                        colors
                    }
                }
            }
        };
        Ok(vec![colors.into_iter().map(Value::Color).collect()])
    })
}

/// How the image mode orders its colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaletteSort {
    /// Darkest first (OKLab lightness).
    Lightness,
    /// Most of the picture first.
    Frequency,
    /// Round the hue wheel from red, greys first.
    Hue,
}

impl PaletteSort {
    fn parse(s: &str) -> PaletteSort {
        match s {
            "frequency" => PaletteSort::Frequency,
            "hue" => PaletteSort::Hue,
            _ => PaletteSort::Lightness,
        }
    }
}

/// Pixels sampled at most (a stride keeps big pictures quick).
const PALETTE_SAMPLES: usize = 16_384;
const KMEANS_ROUNDS: usize = 24;

/// The `count` dominant colours of an image: k-means in OKLab over its
/// opaque pixels (alpha ≥ ½), seeded by k-means++ from a fixed stream so
/// the same picture always gives the same palette. Fewer when the picture
/// has fewer distinct colours; empty when it is all transparent.
pub(crate) fn dominant_colors(
    img: &crate::image::Image,
    count: usize,
    sort: PaletteSort,
) -> Vec<Color> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 || count == 0 {
        return Vec::new();
    }
    let stride = ((w * h) as f64 / PALETTE_SAMPLES as f64)
        .sqrt()
        .ceil()
        .max(1.0) as usize;
    let mut pts: Vec<[f64; 3]> = Vec::new();
    for y in (0..h).step_by(stride) {
        for x in (0..w).step_by(stride) {
            let [r, g, b, a] = img.pixel(x as u32, y as u32);
            if a < 128 {
                continue;
            }
            let c = Color {
                r: f64::from(r) / 255.0,
                g: f64::from(g) / 255.0,
                b: f64::from(b) / 255.0,
                a: 1.0,
            };
            pts.push(to_oklab(c));
        }
    }
    if pts.is_empty() {
        return Vec::new();
    }
    let d2 = |p: &[f64; 3], q: &[f64; 3]| {
        (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)
    };
    // k-means++: each next centre drawn with probability ∝ distance².
    let mut rng = js::Mulberry32::new(0x9e37_79b9);
    let mut centres: Vec<[f64; 3]> =
        vec![pts[(rng.next_f64() * pts.len() as f64) as usize % pts.len()]];
    let mut near: Vec<f64> = pts.iter().map(|p| d2(p, &centres[0])).collect();
    while centres.len() < count {
        let total: f64 = near.iter().sum();
        if total <= 1e-12 {
            break; // every pixel already sits on a centre
        }
        let mut x = rng.next_f64() * total;
        let mut pick = pts.len() - 1;
        for (i, d) in near.iter().enumerate() {
            x -= d;
            if x < 0.0 {
                pick = i;
                break;
            }
        }
        let c = pts[pick];
        centres.push(c);
        for (n, p) in near.iter_mut().zip(&pts) {
            *n = n.min(d2(p, &c));
        }
    }
    // Lloyd rounds.
    let k = centres.len();
    let mut label = vec![0usize; pts.len()];
    let mut sizes = vec![0usize; k];
    for _ in 0..KMEANS_ROUNDS {
        let mut sums = vec![[0.0f64; 3]; k];
        sizes = vec![0; k];
        let mut changed = false;
        for (i, p) in pts.iter().enumerate() {
            let mut best = (f64::INFINITY, 0);
            for (j, c) in centres.iter().enumerate() {
                let d = d2(p, c);
                if d < best.0 {
                    best = (d, j);
                }
            }
            if label[i] != best.1 {
                label[i] = best.1;
                changed = true;
            }
            sizes[best.1] += 1;
            for (s, v) in sums[best.1].iter_mut().zip(p) {
                *s += v;
            }
        }
        for j in 0..k {
            if sizes[j] > 0 {
                centres[j] = sums[j].map(|s| s / sizes[j] as f64);
            }
        }
        if !changed {
            break;
        }
    }
    let mut found: Vec<([f64; 3], usize)> = centres
        .into_iter()
        .zip(sizes)
        .filter(|(_, n)| *n > 0)
        .collect();
    match sort {
        PaletteSort::Lightness => found.sort_by(|a, b| a.0[0].total_cmp(&b.0[0])),
        PaletteSort::Frequency => {
            found.sort_by(|a, b| b.1.cmp(&a.1).then(a.0[0].total_cmp(&b.0[0])))
        }
        PaletteSort::Hue => {
            // Greys (chroma under 0.03) first by lightness, then by hue.
            let key = |c: &[f64; 3]| {
                let chroma = c[1].hypot(c[2]);
                if chroma < 0.03 {
                    (0, c[0])
                } else {
                    (1, c[2].atan2(c[1]).rem_euclid(std::f64::consts::TAU))
                }
            };
            found.sort_by(|a, b| {
                let (ka, kb) = (key(&a.0), key(&b.0));
                ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
            });
        }
    }
    found
        .into_iter()
        .map(|(c, _)| clamp_rgb(from_oklab(c, 1.0)))
        .collect()
}

/// The stops mode: each wired colour, else its `c<i>` hex param.
fn authored_stops(
    inputs: &[crate::value::Spread],
    cx: &crate::node::EvalCtx,
    count: usize,
) -> Vec<Value> {
    (0..count)
        .map(|i| {
            let wired = first(inputs, i);
            // `wired && typeof wired === 'object' && 'r' in wired`.
            let has_r = match &wired {
                Value::Color(_) => true,
                Value::Object(m) => m.contains_key("r"),
                _ => false,
            };
            if has_r {
                return Value::Color(to_color(&wired));
            }
            let hex = cx
                .param(&format!("c{i}"))
                .map(Value::from_json)
                .unwrap_or_else(|| Value::str(DEFAULT_STOPS.get(i).unwrap_or(&"#ffffff")));
            Value::Color(hex_to_color01(&hex))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Expected values recorded from the TypeScript implementation.
    use super::*;
    use crate::node::{EvalCtx, FrameCtx, ParamStore};
    use crate::spec::Catalogue;
    use crate::value::{empty, Spread, Vec2};
    use serde_json::json;

    const NAN: f64 = f64::NAN;
    const W: [f64; 4] = [1.0; 4];

    fn run(ty: &str, params: serde_json::Value, inputs: Vec<Spread>) -> Vec<Spread> {
        let cat = Catalogue::builtin();
        let spec = cat.get(ty).unwrap();
        let params: Params = params.as_object().cloned().unwrap_or_default();
        let resolved = cat.resolved_inputs(spec, &params);
        let init = NodeInit {
            id: "n",
            spec,
            params: &params,
            inputs: &resolved,
        };
        let mut node = crate::nodes::factory(ty).unwrap()(&init);
        let frame = FrameCtx::default();
        let mut store = ParamStore::default();
        let mut cx = EvalCtx {
            node_id: "n",
            frame: &frame,
            tick_frame: &frame,
            params: &mut store,
            saved_params: &params,
        };
        node.eval(&inputs, &mut cx).unwrap()
    }

    /// Scalar-node call with single values (`None` = empty spread → undefined).
    fn call(ty: &str, args: Vec<Option<Value>>) -> Vec<Spread> {
        run(
            ty,
            json!({}),
            args.into_iter()
                .map(|a| a.map_or_else(empty, one))
                .collect(),
        )
    }

    fn c(r: f64, g: f64, b: f64, a: f64) -> Value {
        Value::Color(Color { r, g, b, a })
    }

    fn n(x: f64) -> Value {
        Value::Num(x)
    }

    fn s(x: &str) -> Value {
        Value::str(x)
    }

    fn obj(j: serde_json::Value) -> Value {
        Value::from_json(&j)
    }

    fn close(a: f64, b: f64) -> bool {
        (a.is_nan() && b.is_nan()) || a == b || (a - b).abs() <= 1e-12 * (1.0 + a.abs())
    }

    fn assert_col(v: &Value, want: [f64; 4]) {
        let Value::Color(col) = v else {
            panic!("not a colour: {v:?}")
        };
        let got = [col.r, col.g, col.b, col.a];
        assert!(
            got.iter().zip(&want).all(|(a, b)| close(*a, *b)),
            "{got:?} vs {want:?}"
        );
    }

    fn assert_nums(out: &[Spread], want: &[f64]) {
        let got: Vec<f64> = out.iter().map(|s| s[0].as_f64().unwrap()).collect();
        assert!(
            got.len() == want.len() && got.iter().zip(want).all(|(a, b)| close(*a, *b)),
            "{got:?} vs {want:?}"
        );
    }

    #[test]
    fn hex() {
        let cases: Vec<(Option<Value>, [f64; 4])> = vec![
            (Some(s("#ff8000")), [1.0, 0.5019607843137255, 0.0, 1.0]),
            (
                Some(s("0A0b0C")),
                [
                    0.0392156862745098,
                    0.043137254901960784,
                    0.047058823529411764,
                    1.0,
                ],
            ),
            (Some(s("#fff")), W),
            (Some(s("#ff8000 ")), W),
            (Some(s("##ff8000")), W),
            (
                Some(n(123456.0)),
                [
                    0.07058823529411765,
                    0.20392156862745098,
                    0.33725490196078434,
                    1.0,
                ],
            ),
            (None, W),
            (
                Some(Value::list(vec![s("102030")])),
                [
                    0.06274509803921569,
                    0.12549019607843137,
                    0.18823529411764706,
                    1.0,
                ],
            ),
            (Some(s("#gg0000")), W),
            (Some(Value::Null), W),
            (Some(obj(json!({"r": 1}))), W),
        ];
        for (input, want) in cases {
            assert_col(&call("color/hex", vec![input])[0][0], want);
        }
    }

    #[test]
    fn legacy_hsl_and_rgb() {
        let out = call(
            "color/hsl",
            vec![Some(n(-30.0)), Some(n(80.0)), Some(n(40.0)), Some(n(128.0))],
        );
        assert_col(
            &out[0][0],
            [
                0.7200000000000001,
                0.07999999999999996,
                0.4,
                0.5019607843137255,
            ],
        );
        let out = call(
            "color/hsl",
            vec![Some(n(NAN)), Some(n(50.0)), Some(n(50.0)), Some(n(255.0))],
        );
        assert_col(&out[0][0], [0.75, 0.25, NAN, 1.0]);
        let out = call(
            "color/hsl",
            vec![Some(n(720.5)), Some(s("50")), Some(Value::Bool(true)), None],
        );
        assert_col(
            &out[0][0],
            [
                0.015000000000000005,
                0.005083333333333329,
                0.004999999999999996,
                NAN,
            ],
        );
        let out = call(
            "color/rgb",
            vec![Some(n(300.0)), Some(s("51")), Some(n(NAN)), None],
        );
        assert_col(&out[0][0], [1.0, 0.2, NAN, NAN]);
        let out = call(
            "color/rgb",
            vec![
                Some(Value::Null),
                Some(Value::Bool(true)),
                Some(Value::list(vec![n(7.0)])),
                Some(obj(json!({"r": 1}))),
            ],
        );
        assert_col(
            &out[0][0],
            [0.0, 0.00392156862745098, 0.027450980392156862, NAN],
        );
    }

    #[test]
    fn unified_color() {
        let out = run(
            "color/color",
            json!({"mode": "rgb"}),
            vec![one(n(51.0)), one(Value::Null), one(n(-4.0)), empty()],
        );
        assert_col(&out[0][0], [0.2, 1.0, 0.0, 1.0]);
        let out = run(
            "color/color",
            json!({"mode": "hsl"}),
            vec![one(n(200.0)), one(Value::Null), one(n(25.0)), one(n(510.0))],
        );
        assert_col(&out[0][0], [0.0, 0.33333333333333326, 0.5, 1.0]);
        let out = run(
            "color/color",
            json!({"mode": "hex"}),
            vec![one(s("336699"))],
        );
        assert_col(&out[0][0], [0.2, 0.4, 0.6, 1.0]);
        let out = run("color/color", json!({"mode": "hex"}), vec![empty()]);
        assert_col(&out[0][0], W);
        let out = run("color/color", json!({"mode": "bogus"}), vec![one(n(1.0))]);
        assert_col(&out[0][0], W);
        let out = run("color/color", json!({"mode": null}), vec![one(n(1.0))]);
        assert_col(&out[0][0], W);

        let ports = color_inputs(json!({"mode": "hsl"}).as_object().unwrap());
        let got: Vec<_> = ports
            .iter()
            .map(|p| {
                (
                    p.name.as_str(),
                    p.ty,
                    p.default.as_ref().and_then(Value::as_f64),
                    p.param.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("h", PortType::Number, Some(0.0), Some("h")),
                ("s", PortType::Number, Some(100.0), Some("s")),
                ("l", PortType::Number, Some(50.0), Some("l")),
                ("a", PortType::Number, Some(255.0), Some("a")),
            ]
        );
        let rgb = color_inputs(&Params::new());
        let names: Vec<_> = rgb.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["r", "g", "b", "a"]);
        assert_eq!(rgb[3].default.as_ref().and_then(Value::as_f64), Some(255.0));
        let hex = color_inputs(json!({"mode": "hex"}).as_object().unwrap());
        assert_eq!(hex.len(), 1);
        assert_eq!(hex[0].ty, PortType::String);
        assert_eq!(hex[0].param.as_deref(), Some("hex"));
    }

    #[test]
    fn splits() {
        let col = c(0.2, 0.4, 0.6, 0.5);
        let hsl = |v: Value| call("color/split-hsl", vec![Some(v)]);
        assert_nums(&hsl(col.clone()), &[210.0, 49.99999999999999, 40.0, 127.5]);
        assert_nums(&hsl(c(0.9, 0.1, 0.3, 1.0)), &[345.0, 80.0, 50.0, 255.0]);
        assert_nums(
            &hsl(obj(json!({"r": 0.1, "g": 0.9, "b": 0.3}))),
            &[135.0, 80.0, 50.0, 255.0],
        );
        assert_nums(&hsl(c(0.5, 0.5, 0.5, 1.0)), &[0.0, 0.0, 50.0, 255.0]);
        assert_nums(&hsl(c(NAN, 0.5, 0.5, 1.0)), &[NAN, NAN, NAN, 255.0]);
        let rgb = |v: Value| call("color/split-rgb", vec![Some(v)]);
        assert_nums(&rgb(n(3.0)), &[255.0; 4]);
        assert_nums(&rgb(col.clone()), &[51.0, 102.0, 153.0, 127.5]);
        let split = |mode: serde_json::Value, v: Value| run("color/split", mode, vec![one(v)]);
        assert_nums(
            &split(json!({"mode": "hsl"}), col.clone()),
            &[210.0, 49.99999999999999, 40.0, 127.5],
        );
        assert_nums(
            &split(json!({"mode": "rgb"}), col),
            &[51.0, 102.0, 153.0, 127.5],
        );
        assert_nums(&split(json!({}), Value::Null), &[255.0; 4]);
    }

    #[test]
    fn set_alpha_and_lerp() {
        let col = c(0.2, 0.4, 0.6, 0.5);
        let out = call("color/set-alpha", vec![Some(Value::Null), Some(n(51.0))]);
        assert_col(&out[0][0], [1.0, 1.0, 1.0, 0.2]);
        let out = call("color/set-alpha", vec![Some(col.clone()), Some(s("300"))]);
        assert_col(&out[0][0], [0.2, 0.4, 0.6, 1.0]);
        let out = call("color/set-alpha", vec![Some(col.clone()), None]);
        assert_col(&out[0][0], [0.2, 0.4, 0.6, NAN]);
        let red = c(1.0, 0.0, 0.0, 1.0);
        let out = call(
            "color/lerp",
            vec![Some(col.clone()), Some(red.clone()), Some(s("0.5"))],
        );
        assert_col(&out[0][0], [0.6000000000000001, 0.2, 0.3, 0.75]);
        let out = call("color/lerp", vec![Some(col.clone()), Some(red), None]);
        assert_col(&out[0][0], [NAN; 4]);
        let out = call("color/lerp", vec![Some(col), Some(n(5.0)), Some(n(2.0))]);
        assert_col(&out[0][0], [1.8, 1.6, 1.4, 1.5]);
    }

    #[test]
    fn gradient_stops() {
        let red = c(1.0, 0.0, 0.0, 1.0);
        let blue = c(0.0, 0.0, 1.0, 0.0);
        let stops: Spread = Arc::from(vec![red.clone(), blue, red.clone()]);
        let grad = |stops: Spread, t: Value| run("color/gradient", json!({}), vec![stops, one(t)]);
        let cases = [
            (n(0.25), [0.5, 0.0, 0.5, 0.5]),
            (n(2.0), [1.0, 0.0, 0.0, 1.0]),
            (n(-1.0), [1.0, 0.0, 0.0, 1.0]),
            (n(NAN), [NAN; 4]),
            (n(0.999), [0.998, 0.0, 0.0020000000000000018, 0.998]),
            (Value::Null, [1.0, 0.0, 0.0, 1.0]),
            (s("0.5"), [0.0, 0.0, 1.0, 0.0]),
        ];
        for (t, want) in cases {
            assert_col(&grad(stops.clone(), t)[0][0], want);
        }
        assert_col(&grad(empty(), n(0.5))[0][0], W);
        assert_col(&grad(one(n(5.0)), n(0.5))[0][0], W);
        assert_col(
            &grad(Arc::from(vec![red, n(7.0)]), n(0.75))[0][0],
            [1.0, 0.75, 0.75, 1.0],
        );
    }

    #[test]
    fn palette_stops() {
        let mut ins = vec![one(Value::Null); 10];
        ins[1] = one(c(0.2, 0.4, 0.6, 0.5));
        ins[3] = one(Value::Vec2(Vec2 { x: 1.0, y: 2.0 }));
        ins[4] = one(obj(json!({"r": "x"})));
        let out = run(
            "color/palette",
            json!({"stopCount": 10, "c0": "#000000", "c2": null}),
            ins,
        );
        let want = [
            [0.0, 0.0, 0.0, 1.0],
            [0.2, 0.4, 0.6, 0.5],
            W,
            [
                0.26666666666666666,
                0.39215686274509803,
                0.6313725490196078,
                1.0,
            ],
            W,
            [
                0.42745098039215684,
                0.20784313725490197,
                0.5411764705882353,
                1.0,
            ],
            [
                0.023529411764705882,
                0.7058823529411765,
                0.6901960784313725,
                1.0,
            ],
            [
                0.29411764705882354,
                0.5411764705882353,
                0.37254901960784315,
                1.0,
            ],
            W,
            W,
        ];
        assert_eq!(out[0].len(), want.len());
        for (v, w) in out[0].iter().zip(want) {
            assert_col(v, w);
        }
        let first4 = [
            [0.996078431372549, 0.8156862745098039, 0.0, 1.0],
            [
                0.9882352941176471,
                0.5176470588235295,
                0.0196078431372549,
                1.0,
            ],
            [
                0.9294117647058824,
                0.21176470588235294,
                0.10196078431372549,
                1.0,
            ],
            [
                0.26666666666666666,
                0.39215686274509803,
                0.6313725490196078,
                1.0,
            ],
        ];
        // A string stopCount isn't a finite number at compute time → 4.
        let out = run(
            "color/palette",
            json!({"stopCount": "5"}),
            vec![one(Value::Null); 5],
        );
        assert_eq!(out[0].len(), 4);
        for (v, w) in out[0].iter().zip(first4) {
            assert_col(v, w);
        }
        let out = run(
            "color/palette",
            json!({"stopCount": 2.7}),
            vec![one(Value::Null); 2],
        );
        assert_eq!(out[0].len(), 2);
        let out = run("color/palette", json!({"stopCount": 0}), vec![]);
        assert_eq!(out[0].len(), 1);
        assert_col(&out[0][0], first4[0]);

        let count = |p: serde_json::Value| build_stop_ports(p.as_object().unwrap()).len();
        assert_eq!(count(json!({"stopCount": 10})), 10);
        assert_eq!(count(json!({"stopCount": "0"})), 4);
        assert_eq!(count(json!({"stopCount": 500})), 64);
        assert_eq!(count(json!({})), 4);
        assert_eq!(count(json!({"stopCount": -3})), 1);
        assert_eq!(count(json!({"stopCount": "7"})), 7);
        assert_eq!(count(json!({"stopCount": 2.9})), 2);
        let ports = build_stop_ports(json!({"stopCount": 10}).as_object().unwrap());
        assert_eq!(ports[9].name, "c9");
        assert_eq!(ports[9].label.as_deref(), Some("10"));
        assert_eq!(ports[9].ty, PortType::Color);
        assert!(matches!(ports[9].default, Some(Value::Null)));
    }

    // ─── Colour spaces, cosine palettes, harmonies (native-only) ─────────

    fn rgb01(r: f64, g: f64, b: f64) -> Color {
        Color { r, g, b, a: 1.0 }
    }

    fn near(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn oklab_matches_reference_values() {
        // CSS Color 4 / Ottosson's reference conversions of the sRGB
        // primaries, white and mid grey.
        for (c, want) in [
            (rgb01(1.0, 1.0, 1.0), [1.0, 0.0, 0.0]),
            (rgb01(0.0, 0.0, 0.0), [0.0, 0.0, 0.0]),
            (rgb01(1.0, 0.0, 0.0), [0.627_955, 0.224_863, 0.125_846]),
            (rgb01(0.0, 1.0, 0.0), [0.866_440, -0.233_888, 0.179_498]),
            (rgb01(0.0, 0.0, 1.0), [0.452_014, -0.032_457, -0.311_528]),
            (
                rgb01(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0),
                [0.599_871, 0.0, 0.0],
            ),
        ] {
            let got = to_oklab(c);
            assert!(
                got.iter().zip(want).all(|(g, w)| near(*g, w, 2e-5)),
                "{c:?}: {got:?} vs {want:?}"
            );
        }
        // OKLCH hues of red and blue.
        assert!(near(to_oklch(rgb01(1.0, 0.0, 0.0))[2], 29.2339, 1e-3));
        assert!(near(to_oklch(rgb01(0.0, 0.0, 1.0))[2], 264.052, 1e-3));
    }

    #[test]
    fn oklab_and_oklch_round_trip() {
        for i in 0..=6 {
            for j in 0..=6 {
                for k in 0..=6 {
                    let c = Color {
                        r: f64::from(i) / 6.0,
                        g: f64::from(j) / 6.0,
                        b: f64::from(k) / 6.0,
                        a: 0.25,
                    };
                    for back in [from_oklab(to_oklab(c), c.a), from_oklch(to_oklch(c), c.a)] {
                        let ok = [(c.r, back.r), (c.g, back.g), (c.b, back.b), (c.a, back.a)]
                            .iter()
                            .all(|(a, b)| near(*a, *b, 1e-12));
                        assert!(ok, "{c:?} came back as {back:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn lerp_and_gradient_spaces() {
        let red = c(1.0, 0.0, 0.0, 1.0);
        let blue = c(0.0, 0.0, 1.0, 0.0);
        let lerp = |space: &str, t: f64| {
            run(
                "color/lerp",
                json!({ "space": space }),
                vec![one(red.clone()), one(blue.clone()), one(n(t))],
            )[0][0]
                .clone()
        };
        // RGB (and no space at all) is the old per-channel lerp.
        assert_col(&lerp("rgb", 0.5), [0.5, 0.0, 0.5, 0.5]);
        let none = run(
            "color/lerp",
            json!({}),
            vec![one(red.clone()), one(blue.clone()), one(n(0.5))],
        );
        assert_col(&none[0][0], [0.5, 0.0, 0.5, 0.5]);
        for space in ["oklab", "oklch"] {
            assert_col(&lerp(space, 0.0), [1.0, 0.0, 0.0, 1.0]);
            let Value::Color(end) = lerp(space, 1.0) else {
                panic!()
            };
            assert!(near(end.b, 1.0, 1e-12) && near(end.r, 0.0, 1e-12) && end.a == 0.0);
        }
        // Black → white: OKLab's midpoint is perceptual mid grey (L = ½),
        // darker in RGB terms than the channel average.
        let mid = mix(
            rgb01(0.0, 0.0, 0.0),
            rgb01(1.0, 1.0, 1.0),
            0.5,
            Space::Oklab,
        );
        let want = 1.055 * 0.125_f64.powf(1.0 / 2.4) - 0.055;
        assert!(near(mid.r, want, 1e-6) && near(mid.g, want, 1e-6) && near(mid.b, want, 1e-6));
        // Red → blue in OKLCH keeps its chroma (vivid) instead of passing
        // through RGB's dull purple, and takes the short way round (through
        // magenta, hue ≈ 327°).
        let lch = to_oklch(mix(
            rgb01(1.0, 0.0, 0.0),
            rgb01(0.0, 0.0, 1.0),
            0.5,
            Space::Oklch,
        ));
        let rgb = to_oklch(mix(
            rgb01(1.0, 0.0, 0.0),
            rgb01(0.0, 0.0, 1.0),
            0.5,
            Space::Rgb,
        ));
        assert!(lch[1] > rgb[1] + 0.05, "{lch:?} vs {rgb:?}");
        assert!(near(lch[2], 326.6, 2.0), "{lch:?}");
        // A grey end takes the other's hue: grey → red stays reddish.
        let g = to_oklch(mix(
            rgb01(0.5, 0.5, 0.5),
            rgb01(1.0, 0.0, 0.0),
            0.5,
            Space::Oklch,
        ));
        assert!(near(g[2], 29.2, 3.0), "{g:?}");
        // The gradient blends neighbouring stops in its space.
        let stops: Spread = Arc::from(vec![red.clone(), blue.clone()]);
        let grad = run(
            "color/gradient",
            json!({"space": "oklab"}),
            vec![stops, one(n(0.5))],
        );
        let Value::Color(g) = &grad[0][0] else {
            panic!()
        };
        let want = mix(to_color(&red), to_color(&blue), 0.5, Space::Oklab);
        assert_eq!(*g, want);
        assert_ne!(*g, lerp_color(to_color(&red), to_color(&blue), 0.5));
    }

    fn hue(c: &Value) -> f64 {
        color01_to_hsl(to_color(c))[0]
    }

    #[test]
    fn harmony_hues() {
        // hsl(30°, 80%, 50%)
        let base = Value::Color(hsl_to_color01(30.0, 80.0, 50.0, 1.0));
        let hues = |mode: &str, angle: Option<f64>| -> Vec<f64> {
            let mut ins = vec![one(base.clone())];
            ins.extend(angle.map(|a| one(n(a))));
            run("color/palette", json!({ "mode": mode }), ins)[0]
                .iter()
                .map(hue)
                .collect()
        };
        let check = |got: Vec<f64>, want: &[f64]| {
            assert!(
                got.len() == want.len() && got.iter().zip(want).all(|(g, w)| near(*g, *w, 1e-6)),
                "{got:?} vs {want:?}"
            );
        };
        check(hues("complementary", None), &[30.0, 210.0]);
        check(hues("analogous", Some(20.0)), &[30.0, 10.0, 50.0]);
        check(hues("triadic", None), &[30.0, 150.0, 270.0]);
        check(hues("split", Some(30.0)), &[30.0, 180.0, 240.0]);
        check(hues("tetradic", None), &[30.0, 120.0, 210.0, 300.0]);
        // Saturation and lightness stay the base's; the base comes first, as is.
        let out = run(
            "color/palette",
            json!({"mode": "triadic"}),
            vec![one(base.clone())],
        );
        assert!(matches!((&out[0][0], &base), (Value::Color(a), Value::Color(b)) if a == b));
        for v in out[0].iter() {
            let [_, s, l, _] = color01_to_hsl(to_color(v));
            assert!(near(s, 80.0, 1e-6) && near(l, 50.0, 1e-6));
        }
        // Pins follow the mode.
        let names = |mode: &str| -> Vec<String> {
            build_stop_ports(json!({ "mode": mode }).as_object().unwrap())
                .into_iter()
                .map(|p| p.name)
                .collect()
        };
        assert_eq!(names("complementary"), ["base"]);
        assert_eq!(names("analogous"), ["base", "angle"]);
        assert_eq!(names("split"), ["base", "angle"]);
        assert_eq!(names("cosine"), ["a", "b", "c", "d"]);
        assert_eq!(names("stops"), ["c0", "c1", "c2", "c3"]);
    }

    #[test]
    fn cosine_palettes() {
        let ports = build_stop_ports(json!({"mode": "cosine"}).as_object().unwrap());
        let defaults: Vec<Spread> = ports
            .iter()
            .map(|p| one(p.default.clone().unwrap()))
            .collect();
        let out = run(
            "color/palette",
            json!({"mode": "cosine", "stopCount": 4}),
            defaults.clone(),
        );
        assert_eq!(out[0].len(), 4);
        // t = 0: ½ + ½·cos(2π·d) per channel.
        let ch = |d: f64, t: f64| 0.5 + 0.5 * (std::f64::consts::TAU * (t + d)).cos();
        for (i, v) in out[0].iter().enumerate() {
            let t = i as f64 / 4.0;
            assert_col(v, [ch(0.0, t), ch(0.33, t), ch(0.67, t), 1.0]);
        }
        let out = run(
            "color/palette",
            json!({"mode": "cosine", "stopCount": 7}),
            defaults,
        );
        assert_eq!(out[0].len(), 7);
        // Out-of-range results are clamped.
        let big = cosine_palette(
            rgb01(1.0, 1.0, 1.0),
            rgb01(1.0, 1.0, 1.0),
            rgb01(0.0, 0.0, 0.0),
            rgb01(0.0, 0.5, 0.25),
            0.0,
        );
        assert_eq!((big.r, big.g), (1.0, 0.0));
        assert!(near(big.b, 1.0, 1e-12));
    }
}

#[cfg(test)]
mod palette_image_tests {
    use super::*;
    use crate::image::Image;
    use crate::nodes::value::harness::node;
    use serde_json::json;

    /// An image of horizontal bands, `rows` pixels of each colour.
    fn bands(colors: &[([u8; 3], usize)]) -> Image {
        let w = 8;
        let mut px = Vec::new();
        for (c, rows) in colors {
            for _ in 0..rows * w {
                px.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        let h = colors.iter().map(|c| c.1).sum::<usize>();
        Image::new(w as u32, h as u32, px)
    }

    fn bytes(c: &Color) -> [u8; 3] {
        [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8)
    }

    const RED: [u8; 3] = [220, 30, 40];
    const NAVY: [u8; 3] = [20, 30, 90];
    const CREAM: [u8; 3] = [250, 240, 210];

    #[test]
    fn finds_the_colours_of_a_picture() {
        let img = bands(&[(RED, 10), (NAVY, 30), (CREAM, 20)]);
        let got = dominant_colors(&img, 3, PaletteSort::Lightness);
        let got: Vec<[u8; 3]> = got.iter().map(bytes).collect();
        // Darkest first; each within rounding of the band.
        let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(&b).all(|(x, y)| x.abs_diff(*y) <= 2);
        assert_eq!(got.len(), 3);
        assert!(near(got[0], NAVY), "{got:?}");
        assert!(near(got[1], RED), "{got:?}");
        assert!(near(got[2], CREAM), "{got:?}");
        // By frequency: navy (30 rows), cream (20), red (10).
        let f: Vec<[u8; 3]> = dominant_colors(&img, 3, PaletteSort::Frequency)
            .iter()
            .map(bytes)
            .collect();
        assert!(
            near(f[0], NAVY) && near(f[1], CREAM) && near(f[2], RED),
            "{f:?}"
        );
        // By hue: the near-grey cream first? It has chroma: red (hue ≈ 0.4
        // rad) before cream (yellowish) before navy (blue).
        let h: Vec<[u8; 3]> = dominant_colors(&img, 3, PaletteSort::Hue)
            .iter()
            .map(bytes)
            .collect();
        assert!(near(h[0], RED) && near(h[2], NAVY), "{h:?}");
    }

    #[test]
    fn is_deterministic_and_handles_odd_pictures() {
        let img = bands(&[(RED, 7), (NAVY, 5), (CREAM, 9), ([10, 200, 90], 3)]);
        let a = dominant_colors(&img, 5, PaletteSort::Lightness);
        assert_eq!(a, dominant_colors(&img, 5, PaletteSort::Lightness));
        // Fewer colours than asked: one each.
        let flat = bands(&[(RED, 4)]);
        assert_eq!(dominant_colors(&flat, 6, PaletteSort::Lightness).len(), 1);
        // Transparent or empty: nothing.
        let clear = Image::new(4, 4, vec![0; 64]);
        assert!(dominant_colors(&clear, 4, PaletteSort::Lightness).is_empty());
        assert!(dominant_colors(&Image::new(0, 0, Vec::new()), 4, PaletteSort::Hue).is_empty());
        assert!(dominant_colors(&img, 0, PaletteSort::Hue).is_empty());
        // A big picture is sampled, not read in full, and still works.
        let big = Image::new(1000, 1000, [40u8, 50, 60, 255].repeat(1_000_000));
        let c = dominant_colors(&big, 3, PaletteSort::Lightness);
        assert_eq!(c.len(), 1);
        assert_eq!(bytes(&c[0]), [40, 50, 60]);
    }

    #[test]
    fn the_node_reads_stop_count_and_sort() {
        let mut n = node("color/palette", json!({"mode": "image", "stopCount": 2}));
        let img = Arc::new(bands(&[(RED, 10), (NAVY, 30)]));
        let out = n
            .eval(vec![vec![Value::Image(img)], vec![Value::str("frequency")]])
            .unwrap();
        assert_eq!(out[0].len(), 2);
        let Value::Color(first) = &out[0][0] else {
            panic!()
        };
        assert!(bytes(first)[2] > 80, "navy, the most common, first");
        // No image: no colours.
        let out = n.eval(vec![vec![Value::Null], vec![]]).unwrap();
        assert!(out[0].is_empty());
    }
}
