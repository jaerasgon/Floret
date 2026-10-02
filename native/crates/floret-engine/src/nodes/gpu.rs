//! `gpu/shader`. Ported from `src/nodes/gpu/shader.ts`.
//!
//! The node only describes the draw (a [`SceneNode::Shader`]); compiling
//! and running the GLSL is the renderer's job, every frame, so the shader
//! animates even when the graph is idle.

use super::value::param_or;
use super::Registrar;
use crate::node::{raw, EvalCtx, NodeImpl, NodeInit};
use crate::scene::SceneNode;
use crate::shader::{
    parse_uniforms, uniform_port_type, ShaderBuffer, ShaderChannel, ShaderPass, ShaderShape,
    UniformDecl, UniformValue,
};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Value};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("gpu/shader", shader);
    r.dynamic("gpu/shader", shader_inputs);
}

pub const DEFAULT_SOURCE: &str = "// iTime, iResolution, iMouse, iChannel0 are provided.
// Declare a 'uniform float foo;' to grow a wired input port.
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
  vec2 uv = fragCoord / iResolution.xy;
  vec3 col = 0.5 + 0.5 * cos(iTime + uv.xyx + vec3(0.0, 2.0, 4.0));
  fragColor = vec4(col, 1.0);
}";

/// The params the pass list depends on, as JSON values (missing = `None`).
struct PassParams<'a> {
    source: Option<&'a serde_json::Value>,
    buffers: [Option<&'a serde_json::Value>; 4],
    channels: Option<&'a serde_json::Value>,
}

impl<'a> PassParams<'a> {
    fn from(get: impl Fn(&str) -> Option<&'a serde_json::Value>) -> PassParams<'a> {
        PassParams {
            source: get("source"),
            buffers: ShaderBuffer::BUFFERS.map(|b| get(b.param_key())),
            channels: get("channels"),
        }
    }

    /// `passSources`: Buffer A–D that have (non-blank) source, then Image.
    fn sources(&self) -> Vec<(ShaderBuffer, Arc<str>)> {
        let mut out = Vec::new();
        for (b, src) in ShaderBuffer::BUFFERS.iter().zip(self.buffers) {
            if let Some(serde_json::Value::String(s)) = src {
                if !js_trim(s).is_empty() {
                    out.push((*b, Arc::from(s.as_str())));
                }
            }
        }
        let image = match self.source {
            Some(serde_json::Value::String(s)) => s.as_str(),
            _ => DEFAULT_SOURCE,
        };
        out.push((ShaderBuffer::Image, Arc::from(image)));
        out
    }

    /// `allUniforms`: union across passes, first-seen order, no samplers.
    fn uniforms(&self) -> Vec<UniformDecl> {
        let mut out: Vec<UniformDecl> = Vec::new();
        for (_, src) in self.sources() {
            for u in parse_uniforms(&src) {
                if &*u.glsl_type == "sampler2D" || out.iter().any(|o| o.name == u.name) {
                    continue;
                }
                out.push(u);
            }
        }
        out
    }

    /// `channelsFor`: explicit config wins, else defaults (Image reads the
    /// input, plus its own previous frame with `feedback`; a buffer reads
    /// its previous frame on 0 and the input on 1).
    fn channels(&self, buf: ShaderBuffer, feedback: bool) -> [ShaderChannel; 4] {
        use ShaderChannel as C;
        // `typeof cfg === 'object'` (arrays too); a missing key is undefined.
        let conf = match self.channels {
            Some(serde_json::Value::Object(m)) => m.get(buf.as_str()),
            _ => None,
        };
        if let Some(serde_json::Value::Array(codes)) = conf {
            return std::array::from_fn(|i| match codes.get(i) {
                Some(serde_json::Value::String(code)) => match code.as_str() {
                    "input" => C::Input,
                    "self" => C::Buffer(buf),
                    other => ShaderBuffer::parse(other).map_or(C::None, C::Buffer),
                },
                _ => C::None,
            });
        }
        match buf {
            ShaderBuffer::Image if feedback => {
                [C::Input, C::Buffer(ShaderBuffer::Image), C::None, C::None]
            }
            ShaderBuffer::Image => [C::Input, C::None, C::None, C::None],
            b => [C::Buffer(b), C::Input, C::None, C::None],
        }
    }
}

/// JS `String.prototype.trim`.
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// `shaderInputs`: the primary `input` layer, then one port per uniform.
fn shader_inputs(params: &Params) -> Vec<PortSpec> {
    let pp = PassParams::from(|k| params.get(k));
    let mut ports = vec![PortSpec::new("input", PortType::Layer)
        .label("input")
        .kind(PortKind::Spread)];
    for u in pp.uniforms() {
        ports.push(PortSpec::new(&*u.name, uniform_port_type(&u.glsl_type)).label(&*u.name));
    }
    ports
}

/// `Number(v ?? 0)`.
fn num(v: Value) -> f64 {
    if v.is_nullish() {
        0.0
    } else {
        v.to_number()
    }
}

/// The uniform's value from slot 0 of its port (`toVec2` / `toColor` /
/// `Number`). Colours are 0..1 on the wire and go to the shader as is.
/// (The web app divided by 255 here, which made every colour uniform
/// nearly black; fixed on purpose, no sample relies on it.)
fn uniform_value(ty: PortType, v: Value) -> UniformValue {
    match ty {
        PortType::Vec2 => UniformValue::Vec2([num(v.get("x")), num(v.get("y"))]),
        PortType::Color => {
            let a = match v.get("a") {
                x if x.is_nullish() => 1.0,
                x => x.to_number(),
            };
            UniformValue::Color([num(v.get("r")), num(v.get("g")), num(v.get("b")), a])
        }
        _ => UniformValue::Number(num(v)),
    }
}

/// `asLayer`: a spread of layers → nothing, the one layer, or a group.
fn as_layer(v: Option<&[Value]>) -> Option<Arc<SceneNode>> {
    let nodes: Vec<Arc<SceneNode>> = v?
        .iter()
        .filter(|x| x.truthy())
        .filter_map(|x| match x {
            Value::Scene(n) => Some(n.clone()),
            _ => None,
        })
        .collect();
    match nodes.len() {
        0 => None,
        1 => nodes.into_iter().next(),
        _ => Some(Arc::new(SceneNode::group(nodes))),
    }
}

fn shader(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx: &mut EvalCtx| {
        let pp = PassParams::from(|k| cx.param(k));
        let feedback = param_or(cx, "feedback", Value::Bool(false)).truthy();
        let decls = pp.uniforms();
        let uniforms: Vec<UniformValue> = decls
            .iter()
            .enumerate()
            .map(|(i, u)| {
                let v = inputs
                    .get(i + 1)
                    .and_then(|s| s.first())
                    .cloned()
                    .unwrap_or(Value::Undefined);
                uniform_value(uniform_port_type(&u.glsl_type), v)
            })
            .collect();
        let passes: Vec<ShaderPass> = pp
            .sources()
            .into_iter()
            .map(|(buf, source)| ShaderPass {
                buf,
                source,
                channels: pp.channels(buf, feedback),
            })
            .collect();
        let shape = ShaderShape {
            node_id: Arc::from(cx.node_id),
            passes: Arc::from(passes),
            uniform_decls: Arc::from(decls),
            uniforms: Arc::from(uniforms),
            input: as_layer(inputs.first().map(|s| &s[..])),
        };
        Ok(vec![one(Value::Scene(Arc::new(SceneNode::Shader(
            Arc::new(shape),
        ))))])
    })
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{n, node};
    use super::*;
    use crate::spec::Catalogue;
    use crate::value::{Color, Vec2};
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

