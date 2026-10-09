//! `gpu/effect`: post-effects on a layer (native-only, issue #37).
//!
//! The node describes the effect as a [`SceneNode::Shader`] with built-in
//! GLSL passes, so the renderer runs it exactly like a `gpu/shader` with a
//! wired input: the layer renders offscreen, the passes read it (and their
//! own buffers), the Image pass composites over the canvas. Effects that
//! move (grain, noise displacement) read the renderer's clock, so they keep
//! animating while the graph is idle.
//!
//! Pass conventions (see `floret-render`'s `shader.rs`): `fragCoord` counts
//! pixels from the bottom-left, the input channel holds the layer
//! premultiplied, and the Image pass writes straight alpha.
//!
//! The modes, in dropdown order: the original seven (bloom … vignette),
//! then blur (gaussian, lens, motion, zoom or spin, picked by `kind`),
//! drop shadow and glow of the layer's coverage (outer, or `inner`),
//! barrel / pincushion distortion, Sobel edges, and the colour tools:
//! posterize, threshold (by brightness, or by coverage for metaballs),
//! levels, curves (five colour pins = a curve per channel), a 3D LUT from
//! a `.cube` file, and chroma key. Last, `ascii`: the layer as text
//! characters picked by brightness, drawn from a glyph atlas the node
//! rasterises from the bundled monospace font (see [`glyph_atlas`]).

use super::gpu::as_layer;
use super::Registrar;
use crate::image::{Image, LoadSlot, LoadState};
use crate::node::{always_eval, raw, EvalCtx, NodeImpl, NodeInit};
use crate::scene::SceneNode;
use crate::shader::{
    parse_uniforms, uniform_port_type, ShaderBuffer, ShaderChannel, ShaderPass, ShaderShape,
    UniformDecl, UniformValue,
};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Color, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("gpu/effect", effect);
    r.dynamic("gpu/effect", effect_inputs);
}

/// Every effect's seed goes through here: the node's own seed with the
/// patch seed mixed in ([`crate::seed`]), as the float the passes read.
pub(crate) fn effect_seed(seed: f64) -> f64 {
    let seed = crate::seed::mix(seed);
    if seed.is_finite() {
        seed.floor()
    } else {
        0.0
    }
}

/// The effect modes, in dropdown order.
pub const MODES: [&str; 19] = [
    "bloom",
    "grain",
    "displace",
    "dither",
    "halftone",
    "chromatic",
    "vignette",
    "blur",
    "shadow",
    "glow",
    "barrel",
    "edges",
    "posterize",
    "threshold",
    "levels",
    "curves",
    "lut",
    "key",
    "ascii",
];

/// Blur's `kind`s, in dropdown order.
pub const BLUR_KINDS: [&str; 5] = ["gaussian", "lens", "motion", "zoom", "spin"];

/// Threshold's `channel`s, in dropdown order.
pub const THRESHOLD_CHANNELS: [&str; 2] = ["luma", "alpha"];

/// Glow's `blend`s, in dropdown order: `normal` lays the glow under the
/// layer; `add` adds it as light, over the layer too.
pub const GLOW_BLENDS: [&str; 2] = ["normal", "add"];

/// The mode's sub-choice param (a dropdown pin) and its options.
fn choice_of(mode: &str) -> Option<(&'static str, &'static [&'static str])> {
    match mode {
        "blur" => Some(("kind", &BLUR_KINDS)),
        "threshold" => Some(("channel", &THRESHOLD_CHANNELS)),
        "glow" => Some(("blend", &GLOW_BLENDS)),
        _ => None,
    }
}

fn num(name: &str, default: f64) -> PortSpec {
    PortSpec::new(name, PortType::Number).default(Value::Num(default))
}

fn int(name: &str, default: f64) -> PortSpec {
    let mut p = num(name, default);
    p.integer = true;
    p
}

fn color(name: &str, c: Color) -> PortSpec {
    PortSpec::new(name, PortType::Color).default(Value::Color(c))
}

fn grey(v: f64) -> Color {
    Color {
        r: v,
        g: v,
        b: v,
        a: 1.0,
    }
}

fn mode_of(get: impl Fn(&str) -> Option<String>) -> &'static str {
    let m = get("mode").unwrap_or_default();
    MODES.iter().find(|x| **x == m).copied().unwrap_or("bloom")
}

/// The mode's sub-choice (blur `kind`, threshold `channel`): the first
/// option when unset or unknown, "" for modes without one.
fn sub_of(mode: &str, get: impl Fn(&str) -> Option<String>) -> &'static str {
    let Some((param, options)) = choice_of(mode) else {
        return "";
    };
    let v = get(param).unwrap_or_default();
    options
        .iter()
        .find(|x| **x == v)
        .copied()
        .unwrap_or(options[0])
}

/// The mode's parameter pins, after `input` (`sub`: its sub-choice).
fn mode_ports(mode: &str, sub: &str) -> Vec<PortSpec> {
    let mut ports = match choice_of(mode) {
        Some((param, options)) => {
            let mut p = PortSpec::new(param, PortType::String).default(Value::str(options[0]));
            p.param = Some(param.into());
            vec![p]
        }
        None => Vec::new(),
    };
    ports.extend(own_ports(mode, sub));
    ports
}

fn own_ports(mode: &str, sub: &str) -> Vec<PortSpec> {
    let center = || PortSpec::new("center", PortType::Vec2).default(Value::Vec2(Vec2::default()));
    let inner = || PortSpec::new("inner", PortType::Boolean).default(Value::Bool(false));
    match mode {
        "blur" => match sub {
            "lens" => vec![num("radius", 12.0), num("highlights", 0.3)],
            "motion" => vec![num("amount", 30.0), num("angle", 0.0)],
            "zoom" => vec![num("amount", 0.2), center()],
            "spin" => vec![num("angle", 0.3), center()],
            _ => vec![num("radius", 10.0)],
        },
        "shadow" => vec![
            PortSpec::new("offset", PortType::Vec2).default(Value::Vec2(Vec2 { x: 8.0, y: 8.0 })),
            num("radius", 12.0),
            color("color", Color::BLACK),
            num("opacity", 0.6),
            inner(),
        ],
        "glow" => vec![
            num("radius", 16.0),
            color("color", Color::WHITE),
            num("intensity", 2.0),
            inner(),
        ],
        "barrel" => vec![num("amount", 0.3), num("zoom", 1.0)],
        "edges" => vec![
            num("threshold", 0.1),
            num("width", 1.0),
            color("color", Color::WHITE),
            num("keep", 0.0),
        ],
        "posterize" => vec![int("levels", 4.0)],
        "threshold" => {
            let mut v = vec![num("level", 0.5), num("softness", 0.02)];
            if sub != "alpha" {
                v.push(color("low", Color::BLACK));
                v.push(color("high", Color::WHITE));
            }
            v
        }
        "levels" => vec![
            num("inBlack", 0.0).label("in black"),
            num("inWhite", 1.0).label("in white"),
            num("gamma", 1.0),
            num("outBlack", 0.0).label("out black"),
            num("outWhite", 1.0).label("out white"),
        ],
        "curves" => vec![
            color("black", grey(0.0)),
            color("shadows", grey(0.25)),
            color("midtones", grey(0.5)),
            color("highlights", grey(0.75)),
            color("white", grey(1.0)),
        ],
        "lut" => vec![
            PortSpec::new("lut", PortType::String).default(Value::str("")),
            num("amount", 1.0),
        ],
        "key" => vec![
            color(
                "color",
                Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.0,
                    a: 1.0,
                },
            ),
            num("tolerance", 0.15),
            num("softness", 0.1),
            num("spill", 0.5),
        ],
        "ascii" => vec![
            num("size", 12.0),
            PortSpec::new("chars", PortType::String).default(Value::str(ASCII_RAMP)),
            PortSpec::new("colored", PortType::Boolean).default(Value::Bool(true)),
            color("color", Color::WHITE),
            color(
                "background",
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                },
            ),
        ],
        "grain" => vec![num("amount", 0.15), num("size", 1.5), int("seed", 0.0)],
        "displace" => vec![
            num("amount", 20.0),
            num("scale", 80.0),
            num("speed", 0.3),
            int("seed", 0.0),
            PortSpec::new("map", PortType::Layer).kind(PortKind::Spread),
        ],
        "dither" => vec![int("levels", 4.0), num("size", 2.0)],
        "halftone" => vec![num("size", 8.0), num("angle", std::f64::consts::FRAC_PI_4)],
        "chromatic" => vec![num("amount", 6.0)],
        "vignette" => vec![
            num("amount", 0.7),
            num("radius", 0.6),
            num("softness", 0.6),
            PortSpec::new("color", PortType::Color).default(Value::Color(Color::BLACK)),
        ],
        // bloom
        _ => vec![
            num("threshold", 0.6),
            num("radius", 24.0),
            num("intensity", 1.5),
        ],
    }
}

/// `input`, then the mode's pins.
fn effect_inputs(params: &Params) -> Vec<PortSpec> {
    let get = |k: &str| params.get(k).and_then(|v| v.as_str()).map(str::to_owned);
    let mode = mode_of(get);
    let mut ports = vec![PortSpec::new("input", PortType::Layer).kind(PortKind::Spread)];
    ports.extend(mode_ports(mode, sub_of(mode, get)));
    ports
}

// ─── GLSL ────────────────────────────────────────────────────────────────────

