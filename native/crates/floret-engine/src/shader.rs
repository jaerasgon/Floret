//! The `gpu/shader` scene payload and the GPU-free half of the GLSL plumbing.
//! A port of `src/runtime/gl/glsl-shader.ts` (uniform parsing) plus the
//! `ShaderShape` types from `src/runtime/scene.ts`.
//!
//! The engine only describes a shader draw; `floret-render` compiles the
//! GLSL (naga) and runs the passes.

use crate::value::Value;
use indexmap::IndexMap;
use std::ops::Range;
use std::sync::Arc;

/// ShaderToy built-ins the renderer supplies — never exposed as node ports.
pub const BUILTIN_UNIFORMS: [&str; 11] = [
    "iTime",
    "iTimeDelta",
    "iFrame",
    "iResolution",
    "iMouse",
    "iDate",
    "iChannel0",
    "iChannel1",
    "iChannel2",
    "iChannel3",
    "iChannelResolution",
];

/// A user `uniform` declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniformDecl {
    pub name: Arc<str>,
    pub glsl_type: Arc<str>,
}

/// `uniformPortType`: GLSL uniform type → Floret port type name.
pub fn uniform_port_type(glsl_type: &str) -> crate::spec::PortType {
    use crate::spec::PortType;
    match glsl_type {
        "vec2" => PortType::Vec2,
        // Colours are the common vec3 / vec4 use.
        "vec3" | "vec4" => PortType::Color,
        // An extra iChannel, fed by a wired layer.
        "sampler2D" => PortType::Layer,
        _ => PortType::Number,
    }
}

/// `parseUniforms`: the user-declared (non-built-in) uniforms in `source`,
/// in declaration order, de-duplicated by name. Comments are ignored.
pub fn parse_uniforms(source: &str) -> Vec<UniformDecl> {
    let mut out: Vec<UniformDecl> = Vec::new();
    for d in uniform_declarations(source) {
        if BUILTIN_UNIFORMS.contains(&&*d.decl.name) || out.iter().any(|u| u.name == d.decl.name) {
            continue;
        }
        out.push(d.decl);
    }
    out
}

/// One `uniform <type> <name>;` statement and where it sits in the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniformStatement {
    pub decl: UniformDecl,
    /// Byte range from `uniform` through the closing `;`.
    pub span: Range<usize>,
}

/// Every statement `parseUniforms` would match (built-ins and repeats
/// included), with byte spans into `source`. The renderer uses the spans to
/// lift loose uniforms into a uniform block.
///
/// Like TS: one declaration per statement, an optional precision qualifier;
/// arrays, initialisers and `uniform float a, b;` are not recognised.
pub fn uniform_declarations(source: &str) -> Vec<UniformStatement> {
    let text = strip_comments(source);
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    // `stripped.split(';')`, each piece matched with a `;` appended: the
    // statement's only `;` is its terminator.
    while start <= b.len() {
        let end = text[start..].find(';').map_or(b.len(), |i| start + i);
        if end < b.len() {
            if let Some((decl, at)) = match_statement(&text[start..end]) {
                out.push(UniformStatement {
                    decl,
                    span: start + at..end + 1,
                });
            }
        }
        start = end + 1;
    }
    out
}

/// JS regex `\s`.
fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0B}'
            | '\u{0C}'
            | '\r'
            | ' '
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    ) || ('\u{2000}'..='\u{200A}').contains(&c)
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// `/\buniform\s+(?:(?:lowp|mediump|highp)\s+)?([A-Za-z_]\w*)\s+([A-Za-z_]\w*)\s*;/`
/// against `stmt + ";"`. Returns the declaration and the offset of `uniform`.
fn match_statement(stmt: &str) -> Option<(UniformDecl, usize)> {
    let b = stmt.as_bytes();
    let mut from = 0;
    while let Some(i) = stmt[from..].find("uniform").map(|i| from + i) {
        from = i + 1;
        if i > 0 && is_word(b[i - 1]) {
            continue;
        }
        let rest = &stmt[i + "uniform".len()..];
        let Some(rest) = skip_space(rest, true) else {
            continue;
        };
        // Greedy optional qualifier first, then without it (backtracking).
        let qualified = ident(rest)
            .filter(|(q, _)| matches!(*q, "lowp" | "mediump" | "highp"))
            .and_then(|(_, r)| skip_space(r, true))
            .and_then(type_and_name);
        if let Some(d) = qualified.or_else(|| type_and_name(rest)) {
            return Some((d, i));
        }
    }
    None
}

