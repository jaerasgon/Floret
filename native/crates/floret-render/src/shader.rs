//! `gpu/shader` on wgpu: user GLSL (ShaderToy-style `mainImage`) compiled
//! with naga's GLSL frontend, plus the per-node pass runner. A port of the
//! shader half of `gl-vector-renderer.ts` and `glsl-shader.ts`.
//!
//! **Source compatibility.** Users write GLSL ES 3.00 bodies. naga reads
//! desktop GLSL 450 with Vulkan resource rules, so [`wrap`] builds a 450
//! preamble: the built-ins and every loose user `uniform` move into one
//! std140 block (the user's declarations are blanked in place, keeping line
//! numbers; `uniform float a, b;` and arrays included, though like TS only
//! single declarations become ports), and `iChannel0..3` become `sampler2D(texture, sampler)`
//! macros. A user `uniform sampler2D` reads iChannel0, like an unset GL
//! sampler.
//!
//! **Orientation.** Pass targets are stored GL-style (row 0 = bottom):
//! `gl_FragCoord` in wgpu counts rows from the top of memory, so with no
//! flip `fragCoord.y = 0.5` lands in row 0 and `texture(iChannelN, uv)`
//! reads back what the same pixel wrote. The wired input layer is copied in
//! flipped, and the Image pass is composited flipped.
//!
//! **Errors.** [`check`] parses and validates without a GPU; errors carry
//! user line numbers. A pass that fails to compile draws with the fallback
//! (input passthrough, magenta when nothing is wired), as in TS; the errors
//! are kept per node for the editor ([`ShaderRunner::errors`]).

use floret_engine::shader::{uniform_declarations, ShaderBuffer, ShaderChannel, ShaderShape};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Pass targets: 8-bit RGBA, like the TS `UNSIGNED_BYTE` FBOs.
const PASS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Drop a compiled program after this many frames unused.
const PROGRAM_TTL_FRAMES: u64 = 600;
const UNIFORM_BLOCK: &str = "FloretUniforms";

/// A compile error, `line` in the user's source (0 = not tied to a line).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderError {
    pub line: u32,
    pub message: String,
}

impl fmt::Display for ShaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "L{}: {}", self.line, self.message)
        } else {
            f.write_str(&self.message)
        }
    }
}

/// The renderer's per-frame built-ins (`RenderClock` in TS): the engine
/// frame's `t`, `dt`, `frameCount` and mouse (centre origin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderClock {
    pub time: f64,
    pub time_delta: f64,
    pub frame: f64,
    pub mouse_x: f64,
    pub mouse_y: f64,
}

impl Default for RenderClock {
    fn default() -> Self {
        RenderClock {
            time: 0.0,
            time_delta: 1.0 / 60.0,
            frame: 0.0,
            mouse_x: 0.0,
            mouse_y: 0.0,
        }
    }
}

// ─── GLSL wrapping ───────────────────────────────────────────────────────────

/// A user shader wrapped for naga.
pub struct Wrapped {
    pub text: String,
    /// Lines before the user's first line.
    pub preamble_lines: u32,
    /// Lines in the user's source.
    pub user_lines: u32,
}

const BUILTIN_MEMBERS: &str = "    float iTime;
    float iTimeDelta;
    int iFrame;
    vec3 iResolution;
    vec4 iMouse;
";

const RESOURCES: &str = "layout(set = 0, binding = 1) uniform sampler floret_sampler;
layout(set = 0, binding = 2) uniform texture2D floret_channel0;
layout(set = 0, binding = 3) uniform texture2D floret_channel1;
layout(set = 0, binding = 4) uniform texture2D floret_channel2;
layout(set = 0, binding = 5) uniform texture2D floret_channel3;
layout(location = 0) out vec4 floret_fragColor;
#define iChannel0 sampler2D(floret_channel0, floret_sampler)
#define iChannel1 sampler2D(floret_channel1, floret_sampler)
#define iChannel2 sampler2D(floret_channel2, floret_sampler)
#define iChannel3 sampler2D(floret_channel3, floret_sampler)
";

const EPILOGUE: &str = "
void main() { mainImage(floret_fragColor, gl_FragCoord.xy); }
";