/// Helpers every pass may use.
const COMMON: &str = "
const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);
vec4 straight(vec4 c) {
  return vec4(c.a > 0.0 ? clamp(c.rgb / c.a, 0.0, 1.0) : vec3(0.0), clamp(c.a, 0.0, 1.0));
}
// Transparent outside the canvas (no edge smear).
vec4 fetch(int ch, vec2 uv) {
  if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0) return vec4(0.0);
  if (ch == 1) return texture(iChannel1, uv);
  return texture(iChannel0, uv);
}
float hash12(vec2 p) {
  vec3 q = fract(vec3(p.xyx) * 0.1031);
  q += dot(q, q.yzx + 33.33);
  return fract((q.x + q.y) * q.z);
}
float vnoise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash12(i), hash12(i + vec2(1.0, 0.0)), u.x),
             mix(hash12(i + vec2(0.0, 1.0)), hash12(i + vec2(1.0, 1.0)), u.x), u.y);
}
// A per-seed offset into noise space.
vec2 seeded(float seed, float salt) {
  return 512.0 * vec2(hash12(vec2(seed, salt)), hash12(vec2(salt, seed + 7.0)));
}
";

/// A separable Gaussian along `dir`, `radius` ≈ 3σ, at most 16 taps a side
/// (wider radii step further and lean on linear filtering). `SAMPLE(uv)`
/// is the pass's own source.
const BLUR: &str = "
uniform float radius;
vec4 blur(vec2 fc, vec2 dir) {
  float r = max(radius, 0.0);
  float sigma = max(r / 3.0, 0.5);
  float stp = max(1.0, r / 16.0);
  int n = int(ceil(r / stp));
  vec4 acc = vec4(0.0);
  float ws = 0.0;
  for (int i = -16; i <= 16; i++) {
    if (i < -n || i > n) continue;
    float d = float(i) * stp;
    float w = exp(-0.5 * d * d / (sigma * sigma));
    acc += SAMPLE((fc + dir * d) / iResolution.xy) * w;
    ws += w;
  }
  return acc / ws;
}
";

const BLOOM_A: &str = "
uniform float threshold;
vec4 SAMPLE(vec2 uv) {
  vec4 c = fetch(0, uv);
  float l = dot(c.rgb, LUMA);
  return c * smoothstep(threshold, threshold + 0.2, l);
}
#BLUR
void mainImage(out vec4 o, in vec2 fc) { o = blur(fc, vec2(1.0, 0.0)); }
";

const BLOOM_B: &str = "
vec4 SAMPLE(vec2 uv) { return fetch(0, uv); }
#BLUR
void mainImage(out vec4 o, in vec2 fc) { o = blur(fc, vec2(0.0, 1.0)); }
";

const BLOOM_IMAGE: &str = "
uniform float intensity;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 uv = fc / iResolution.xy;
  vec4 c = fetch(0, uv);
  vec3 rgb = min(c.rgb + fetch(1, uv).rgb * max(intensity, 0.0), vec3(1.0));
  // Light adds; over transparent the glow takes its brightest channel as
  // coverage.
  float a = max(c.a, max(rgb.r, max(rgb.g, rgb.b)));
  o = straight(vec4(rgb, a));
}
";

const GRAIN: &str = "
uniform float amount;
uniform float size;
uniform float seed;
void mainImage(out vec4 o, in vec2 fc) {
  vec4 c = fetch(0, fc / iResolution.xy);
  float frame = float(iFrame % 4096);
  vec2 p = fc / max(size, 0.5) + seeded(seed, frame);
  float n = vnoise(p) + 0.5 * vnoise(p * 2.03 + 17.0);
  n = n / 1.5 - 0.5;
  vec3 rgb = clamp(c.rgb + n * 2.0 * amount * c.a, 0.0, c.a);
  o = straight(vec4(rgb, c.a));
}
";

const DISPLACE_NOISE: &str = "
uniform float amount;
uniform float scale;
uniform float speed;
uniform float seed;
float fbm(vec2 p) { return (vnoise(p) + 0.5 * vnoise(p * 2.03 + 11.0)) / 1.5; }
void mainImage(out vec4 o, in vec2 fc) {
  vec2 p = fc / max(scale, 1.0) + seeded(seed, 3.0);
  float t = iTime * speed;
  vec2 d = vec2(fbm(p + t * vec2(1.0, 0.7)), fbm(p + vec2(31.4, 17.7) - t * vec2(0.6, 1.0)));
  d = d * 2.0 - 1.0;
  o = straight(fetch(0, (fc + d * amount) / iResolution.xy));
}
";

const DISPLACE_MAP: &str = "
uniform float amount;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 uv = fc / iResolution.xy;
  vec4 m = fetch(1, uv);
  // Red pushes right, green pushes down (0.5 = still); transparent = still.
  vec2 d = (straight(m).rg - 0.5) * 2.0 * m.a;
  o = straight(fetch(0, (fc - vec2(d.x, -d.y) * amount) / iResolution.xy));
}
";

const DITHER: &str = "
uniform float levels;
uniform float size;
float bayer2(vec2 a) { a = floor(a); return fract(a.x / 2.0 + a.y * a.y * 0.75); }
float bayer4(vec2 a) { return bayer2(0.5 * a) * 0.25 + bayer2(a); }
float bayer8(vec2 a) { return bayer4(0.5 * a) * 0.25 + bayer2(a); }
void mainImage(out vec4 o, in vec2 fc) {
  float s = max(floor(size), 1.0);
  vec2 cell = floor(fc / s);
  vec4 c = straight(fetch(0, (cell + 0.5) * s / iResolution.xy));
  float l = max(floor(levels), 2.0) - 1.0;
  float b = bayer8(cell);
  o = vec4(floor(c.rgb * l + b) / l, floor(c.a * l + b) / l);
}
";

const HALFTONE: &str = "
uniform float size;
uniform float angle;
void mainImage(out vec4 o, in vec2 fc) {
  float s = max(size, 2.0);
  float cs = cos(angle);
  float sn = sin(angle);
  vec2 p = vec2(cs * fc.x + sn * fc.y, -sn * fc.x + cs * fc.y);
  vec2 cell = (floor(p / s) + 0.5) * s;
  vec2 centre = vec2(cs * cell.x - sn * cell.y, sn * cell.x + cs * cell.y);
  vec4 c = straight(fetch(0, centre / iResolution.xy));
  // Dot area follows brightness × coverage.
  float v = clamp(dot(c.rgb, LUMA) * c.a, 0.0, 1.0);
  float r = s * sqrt(v / 3.14159265);
  float k = clamp(r - length(p - cell), 0.0, 1.0);
  o = vec4(c.rgb, k);
}
";

const CHROMATIC: &str = "
uniform float amount;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 res = iResolution.xy;
  vec2 uv = fc / res;
  // Radial: 0 at the centre, `amount` pixels at the frame's far edge; red
  // spreads outwards, blue inwards.
  vec2 off = (fc - 0.5 * res) / (0.5 * max(res.x, res.y)) * amount / res;
  vec4 r = fetch(0, uv - off);
  vec4 g = fetch(0, uv);
  vec4 b = fetch(0, uv + off);
  o = straight(vec4(r.r, g.g, b.b, max(max(r.a, g.a), b.a)));
}
";

const VIGNETTE: &str = "
uniform float amount;
uniform float radius;
uniform float softness;
uniform vec4 color;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 uv = fc / iResolution.xy;
  // 0 at the centre, 1 in the corners.
  float d = length((uv - 0.5) * 2.0) / 1.41421356;
  float s = max(softness, 0.001) * 0.5;
  float v = smoothstep(radius - s, radius + s, d) * clamp(amount, 0.0, 1.0) * clamp(color.a, 0.0, 1.0);
  vec4 c = fetch(0, uv);
  o = straight(vec4(color.rgb * v + c.rgb * (1.0 - v), v + c.a * (1.0 - v)));
}
";

// ─── Blur ────────────────────────────────────────────────────────────────────

/// Gaussian, horizontal half (the vertical half is `BLOOM_B`).
const BLUR_A: &str = "
vec4 SAMPLE(vec2 uv) { return fetch(0, uv); }
#BLUR
void mainImage(out vec4 o, in vec2 fc) { o = blur(fc, vec2(1.0, 0.0)); }
";

/// The last buffer, as the output.
const COPY: &str = "
void mainImage(out vec4 o, in vec2 fc) { o = straight(fetch(0, fc / iResolution.xy)); }
";

/// A disc kernel: taps on a golden-angle spiral (evenly spread over the
/// disc, turned per pixel so the pattern reads as fine grain, not rings).
/// `highlights` weights bright taps up, so points of light open into
/// discs, as through a real lens.
const LENS_BLUR: &str = "
uniform float radius;
uniform float highlights;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 res = iResolution.xy;
  float r = max(radius, 0.0);
  if (r < 0.5) { o = straight(fetch(0, fc / res)); return; }
  int n = int(clamp(r * r * 0.4, 24.0, 256.0));
  float spin = hash12(fc) * 6.2831853;
  float boost = max(highlights, 0.0) * 16.0;
  vec4 acc = vec4(0.0);
  float ws = 0.0;
  for (int i = 0; i < 256; i++) {
    if (i >= n) break;
    float fi = float(i) + 0.5;
    float th = fi * 2.3999632 + spin;
    vec2 d = r * sqrt(fi / float(n)) * vec2(cos(th), sin(th));
    vec4 c = fetch(0, (fc + d) / res);
    float l = dot(c.rgb, LUMA);
    float w = 1.0 + boost * l * l * l * l;
    acc += c * w;
    ws += w;
  }
  o = straight(acc / ws);
}
";