/// `([A-Za-z_]\w*)\s+([A-Za-z_]\w*)\s*$`
fn type_and_name(s: &str) -> Option<UniformDecl> {
    let (ty, r) = ident(s)?;
    let r = skip_space(r, true)?;
    let (name, r) = ident(r)?;
    if !r.chars().all(js_space) {
        return None;
    }
    Some(UniformDecl {
        name: Arc::from(name),
        glsl_type: Arc::from(ty),
    })
}

fn ident(s: &str) -> Option<(&str, &str)> {
    let b = s.as_bytes();
    if b.is_empty() || !(b[0].is_ascii_alphabetic() || b[0] == b'_') {
        return None;
    }
    let n = b.iter().position(|&c| !is_word(c)).unwrap_or(b.len());
    Some((&s[..n], &s[n..]))
}

/// Skip `\s*` (or `\s+` when `required`).
fn skip_space(s: &str, required: bool) -> Option<&str> {
    let t = s.trim_start_matches(js_space);
    (!required || t.len() < s.len()).then_some(t)
}

/// Blank out block comments, then line comments, as TS does (in that
/// order). Comment bytes become spaces and newlines are kept, so byte
/// offsets and line numbers still match `src`.
pub fn strip_comments(src: &str) -> String {
    let mut b = src.as_bytes().to_vec();
    let blank = |b: &mut [u8], r: Range<usize>| {
        for c in &mut b[r] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    let mut i = 0;
    while let Some(p) = find(&b, b"/*", i) {
        let Some(q) = find(&b, b"*/", p + 2) else {
            break;
        };
        blank(&mut b, p..q + 2);
        i = q + 2;
    }
    let mut i = 0;
    while let Some(p) = find(&b, b"//", i) {
        let q = b[p..]
            .iter()
            .position(|&c| c == b'\n')
            .map_or(b.len(), |n| p + n);
        blank(&mut b, p..q);
        i = q;
    }
    // Only ASCII bytes of whole comments were replaced; multi-byte chars
    // inside comments became runs of spaces, which is still valid UTF-8.
    String::from_utf8(b).unwrap_or_default()
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

// ─── Scene payload ───────────────────────────────────────────────────────────

/// Which offscreen buffer a pass writes. `Image` is the final, composited pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderBuffer {
    A,
    B,
    C,
    D,
    Image,
}

impl ShaderBuffer {
    pub const BUFFERS: [ShaderBuffer; 4] = [
        ShaderBuffer::A,
        ShaderBuffer::B,
        ShaderBuffer::C,
        ShaderBuffer::D,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ShaderBuffer::A => "A",
            ShaderBuffer::B => "B",
            ShaderBuffer::C => "C",
            ShaderBuffer::D => "D",
            ShaderBuffer::Image => "image",
        }
    }

    pub fn parse(code: &str) -> Option<ShaderBuffer> {
        match code {
            "A" => Some(ShaderBuffer::A),
            "B" => Some(ShaderBuffer::B),
            "C" => Some(ShaderBuffer::C),
            "D" => Some(ShaderBuffer::D),
            "image" => Some(ShaderBuffer::Image),
            _ => None,
        }
    }

    /// The node param holding this pass's source.
    pub fn param_key(self) -> &'static str {
        match self {
            ShaderBuffer::A => "bufferA",
            ShaderBuffer::B => "bufferB",
            ShaderBuffer::C => "bufferC",
            ShaderBuffer::D => "bufferD",
            ShaderBuffer::Image => "source",
        }
    }
}

/// What an iChannel samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderChannel {
    None,
    /// The wired `input` layer.
    Input,
    /// The second layer, [`ShaderShape::map`] (built-in effects only).
    Map,
    /// The colour lookup table, [`ShaderShape::lut`] (built-in effects
    /// only), sampled as it is: no flip, straight colour.
    Lut,
    /// Another buffer's output (its own: the previous frame).
    Buffer(ShaderBuffer),
}