/// Wrap `source` (a `mainImage` shader) as a GLSL 450 fragment shader.
pub fn wrap(source: &str) -> Wrapped {
    let mut body = source.as_bytes().to_vec();
    let mut members = String::new();
    let mut samplers = String::new();
    let mut seen: Vec<String> = Vec::new();
    for d in uniform_declarations(source) {
        // Built-ins stay put: re-declaring one is a duplicate, as in GL.
        if floret_engine::shader::BUILTIN_UNIFORMS.contains(&&*d.decl.name) {
            continue;
        }
        for c in &mut body[d.span.clone()] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
        let name = d.decl.name.to_string();
        if seen.contains(&name) {
            continue;
        }
        if &*d.decl.glsl_type == "sampler2D" {
            samplers.push_str(&format!(
                "#define {name} sampler2D(floret_channel0, floret_sampler)\n"
            ));
        } else {
            members.push_str(&format!("    {} {name};\n", d.decl.glsl_type));
        }
        seen.push(name);
    }
    // Forms the port parser skips but GLSL ES accepts (`uniform float a, b;`,
    // arrays) move into the block too; like TS they get no ports and read 0.
    for u in other_uniforms(source) {
        if u.glsl_type == "sampler2D" && u.declarators.iter().any(|(_, d)| !d.is_empty()) {
            // Sampler arrays have no equivalent; naga reports them in place.
            continue;
        }
        for c in &mut body[u.span.clone()] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
        for (name, dims) in u.declarators {
            if floret_engine::shader::BUILTIN_UNIFORMS.contains(&name.as_str())
                || seen.contains(&name)
            {
                continue;
            }
            if u.glsl_type == "sampler2D" && dims.is_empty() {
                samplers.push_str(&format!(
                    "#define {name} sampler2D(floret_channel0, floret_sampler)\n"
                ));
            } else {
                members.push_str(&format!("    {} {name}{dims};\n", u.glsl_type));
            }
            seen.push(name);
        }
    }
    let preamble = format!(
        "#version 450 core\nlayout(set = 0, binding = 0, std140) uniform {UNIFORM_BLOCK} {{\n{BUILTIN_MEMBERS}{members}}};\n{RESOURCES}{samplers}"
    );
    let preamble_lines = preamble.matches('\n').count() as u32;
    // Blanking only touched whole ASCII statements.
    let body = String::from_utf8(body).unwrap_or_default();
    Wrapped {
        text: format!("{preamble}{body}{EPILOGUE}"),
        preamble_lines,
        user_lines: source.matches('\n').count() as u32 + 1,
    }
}

/// A loose `uniform` statement [`uniform_declarations`] doesn't match:
/// several declarators and/or array dimensions.
struct OtherUniform {
    span: std::ops::Range<usize>,
    glsl_type: String,
    /// (name, array suffix such as `[4]`)
    declarators: Vec<(String, String)>,
}

fn other_uniforms(source: &str) -> Vec<OtherUniform> {
    let text = floret_engine::shader::strip_comments(source);
    let simple: Vec<usize> = uniform_declarations(source)
        .iter()
        .map(|d| d.span.start)
        .collect();
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(len) = text[start..].find(';') {
        let end = start + len;
        if let Some(u) = parse_other_uniform(&text[start..end]) {
            let span = start + u.0..end + 1;
            if !simple.contains(&span.start) {
                out.push(OtherUniform {
                    span,
                    glsl_type: u.1,
                    declarators: u.2,
                });
            }
        }
        start = end + 1;
    }
    out
}

type Declarators = Vec<(String, String)>;

/// `uniform [precision] TYPE name[dims]?, name[dims]?…` at the end of
/// `stmt` (the text between two `;`), starting a statement. Returns the
/// offset of `uniform`, the type and the declarators.
fn parse_other_uniform(stmt: &str) -> Option<(usize, String, Declarators)> {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let at = stmt.match_indices("uniform").map(|(i, _)| i).find(|&i| {
        let before = stmt[..i].trim_end();
        let starts_statement = before.is_empty()
            || before.ends_with('}')
            || before
                .lines()
                .last()
                .is_some_and(|l| l.trim_start().starts_with('#'));
        let bounded = !stmt[i + 7..].starts_with(word);
        starts_statement && bounded
    })?;
    let rest = &stmt[at + 7..];
    if rest.contains(['{', '=', '(']) {
        return None;
    }
    let mut words = rest.split_whitespace().peekable();
    let mut glsl_type = words.next()?;
    if matches!(glsl_type, "lowp" | "mediump" | "highp") {
        glsl_type = words.next()?;
    }
    if !glsl_type.chars().all(word) {
        return None;
    }
    let decls: String = words.collect::<Vec<_>>().join(" ");
    let mut declarators = Vec::new();
    for d in decls.split(',') {
        let d = d.trim();
        let name_end = d.find(|c: char| !word(c)).unwrap_or(d.len());
        let (name, dims) = d.split_at(name_end);
        let dims: String = dims.chars().filter(|c| !c.is_whitespace()).collect();
        let valid_dims = dims.is_empty()
            || (dims.starts_with('[')
                && dims.ends_with(']')
                && dims
                    .chars()
                    .all(|c| word(c) || matches!(c, '[' | ']' | '+' | '-' | '*' | '/')));
        if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) || !valid_dims {
            return None;
        }
        declarators.push((name.to_owned(), dims));
    }
    Some((at, glsl_type.to_owned(), declarators))
}

// ─── Compiling ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scalar {
    Float,
    Sint,
    Uint,
}

/// A member of the uniform block: byte offset, scalar kind, component count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Member {
    offset: u32,
    scalar: Scalar,
    size: u32,
}

/// Where everything lives in the uniform block.
#[derive(Clone, Debug, Default)]
struct UniformLayout {
    size: u32,
    members: HashMap<String, Member>,
}