/// Directional: `amount` px along `angle` (radians, y down like the
/// canvas), centred on the pixel.
const MOTION_BLUR: &str = "
uniform float amount;
uniform float angle;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 res = iResolution.xy;
  float l = abs(amount);
  int n = int(clamp(ceil(l), 1.0, 64.0));
  vec2 dir = vec2(cos(angle), -sin(angle));
  vec4 acc = vec4(0.0);
  for (int i = 0; i < 64; i++) {
    if (i >= n) break;
    float t = n == 1 ? 0.0 : float(i) / float(n - 1) - 0.5;
    acc += fetch(0, (fc + dir * t * l) / res);
  }
  o = straight(acc / float(n));
}
";

/// The canvas point `center` (centre origin, y down) as a fragCoord.
const CENTER: &str = "
uniform vec2 center;
vec2 centre() { return vec2(center.x, -center.y) + 0.5 * iResolution.xy; }
";

/// Radial streaks towards `center`: each pixel averages the line from it
/// `amount` of the way to the centre.
const ZOOM_BLUR: &str = "
uniform float amount;
#CENTER
void mainImage(out vec4 o, in vec2 fc) {
  vec2 res = iResolution.xy;
  vec2 c = centre();
  vec2 d = fc - c;
  float a = clamp(amount, -1.0, 1.0);
  int n = int(clamp(ceil(length(d) * abs(a)), 1.0, 64.0));
  vec4 acc = vec4(0.0);
  for (int i = 0; i < 64; i++) {
    if (i >= n) break;
    float t = n == 1 ? 0.0 : float(i) / float(n - 1);
    acc += fetch(0, (c + d * (1.0 - a * t)) / res);
  }
  o = straight(acc / float(n));
}
";

/// Circular streaks round `center`, `angle` radians long.
const SPIN_BLUR: &str = "
uniform float angle;
#CENTER
void mainImage(out vec4 o, in vec2 fc) {
  vec2 res = iResolution.xy;
  vec2 c = centre();
  vec2 d = fc - c;
  int n = int(clamp(ceil(length(d) * abs(angle)), 1.0, 64.0));
  vec4 acc = vec4(0.0);
  for (int i = 0; i < 64; i++) {
    if (i >= n) break;
    float t = n == 1 ? 0.0 : float(i) / float(n - 1) - 0.5;
    float a = angle * t;
    float cs = cos(a);
    float sn = sin(a);
    acc += fetch(0, (c + vec2(cs * d.x - sn * d.y, sn * d.x + cs * d.y)) / res);
  }
  o = straight(acc / float(n));
}
";

// ─── Shadow / glow ───────────────────────────────────────────────────────────

/// The layer's coverage (its inverse for `inner`), moved by `#SHIFT`,
/// blurred horizontally; `BLOOM_B` blurs it vertically.
const ALPHA_A: &str = "
uniform float inner;
#SHIFT
vec4 SAMPLE(vec2 uv) {
  float a = fetch(0, uv - shift() / iResolution.xy).a;
  return vec4(inner > 0.5 ? 1.0 - a : a);
}
#BLUR
void mainImage(out vec4 o, in vec2 fc) { o = blur(fc, vec2(1.0, 0.0)); }
";

const SHIFT_OFFSET: &str = "uniform vec2 offset;
vec2 shift() { return vec2(offset.x, -offset.y); }";

const SHIFT_NONE: &str = "vec2 shift() { return vec2(0.0); }";

/// `s` (the blurred coverage, scaled) in `color`: under the layer, or for
/// `inner` on top of it inside its own coverage (source-atop).
const ALPHA_COMPOSITE: &str = "
uniform float inner;
uniform vec4 color;
vec4 composite(vec4 c, float s) {
  s = clamp(s, 0.0, 1.0) * clamp(color.a, 0.0, 1.0);
  if (inner > 0.5) {
    return straight(vec4(color.rgb * s * c.a + c.rgb * (1.0 - s), c.a));
  }
  return straight(c + vec4(color.rgb * s, s) * (1.0 - c.a));
}
";

const SHADOW_IMAGE: &str = "
uniform float opacity;
#COMPOSITE
void mainImage(out vec4 o, in vec2 fc) {
  vec2 uv = fc / iResolution.xy;
  o = composite(fetch(0, uv), fetch(1, uv).a * clamp(opacity, 0.0, 1.0));
}
";

const GLOW_IMAGE: &str = "
uniform float intensity;
#COMPOSITE
void mainImage(out vec4 o, in vec2 fc) {
  vec2 uv = fc / iResolution.xy;
  o = composite(fetch(0, uv), fetch(1, uv).a * max(intensity, 0.0));
}
";

/// Glow `blend` add: the glow alone (`color` at the blurred coverage),
/// which the node lays over the layer with additive blending, so it
/// lights the layer and whatever lies under the halo instead of covering
/// them. `inner` keeps it inside the layer's coverage.
const GLOW_ADD_IMAGE: &str = "
uniform float intensity;
uniform float inner;
uniform vec4 color;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 uv = fc / iResolution.xy;
  float s = fetch(1, uv).a * max(intensity, 0.0) * clamp(color.a, 0.0, 1.0);
  if (inner > 0.5) s *= fetch(0, uv).a;
  o = vec4(clamp(color.rgb, 0.0, 1.0), clamp(s, 0.0, 1.0));
}
";

// ─── Distort / detect ────────────────────────────────────────────────────────

/// Radial lens distortion: positive `amount` bulges (barrel), negative
/// pinches (pincushion); `zoom` > 1 crops in to hide the edges.
const BARREL: &str = "
uniform float amount;
uniform float zoom;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 res = iResolution.xy;
  float hs = 0.5 * max(res.x, res.y);
  vec2 p = (fc - 0.5 * res) / hs;
  vec2 q = p * (1.0 + amount * dot(p, p)) / max(zoom, 0.01);
  o = straight(fetch(0, (q * hs + 0.5 * res) / res));
}
";

/// Sobel on brightness and coverage, `width` px apart: edges in `color`
/// over `keep` × the layer.
const EDGES: &str = "
uniform float threshold;
uniform float width;
uniform vec4 color;
uniform float keep;
vec2 val(vec2 fc) {
  vec4 c = fetch(0, fc / iResolution.xy);
  return vec2(dot(c.rgb, LUMA), c.a);
}
void mainImage(out vec4 o, in vec2 fc) {
  float w = max(width, 0.5);
  vec2 tl = val(fc + w * vec2(-1.0, 1.0));
  vec2 tc = val(fc + w * vec2(0.0, 1.0));
  vec2 tr = val(fc + w * vec2(1.0, 1.0));
  vec2 ml = val(fc + w * vec2(-1.0, 0.0));
  vec2 mr = val(fc + w * vec2(1.0, 0.0));
  vec2 bl = val(fc + w * vec2(-1.0, -1.0));
  vec2 bc = val(fc + w * vec2(0.0, -1.0));
  vec2 br = val(fc + w * vec2(1.0, -1.0));
  vec2 gx = (tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl);
  vec2 gy = (tl + 2.0 * tc + tr) - (bl + 2.0 * bc + br);
  // A hard 0 → 1 step reads 1.
  float g = max(length(vec2(gx.x, gy.x)), length(vec2(gx.y, gy.y))) * 0.25;
  float e = smoothstep(threshold, threshold + 0.1, g) * clamp(color.a, 0.0, 1.0);
  vec4 base = fetch(0, fc / iResolution.xy) * clamp(keep, 0.0, 1.0);
  o = straight(vec4(color.rgb * e, e) + base * (1.0 - e));
}
";

// ─── Colour ──────────────────────────────────────────────────────────────────

const POSTERIZE: &str = "
uniform float levels;
void mainImage(out vec4 o, in vec2 fc) {
  vec4 c = straight(fetch(0, fc / iResolution.xy));
  float l = max(floor(levels), 2.0) - 1.0;
  o = vec4(floor(c.rgb * l + 0.5) / l, c.a);
}
";

/// Brightness against `level`, `softness` wide: `low` below, `high` above
/// (`channel` luma), or the coverage itself cut there (`channel` alpha —
/// blur first for gooey metaballs).
const THRESHOLD_LUMA: &str = "
uniform float level;
uniform float softness;
uniform vec4 low;
uniform vec4 high;
void mainImage(out vec4 o, in vec2 fc) {
  vec4 c = straight(fetch(0, fc / iResolution.xy));
  float s = max(softness, 0.0) * 0.5 + 1e-4;
  float t = smoothstep(level - s, level + s, dot(c.rgb, LUMA));
  vec4 k = mix(low, high, t);
  o = vec4(k.rgb, c.a * clamp(k.a, 0.0, 1.0));
}
";

const THRESHOLD_ALPHA: &str = "
uniform float level;
uniform float softness;
void mainImage(out vec4 o, in vec2 fc) {
  vec4 c = straight(fetch(0, fc / iResolution.xy));
  float s = max(softness, 0.0) * 0.5 + 1e-4;
  o = vec4(c.rgb, smoothstep(level - s, level + s, c.a));
}
";

/// Input black / white points, gamma (> 1 brightens the mids), output
/// black / white points, on every colour channel.
const LEVELS: &str = "
uniform float inBlack;
uniform float inWhite;
uniform float gamma;
uniform float outBlack;
uniform float outWhite;
void mainImage(out vec4 o, in vec2 fc) {
  vec4 c = straight(fetch(0, fc / iResolution.xy));
  float span = inWhite - inBlack;
  span = abs(span) < 1e-5 ? 1e-5 : span;
  vec3 v = clamp((c.rgb - inBlack) / span, 0.0, 1.0);
  v = pow(v, vec3(1.0 / max(gamma, 0.01)));
  o = vec4(clamp(mix(vec3(outBlack), vec3(outWhite), v), 0.0, 1.0), c.a);
}
";