/// One fragment-shader pass: its source and its four iChannel routings.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderPass {
    pub buf: ShaderBuffer,
    pub source: Arc<str>,
    pub channels: [ShaderChannel; 4],
}

/// A uniform value resolved from the graph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UniformValue {
    Number(f64),
    Vec2([f64; 2]),
    Color([f64; 4]),
}

impl UniformValue {
    /// The components, as the GL setters read them.
    pub fn components(&self) -> &[f64] {
        match self {
            UniformValue::Number(v) => std::slice::from_ref(v),
            UniformValue::Vec2(v) => v,
            UniformValue::Color(v) => v,
        }
    }
}

/// `SceneNode::Shader`'s contents (`ShaderShape` in TS).
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderShape {
    /// Keys the renderer's persistent feedback buffers.
    pub node_id: Arc<str>,
    /// Present buffers A–D, then the Image pass.
    pub passes: Arc<[ShaderPass]>,
    /// Detected user uniforms (samplers excluded), and their values in the
    /// same order.
    pub uniform_decls: Arc<[UniformDecl]>,
    pub uniforms: Arc<[UniformValue]>,
    /// The `input` channel's layer (rendered to a texture first).
    pub input: Option<Arc<crate::scene::SceneNode>>,
    /// A second layer for [`ShaderChannel::Map`] (`gpu/effect`'s
    /// displacement map); `gpu/shader` never sets it.
    pub map: Option<Arc<crate::scene::SceneNode>>,
    /// Keep the input's last frames for the passes (`gpu/echo`); `None`
    /// for every other draw.
    pub history: Option<ShaderHistory>,
    /// An image for [`ShaderChannel::Lut`] (`gpu/effect`'s 3D colour
    /// lookup table, laid out as N slices of N × N side by side); `gpu/shader`
    /// never sets it.
    pub lut: Option<Arc<crate::image::Image>>,
}

/// A ring of the input's past frames the renderer keeps for one draw: the
/// passes read it as `iHistory` (a `sampler2DArray`), with `iHistoryHead`
/// (this frame's layer), `iHistoryCount` (frames held) and `iHistorySize`
/// (layers in the ring).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShaderHistory {
    /// Frames wanted, this one included (the renderer caps it by memory).
    pub frames: u32,
    /// Forget the frames held (they refill from this one).
    pub reset: bool,
}

