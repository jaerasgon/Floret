//! `gpu/echo`: time effects on a layer, on the GPU (native-only).
//!
//! Like `gpu/effect`, the node describes GLSL passes as a
//! [`SceneNode::Shader`]; the renderer keeps the state between frames, so
//! nothing here is stateful and the effect keeps running while the graph
//! is idle. Modes:
//!
//! - `trails`: every frame drawn over the last, which fades by `decay`
//!   (one feedback buffer, no history).
//! - `echo`: `copies` earlier frames, `spacing` frames apart, each fainter
//!   by `decay`, under the live one.
//! - `delay`: the input as it was `frames` frames ago.
//! - `freeze`: while `freeze` is on, hold the last frame shown.
//! - `slit-scan`: each row (or column) from a different moment: the edge
//!   `direction` points away from is now, the far edge `frames` ago.
//!
//! `echo`, `delay` and `slit-scan` read a ring of past frames the renderer
//! keeps per node ([`ShaderHistory`]), capped by memory (256 MB, at most
//! 240 frames). `reset` forgets the past (trails clear, history refills).

use super::gpu::as_layer;
use super::Registrar;
use crate::node::{raw, EvalCtx, NodeImpl, NodeInit};
use crate::scene::SceneNode;
use crate::shader::{
    parse_uniforms, ShaderBuffer, ShaderChannel, ShaderHistory, ShaderPass, ShaderShape,
    UniformDecl, UniformValue,
};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Value};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("gpu/echo", echo);
    r.dynamic("gpu/echo", echo_inputs);
}

/// The modes, in dropdown order.
pub const MODES: [&str; 5] = ["trails", "echo", "delay", "freeze", "slit-scan"];
/// `slit-scan` directions: where the past lies, from the "now" edge.
pub const DIRECTIONS: [&str; 4] = ["down", "up", "right", "left"];
/// Most copies `echo` draws.
const MAX_COPIES: f64 = 32.0;

fn num(name: &str, default: f64) -> PortSpec {
    PortSpec::new(name, PortType::Number).default(Value::Num(default))
}

fn int(name: &str, default: f64) -> PortSpec {
    let mut p = num(name, default);
    p.integer = true;
    p
}

fn flag(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Boolean).default(Value::Bool(false))
}

fn mode_of(get: impl Fn(&str) -> Option<String>) -> &'static str {
    let m = get("mode").unwrap_or_default();
    MODES.iter().find(|x| **x == m).copied().unwrap_or("trails")
}

/// The mode's pins, after `input`.
fn mode_ports(mode: &str) -> Vec<PortSpec> {
    match mode {
        "echo" => vec![
            int("copies", 5.0),
            int("spacing", 6.0),
            num("decay", 0.7),
            flag("reset"),
        ],
        "delay" => vec![int("frames", 30.0), flag("reset")],
        "freeze" => vec![flag("freeze")],
        "slit-scan" => {
            let mut dir = PortSpec::new("direction", PortType::String).default(Value::str("down"));
            dir.param = Some("direction".into());
            vec![int("frames", 60.0), dir, flag("reset")]
        }
        // trails
        _ => vec![num("decay", 0.92), flag("reset")],
    }
}

fn echo_inputs(params: &Params) -> Vec<PortSpec> {
    let mode = mode_of(|k| params.get(k).and_then(|v| v.as_str()).map(str::to_owned));
    let mut ports = vec![PortSpec::new("input", PortType::Layer).kind(PortKind::Spread)];
    ports.extend(mode_ports(mode));
    ports
}

// ─── GLSL ────────────────────────────────────────────────────────────────────