impl UniformLayout {
    fn from_module(module: &naga::Module) -> UniformLayout {
        let mut out = UniformLayout::default();
        let Some((_, ty)) = module
            .types
            .iter()
            .find(|(_, t)| t.name.as_deref() == Some(UNIFORM_BLOCK))
        else {
            return out;
        };
        let naga::TypeInner::Struct { members, span } = &ty.inner else {
            return out;
        };
        out.size = *span;
        for m in members {
            let scalar_of = |s: naga::Scalar| match s.kind {
                naga::ScalarKind::Float if s.width == 4 => Some(Scalar::Float),
                naga::ScalarKind::Sint if s.width == 4 => Some(Scalar::Sint),
                naga::ScalarKind::Uint if s.width == 4 => Some(Scalar::Uint),
                _ => None,
            };
            let (scalar, size) = match module.types[m.ty].inner {
                naga::TypeInner::Scalar(s) => (scalar_of(s), 1),
                naga::TypeInner::Vector { size, scalar } => (scalar_of(scalar), size as u32),
                // Matrices, arrays, structs: declared, left zero.
                _ => (None, 0),
            };
            if let (Some(name), Some(scalar)) = (&m.name, scalar) {
                out.members.insert(
                    name.clone(),
                    Member {
                        offset: m.offset,
                        scalar,
                        size,
                    },
                );
            }
        }
        out
    }

    fn put(&self, bytes: &mut [u8], name: &str, v: &[f64]) {
        let Some(m) = self.members.get(name) else {
            return;
        };
        for i in 0..m.size as usize {
            let x = v.get(i).copied().unwrap_or(0.0);
            let word = match m.scalar {
                Scalar::Float => (x as f32).to_bits(),
                Scalar::Sint => floret_engine::js::to_int32(x) as u32,
                Scalar::Uint => floret_engine::js::to_uint32(x),
            };
            let at = m.offset as usize + i * 4;
            if let Some(dst) = bytes.get_mut(at..at + 4) {
                dst.copy_from_slice(&word.to_le_bytes());
            }
        }
    }
}

/// A parsed + validated user shader.
struct Compiled {
    module: naga::Module,
    layout: UniformLayout,
}

/// Parse and validate a user shader without a GPU. `Ok` means naga accepts
/// it (wgpu may still reject what the backend can't express).
pub fn check(source: &str) -> Result<(), Vec<ShaderError>> {
    compile(source).map(|_| ())
}

fn compile(source: &str) -> Result<Compiled, Vec<ShaderError>> {
    let w = wrap(source);
    // naga's frontend has panicked on malformed input before; a bad shader
    // must never take the app down.
    let run = || -> Result<Compiled, Vec<ShaderError>> {
        let mut fe = naga::front::glsl::Frontend::default();
        let opts = naga::front::glsl::Options::from(naga::ShaderStage::Fragment);
        let module = fe.parse(&opts, &w.text).map_err(|e| {
            e.errors
                .iter()
                .map(|err| ShaderError {
                    line: user_line(&w, err.meta.location(&w.text).line_number),
                    message: err.kind.to_string(),
                })
                .collect::<Vec<_>>()
        })?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .map_err(|e| {
            let line = e
                .location(&w.text)
                .map_or(0, |l| user_line(&w, l.line_number));
            vec![ShaderError {
                line,
                message: error_chain(e.as_inner()),
            }]
        })?;
        let layout = UniformLayout::from_module(&module);
        Ok(Compiled { module, layout })
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|_| {
        Err(vec![ShaderError {
            line: 0,
            message: "internal shader compiler error".into(),
        }])
    })
}

/// A wrapped-source line → the user's line (0 inside the preamble /
/// epilogue).
fn user_line(w: &Wrapped, line: u32) -> u32 {
    match line.saturating_sub(w.preamble_lines) {
        n if n > w.user_lines => 0,
        n => n,
    }
}

/// `error: cause: cause…`, skipping repeats.
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut msg = e.to_string();
    let mut cur = e.source();
    while let Some(c) = cur {
        let s = c.to_string();
        if !msg.contains(&s) {
            msg.push_str(": ");
            msg.push_str(&s);
        }
        cur = c.source();
    }
    msg
}

/// TS `FALLBACK_FRAGMENT`: the pass's iChannel0, or magenta when that's
/// empty.
const FALLBACK_SOURCE: &str = "void mainImage(out vec4 c, in vec2 fc){
  vec2 uv = fc / iResolution.xy;
  vec4 src = texture(iChannel0, uv);
  c = src.a > 0.0 ? src : vec4(0.85, 0.1, 0.45, 1.0);
}";

// ─── The runner ──────────────────────────────────────────────────────────────

struct Program {
    pipeline: wgpu::RenderPipeline,
    layout: UniformLayout,
}

struct CacheEntry {
    program: Result<Arc<Program>, Vec<ShaderError>>,
    last_used: u64,
}

/// A pass target (single-sample, `PASS_FORMAT`, canvas-sized).
struct Target {
    view: wgpu::TextureView,
    /// For the final composite (the renderer's texture layout).
    bind: wgpu::BindGroup,
}

/// Persistent ping-pong buffers for one `nodeId:buf`.
struct Feedback {
    read: usize,
    write: usize,
    used: bool,
}

/// Which texture a channel binds this frame.
#[derive(Clone, Copy)]
enum Source {
    Transparent,
    Input,
    Target(usize),
    /// An uploaded lookup table, by image id.
    Lut(u64),
}

/// A lookup-table image on the GPU (`gpu/effect`'s lut mode).
struct LutTexture {
    view: wgpu::TextureView,
    last_used: u64,
}

/// Frames a table stays uploaded after its last use.
const LUT_TTL_FRAMES: u64 = 120;