    #[test]
    fn ports_follow_uniforms_across_passes() {
        let params = json!({
            "source": "uniform float a; uniform vec3 tint; uniform sampler2D s;",
            "bufferB": "uniform vec2 off; uniform float a;",
            "bufferA": "   ",
        });
        let cat = Catalogue::builtin();
        let ports =
            cat.resolved_inputs(cat.get("gpu/shader").unwrap(), params.as_object().unwrap());
        let got: Vec<_> = ports.iter().map(|p| (p.name.as_str(), p.ty)).collect();
        // Buffer B is scanned before Image.
        assert_eq!(
            got,
            vec![
                ("input", PortType::Layer),
                ("off", PortType::Vec2),
                ("a", PortType::Number),
                ("tint", PortType::Color),
            ]
        );
        assert_eq!(ports[0].kind, PortKind::Spread);
    }

    #[test]
    fn builds_the_ts_shape() {
        let mut h = node(
            "gpu/shader",
            json!({
                "source": "uniform float k; uniform vec2 p; uniform vec4 c;",
                "bufferA": "void mainImage(out vec4 o, in vec2 f){}",
                "channels": { "image": ["A", "self", "bogus"], "A": [] },
            }),
        );
        let col = Value::Color(Color {
            r: 1.0,
            g: 0.5,
            b: 0.0,
            a: 1.0,
        });
        let out = h
            .eval(vec![
                vec![],
                vec![Value::Bool(true)],
                vec![Value::Vec2(Vec2 { x: 3.0, y: 4.0 })],
                vec![col],
            ])
            .unwrap();
        let sh = shape(&out[0][0]);
        assert_eq!(&*sh.node_id, "n1");
        assert_eq!(sh.passes.len(), 2);
        assert_eq!(sh.passes[0].buf, ShaderBuffer::A);
        assert_eq!(sh.passes[0].channels, [ShaderChannel::None; 4]);
        assert_eq!(
            sh.passes[1].channels,
            [
                ShaderChannel::Buffer(ShaderBuffer::A),
                ShaderChannel::Buffer(ShaderBuffer::Image),
                ShaderChannel::None,
                ShaderChannel::None
            ]
        );
        assert_eq!(
            &sh.uniforms[..],
            &[
                UniformValue::Number(1.0),
                UniformValue::Vec2([3.0, 4.0]),
                UniformValue::Color([1.0, 0.5, 0.0, 1.0]),
            ]
        );
        assert!(sh.input.is_none());
    }

    #[test]
    fn defaults_and_feedback() {
        let mut h = node("gpu/shader", json!({ "feedback": true }));
        let out = h.eval(vec![vec![Value::Null, n(1.0)]]).unwrap();
        let sh = shape(&out[0][0]);
        assert_eq!(&*sh.passes[0].source, DEFAULT_SOURCE);
        assert_eq!(
            sh.passes[0].channels[..2],
            [
                ShaderChannel::Input,
                ShaderChannel::Buffer(ShaderBuffer::Image)
            ]
        );
        assert!(sh.uniform_decls.is_empty());
    }
}