/// Helpers every pass may use. `past(ago, uv)` reads the input `ago`
/// frames back (0 = this frame; older than what's held reads the oldest),
/// premultiplied; `pastSmooth` blends between whole frames.
const COMMON: &str = "
vec4 straight(vec4 c) {
  return vec4(c.a > 0.0 ? clamp(c.rgb / c.a, 0.0, 1.0) : vec3(0.0), clamp(c.a, 0.0, 1.0));
}
bool outside(vec2 uv) { return uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0; }
vec4 fetch(int ch, vec2 uv) {
  if (outside(uv)) return vec4(0.0);
  if (ch == 1) return texture(iChannel1, uv);
  return texture(iChannel0, uv);
}
vec4 past(float ago, vec2 uv) {
  if (outside(uv)) return vec4(0.0);
  int size = max(iHistorySize, 1);
  int k = clamp(int(floor(ago + 0.5)), 0, max(iHistoryCount, 1) - 1);
  int layer = (iHistoryHead - k + size) % size;
  return texture(iHistory, vec3(uv, float(layer)));
}
vec4 pastSmooth(float ago, vec2 uv) {
  float a = clamp(ago, 0.0, float(max(iHistoryCount, 1) - 1));
  float f = floor(a);
  return mix(past(f, uv), past(f + 1.0, uv), a - f);
}
";

/// Buffer A: the input over the last frame's A, faded. The extra small
/// step down lets 8-bit trails die out completely.
const TRAILS_A: &str = "
uniform float decay;
uniform float reset;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 uv = fc / iResolution.xy;
  vec4 c = fetch(0, uv);
  vec4 prev = reset > 0.5 ? vec4(0.0) : fetch(1, uv);
  prev = max(prev * clamp(decay, 0.0, 1.0) - 1.5 / 255.0, 0.0);
  o = c + prev * (1.0 - c.a);
}
";

/// Buffer A: the last frame's A while frozen, else the input.
const FREEZE_A: &str = "
uniform float freeze;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 uv = fc / iResolution.xy;
  o = freeze > 0.5 ? fetch(1, uv) : fetch(0, uv);
}
";

/// Show channel 0 (a buffer, premultiplied).
const SHOW: &str = "
void mainImage(out vec4 o, in vec2 fc) {
  o = straight(fetch(0, fc / iResolution.xy));
}
";

const ECHO: &str = "
uniform float copies;
uniform float spacing;
uniform float decay;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 uv = fc / iResolution.xy;
  int n = int(clamp(floor(copies), 0.0, 32.0));
  float s = max(floor(spacing), 1.0);
  float held = float(max(iHistoryCount, 1) - 1);
  vec4 acc = vec4(0.0);
  // Oldest first, each newer copy over it; copies not held yet are left out.
  for (int i = 32; i >= 1; i--) {
    if (i > n || float(i) * s > held) continue;
    vec4 c = past(float(i) * s, uv) * pow(clamp(decay, 0.0, 1.0), float(i));
    acc = c + acc * (1.0 - c.a);
  }
  vec4 now = past(0.0, uv);
  o = straight(now + acc * (1.0 - now.a));
}
";

const DELAY: &str = "
uniform float frames;
void mainImage(out vec4 o, in vec2 fc) {
  o = straight(past(max(floor(frames), 0.0), fc / iResolution.xy));
}
";

/// `dir`: 0 down (top row is now), 1 up, 2 right (left column is now), 3
/// left. `fragCoord` counts from the bottom-left.
const SLIT_SCAN: &str = "
uniform float frames;
uniform float dir;
void mainImage(out vec4 o, in vec2 fc) {
  vec2 uv = fc / iResolution.xy;
  float f = dir < 0.5 ? 1.0 - uv.y : dir < 1.5 ? uv.y : dir < 2.5 ? uv.x : 1.0 - uv.x;
  float ago = clamp(f, 0.0, 1.0) * max(floor(frames) - 1.0, 0.0);
  o = straight(pastSmooth(ago, uv));
}
";

fn source(body: &str) -> Arc<str> {
    Arc::from(format!("{COMMON}{body}"))
}