impl ShaderShape {
    /// The TS object fields after `kind`.
    pub(crate) fn fields(&self, m: &mut IndexMap<String, Value>) {
        let obj = |pairs: Vec<(&str, Value)>| {
            Value::Object(Arc::new(
                pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
            ))
        };
        let channel = |c: &ShaderChannel| match c {
            ShaderChannel::None => Value::Null,
            ShaderChannel::Input => obj(vec![("kind", Value::str("input"))]),
            ShaderChannel::Map => obj(vec![("kind", Value::str("map"))]),
            ShaderChannel::Lut => obj(vec![("kind", Value::str("lut"))]),
            ShaderChannel::Buffer(b) => obj(vec![
                ("kind", Value::str("buffer")),
                ("buf", Value::str(b.as_str())),
            ]),
        };
        m.insert("nodeId".into(), Value::Str(self.node_id.clone()));
        let passes = self
            .passes
            .iter()
            .map(|p| {
                obj(vec![
                    ("buf", Value::str(p.buf.as_str())),
                    ("source", Value::Str(p.source.clone())),
                    (
                        "channels",
                        Value::list(p.channels.iter().map(channel).collect()),
                    ),
                ])
            })
            .collect();
        m.insert("passes".into(), Value::list(passes));
        let decls = self
            .uniform_decls
            .iter()
            .map(|d| {
                obj(vec![
                    ("name", Value::Str(d.name.clone())),
                    ("glslType", Value::Str(d.glsl_type.clone())),
                ])
            })
            .collect();
        m.insert("uniformDecls".into(), Value::list(decls));
        let nums = |v: &[f64]| Value::list(v.iter().map(|x| Value::Num(*x)).collect());
        let uniforms = self
            .uniform_decls
            .iter()
            .zip(self.uniforms.iter())
            .map(|(d, u)| {
                let (t, v) = match u {
                    UniformValue::Number(n) => ("number", Value::Num(*n)),
                    UniformValue::Vec2(v) => ("vec2", nums(v)),
                    UniformValue::Color(v) => ("color", nums(v)),
                };
                (
                    d.name.to_string(),
                    obj(vec![("t", Value::str(t)), ("v", v)]),
                )
            })
            .collect();
        m.insert("uniforms".into(), Value::Object(Arc::new(uniforms)));
        m.insert(
            "input".into(),
            self.input.clone().map_or(Value::Null, Value::Scene),
        );
        // Native-only, so absent unless set (keeps the TS traces comparable).
        if let Some(map) = &self.map {
            m.insert("map".into(), Value::Scene(map.clone()));
        }
        if let Some(h) = &self.history {
            m.insert(
                "history".into(),
                obj(vec![
                    ("frames", Value::Num(f64::from(h.frames))),
                    ("reset", Value::Bool(h.reset)),
                ]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &str) -> Vec<(String, String)> {
        parse_uniforms(src)
            .into_iter()
            .map(|u| (u.name.to_string(), u.glsl_type.to_string()))
            .collect()
    }

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    // Cases from `glsl-shader.test.ts`.
    #[test]
    fn parses_uniforms_like_ts() {
        let src = "uniform float strength;
      uniform vec2 offset;
      uniform vec3 tint;
      uniform sampler2D extra;
      void mainImage(out vec4 c, in vec2 f){ c = vec4(strength); }";
        assert_eq!(
            names(src),
            pairs(&[
                ("strength", "float"),
                ("offset", "vec2"),
                ("tint", "vec3"),
                ("extra", "sampler2D")
            ])
        );
        let src = "uniform float iTime; uniform vec3 iResolution; uniform sampler2D iChannel0; uniform float k;";
        assert_eq!(names(src), pairs(&[("k", "float")]));
        let src = "// uniform float commented;
      /* uniform float blockComment; */
      uniform float real;";
        assert_eq!(names(src), pairs(&[("real", "float")]));
        let src = "uniform highp float a; uniform float a; uniform mediump vec2 b;";
        assert_eq!(names(src), pairs(&[("a", "float"), ("b", "vec2")]));
    }

    #[test]
    fn regex_edge_cases() {
        // `highp` alone is a type name when no second identifier follows.
        assert_eq!(names("uniform highp x;"), pairs(&[("x", "highp")]));
        // No word boundary → no match; a later `uniform` in the statement can.
        assert_eq!(names("myuniform float a;"), pairs(&[]));
        assert_eq!(
            names("foo uniform float a uniform int b;"),
            pairs(&[("b", "int")])
        );
        // Arrays, initialisers and lists aren't parsed (v1).
        assert_eq!(names("uniform float k[4]; uniform float a, b;"), pairs(&[]));
        // A `/*` inside a line comment still opens a block comment (TS strips
        // block comments first).
        assert_eq!(
            names("// note /*\nuniform float hidden;\n*/ uniform float shown;"),
            pairs(&[("shown", "float")])
        );
        // Unterminated statement (no `;`) doesn't count.
        assert_eq!(names("uniform float tail"), pairs(&[]));
    }

    #[test]
    fn spans_cover_the_statement() {
        let src = "uniform float a; /* x */ uniform  highp vec2 b ;\nvoid f(){}";
        let decls = uniform_declarations(src);
        assert_eq!(&src[decls[0].span.clone()], "uniform float a;");
        assert_eq!(&src[decls[1].span.clone()], "uniform  highp vec2 b ;");
    }

    #[test]
    fn strip_comments_keeps_offsets() {
        let src = "a /* é\n */ b // c\nd";
        let s = strip_comments(src);
        assert_eq!(s.len(), src.len());
        assert_eq!(s.lines().count(), src.lines().count());
        assert!(s.starts_with("a ") && s.ends_with("\nd"));
        assert!(!s.contains('c'));
    }
}