/// A smooth curve per channel through five points at 0, ¼, ½, ¾ and 1:
/// the colours `black` … `white` give each point's red, green and blue
/// (defaults: a straight line). Monotone cubic (Fritsch–Butland slopes):
/// no overshoot between points.
const CURVES: &str = "
uniform vec4 black;
uniform vec4 shadows;
uniform vec4 midtones;
uniform vec4 highlights;
uniform vec4 white;
float slope(float a, float b) { return a * b > 0.0 ? 2.0 * a * b / (a + b) : 0.0; }
float curve(float x, float y0, float y1, float y2, float y3, float y4) {
  float d0 = (y1 - y0) * 4.0;
  float d1 = (y2 - y1) * 4.0;
  float d2 = (y3 - y2) * 4.0;
  float d3 = (y4 - y3) * 4.0;
  float m1 = slope(d0, d1);
  float m2 = slope(d1, d2);
  float m3 = slope(d2, d3);
  x = clamp(x, 0.0, 1.0);
  float k = min(floor(x * 4.0), 3.0);
  float t = x * 4.0 - k;
  float ya = y0, yb = y1, ma = d0, mb = m1;
  if (k == 1.0) { ya = y1; yb = y2; ma = m1; mb = m2; }
  if (k == 2.0) { ya = y2; yb = y3; ma = m2; mb = m3; }
  if (k == 3.0) { ya = y3; yb = y4; ma = m3; mb = d3; }
  float t2 = t * t;
  float t3 = t2 * t;
  return (2.0 * t3 - 3.0 * t2 + 1.0) * ya + (t3 - 2.0 * t2 + t) * 0.25 * ma
    + (-2.0 * t3 + 3.0 * t2) * yb + (t3 - t2) * 0.25 * mb;
}
void mainImage(out vec4 o, in vec2 fc) {
  vec4 c = straight(fetch(0, fc / iResolution.xy));
  vec3 v = vec3(
    curve(c.r, black.r, shadows.r, midtones.r, highlights.r, white.r),
    curve(c.g, black.g, shadows.g, midtones.g, highlights.g, white.g),
    curve(c.b, black.b, shadows.b, midtones.b, highlights.b, white.b));
  o = vec4(clamp(v, 0.0, 1.0), c.a);
}
";

/// A 3D lookup table on channel 1: N slices (one per blue step) of N × N
/// (red across, green down), side by side. Trilinear, mixed in by
/// `amount`.
const LUT: &str = "
uniform float amount;
vec3 lut(vec3 c) {
  vec2 size = vec2(textureSize(iChannel1, 0));
  float n = size.y;
  vec3 p = clamp(c, 0.0, 1.0) * (n - 1.0);
  float b0 = floor(p.z);
  float b1 = min(b0 + 1.0, n - 1.0);
  vec2 rg = (p.xy + 0.5);
  vec3 a = texture(iChannel1, vec2(b0 * n + rg.x, rg.y) / size).rgb;
  vec3 b = texture(iChannel1, vec2(b1 * n + rg.x, rg.y) / size).rgb;
  return mix(a, b, p.z - b0);
}
void mainImage(out vec4 o, in vec2 fc) {
  vec4 c = straight(fetch(0, fc / iResolution.xy));
  o = vec4(mix(c.rgb, lut(c.rgb), clamp(amount, 0.0, 1.0)), c.a);
}
";

/// ASCII art: the canvas in cells `size` pixels tall and half as wide;
/// each cell's mean brightness (on black) picks a character from the
/// atlas in channel 1 (the characters side by side, darkest first, each
/// tile half as wide as it is tall), coloured from the cell or `color`,
/// over `background`.
const ASCII: &str = "
uniform float size;
uniform float colored;
uniform vec4 color;
uniform vec4 background;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 cell = vec2(0.5, 1.0) * max(size, 2.0);
  vec2 c0 = floor(fc / cell) * cell;
  vec4 acc = vec4(0.0);
  for (int j = 0; j < 4; j++) {
    for (int i = 0; i < 4; i++) {
      vec2 p = c0 + (vec2(float(i), float(j)) + 0.5) * cell / 4.0;
      acc += fetch(0, p / iResolution.xy);
    }
  }
  acc /= 16.0;
  vec2 tex = vec2(textureSize(iChannel1, 0));
  float n = max(floor(tex.x / (tex.y * 0.5) + 0.5), 1.0);
  float l = clamp(dot(acc.rgb, LUMA), 0.0, 1.0);
  float k = min(floor(l * n), n - 1.0);
  vec2 f = clamp(fract(fc / cell), vec2(0.5) / (tex.y * vec2(0.5, 1.0)),
                 vec2(1.0) - vec2(0.5) / (tex.y * vec2(0.5, 1.0)));
  float cov = texture(iChannel1, vec2((k + f.x) / n, 1.0 - f.y)).a;
  vec3 fg = colored > 0.5 ? (acc.a > 0.0 ? clamp(acc.rgb / acc.a, 0.0, 1.0) : vec3(0.0))
                          : color.rgb;
  float fa = cov * (colored > 0.5 ? 1.0 : color.a);
  float a = fa + background.a * (1.0 - fa);
  vec3 rgb = fg * fa + background.rgb * background.a * (1.0 - fa);
  o = vec4(a > 0.0 ? rgb / a : vec3(0.0), a);
}
";

/// Chroma key: pixels whose chroma (YCbCr) is within `tolerance` of the key
/// `color` go transparent, fading over `softness`; `spill` takes the key's
/// tint out of what's left (green fringes on hair).
const KEY: &str = "
uniform vec4 color;
uniform float tolerance;
uniform float softness;
uniform float spill;
vec2 chroma(vec3 c) {
  return vec2(-0.168736 * c.r - 0.331264 * c.g + 0.5 * c.b,
              0.5 * c.r - 0.418688 * c.g - 0.081312 * c.b);
}
void mainImage(out vec4 o, in vec2 fc) {
  vec4 c = straight(fetch(0, fc / iResolution.xy));
  vec2 k = chroma(color.rgb);
  vec2 p = chroma(c.rgb);
  float m = smoothstep(tolerance, tolerance + max(softness, 1e-4), distance(p, k));
  vec2 kn = length(k) > 1e-4 ? normalize(k) : vec2(0.0);
  p -= kn * max(dot(p, kn), 0.0) * clamp(spill, 0.0, 1.0);
  float y = dot(c.rgb, vec3(0.299, 0.587, 0.114));
  vec3 rgb = vec3(y + 1.402 * p.y, y - 0.344136 * p.x - 0.714136 * p.y, y + 1.772 * p.x);
  o = vec4(clamp(rgb, 0.0, 1.0), c.a * m);
}
";

fn source(body: &str) -> Arc<str> {
    let body = body
        .replace("#BLUR", BLUR)
        .replace("#CENTER", CENTER)
        .replace("#COMPOSITE", ALPHA_COMPOSITE);
    Arc::from(format!("{COMMON}{body}"))
}

/// The passes for `mode` at its default sub-choice (`map`: a displacement
/// layer is wired).
pub fn passes(mode: &str, map: bool) -> Vec<ShaderPass> {
    let sub = sub_of(mode, |_| None);
    mode_passes(mode, sub, map, true)
}

/// The passes for `mode` with sub-choice `sub` (`map`: a displacement
/// layer is wired; `lut`: a lookup table is loaded).
pub fn mode_passes(mode: &str, sub: &str, map: bool, lut: bool) -> Vec<ShaderPass> {
    use ShaderChannel as C;
    let image = |body: &str, channels| ShaderPass {
        buf: ShaderBuffer::Image,
        source: source(body),
        channels,
    };
    let input = [C::Input, C::None, C::None, C::None];
    let pass = |buf, body: &str, ch0| ShaderPass {
        buf,
        source: source(body),
        channels: [ch0, C::None, C::None, C::None],
    };
    // Coverage (shifted for a shadow) blurred in A then B, composited over
    // or into the input.
    let coverage = |shift: &str, body: &str| {
        vec![
            pass(ShaderBuffer::A, &ALPHA_A.replace("#SHIFT", shift), C::Input),
            pass(ShaderBuffer::B, BLOOM_B, C::Buffer(ShaderBuffer::A)),
            image(
                body,
                [C::Input, C::Buffer(ShaderBuffer::B), C::None, C::None],
            ),
        ]
    };
    match mode {
        "blur" => match sub {
            "lens" => vec![image(LENS_BLUR, input)],
            "motion" => vec![image(MOTION_BLUR, input)],
            "zoom" => vec![image(ZOOM_BLUR, input)],
            "spin" => vec![image(SPIN_BLUR, input)],
            _ => vec![
                pass(ShaderBuffer::A, BLUR_A, C::Input),
                pass(ShaderBuffer::B, BLOOM_B, C::Buffer(ShaderBuffer::A)),
                image(
                    COPY,
                    [C::Buffer(ShaderBuffer::B), C::None, C::None, C::None],
                ),
            ],
        },
        "shadow" => coverage(SHIFT_OFFSET, SHADOW_IMAGE),
        "glow" if sub == "add" => coverage(SHIFT_NONE, GLOW_ADD_IMAGE),
        "glow" => coverage(SHIFT_NONE, GLOW_IMAGE),
        "barrel" => vec![image(BARREL, input)],
        "edges" => vec![image(EDGES, input)],
        "posterize" => vec![image(POSTERIZE, input)],
        "threshold" if sub == "alpha" => vec![image(THRESHOLD_ALPHA, input)],
        "threshold" => vec![image(THRESHOLD_LUMA, input)],
        "levels" => vec![image(LEVELS, input)],
        "curves" => vec![image(CURVES, input)],
        "lut" if lut => vec![image(LUT, [C::Input, C::Lut, C::None, C::None])],
        // No table (yet): the layer as it is.
        "lut" => vec![image(COPY, input)],
        "key" => vec![image(KEY, input)],
        "ascii" if lut => vec![image(ASCII, [C::Input, C::Lut, C::None, C::None])],
        // No characters (no font): the layer as it is.
        "ascii" => vec![image(COPY, input)],
        "grain" => vec![image(GRAIN, input)],
        "displace" if map => vec![image(DISPLACE_MAP, [C::Input, C::Map, C::None, C::None])],
        "displace" => vec![image(DISPLACE_NOISE, input)],
        "dither" => vec![image(DITHER, input)],
        "halftone" => vec![image(HALFTONE, input)],
        "chromatic" => vec![image(CHROMATIC, input)],
        "vignette" => vec![image(VIGNETTE, input)],
        _ => vec![
            ShaderPass {
                buf: ShaderBuffer::A,
                source: source(BLOOM_A),
                channels: input,
            },
            ShaderPass {
                buf: ShaderBuffer::B,
                source: source(BLOOM_B),
                channels: [C::Buffer(ShaderBuffer::A), C::None, C::None, C::None],
            },
            image(
                BLOOM_IMAGE,
                [C::Input, C::Buffer(ShaderBuffer::B), C::None, C::None],
            ),
        ],
    }
}