/// The passes for `mode`.
pub fn passes(mode: &str) -> Vec<ShaderPass> {
    use ShaderChannel as C;
    let image = |body: &str, channels| ShaderPass {
        buf: ShaderBuffer::Image,
        source: source(body),
        channels,
    };
    let none = [C::None; 4];
    let buffered = |body: &str| {
        vec![
            ShaderPass {
                buf: ShaderBuffer::A,
                source: source(body),
                channels: [C::Input, C::Buffer(ShaderBuffer::A), C::None, C::None],
            },
            image(
                SHOW,
                [C::Buffer(ShaderBuffer::A), C::None, C::None, C::None],
            ),
        ]
    };
    match mode {
        "echo" => vec![image(ECHO, none)],
        "delay" => vec![image(DELAY, none)],
        "slit-scan" => vec![image(SLIT_SCAN, none)],
        "freeze" => buffered(FREEZE_A),
        _ => buffered(TRAILS_A),
    }
}

/// Frames of history `mode` needs (`None`: it keeps none).
fn history_frames(mode: &str, n: &dyn Fn(&str, f64) -> f64) -> Option<u32> {
    let frames = match mode {
        "echo" => {
            let copies = n("copies", 5.0).clamp(0.0, MAX_COPIES).floor();
            copies * n("spacing", 6.0).floor().max(1.0) + 1.0
        }
        "delay" => n("frames", 30.0).floor().max(0.0) + 1.0,
        "slit-scan" => n("frames", 60.0).floor().max(1.0),
        _ => return None,
    };
    Some(frames.min(f64::from(u32::MAX)) as u32)
}

