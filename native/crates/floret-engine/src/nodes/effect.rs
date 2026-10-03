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

use super::gpu::as_layer;
use super::Registrar;
use crate::node::{raw, EvalCtx, NodeImpl, NodeInit};
use crate::scene::SceneNode;
use crate::shader::{
    parse_uniforms, uniform_port_type, ShaderBuffer, ShaderChannel, ShaderPass, ShaderShape,
    UniformDecl, UniformValue,
};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Color, Value};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("gpu/effect", effect);
    r.dynamic("gpu/effect", effect_inputs);
}

/// Every effect's seed goes through here (the place to mix in a
/// patch-wide seed): the node's own seed, as the float the passes read.
pub(crate) fn effect_seed(seed: f64) -> f64 {
    if seed.is_finite() {
        seed.floor()
    } else {
        0.0
    }
}

/// The effect modes, in dropdown order.
pub const MODES: [&str; 7] = [
    "bloom",
    "grain",
    "displace",
    "dither",
    "halftone",
    "chromatic",
    "vignette",
];

fn num(name: &str, default: f64) -> PortSpec {
    PortSpec::new(name, PortType::Number).default(Value::Num(default))
}

fn int(name: &str, default: f64) -> PortSpec {
    let mut p = num(name, default);
    p.integer = true;
    p
}

fn mode_of(get: impl Fn(&str) -> Option<String>) -> &'static str {
    let m = get("mode").unwrap_or_default();
    MODES.iter().find(|x| **x == m).copied().unwrap_or("bloom")
}

/// The mode's parameter pins, after `input`.
fn mode_ports(mode: &str) -> Vec<PortSpec> {
    match mode {
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
    let mode = mode_of(|k| params.get(k).and_then(|v| v.as_str()).map(str::to_owned));
    let mut ports = vec![PortSpec::new("input", PortType::Layer).kind(PortKind::Spread)];
    ports.extend(mode_ports(mode));
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

fn source(body: &str) -> Arc<str> {
    Arc::from(format!("{COMMON}{}", body.replace("#BLUR", BLUR)))
}

/// The passes for `mode` (`map`: a displacement layer is wired).
pub fn passes(mode: &str, map: bool) -> Vec<ShaderPass> {
    use ShaderChannel as C;
    let image = |body: &str, channels| ShaderPass {
        buf: ShaderBuffer::Image,
        source: source(body),
        channels,
    };
    let input = [C::Input, C::None, C::None, C::None];
    match mode {
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

fn effect(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx: &mut EvalCtx| {
        let mode = mode_of(|k| cx.param(k).and_then(|v| v.as_str()).map(str::to_owned));
        let ports = mode_ports(mode);
        let map_port = ports.iter().position(|p| p.ty == PortType::Layer);
        let map = map_port.and_then(|i| as_layer(inputs.get(i + 1).map(|s| &s[..])));
        let passes = passes(mode, map.is_some());
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
                    _ => UniformValue::Number(0.0),
                },
            })
            .collect();
        let shape = ShaderShape {
            node_id: Arc::from(cx.node_id),
            passes: Arc::from(passes),
            uniform_decls: Arc::from(decls),
            uniforms: Arc::from(uniforms),
            input: as_layer(inputs.first().map(|s| &s[..])),
            map,
        };
        Ok(vec![one(Value::Scene(Arc::new(SceneNode::Shader(
            Arc::new(shape),
        ))))])
    })
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
            let ports = mode_ports(mode);
            for map in [false, true] {
                for p in passes(mode, map) {
                    for u in parse_uniforms(&p.source) {
                        assert!(
                            ports.iter().any(|q| *q.name == *u.name),
                            "{mode}: uniform {} has no pin",
                            u.name
                        );
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
}