/// Slot 0 of a number / colour pin as a uniform (unwired → the default).
fn uniform(port: &PortSpec, v: Option<&Value>) -> UniformValue {
    let v = match v {
        Some(v) if !v.is_nullish() => v.clone(),
        _ => port.default.clone().unwrap_or(Value::Num(0.0)),
    };
    match port.ty {
        PortType::Color => {
            let c = super::shared::to_color(&v);
            UniformValue::Color([c.r, c.g, c.b, c.a])
        }
        PortType::Vec2 => {
            let p = super::shared::to_vec2(&v);
            let f = |n: f64| if n.is_finite() { n } else { 0.0 };
            UniformValue::Vec2([f(p.x), f(p.y)])
        }
        _ => {
            let n = v.to_number();
            let n = if port.name == "seed" {
                effect_seed(n)
            } else {
                n
            };
            UniformValue::Number(if n.is_finite() { n } else { 0.0 })
        }
    }
}

fn effect(init: &NodeInit) -> Box<dyn NodeImpl> {
    let lut_mode = init.params.get("mode").and_then(|v| v.as_str()) == Some("lut");
    let mut slot: LoadSlot<Arc<Image>> = LoadSlot::default();
    // The ASCII mode's atlas, for the characters it was made from.
    let mut atlas: Option<(String, Option<Arc<Image>>)> = None;
    let node = raw(move |inputs, cx: &mut EvalCtx| {
        let get = |k: &str| cx.param(k).and_then(|v| v.as_str()).map(str::to_owned);
        let mode = mode_of(get);
        let mut sub = sub_of(mode, get);
        let ports = mode_ports(mode, sub);
        let pin = |name: &str| {
            ports
                .iter()
                .position(|p| p.name == name)
                .and_then(|i| inputs.get(i + 1))
                .and_then(|s| s.first())
        };
        // A wired / typed sub-choice wins over the saved param.
        if let (Some((_, options)), Some(Value::Str(s))) = (
            choice_of(mode),
            pin("kind").or(pin("channel")).or(pin("blend")),
        ) {
            if let Some(o) = options.iter().find(|o| **o == &**s) {
                sub = o;
            }
        }
        let map_port = ports.iter().position(|p| p.ty == PortType::Layer);
        let map = map_port.and_then(|i| as_layer(inputs.get(i + 1).map(|s| &s[..])));
        let lut = match mode {
            "lut" => load_lut(&mut slot, pin("lut"), cx),
            "ascii" => {
                let chars = match pin("chars") {
                    Some(Value::Str(s)) if !s.is_empty() => s.to_string(),
                    _ => ASCII_RAMP.to_owned(),
                };
                match &atlas {
                    Some((c, img)) if *c == chars => img.clone(),
                    _ => {
                        let img = glyph_atlas(&chars).map(Arc::new);
                        atlas = Some((chars, img.clone()));
                        img
                    }
                }
            }
            _ => None,
        };
        let passes = mode_passes(mode, sub, map.is_some(), lut.is_some());
        let mut decls: Vec<UniformDecl> = Vec::new();
        for p in &passes {
            for u in parse_uniforms(&p.source) {
                if !decls.iter().any(|d| d.name == u.name) {
                    decls.push(u);
                }
            }
        }
        let uniforms: Vec<UniformValue> = decls
            .iter()
            .map(|d| match ports.iter().position(|p| *p.name == *d.name) {
                Some(i) => uniform(&ports[i], inputs.get(i + 1).and_then(|s| s.first())),
                None => match uniform_port_type(&d.glsl_type) {
                    PortType::Color => UniformValue::Color([0.0; 4]),
                    PortType::Vec2 => UniformValue::Vec2([0.0; 2]),
                    _ => UniformValue::Number(0.0),
                },
            })
            .collect();
        let input = as_layer(inputs.first().map(|s| &s[..]));
        let shape = ShaderShape {
            node_id: Arc::from(cx.node_id),
            passes: Arc::from(passes),
            uniform_decls: Arc::from(decls),
            uniforms: Arc::from(uniforms),
            input: input.clone(),
            map,
            history: None,
            lut,
        };
        let shader = Arc::new(SceneNode::Shader(Arc::new(shape)));
        let out = match (mode, sub, input) {
            // Additive glow: the layer, then the glow added over it.
            ("glow", "add", Some(layer)) => Arc::new(SceneNode::group(vec![
                layer,
                Arc::new(SceneNode::Group {
                    children: Arc::from(vec![shader]),
                    transform: None,
                    blend_mode: Some(Arc::from("lighter")),
                    clip: None,
                }),
            ])),
            _ => shader,
        };
        Ok(vec![one(Value::Scene(out))])
    });
    // The table loads off the tick: poll it every frame (like the file
    // nodes). Every other mode is memoised as before.
    if lut_mode {
        always_eval(node)
    } else {
        node
    }
}

/// The table named on the `lut` pin (a path, URL or `data:` URL such as
/// a File node's `url`), once loaded.
fn load_lut(
    slot: &mut LoadSlot<Arc<Image>>,
    src: Option<&Value>,
    cx: &mut EvalCtx,
) -> Option<Arc<Image>> {
    let src = match src {
        Some(Value::Str(s)) => s.trim().to_owned(),
        _ => String::new(),
    };
    if src.is_empty() {
        slot.clear();
        return None;
    }
    match slot.ensure(&src, cx.host(), parse_lut_file).state() {
        LoadState::Ready(img) => Some(img),
        LoadState::Failed(e) => {
            cx.warn(format!("Couldn't load the LUT: {e}"));
            None
        }
        LoadState::Loading => None,
    }
}

// ─── ASCII ───────────────────────────────────────────────────────────────────

/// The ASCII mode's default characters, darkest (emptiest) first.
pub const ASCII_RAMP: &str = " .:-=+*#%@";
/// An atlas tile's height in pixels (its width is half that).
pub const GLYPH_TILE: u32 = 48;
/// Most characters an atlas holds.
pub const MAX_GLYPHS: usize = 128;

/// The characters of `chars` (at most [`MAX_GLYPHS`]) rasterised from the
/// bundled monospace font, side by side in tiles [`GLYPH_TILE`] tall and
/// half as wide: white, coverage in alpha. `None` without the font or
/// characters.
pub fn glyph_atlas(chars: &str) -> Option<Image> {
    use skrifa::MetadataProvider;
    let chars: Vec<char> = chars.chars().take(MAX_GLYPHS).collect();
    if chars.is_empty() {
        return None;
    }
    let face = crate::text::face(crate::text::resolve("monospace", 400, false)?)?;
    let font = face.font()?;
    let charmap = font.charmap();
    let (th, tw) = (GLYPH_TILE as usize, GLYPH_TILE as usize / 2);
    // About 0.6 em wide: 35 px type fits a 24 × 48 tile with a margin.
    let size = th as f64 * 0.74;
    let baseline = th as f64 * 0.74;
    let w = tw * chars.len();
    let mut rgba = vec![0u8; w * th * 4];
    for (k, c) in chars.iter().enumerate() {
        let Some(gid) = charmap.map(*c) else { continue };
        let Ok(id) = u16::try_from(gid.to_u32()) else {
            continue;
        };
        let contours = crate::text::outline::glyph_contours(&face, id, size, &[]);
        if contours.is_empty() {
            continue;
        }
        let Some((lo, hi)) = crate::text::outline::bounds(&contours) else {
            continue;
        };
        // Centred across the tile (monospace glyphs sit in their advance).
        let dx = (tw as f64 - (hi.x - lo.x)) / 2.0 - lo.x;
        for y in 0..th {
            for x in 0..tw {
                // 4 × 4 samples a pixel.
                let mut hits = 0;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let p = crate::value::Vec2 {
                            x: x as f64 + (sx as f64 + 0.5) / 4.0 - dx,
                            y: y as f64 + (sy as f64 + 0.5) / 4.0 - baseline,
                        };
                        if crate::text::outline::winding(&contours, p) != 0 {
                            hits += 1;
                        }
                    }
                }
                if hits > 0 {
                    let o = (y * w + k * tw + x) * 4;
                    rgba[o..o + 4].copy_from_slice(&[255, 255, 255, (hits * 255 / 16) as u8]);
                }
            }
        }
    }
    Some(Image::new(w as u32, th as u32, rgba))
}