fn echo(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx: &mut EvalCtx| {
        let mode = mode_of(|k| cx.param(k).and_then(|v| v.as_str()).map(str::to_owned));
        let ports = mode_ports(mode);
        // Slot 0 of a pin (after `input`), unwired → its default.
        let value = |name: &str| -> Value {
            let i = ports.iter().position(|p| p.name == name);
            let wired = i.and_then(|i| inputs.get(i + 1)).and_then(|s| s.first());
            match wired {
                Some(v) if !v.is_nullish() => v.clone(),
                _ => i
                    .and_then(|i| ports[i].default.clone())
                    .unwrap_or(Value::Undefined),
            }
        };
        let n = |name: &str, fallback: f64| {
            let v = value(name).to_number();
            if v.is_finite() {
                v
            } else {
                fallback
            }
        };
        let reset = value("reset").truthy();
        let passes = passes(mode);
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
            .map(|d| {
                UniformValue::Number(match &*d.name {
                    "reset" | "freeze" => f64::from(u8::from(value(&d.name).truthy())),
                    "dir" => {
                        let s = super::convert::js_string_or_empty(&value("direction"));
                        DIRECTIONS.iter().position(|x| *x == s).unwrap_or(0) as f64
                    }
                    other => n(other, 0.0),
                })
            })
            .collect();
        let history = history_frames(mode, &n).map(|frames| ShaderHistory { frames, reset });
        let shape = ShaderShape {
            node_id: Arc::from(cx.node_id),
            passes: Arc::from(passes),
            uniform_decls: Arc::from(decls),
            uniforms: Arc::from(uniforms),
            input: as_layer(inputs.first().map(|s| &s[..])),
            map: None,
            history,
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

    fn uniform(sh: &ShaderShape, name: &str) -> UniformValue {
        let i = sh.uniform_decls.iter().position(|d| &*d.name == name);
        sh.uniforms[i.unwrap_or_else(|| panic!("no uniform {name}"))]
    }

    #[test]
    fn pins_follow_the_mode() {
        let cat = Catalogue::builtin();
        let spec = cat.get("gpu/echo").unwrap();
        let names = |mode: &str| -> Vec<String> {
            let params: Params = serde_json::from_value(json!({ "mode": mode })).unwrap();
            cat.resolved_inputs(spec, &params)
                .into_iter()
                .map(|p| p.name)
                .collect()
        };
        assert_eq!(names("trails"), ["input", "decay", "reset"]);
        assert_eq!(
            names("echo"),
            ["input", "copies", "spacing", "decay", "reset"]
        );
        assert_eq!(names("delay"), ["input", "frames", "reset"]);
        assert_eq!(names("freeze"), ["input", "freeze"]);
        assert_eq!(
            names("slit-scan"),
            ["input", "frames", "direction", "reset"]
        );
        assert_eq!(names("nope"), names("trails"));
        assert_eq!(spec.param_options["mode"], MODES);
        assert_eq!(spec.param_options["direction"], DIRECTIONS);
        // The static pins are the default mode's.
        let statics: Vec<&str> = spec.inputs.iter().map(|p| &*p.name).collect();
        assert_eq!(statics, names("trails"));
    }

    #[test]
    fn every_pass_declares_only_its_pins() {
        for mode in MODES {
            let ports = mode_ports(mode);
            for p in passes(mode) {
                for u in parse_uniforms(&p.source) {
                    let pin = if &*u.name == "dir" {
                        "direction"
                    } else {
                        &u.name
                    };
                    assert!(
                        ports.iter().any(|q| q.name == pin),
                        "{mode}: uniform {} has no pin",
                        u.name
                    );
                }
            }
        }
    }

    #[test]
    fn history_follows_the_mode() {
        let mut h = node("gpu/echo", json!({ "mode": "trails" }));
        let sh = shape(&h.eval(vec![vec![dot()], vec![], vec![]]).unwrap()[0][0]);
        assert!(sh.history.is_none());
        assert_eq!(sh.passes.len(), 2);
        assert_eq!(uniform(&sh, "decay"), UniformValue::Number(0.92));
        assert_eq!(uniform(&sh, "reset"), UniformValue::Number(0.0));

        // echo: copies × spacing frames back, and this one.
        let mut h = node("gpu/echo", json!({ "mode": "echo" }));
        let out = h
            .eval(vec![
                vec![dot()],
                vec![Value::Num(4.0)],
                vec![Value::Num(3.0)],
                vec![],
                vec![Value::Bool(true)],
            ])
            .unwrap();
        let sh = shape(&out[0][0]);
        assert_eq!(
            sh.history,
            Some(ShaderHistory {
                frames: 13,
                reset: true
            })
        );
        assert!(sh.input.is_some());

        // delay: NaN / negative frames fall back safely.
        let mut h = node("gpu/echo", json!({ "mode": "delay" }));
        let out = h
            .eval(vec![vec![dot()], vec![Value::Num(f64::NAN)], vec![]])
            .unwrap();
        assert_eq!(shape(&out[0][0]).history.unwrap().frames, 31);
        let out = h
            .eval(vec![vec![dot()], vec![Value::Num(-5.0)], vec![]])
            .unwrap();
        assert_eq!(shape(&out[0][0]).history.unwrap().frames, 1);

        // slit-scan: the direction becomes a number for the pass.
        let mut h = node("gpu/echo", json!({ "mode": "slit-scan" }));
        let out = h
            .eval(vec![
                vec![dot()],
                vec![Value::Num(90.0)],
                vec![Value::str("left")],
                vec![],
            ])
            .unwrap();
        let sh = shape(&out[0][0]);
        assert_eq!(sh.history.unwrap().frames, 90);
        assert_eq!(uniform(&sh, "dir"), UniformValue::Number(3.0));

        // Nothing wired: still a shape, with no input.
        let mut h = node("gpu/echo", json!({ "mode": "freeze" }));
        let out = h.eval(vec![vec![], vec![]]).unwrap();
        let sh = shape(&out[0][0]);
        assert!(sh.input.is_none() && sh.history.is_none());
        assert_eq!(uniform(&sh, "freeze"), UniformValue::Number(0.0));
    }
}