struct PassPlan {
    program: Arc<Program>,
    target: usize,
    bind: wgpu::BindGroup,
}

struct DrawPlan {
    passes: Vec<PassPlan>,
    /// The Image pass's target, composited onto the parent.
    output: Option<usize>,
    /// The target holding the map layer ([`ShaderChannel::Map`]).
    map: Option<usize>,
}

/// Compiles, caches and runs `gpu/shader` draws for one [`crate::Renderer`].
pub struct ShaderRunner {
    pass_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    vs: wgpu::ShaderModule,
    /// Flipped copy of the input layer → `input`.
    flip_pipe: wgpu::RenderPipeline,
    /// Straight-alpha, GL-oriented target → premultiplied scene.
    composite_pipe: wgpu::RenderPipeline,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    transparent: wgpu::TextureView,
    programs: HashMap<String, CacheEntry>,
    fallback: Option<Arc<Program>>,
    fallback_tried: bool,
    size: (u32, u32),
    input: Option<wgpu::TextureView>,
    targets: Vec<Target>,
    /// Targets not owned by a feedback pair (reused across draws).
    free: Vec<usize>,
    feedback: HashMap<String, Feedback>,
    uniforms: Vec<wgpu::Buffer>,
    plans: Vec<DrawPlan>,
    errors: Vec<(Arc<str>, ShaderBuffer, Vec<ShaderError>)>,
    frame: u64,
    luts: HashMap<u64, LutTexture>,
}

const HELPER_WGSL: &str = "
@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}
";

const FLIP_WGSL: &str = "
@group(0) @binding(0) var src: texture_2d<f32>;
@fragment
fn fs_flip(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(src));
    let p = vec2<i32>(floor(pos.xy));
    return textureLoad(src, clamp(vec2<i32>(p.x, dims.y - 1 - p.y), vec2<i32>(0), dims - vec2<i32>(1)), 0);
}
";

const COMPOSITE_WGSL: &str = "
struct View { size: vec2<f32>, _pad: vec2<f32> };
@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var src: texture_2d<f32>;
@vertex
fn vs_view(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) tint: vec4<f32>)
    -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos.x / (view.size.x * 0.5), -pos.y / (view.size.y * 0.5), 0.0, 1.0);
}
@fragment
fn fs_composite(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(src));
    let p = vec2<i32>(floor(pos.xy));
    let c = textureLoad(src, clamp(vec2<i32>(p.x, dims.y - 1 - p.y), vec2<i32>(0), dims - vec2<i32>(1)), 0);
    return vec4<f32>(c.rgb * c.a, c.a);
}
";