// ─── .cube lookup tables ─────────────────────────────────────────────────────

/// The largest table edge kept; bigger tables are resampled to it (a
/// 64-step table is finer than 8-bit colour can show).
pub const LUT_MAX: usize = 64;

/// A 1D table becomes a 3D one this fine.
const LUT_FROM_1D: usize = 33;

fn parse_lut_file(bytes: Vec<u8>, _: Option<&std::path::Path>) -> Result<Arc<Image>, String> {
    let text = String::from_utf8_lossy(&bytes);
    parse_cube(&text).map(Arc::new)
}

/// A colour table: `n` steps a side, red fastest, then green, then blue.
struct Cube {
    n: usize,
    data: Vec<[f64; 3]>,
}

impl Cube {
    fn at(&self, r: usize, g: usize, b: usize) -> [f64; 3] {
        self.data[r + self.n * (g + self.n * b)]
    }

    /// Trilinear lookup at (r, g, b) in table steps (clamped).
    fn sample(&self, p: [f64; 3]) -> [f64; 3] {
        let top = (self.n - 1) as f64;
        let mut i0 = [0usize; 3];
        let mut i1 = [0usize; 3];
        let mut f = [0.0; 3];
        for k in 0..3 {
            let v = if p[k].is_finite() {
                p[k].clamp(0.0, top)
            } else {
                0.0
            };
            let fl = v.floor();
            i0[k] = fl as usize;
            i1[k] = (i0[k] + 1).min(self.n - 1);
            f[k] = v - fl;
        }
        let mut out = [0.0; 3];
        for (corner, w) in (0..8).map(|c| {
            let pick = |k: usize| c >> k & 1 == 1;
            let w = (0..3)
                .map(|k| if pick(k) { f[k] } else { 1.0 - f[k] })
                .product::<f64>();
            let idx = |k: usize| if pick(k) { i1[k] } else { i0[k] };
            ((idx(0), idx(1), idx(2)), w)
        }) {
            let v = self.at(corner.0, corner.1, corner.2);
            for k in 0..3 {
                out[k] += v[k] * w;
            }
        }
        out
    }
}