impl ShaderRunner {
    /// `view_layout` / `tex_layout` / `tex_vertex`: the renderer's view
    /// uniform, texture bind group and composite-vertex layouts; `format` /
    /// `samples`: the scene targets the composite draws into.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_layout: &wgpu::BindGroupLayout,
        tex_layout: &wgpu::BindGroupLayout,
        tex_vertex: wgpu::VertexBufferLayout<'_>,
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> ShaderRunner {
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let pass_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("floret shader pass"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                tex_entry(2),
                tex_entry(3),
                tex_entry(4),
                tex_entry(5),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("floret shader pass"),
            bind_group_layouts: &[Some(&pass_layout)],
            immediate_size: 0,
        });
        let module = |label, src: &str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            })
        };
        let vs = module("floret shader vs", HELPER_WGSL);
        let flip = module("floret shader flip", FLIP_WGSL);
        let comp = module("floret shader composite", COMPOSITE_WGSL);
        let flip_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("floret shader flip"),
            bind_group_layouts: &[Some(tex_layout)],
            immediate_size: 0,
        });
        let comp_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("floret shader composite"),
            bind_group_layouts: &[Some(view_layout), Some(tex_layout)],
            immediate_size: 0,
        });
        let flip_pipe = fullscreen_pipeline(device, &flip_layout, &vs, &flip, "fs_flip");
        let composite_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("floret shader composite"),
            layout: Some(&comp_layout),
            vertex: wgpu::VertexState {
                module: &comp,
                entry_point: Some("vs_view"),
                compilation_options: Default::default(),
                buffers: &[Some(tex_vertex)],
            },
            primitive: Default::default(),
            // Scene passes always carry the shared stencil attachment.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::gpu::STENCIL_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState {
                    front: wgpu::StencilFaceState::IGNORE,
                    back: wgpu::StencilFaceState::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &comp,
                entry_point: Some("fs_composite"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("floret shader channels"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let transparent = {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("floret shader transparent"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: PASS_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                t.as_image_copy(),
                &[0; 4],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            t.create_view(&Default::default())
        };
        ShaderRunner {
            pass_layout,
            pipeline_layout,
            vs,
            flip_pipe,
            composite_pipe,
            tex_layout: tex_layout.clone(),
            sampler,
            transparent,
            programs: HashMap::new(),
            fallback: None,
            fallback_tried: false,
            size: (0, 0),
            input: None,
            targets: Vec::new(),
            free: Vec::new(),
            feedback: HashMap::new(),
            uniforms: Vec::new(),
            plans: Vec::new(),
            errors: Vec::new(),
            frame: 0,
            luts: HashMap::new(),
        }
    }

    /// Compile errors of the shader draws in the last frame, per node and
    /// pass. Empty when every pass compiled.
    pub fn errors(&self) -> &[(Arc<str>, ShaderBuffer, Vec<ShaderError>)] {
        &self.errors
    }

    /// The program for `source`, compiling (and caching) on first use.
    fn program(
        &mut self,
        device: &wgpu::Device,
        source: &str,
    ) -> Result<Arc<Program>, Vec<ShaderError>> {
        let frame = self.frame;
        if let Some(e) = self.programs.get_mut(source) {
            e.last_used = frame;
            return e.program.clone();
        }
        let program = self.build(device, source);
        self.programs.insert(
            source.to_owned(),
            CacheEntry {
                program: program.clone(),
                last_used: frame,
            },
        );
        program
    }

    fn build(&self, device: &wgpu::Device, source: &str) -> Result<Arc<Program>, Vec<ShaderError>> {
        let Compiled { module, layout } = compile(source)?;
        // Validated by naga already; the scope catches what the backend
        // still refuses, so a bad shader can't reach the uncaptured-error
        // handler (which panics by default).
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("floret user shader"),
            source: wgpu::ShaderSource::Naga(Cow::Owned(module)),
        });
        let pipeline = fullscreen_pipeline(device, &self.pipeline_layout, &self.vs, &fs, "main");
        match pollster::block_on(scope.pop()) {
            None => Ok(Arc::new(Program { pipeline, layout })),
            Some(e) => Err(vec![ShaderError {
                line: 0,
                message: error_chain(&e),
            }]),
        }
    }

    fn fallback(&mut self, device: &wgpu::Device) -> Option<Arc<Program>> {
        if !self.fallback_tried {
            self.fallback_tried = true;
            self.fallback = self.build(device, FALLBACK_SOURCE).ok();
        }
        self.fallback.clone()
    }

    fn ensure_size(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.size == size {
            return;
        }
        self.size = size;
        self.targets.clear();
        self.free.clear();
        self.feedback.clear();
        self.input = Some(self.texture(device, "floret shader input").0);
    }

    fn texture(&self, device: &wgpu::Device, label: &str) -> (wgpu::TextureView, wgpu::BindGroup) {
        // Textures start zeroed (wgpu initialises on first use): a new
        // feedback buffer reads transparent black, like the cleared TS FBOs.
        let view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: self.size.0,
                    height: self.size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: PASS_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.tex_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        (view, bind)
    }

    fn new_target(&mut self, device: &wgpu::Device) -> usize {
        let (view, bind) = self.texture(device, "floret shader target");
        self.targets.push(Target { view, bind });
        self.targets.len() - 1
    }

    /// A target of the draw's own that lives while the node draws (not
    /// shared with other draws' transient passes): its map channel.
    fn owned_target(&mut self, device: &wgpu::Device, key: String) -> usize {
        if let Some(f) = self.feedback.get_mut(&key) {
            f.used = true;
            return f.read;
        }
        let t = self.new_target(device);
        self.feedback.insert(
            key,
            Feedback {
                read: t,
                write: t,
                used: true,
            },
        );
        t
    }

    fn feedback_pair(&mut self, device: &wgpu::Device, key: String) -> (usize, usize) {
        if let Some(f) = self.feedback.get_mut(&key) {
            f.used = true;
            return (f.read, f.write);
        }
        let (read, write) = (self.new_target(device), self.new_target(device));
        self.feedback.insert(
            key,
            Feedback {
                read,
                write,
                used: true,
            },
        );
        (read, write)
    }

    /// Plan, compile and upload every shader draw of the frame (in command
    /// order). Records nothing; [`ShaderRunner::encode`] does that.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draws: &[Arc<ShaderShape>],
        size: (u32, u32),
        clock: &RenderClock,
    ) {
        self.frame += 1;
        self.plans.clear();
        self.errors.clear();
        let frame = self.frame;
        self.luts
            .retain(|_, t| t.last_used + LUT_TTL_FRAMES >= frame);
        if draws.is_empty() {
            self.programs
                .retain(|_, e| e.last_used + PROGRAM_TTL_FRAMES >= self.frame);
            return;
        }
        self.ensure_size(device, size);
        for f in self.feedback.values_mut() {
            f.used = false;
        }
        // Targets free for transient use this frame: everything not held by
        // a feedback pair. Recomputed per frame (pairs may have been dropped).
        let mut uniform_slot = 0;
        for sh in draws {
            let plan = self.plan_draw(device, queue, sh, size, clock, &mut uniform_slot);
            self.plans.push(plan);
        }
        // Drop feedback buffers whose node didn't draw this frame (their
        // targets become free).
        self.feedback.retain(|_, f| f.used);
        self.programs
            .retain(|_, e| e.last_used + PROGRAM_TTL_FRAMES >= self.frame);
    }

    fn plan_draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sh: &ShaderShape,
        size: (u32, u32),
        clock: &RenderClock,
        uniform_slot: &mut usize,
    ) -> DrawPlan {
        let mut referenced: Vec<ShaderBuffer> = Vec::new();
        for p in sh.passes.iter() {
            for c in p.channels {
                if let ShaderChannel::Buffer(b) = c {
                    if !referenced.contains(&b) {
                        referenced.push(b);
                    }
                }
            }
        }
        let mut pairs: HashMap<ShaderBuffer, (usize, usize)> = HashMap::new();
        for b in &referenced {
            let key = format!("{}:{}", sh.node_id, b.as_str());
            pairs.insert(*b, self.feedback_pair(device, key));
        }
        let map = sh
            .map
            .as_ref()
            .map(|_| self.owned_target(device, format!("{}:map", sh.node_id)));
        let lut = sh
            .lut
            .as_ref()
            .and_then(|img| self.upload_lut(device, queue, img));
        // Transient targets: any target not owned by a live pair.
        let owned: Vec<usize> = self
            .feedback
            .values()
            .flat_map(|f| [f.read, f.write])
            .collect();
        self.free = (0..self.targets.len())
            .filter(|i| !owned.contains(i))
            .collect();
        let mut transient: HashMap<ShaderBuffer, usize> = HashMap::new();
        let mut rendered: Vec<ShaderBuffer> = Vec::new();
        let mut passes = Vec::new();
        let mut output = None;
        for pass in sh.passes.iter() {
            let target = match pairs.get(&pass.buf) {
                Some(&(_, w)) => w,
                None => match transient.get(&pass.buf) {
                    Some(&t) => t,
                    None => {
                        let t = match self.free.pop() {
                            Some(t) => t,
                            None => self.new_target(device),
                        };
                        transient.insert(pass.buf, t);
                        t
                    }
                },
            };
            let source_of = |c: ShaderChannel| match c {
                ShaderChannel::None => Source::Transparent,
                ShaderChannel::Input => Source::Input,
                ShaderChannel::Map => map.map_or(Source::Transparent, Source::Target),
                ShaderChannel::Lut => lut.map_or(Source::Transparent, Source::Lut),
                ShaderChannel::Buffer(b) if rendered.contains(&b) => Source::Target(
                    pairs
                        .get(&b)
                        .map(|p| p.1)
                        .or_else(|| transient.get(&b).copied())
                        .unwrap_or(usize::MAX),
                ),
                // Self / forward reference → the previous frame.
                ShaderChannel::Buffer(b) => pairs
                    .get(&b)
                    .map_or(Source::Transparent, |p| Source::Target(p.0)),
            };
            let sources = pass.channels.map(source_of);
            let program = match self.program(device, &pass.source) {
                Ok(p) => Some(p),
                Err(errs) => {
                    self.errors.push((sh.node_id.clone(), pass.buf, errs));
                    self.fallback(device)
                }
            };
            rendered.push(pass.buf);
            if pass.buf == ShaderBuffer::Image {
                output = Some(target);
            }
            let Some(program) = program else {
                continue;
            };
            let buffer = self.uniform_buffer(device, *uniform_slot, program.layout.size);
            *uniform_slot += 1;
            queue.write_buffer(&buffer, 0, &uniform_bytes(&program.layout, sh, size, clock));
            let bind = self.pass_bind_group(device, &buffer, sources);
            passes.push(PassPlan {
                program,
                target,
                bind,
            });
        }
        // Next frame reads what this frame wrote.
        for (b, (r, w)) in &pairs {
            if let Some(f) = self
                .feedback
                .get_mut(&format!("{}:{}", sh.node_id, b.as_str()))
            {
                f.read = *w;
                f.write = *r;
            }
        }
        DrawPlan {
            passes,
            output,
            map,
        }
    }

    /// The table's texture (uploaded on first use, as it is: straight
    /// colour, no flip), or `None` when it's too big for the device.
    fn upload_lut(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        img: &floret_engine::image::Image,
    ) -> Option<u64> {
        let id = img.id();
        if let Some(t) = self.luts.get_mut(&id) {
            t.last_used = self.frame;
            return Some(id);
        }
        let (w, h) = (img.width(), img.height());
        let max = device.limits().max_texture_dimension_2d;
        if w == 0 || h == 0 || w > max || h > max {
            return None;
        }
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("floret shader lut"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            img.rgba(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            size,
        );
        self.luts.insert(
            id,
            LutTexture {
                view: texture.create_view(&Default::default()),
                last_used: self.frame,
            },
        );
        Some(id)
    }

    fn uniform_buffer(&mut self, device: &wgpu::Device, slot: usize, size: u32) -> wgpu::Buffer {
        let size = u64::from(size.max(16)).next_multiple_of(16);
        while self.uniforms.len() <= slot {
            self.uniforms.push(make_uniform(device, 256));
        }
        if self.uniforms[slot].size() < size {
            self.uniforms[slot] = make_uniform(device, size.next_multiple_of(256));
        }
        self.uniforms[slot].clone()
    }

    fn pass_bind_group(
        &self,
        device: &wgpu::Device,
        uniforms: &wgpu::Buffer,
        sources: [Source; 4],
    ) -> wgpu::BindGroup {
        let views = sources.map(|s| match s {
            Source::Transparent => &self.transparent,
            Source::Input => self.input.as_ref().unwrap_or(&self.transparent),
            Source::Target(i) => self.targets.get(i).map_or(&self.transparent, |t| &t.view),
            Source::Lut(id) => self.luts.get(&id).map_or(&self.transparent, |t| &t.view),
        });
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
        ];
        for (i, v) in views.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: 2 + i as u32,
                resource: wgpu::BindingResource::TextureView(v),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("floret shader pass"),
            layout: &self.pass_layout,
            entries: &entries,
        })
    }

    /// Copy the layer holding draw `index`'s rendered map subtree (the
    /// renderer's bind group) into its map channel.
    pub(crate) fn encode_map(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        index: usize,
        src: &wgpu::BindGroup,
    ) {
        let Some(t) = self
            .plans
            .get(index)
            .and_then(|p| p.map)
            .and_then(|m| self.targets.get(m))
        else {
            return;
        };
        let mut pass = begin(encoder, &t.view, "floret shader map");
        pass.set_pipeline(&self.flip_pipe);
        pass.set_bind_group(0, src, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Record draw `index`'s passes. `input`: the renderer's bind group of
    /// the layer holding the rendered input subtree.
    pub(crate) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        index: usize,
        input: Option<&wgpu::BindGroup>,
    ) {
        let Some(plan) = self.plans.get(index) else {
            return;
        };
        if let (Some(src), Some(dst)) = (input, &self.input) {
            let mut pass = begin(encoder, dst, "floret shader input");
            pass.set_pipeline(&self.flip_pipe);
            pass.set_bind_group(0, src, &[]);
            pass.draw(0..3, 0..1);
        }
        for p in &plan.passes {
            let Some(t) = self.targets.get(p.target) else {
                continue;
            };
            let mut pass = begin(encoder, &t.view, "floret shader pass");
            pass.set_pipeline(&p.program.pipeline);
            pass.set_bind_group(0, &p.bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// The composite pipeline and draw `index`'s output (`None`: nothing
    /// to composite).
    pub(crate) fn output(&self, index: usize) -> Option<(&wgpu::RenderPipeline, &wgpu::BindGroup)> {
        let t = self.plans.get(index)?.output?;
        Some((&self.composite_pipe, &self.targets.get(t)?.bind))
    }
}

fn make_uniform(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("floret shader uniforms"),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn begin<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: &str,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

fn fullscreen_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    vs: &wgpu::ShaderModule,
    fs: &wgpu::ShaderModule,
    entry: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("floret shader"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: vs,
            entry_point: Some("vs_fullscreen"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: fs,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            // The pass writes, it doesn't blend.
            targets: &[Some(wgpu::ColorTargetState {
                format: PASS_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// The uniform block for one pass: built-ins, then the node's uniforms
/// (GL setter rules: missing components 0, a vec4's w 1).
fn uniform_bytes(
    layout: &UniformLayout,
    sh: &ShaderShape,
    size: (u32, u32),
    clock: &RenderClock,
) -> Vec<u8> {
    let mut b = vec![0u8; layout.size.max(16) as usize];
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    layout.put(&mut b, "iTime", &[clock.time]);
    layout.put(&mut b, "iTimeDelta", &[clock.time_delta]);
    layout.put(&mut b, "iFrame", &[clock.frame]);
    layout.put(&mut b, "iResolution", &[w, h, 1.0]);
    // ShaderToy iMouse is pixels from the bottom-left; Floret's mouse is
    // centre-origin, y down.
    layout.put(
        &mut b,
        "iMouse",
        &[clock.mouse_x + w / 2.0, h / 2.0 - clock.mouse_y, 0.0, 0.0],
    );
    for (d, v) in sh.uniform_decls.iter().zip(sh.uniforms.iter()) {
        let mut c: Vec<f64> = v.components().to_vec();
        if &*d.glsl_type == "vec4" && c.len() < 4 {
            c.resize(3, 0.0);
            c.push(1.0);
        }
        layout.put(&mut b, &d.name, &c);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use floret_engine::shader::UniformValue;

    const PLASMA: &str = "uniform float scale;
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
  vec2 uv = (fragCoord - 0.5 * iResolution.xy) / iResolution.y;
  uv *= scale;
  fragColor = vec4(0.5 + 0.5 * cos(vec3(0.0, 2.0, 4.0) + uv.x + iTime), 1.0);
}";

    #[test]
    fn wraps_and_lifts_uniforms() {
        let w = wrap(PLASMA);
        assert!(w.text.starts_with("#version 450 core\n"));
        assert!(w.text.contains("    float scale;\n"));
        // The user's declaration is blanked in place: same line count.
        let user = &w.text[w.text.find("void mainImage").unwrap() - 21..];
        assert!(user.starts_with("                    \nvoid"));
        assert_eq!(
            w.text.lines().nth(w.preamble_lines as usize + 1).unwrap(),
            "void mainImage(out vec4 fragColor, in vec2 fragCoord) {"
        );
    }

    #[test]
    fn lifts_multi_declarator_and_array_uniforms() {
        let src = "uniform float a, b;
uniform highp vec3 cols[2];
uniform float k;
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
  fragColor = vec4(cols[0] + cols[1] + a + b + k, 1.0);
}";
        let c = compile(src).unwrap_or_else(|e| panic!("{e:?}"));
        let m = &c.layout.members;
        assert!(m.contains_key("a") && m.contains_key("b") && m.contains_key("k"));
        let w = wrap(src);
        assert!(w.text.contains("    vec3 cols[2];\n"), "{}", w.text);
        // Not lifted: the user's own block, and an initialiser (an error in
        // GLSL ES too).
        assert!(parse_other_uniform("uniform Foo { float x; }").is_none());
        assert!(parse_other_uniform("uniform float x = 1.0").is_none());
        assert!(parse_other_uniform("vec3 c = vec3(1.0)").is_none());
        assert_eq!(
            parse_other_uniform("}\n uniform float a , b[N * 2]").map(|u| u.2),
            Some(vec![
                ("a".to_owned(), String::new()),
                ("b".to_owned(), "[N*2]".to_owned())
            ])
        );
    }

    #[test]
    fn compiles_and_lays_out_uniforms() {
        let c = compile(PLASMA).unwrap_or_else(|e| panic!("{e:?}"));
        let m = &c.layout.members;
        assert_eq!(m["iTime"].offset, 0);
        assert_eq!(m["iFrame"].scalar, Scalar::Sint);
        assert_eq!(m["iResolution"].offset, 16);
        assert_eq!(m["iMouse"].offset, 32);
        assert_eq!(m["scale"].offset, 48);
        assert!(c.layout.size >= 52);
    }

    #[test]
    fn sample_shaders_compile() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
        for id in [
            "shader-plasma",
            "shader-feedback",
            "shader-reaction-diffusion",
        ] {
            let text = std::fs::read_to_string(dir.join(format!("{id}.flora"))).unwrap();
            let patch: serde_json::Value = serde_json::from_str(&text).unwrap();
            for n in patch["nodes"].as_array().unwrap() {
                for key in ["source", "bufferA"] {
                    if let Some(src) = n["params"][key].as_str() {
                        assert_eq!(check(src), Ok(()), "{id} {key}");
                    }
                }
            }
        }
        assert_eq!(check(FALLBACK_SOURCE), Ok(()));
        assert_eq!(check(floret_engine::nodes::gpu::DEFAULT_SOURCE), Ok(()));
    }

    #[test]
    fn errors_map_to_user_lines() {
        let src = "void mainImage(out vec4 c, in vec2 f) {\n  c = vec4(nope, 1.0);\n}";
        let errs = check(src).unwrap_err();
        assert_eq!(errs[0].line, 2, "{errs:?}");
        assert!(!errs[0].message.is_empty());
        // Missing entry point: reported, no line in the user's source.
        let errs = check("void other() {}").unwrap_err();
        assert_eq!(errs[0].line, 0, "{errs:?}");
        // A type error caught by validation, not parsing.
        assert!(check("void mainImage(out vec4 c, in vec2 f) { c = 1; }").is_err());
    }

    #[test]
    fn samplers_and_texture_builtins() {
        let src = "uniform sampler2D extra;
uniform vec4 tint;
uniform int steps;
void mainImage(out vec4 c, in vec2 f) {
  ivec2 sz = textureSize(iChannel1, 0);
  c = texture(extra, f / vec2(sz)) * tint + texelFetch(iChannel2, ivec2(f), 0) * float(steps);
}";
        assert_eq!(check(src), Ok(()));
        let c = compile(src).unwrap();
        assert!(!c.layout.members.contains_key("extra"));
        assert_eq!(c.layout.members["tint"].size, 4);
        assert_eq!(c.layout.members["steps"].scalar, Scalar::Sint);
    }

    #[test]
    fn uniform_values_follow_gl_setters() {
        use floret_engine::shader::UniformDecl;
        let src = "uniform float k; uniform vec4 col; uniform int n;
void mainImage(out vec4 c, in vec2 f) { c = col * k * float(n); }";
        let c = compile(src).unwrap();
        let decl = |n: &str, t: &str| UniformDecl {
            name: Arc::from(n),
            glsl_type: Arc::from(t),
        };
        let sh = ShaderShape {
            node_id: Arc::from("n"),
            passes: Arc::from(vec![]),
            uniform_decls: Arc::from(vec![
                decl("k", "float"),
                decl("col", "vec4"),
                decl("n", "int"),
            ]),
            uniforms: Arc::from(vec![
                UniformValue::Number(2.5),
                UniformValue::Vec2([0.25, 0.5]),
                UniformValue::Number(-3.7),
            ]),
            input: None,
            map: None,
            lut: None,
        };
        let clock = RenderClock {
            time: 1.5,
            frame: 7.0,
            mouse_x: 10.0,
            mouse_y: 20.0,
            ..Default::default()
        };
        let b = uniform_bytes(&c.layout, &sh, (200, 100), &clock);
        let f = |at: usize| f32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let i = |at: usize| i32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let m = &c.layout.members;
        assert_eq!(f(0), 1.5);
        assert_eq!(i(8), 7);
        assert_eq!(f(16), 200.0);
        assert_eq!(f(32), 110.0); // mouse x + w/2
        assert_eq!(f(36), 30.0); // h/2 - mouse y
        assert_eq!(f(m["k"].offset as usize), 2.5);
        let col = m["col"].offset as usize;
        assert_eq!(
            [f(col), f(col + 4), f(col + 8), f(col + 12)],
            [0.25, 0.5, 0.0, 1.0]
        );
        assert_eq!(i(m["n"].offset as usize), -3);
    }
}