/// Parse an Adobe / Resolve `.cube` file (3D, or 1D applied per channel)
/// into the image the Lut channel reads: `n` slices of `n` × `n` side by
/// side (slice = blue step, x = red, y = green), values clamped to 0..1.
/// `DOMAIN_MIN` / `DOMAIN_MAX` (and Resolve's `*_INPUT_RANGE`) are folded
/// in, so the image always maps 0..1.
pub(crate) fn parse_cube(text: &str) -> Result<Image, String> {
    let mut size3 = None;
    let mut size1 = None;
    let mut lo = [0.0; 3];
    let mut hi = [1.0; 3];
    let mut data: Vec<[f64; 3]> = Vec::new();
    let nums = |rest: &str, line: usize| -> Result<Vec<f64>, String> {
        rest.split_whitespace()
            .map(|t| {
                t.parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| format!("line {line}: not a number: {t}"))
            })
            .collect()
    };
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let l = raw.split('#').next().unwrap_or("").trim();
        if l.is_empty() {
            continue;
        }
        let (key, rest) = l.split_once(char::is_whitespace).unwrap_or((l, ""));
        let size = |rest: &str| -> Result<usize, String> {
            rest.trim()
                .parse::<usize>()
                .ok()
                .filter(|n| (2..=256).contains(n))
                .ok_or_else(|| format!("line {line}: bad size {}", rest.trim()))
        };
        match key {
            "TITLE" => {}
            "LUT_3D_SIZE" => size3 = Some(size(rest)?),
            "LUT_1D_SIZE" => size1 = Some(size(rest)?.max(2)),
            "DOMAIN_MIN" | "DOMAIN_MAX" => {
                let v = nums(rest, line)?;
                let v: [f64; 3] = v
                    .try_into()
                    .map_err(|_| format!("line {line}: {key} needs three numbers"))?;
                if key == "DOMAIN_MIN" {
                    lo = v;
                } else {
                    hi = v;
                }
            }
            "LUT_3D_INPUT_RANGE" | "LUT_1D_INPUT_RANGE" => {
                let v = nums(rest, line)?;
                let [a, b]: [f64; 2] = v
                    .try_into()
                    .map_err(|_| format!("line {line}: {key} needs two numbers"))?;
                lo = [a; 3];
                hi = [b; 3];
            }
            k if k.starts_with(|c: char| c.is_ascii_alphabetic()) => {
                // Other keywords (LUT_IN_VIDEO_RANGE …) change nothing here.
            }
            _ => {
                let v = nums(l, line)?;
                let rgb: [f64; 3] = v
                    .try_into()
                    .map_err(|_| format!("line {line}: expected three numbers"))?;
                data.push(rgb);
            }
        }
    }
    if (0..3).any(|k| hi[k] <= lo[k]) {
        return Err("DOMAIN_MAX must be above DOMAIN_MIN".into());
    }
    let (cube, one_d) = match (size3, size1) {
        (Some(n), _) => (Cube { n, data }, false),
        (None, Some(n)) => (Cube { n, data }, true),
        (None, None) => return Err("no LUT_3D_SIZE or LUT_1D_SIZE".into()),
    };
    let want = if one_d {
        cube.n
    } else {
        cube.n * cube.n * cube.n
    };
    if cube.data.len() != want {
        return Err(format!(
            "expected {want} table entries, found {}",
            cube.data.len()
        ));
    }
    // The output grid, and where each of its steps (0..1) reads the table.
    let n = if one_d {
        LUT_FROM_1D
    } else {
        cube.n.min(LUT_MAX)
    };
    let step = |k: usize, i: usize| {
        let x = i as f64 / (n - 1) as f64;
        (x - lo[k]) / (hi[k] - lo[k]) * (cube.n - 1) as f64
    };
    let entry = |r: usize, g: usize, b: usize| -> [f64; 3] {
        if one_d {
            let ch = |k: usize, i: usize| {
                let p = step(k, i).clamp(0.0, (cube.n - 1) as f64);
                let i0 = p.floor() as usize;
                let i1 = (i0 + 1).min(cube.n - 1);
                let f = p - i0 as f64;
                cube.data[i0][k] * (1.0 - f) + cube.data[i1][k] * f
            };
            [ch(0, r), ch(1, g), ch(2, b)]
        } else if n == cube.n && lo == [0.0; 3] && hi == [1.0; 3] {
            cube.at(r, g, b)
        } else {
            cube.sample([step(0, r), step(1, g), step(2, b)])
        }
    };
    let w = n * n;
    let mut px = vec![255u8; w * n * 4];
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let v = entry(r, g, b);
                let o = (g * w + b * n + r) * 4;
                for k in 0..3 {
                    px[o + k] = (v[k].clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
    }
    Ok(Image::new(w as u32, n as u32, px))
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::node;
    use super::*;
    use crate::scene::default_style;
    use crate::spec::Catalogue;
    use serde_json::json;

    fn shape(v: &Value) -> Arc<ShaderShape> {
        match v {
            Value::Scene(s) => match &**s {
                SceneNode::Shader(sh) => sh.clone(),
                other => panic!("not a shader: {other:?}"),
            },
            other => panic!("not a scene: {other:?}"),
        }
    }

    fn dot() -> Value {
        Value::Scene(Arc::new(SceneNode::Point {
            x: 0.0,
            y: 0.0,
            style: default_style().clone(),
        }))
    }

    #[test]
    fn pins_follow_the_mode() {
        let cat = Catalogue::builtin();
        let spec = cat.get("gpu/effect").unwrap();
        let names = |mode: &str| -> Vec<String> {
            let params: Params = serde_json::from_value(json!({ "mode": mode })).unwrap();
            cat.resolved_inputs(spec, &params)
                .into_iter()
                .map(|p| p.name)
                .collect()
        };
        assert_eq!(
            names("bloom"),
            ["input", "threshold", "radius", "intensity"]
        );
        assert_eq!(
            names("displace"),
            ["input", "amount", "scale", "speed", "seed", "map"]
        );
        assert_eq!(
            names("vignette"),
            ["input", "amount", "radius", "softness", "color"]
        );
        // Unknown modes fall back to bloom.
        assert_eq!(names("nope"), names("bloom"));
        // Every mode in the catalogue's dropdown has its own pins.
        assert_eq!(spec.param_options["mode"], MODES);
    }

    #[test]
    fn every_pass_declares_only_its_pins() {
        for mode in MODES {
            let subs = choice_of(mode).map_or(&[""][..], |(_, o)| o);
            for sub in subs {
                let ports = mode_ports(mode, sub);
                for map in [false, true] {
                    for p in mode_passes(mode, sub, map, map) {
                        for u in parse_uniforms(&p.source) {
                            assert!(
                                ports.iter().any(|q| *q.name == *u.name),
                                "{mode} {sub}: uniform {} has no pin",
                                u.name
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn bloom_is_three_passes_over_the_input() {
        let mut h = node("gpu/effect", json!({ "mode": "bloom" }));
        let out = h
            .eval(vec![vec![dot()], vec![Value::Num(0.3)], vec![], vec![]])
            .unwrap();
        let sh = shape(&out[0][0]);
        let bufs: Vec<_> = sh.passes.iter().map(|p| p.buf).collect();
        assert_eq!(
            bufs,
            [ShaderBuffer::A, ShaderBuffer::B, ShaderBuffer::Image]
        );
        assert!(sh.input.is_some() && sh.map.is_none());
        let get = |name: &str| {
            let i = sh.uniform_decls.iter().position(|d| &*d.name == name);
            sh.uniforms[i.unwrap()]
        };
        assert_eq!(get("threshold"), UniformValue::Number(0.3));
        // Unwired pins send their defaults.
        assert_eq!(get("radius"), UniformValue::Number(24.0));
        assert_eq!(get("intensity"), UniformValue::Number(1.5));
    }

    #[test]
    fn displace_reads_the_map_when_wired() {
        let mut h = node("gpu/effect", json!({ "mode": "displace" }));
        let noise = h
            .eval(vec![
                vec![dot()],
                vec![],
                vec![],
                vec![],
                vec![Value::Num(4.7)],
                vec![],
            ])
            .unwrap();
        let sh = shape(&noise[0][0]);
        assert!(sh.map.is_none());
        assert_eq!(sh.passes[0].channels[1], ShaderChannel::None);
        let seed = sh.uniform_decls.iter().position(|d| &*d.name == "seed");
        assert_eq!(sh.uniforms[seed.unwrap()], UniformValue::Number(4.0));
        let mapped = h
            .eval(vec![
                vec![dot()],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![dot()],
            ])
            .unwrap();
        let sh = shape(&mapped[0][0]);
        assert!(sh.map.is_some());
        assert_eq!(sh.passes[0].channels[1], ShaderChannel::Map);
    }

    #[test]
    fn vignette_colour_is_a_vec4() {
        let mut h = node("gpu/effect", json!({ "mode": "vignette" }));
        let red = Value::Color(Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 0.5,
        });
        let out = h
            .eval(vec![vec![dot()], vec![], vec![], vec![], vec![red]])
            .unwrap();
        let sh = shape(&out[0][0]);
        let i = sh.uniform_decls.iter().position(|d| &*d.name == "color");
        assert_eq!(&*sh.uniform_decls[i.unwrap()].glsl_type, "vec4");
        assert_eq!(
            sh.uniforms[i.unwrap()],
            UniformValue::Color([1.0, 0.0, 0.0, 0.5])
        );
    }

    #[test]
    fn the_patch_seed_shifts_the_effect_seed() {
        assert_eq!(effect_seed(3.7), 3.0);
        let _patch = crate::seed::install(4242);
        let mixed = effect_seed(3.0);
        assert_ne!(mixed, 3.0);
        assert_eq!(mixed, mixed.floor());
    }

    fn pins(mode: &str, extra: serde_json::Value) -> Vec<String> {
        let cat = Catalogue::builtin();
        let spec = cat.get("gpu/effect").unwrap();
        let mut params = json!({ "mode": mode });
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let params: Params = serde_json::from_value(params).unwrap();
        cat.resolved_inputs(spec, &params)
            .into_iter()
            .map(|p| p.name)
            .collect()
    }

    #[test]
    fn new_modes_have_their_pins() {
        let none = json!({});
        assert_eq!(pins("blur", none.clone()), ["input", "kind", "radius"]);
        for (kind, want) in [
            ("gaussian", vec!["radius"]),
            ("lens", vec!["radius", "highlights"]),
            ("motion", vec!["amount", "angle"]),
            ("zoom", vec!["amount", "center"]),
            ("spin", vec!["angle", "center"]),
        ] {
            let mut all = vec!["input", "kind"];
            all.extend(want);
            assert_eq!(pins("blur", json!({ "kind": kind })), all, "{kind}");
        }
        // An unknown kind is gaussian.
        assert_eq!(
            pins("blur", json!({ "kind": "nope" })),
            pins("blur", none.clone())
        );
        assert_eq!(
            pins("shadow", none.clone()),
            ["input", "offset", "radius", "color", "opacity", "inner"]
        );
        assert_eq!(
            pins("glow", none.clone()),
            ["input", "blend", "radius", "color", "intensity", "inner"]
        );
        assert_eq!(
            pins("glow", json!({ "blend": "add" })),
            pins("glow", none.clone())
        );
        assert_eq!(pins("barrel", none.clone()), ["input", "amount", "zoom"]);
        assert_eq!(
            pins("edges", none.clone()),
            ["input", "threshold", "width", "color", "keep"]
        );
        assert_eq!(pins("posterize", none.clone()), ["input", "levels"]);
        assert_eq!(
            pins("threshold", none.clone()),
            ["input", "channel", "level", "softness", "low", "high"]
        );
        assert_eq!(
            pins("threshold", json!({ "channel": "alpha" })),
            ["input", "channel", "level", "softness"]
        );
        assert_eq!(
            pins("levels", none.clone()),
            ["input", "inBlack", "inWhite", "gamma", "outBlack", "outWhite"]
        );
        assert_eq!(
            pins("curves", none.clone()),
            [
                "input",
                "black",
                "shadows",
                "midtones",
                "highlights",
                "white"
            ]
        );
        assert_eq!(pins("lut", none.clone()), ["input", "lut", "amount"]);
        assert_eq!(
            pins("key", none),
            ["input", "color", "tolerance", "softness", "spill"]
        );
        let cat = Catalogue::builtin();
        let spec = cat.get("gpu/effect").unwrap();
        assert_eq!(spec.param_options["kind"], BLUR_KINDS);
        assert_eq!(spec.param_options["channel"], THRESHOLD_CHANNELS);
    }

    #[test]
    fn the_original_modes_are_untouched() {
        // Their passes are what they were: the same sources and channels.
        for mode in &MODES[..7] {
            assert_eq!(
                passes(mode, false),
                mode_passes(mode, "", false, false),
                "{mode}"
            );
            assert!(choice_of(mode).is_none());
        }
        assert_eq!(passes("bloom", false).len(), 3);
    }

    #[test]
    fn blur_kinds_pick_their_passes() {
        let bufs = |kind: &str| -> Vec<ShaderBuffer> {
            mode_passes("blur", kind, false, false)
                .iter()
                .map(|p| p.buf)
                .collect()
        };
        assert_eq!(
            bufs("gaussian"),
            [ShaderBuffer::A, ShaderBuffer::B, ShaderBuffer::Image]
        );
        for k in ["lens", "motion", "zoom", "spin"] {
            assert_eq!(bufs(k), [ShaderBuffer::Image], "{k}");
        }
        // The kind on its pin (wired or typed) wins over the saved param.
        let mut h = node("gpu/effect", json!({ "mode": "blur", "kind": "gaussian" }));
        let out = h
            .eval(vec![vec![dot()], vec![Value::str("gaussian")], vec![]])
            .unwrap();
        assert_eq!(shape(&out[0][0]).passes.len(), 3);
        let out = h
            .eval(vec![vec![dot()], vec![Value::str("lens")], vec![]])
            .unwrap();
        assert_eq!(shape(&out[0][0]).passes.len(), 1);
    }

    #[test]
    fn shadow_sends_its_offset_and_colour() {
        let mut h = node("gpu/effect", json!({ "mode": "shadow" }));
        let out = h
            .eval(vec![
                vec![dot()],
                vec![Value::Vec2(Vec2 { x: 3.0, y: -4.0 })],
                vec![],
                vec![],
                vec![],
                vec![Value::Bool(true)],
            ])
            .unwrap();
        let sh = shape(&out[0][0]);
        let get = |name: &str| {
            let i = sh.uniform_decls.iter().position(|d| &*d.name == name);
            sh.uniforms[i.unwrap_or_else(|| panic!("no {name}"))]
        };
        assert_eq!(get("offset"), UniformValue::Vec2([3.0, -4.0]));
        assert_eq!(get("radius"), UniformValue::Number(12.0));
        assert_eq!(get("color"), UniformValue::Color([0.0, 0.0, 0.0, 1.0]));
        assert_eq!(get("opacity"), UniformValue::Number(0.6));
        assert_eq!(get("inner"), UniformValue::Number(1.0));
        let bufs: Vec<_> = sh.passes.iter().map(|p| p.buf).collect();
        assert_eq!(
            bufs,
            [ShaderBuffer::A, ShaderBuffer::B, ShaderBuffer::Image]
        );
        // Glow has no offset: its coverage pass doesn't shift.
        let glow = mode_passes("glow", "", false, false);
        assert!(!parse_uniforms(&glow[0].source)
            .iter()
            .any(|u| &*u.name == "offset"));
    }

    #[test]
    fn curves_default_to_a_straight_line() {
        let ports = mode_ports("curves", "");
        let greys: Vec<f64> = ports
            .iter()
            .map(|p| match p.default {
                Some(Value::Color(c)) => {
                    assert!(c.r == c.g && c.g == c.b && c.a == 1.0);
                    c.r
                }
                _ => panic!("{} isn't a colour", p.name),
            })
            .collect();
        assert_eq!(greys, [0.0, 0.25, 0.5, 0.75, 1.0]);
    }

    // ─── LUTs ────────────────────────────────────────────────────────────

    /// A `.cube` of size `n` whose entries are `f(r, g, b)` (0..1 inputs).
    fn cube(n: usize, f: impl Fn(f64, f64, f64) -> [f64; 3]) -> String {
        let mut s = format!("TITLE \"test\"\n# a comment\nLUT_3D_SIZE {n}\n\n");
        let t = |i: usize| i as f64 / (n - 1) as f64;
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let v = f(t(r), t(g), t(b));
                    s += &format!("{} {} {}\n", v[0], v[1], v[2]);
                }
            }
        }
        s
    }

    /// The table image's entry for steps (r, g, b).
    fn entry(img: &Image, r: u32, g: u32, b: u32) -> [u8; 3] {
        let n = img.height();
        let p = img.pixel(b * n + r, g);
        [p[0], p[1], p[2]]
    }

    #[test]
    fn cube_files_become_slices() {
        let img = parse_cube(&cube(3, |r, g, b| [1.0 - r, g, b * 0.5])).unwrap();
        assert_eq!((img.width(), img.height()), (9, 3));
        assert_eq!(entry(&img, 0, 0, 0), [255, 0, 0]);
        assert_eq!(entry(&img, 2, 1, 2), [0, 128, 128]);
        assert_eq!(entry(&img, 1, 2, 1), [128, 255, 64]);
        // Out-of-range values clamp.
        let hot = parse_cube(&cube(2, |r, _, _| [r * 3.0 - 1.0, 0.0, 0.0])).unwrap();
        assert_eq!(entry(&hot, 0, 0, 0)[0], 0);
        assert_eq!(entry(&hot, 1, 0, 0)[0], 255);
    }

    #[test]
    fn cube_domains_and_1d_tables() {
        // A domain of 0..2: input 1 reads the table's middle.
        let mut text = cube(3, |r, g, b| [r, g, b]);
        text = text.replace(
            "LUT_3D_SIZE 3",
            "LUT_3D_SIZE 3\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 2 2 2",
        );
        let img = parse_cube(&text).unwrap();
        assert_eq!(entry(&img, 2, 2, 2), [128, 128, 128]);
        assert_eq!(entry(&img, 1, 0, 0), [64, 0, 0]);
        // A 1D table applies per channel, on a 33-step cube.
        let one = "LUT_1D_SIZE 2\n1 0 0.5\n0 1 1\n";
        let img = parse_cube(one).unwrap();
        assert_eq!(img.height(), 33);
        assert_eq!(entry(&img, 0, 0, 0), [255, 0, 128]);
        assert_eq!(entry(&img, 32, 32, 32), [0, 255, 255]);
        assert_eq!(entry(&img, 16, 16, 16), [128, 128, 191]);
    }

    #[test]
    fn big_cubes_shrink_to_64() {
        let img = parse_cube(&cube(65, |r, g, b| [r, g, b])).unwrap();
        assert_eq!((img.width(), img.height()), (64 * 64, 64));
        for (r, g, b) in [(0, 0, 0), (63, 0, 31), (17, 40, 63)] {
            let want = [r, g, b].map(|v| (v as f64 / 63.0 * 255.0).round() as u8);
            let got = entry(&img, r, g, b);
            for k in 0..3 {
                assert!(got[k].abs_diff(want[k]) <= 1, "{got:?} vs {want:?}");
            }
        }
    }

    #[test]
    fn bad_cubes_say_why() {
        assert!(parse_cube("").unwrap_err().contains("LUT_3D_SIZE"));
        assert!(parse_cube("LUT_3D_SIZE 2\n0 0 0\n")
            .unwrap_err()
            .contains("expected 8"));
        assert!(parse_cube("LUT_3D_SIZE 1\n").unwrap_err().contains("size"));
        assert!(parse_cube("LUT_3D_SIZE 2\n0 0 x\n")
            .unwrap_err()
            .contains("line 2"));
        assert!(parse_cube("LUT_3D_SIZE 2\n0 0\n")
            .unwrap_err()
            .contains("three"));
        let flipped = cube(2, |r, g, b| [r, g, b]).replace(
            "LUT_3D_SIZE 2",
            "LUT_3D_SIZE 2\nDOMAIN_MIN 1 1 1\nDOMAIN_MAX 0 0 0",
        );
        assert!(parse_cube(&flipped).unwrap_err().contains("DOMAIN"));
    }

    #[test]
    fn lut_mode_loads_the_table_from_its_pin() {
        let mut h = node("gpu/effect", json!({ "mode": "lut" }));
        // Nothing named: the layer passes through.
        let out = h.eval(vec![vec![dot()], vec![], vec![]]).unwrap();
        let sh = shape(&out[0][0]);
        assert!(sh.lut.is_none());
        assert!(!sh.passes[0].channels.contains(&ShaderChannel::Lut));
        // A data URL, as a File node gives it.
        let text = cube(2, |r, g, b| [1.0 - r, 1.0 - g, 1.0 - b]);
        let url = format!(
            "data:text/plain,{}",
            text.replace(' ', "%20")
                .replace('\n', "%0A")
                .replace('"', "%22")
                .replace('#', "%23")
        );
        let start = std::time::Instant::now();
        let sh = loop {
            let out = h
                .eval(vec![
                    vec![dot()],
                    vec![Value::str(&url)],
                    vec![Value::Num(0.5)],
                ])
                .unwrap();
            let sh = shape(&out[0][0]);
            if sh.lut.is_some() || start.elapsed().as_secs() > 10 {
                break sh;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let lut = sh.lut.as_ref().expect("the table loads");
        assert_eq!((lut.width(), lut.height()), (4, 2));
        assert_eq!(entry(lut, 0, 0, 0), [255, 255, 255]);
        assert_eq!(sh.passes[0].channels[1], ShaderChannel::Lut);
        let i = sh.uniform_decls.iter().position(|d| &*d.name == "amount");
        assert_eq!(sh.uniforms[i.unwrap()], UniformValue::Number(0.5));
    }

    #[test]
    fn only_lut_mode_skips_the_memo() {
        let cat = Catalogue::builtin();
        let spec = cat.get("gpu/effect").unwrap();
        for (mode, memo) in [("bloom", true), ("curves", true), ("lut", false)] {
            let params: Params = serde_json::from_value(json!({ "mode": mode })).unwrap();
            let inputs = cat.resolved_inputs(spec, &params);
            let init = NodeInit {
                id: "fx",
                spec,
                params: &params,
                inputs: &inputs,
            };
            assert_eq!(effect(&init).memoise(), memo, "{mode}");
        }
    }
}

#[cfg(test)]
mod ascii_tests {
    use super::*;
    use crate::nodes::value::harness::node;
    use serde_json::json;

    fn coverage(img: &Image, k: u32) -> u32 {
        let tw = GLYPH_TILE / 2;
        let mut sum = 0;
        for y in 0..img.height() {
            for x in k * tw..(k + 1) * tw {
                sum += u32::from(img.pixel(x, y)[3]);
            }
        }
        sum
    }

    #[test]
    fn the_atlas_holds_each_character_in_a_tile() {
        let img = glyph_atlas(ASCII_RAMP).expect("the bundled monospace font");
        let n = ASCII_RAMP.chars().count() as u32;
        assert_eq!(
            (img.width(), img.height()),
            (n * GLYPH_TILE / 2, GLYPH_TILE)
        );
        // A space is empty; the ramp gets denser towards '@'.
        assert_eq!(coverage(&img, 0), 0);
        assert!(coverage(&img, 1) > 0, "'.' has ink");
        assert!(coverage(&img, n - 1) > coverage(&img, 1) * 4);
        // Ink stays inside its tile (no glyph spills into the next).
        for k in 0..n {
            let tw = GLYPH_TILE / 2;
            for y in 0..GLYPH_TILE {
                assert_eq!(img.pixel(k * tw, y)[3], 0, "tile {k}'s left column");
            }
        }
        assert!(glyph_atlas("").is_none());
        // At most MAX_GLYPHS characters.
        let long: String = std::iter::repeat_n('#', 500).collect();
        let img = glyph_atlas(&long).unwrap();
        assert_eq!(img.width(), MAX_GLYPHS as u32 * GLYPH_TILE / 2);
    }

    #[test]
    fn ascii_mode_hands_the_atlas_to_its_pass() {
        let shader = |chars: &str| {
            let mut n = node("gpu/effect", json!({"mode": "ascii"}));
            let layer = Value::Scene(Arc::new(SceneNode::group(Vec::new())));
            // input, size, chars, colored, color, background
            let out = n
                .eval(vec![
                    vec![layer],
                    vec![Value::Num(10.0)],
                    vec![Value::str(chars)],
                    vec![Value::Bool(true)],
                    vec![],
                    vec![],
                ])
                .unwrap();
            match &out[0][0] {
                Value::Scene(s) => match &**s {
                    SceneNode::Shader(sh) => sh.clone(),
                    other => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            }
        };
        let sh = shader(" .oO@");
        let atlas = sh.lut.clone().expect("an atlas");
        assert_eq!(atlas.width(), 5 * GLYPH_TILE / 2);
        assert_eq!(sh.passes.len(), 1);
        assert_eq!(sh.passes[0].channels[1], ShaderChannel::Lut);
        assert!(sh.uniform_decls.iter().any(|d| &*d.name == "colored"));
        // Empty chars: the default ramp.
        let sh = shader("");
        let n = ASCII_RAMP.chars().count() as u32;
        assert_eq!(sh.lut.as_ref().unwrap().width(), n * GLYPH_TILE / 2);
    }
}
