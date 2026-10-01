//! The wgpu renderer: replays a [`FrameBuilder`]'s command list. A port of
//! `VectorRenderer` in `src/runtime/gl/gl-vector-renderer.ts`.
//!
//! - One MSAA colour target per layer level (the base level resolves into
//!   the caller's texture), plus one shared MSAA stencil buffer. Polygon
//!   fills leave the stencil all-zero after their cover pass, so every pass
//!   can clear it and levels can share it.
//! - Layers (blend groups, rotated clips, blurred shapes) are a LIFO pool of
//!   full-viewport textures, kept across frames and rebuilt on resize.
//! - Draws within a level are recorded into one render pass; pushing or
//!   popping a layer ends the pass and starts one on the new target.

use crate::batch::{Blend, Cmd, Emitter, FillVertex, FrameBuilder, SdfInstance, TexVertex};
use crate::text::{TextKey, TextRasterizer};
use floret_engine::scene::{SceneNode, IDENTITY};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// The stencil format every scene pass uses.
pub const STENCIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Stencil8;
/// Blur uniforms live at 256-byte strides (dynamic offsets).
const BLUR_STRIDE: u64 = 256;
/// Drop a text texture after this many frames unused.
const TEXT_TTL_FRAMES: u64 = 240;
/// Image textures go sooner: a changing image makes a new id every frame.
/// Unused ones are recycled for new images of the same size first.
const IMAGE_TTL_FRAMES: u64 = 2;

/// Counts from the last [`Renderer::render`], for diagnostics / tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub sdf_instances: usize,
    /// Stencil-filled polygons and their vertex count.
    pub poly_fills: usize,
    pub fill_vertices: usize,
    pub text_quads: usize,
    pub image_quads: usize,
    pub commands: usize,
    pub layers: usize,
    /// Shapes the renderer couldn't draw (unsupported / non-finite).
    pub skipped: usize,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewUniform {
    size: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurUniform {
    dir: [f32; 2],
    sigma: f32,
    _pad: f32,
}

/// A vertex / uniform buffer that grows on demand and is rewritten per frame.
struct GrowBuffer {
    label: &'static str,
    usage: wgpu::BufferUsages,
    buf: Option<wgpu::Buffer>,
    cap: u64,
    /// Bumped on reallocation (bind groups referencing it must be rebuilt).
    generation: u64,
}

impl GrowBuffer {
    fn new(label: &'static str, usage: wgpu::BufferUsages) -> GrowBuffer {
        GrowBuffer {
            label,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            buf: None,
            cap: 0,
            generation: 0,
        }
    }

    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let len = data.len() as u64;
        if self.buf.is_none() || self.cap < len {
            let cap = (len * 3 / 2)
                .max(4096)
                .next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
            self.buf = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: cap,
                usage: self.usage,
                mapped_at_creation: false,
            }));
            self.cap = cap;
            self.generation += 1;
        }
        if let Some(b) = &self.buf {
            queue.write_buffer(b, 0, data);
        }
    }

    fn slice(&self) -> Option<wgpu::BufferSlice<'_>> {
        self.buf.as_ref().map(|b| b.slice(..))
    }
}

/// One layer level: an (optional) MSAA colour target that resolves into a
/// sampleable texture.
struct Layer {
    msaa: Option<wgpu::TextureView>,
    resolve: wgpu::TextureView,
    bind: wgpu::BindGroup,
}

/// Size-dependent render targets.
struct Targets {
    size: (u32, u32),
    base_msaa: Option<wgpu::TextureView>,
    stencil: wgpu::TextureView,
    layers: Vec<Layer>,
    /// Single-sample scratch for the horizontal blur pass.
    blur_tmp: Option<Layer>,
}

struct TextTexture {
    bind: wgpu::BindGroup,
    last_used: u64,
}

struct ImageTexture {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
    size: (u32, u32),
    last_used: u64,
}

#[derive(Clone, Copy)]
enum Level {
    Base,
    Layer(usize),
}

/// Draw a composite at the start of the next pass.
struct PendingComposite {
    layer: usize,
    blend: Blend,
    /// Vertex offset into the composite buffer (6 vertices).
    first_vertex: u32,
    /// World AABB worth compositing.
    bounds: [f32; 4],
}

pub struct Renderer {
    format: wgpu::TextureFormat,
    sample_count: u32,
    max_texture: u32,
    view_buf: wgpu::Buffer,
    view_bg: wgpu::BindGroup,
    tex_layout: wgpu::BindGroupLayout,
    blur_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    sdf_pipe: wgpu::RenderPipeline,
    winding_pipe: wgpu::RenderPipeline,
    cover_pipe: wgpu::RenderPipeline,
    tex_pipe: wgpu::RenderPipeline,
    composite_pipes: [wgpu::RenderPipeline; 4],
    gauss_pipe: wgpu::RenderPipeline,
    sdf_buf: GrowBuffer,
    fill_buf: GrowBuffer,
    tex_buf: GrowBuffer,
    comp_buf: GrowBuffer,
    blur_buf: GrowBuffer,
    blur_bg: Option<(u64, wgpu::BindGroup)>,
    targets: Option<Targets>,
    text_textures: HashMap<u64, TextTexture>,
    /// By image generation id.
    image_textures: HashMap<u64, ImageTexture>,
    /// Clear to transparent instead of the opaque background.
    transparent: bool,
    fb: FrameBuilder,
    text: TextRasterizer,
    frame: u64,
    stats: RenderStats,
}

impl Renderer {
    /// Build the pipelines. `format` is the colour format of the targets
    /// you'll render into — use a non-sRGB format (e.g. `Rgba8Unorm`): like
    /// WebGL, colours are written and blended in gamma space. `sample_count`
    /// is the MSAA level (1 or 4 are always valid).
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Renderer {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("floret-render"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.wgsl").into()),
        });
        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("floret-render view"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("floret-render texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("floret-render blur"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<BlurUniform>() as u64
                    ),
                },
                count: None,
            }],
        });
        let view_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("floret-render view"),
            contents: bytemuck::bytes_of(&ViewUniform {
                size: [1.0, 1.0],
                _pad: [0.0; 2],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let view_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("floret-render view"),
            layout: &view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_buf.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("floret-render linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shape_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("floret-render shapes"),
            bind_group_layouts: &[Some(&view_layout)],
            immediate_size: 0,
        });
        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("floret-render textured"),
            bind_group_layouts: &[Some(&view_layout), Some(&tex_layout)],
            immediate_size: 0,
        });
        let gauss_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("floret-render gauss"),
            bind_group_layouts: &[Some(&view_layout), Some(&tex_layout), Some(&blur_layout)],
            immediate_size: 0,
        });

        let sdf_attrs = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4
        ];
        let sdf_vb = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SdfInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &sdf_attrs,
        };
        let fill_attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
        let fill_vb = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<FillVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &fill_attrs,
        };
        let tex_attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let tex_vb = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TexVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &tex_attrs,
        };

        let premul = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let ignore = wgpu::StencilState {
            front: wgpu::StencilFaceState::IGNORE,
            back: wgpu::StencilFaceState::IGNORE,
            read_mask: 0,
            write_mask: 0,
        };
        let face = |compare, pass_op| wgpu::StencilFaceState {
            compare,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op,
        };
        let winding = wgpu::StencilState {
            front: face(
                wgpu::CompareFunction::Always,
                wgpu::StencilOperation::IncrementWrap,
            ),
            back: face(
                wgpu::CompareFunction::Always,
                wgpu::StencilOperation::DecrementWrap,
            ),
            read_mask: 0xff,
            write_mask: 0xff,
        };
        let cover = wgpu::StencilState {
            front: face(
                wgpu::CompareFunction::NotEqual,
                wgpu::StencilOperation::Zero,
            ),
            back: face(
                wgpu::CompareFunction::NotEqual,
                wgpu::StencilOperation::Zero,
            ),
            read_mask: 0xff,
            write_mask: 0xff,
        };

        let mk = |label: &str,
                  layout: &wgpu::PipelineLayout,
                  vs: &str,
                  fs: &str,
                  buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
                  topology: wgpu::PrimitiveTopology,
                  stencil: Option<wgpu::StencilState>,
                  blend: Option<wgpu::BlendState>,
                  write_mask: wgpu::ColorWrites,
                  samples: u32| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: stencil.map(|stencil| wgpu::DepthStencilState {
                    format: STENCIL_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil,
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: samples,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        use wgpu::PrimitiveTopology::{TriangleList, TriangleStrip};
        let all = wgpu::ColorWrites::ALL;
        let n = sample_count;
        let sdf_pipe = mk(
            "floret sdf",
            &shape_layout,
            "vs_sdf",
            "fs_sdf",
            &[Some(sdf_vb.clone())],
            TriangleStrip,
            Some(ignore.clone()),
            premul,
            all,
            n,
        );
        let winding_pipe = mk(
            "floret fill winding",
            &shape_layout,
            "vs_fill",
            "fs_fill",
            &[Some(fill_vb.clone())],
            TriangleList,
            Some(winding),
            None,
            wgpu::ColorWrites::empty(),
            n,
        );
        let cover_pipe = mk(
            "floret fill cover",
            &shape_layout,
            "vs_fill",
            "fs_fill",
            &[Some(fill_vb.clone())],
            TriangleList,
            Some(cover),
            premul,
            all,
            n,
        );
        let tex_pipe = mk(
            "floret tex",
            &scene_layout,
            "vs_tex",
            "fs_tex",
            &[Some(tex_vb.clone())],
            TriangleList,
            Some(ignore.clone()),
            premul,
            all,
            n,
        );
        let comp = |blend: Blend| {
            use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation as O};
            let c = |src, dst| C {
                src_factor: src,
                dst_factor: dst,
                operation: O::Add,
            };
            let state = match blend {
                Blend::SourceOver => c(F::One, F::OneMinusSrcAlpha),
                Blend::Add => c(F::One, F::One),
                Blend::Multiply => c(F::Dst, F::OneMinusSrcAlpha),
                Blend::Screen => c(F::One, F::OneMinusSrc),
            };
            mk(
                "floret composite",
                &scene_layout,
                "vs_composite",
                "fs_composite",
                &[Some(tex_vb.clone())],
                TriangleList,
                Some(ignore.clone()),
                Some(wgpu::BlendState {
                    color: state,
                    alpha: state,
                }),
                all,
                n,
            )
        };
        let composite_pipes = [
            comp(Blend::SourceOver),
            comp(Blend::Add),
            comp(Blend::Multiply),
            comp(Blend::Screen),
        ];
        let gauss_pipe = mk(
            "floret gauss",
            &gauss_layout,
            "vs_fullscreen",
            "fs_gauss",
            &[],
            TriangleList,
            None,
            None,
            all,
            1,
        );

        use wgpu::BufferUsages as U;
        Renderer {
            format,
            sample_count,
            max_texture: device.limits().max_texture_dimension_2d,
            view_buf,
            view_bg,
            tex_layout,
            blur_layout,
            sampler,
            sdf_pipe,
            winding_pipe,
            cover_pipe,
            tex_pipe,
            composite_pipes,
            gauss_pipe,
            sdf_buf: GrowBuffer::new("floret sdf instances", U::VERTEX),
            fill_buf: GrowBuffer::new("floret fill vertices", U::VERTEX),
            tex_buf: GrowBuffer::new("floret tex vertices", U::VERTEX),
            comp_buf: GrowBuffer::new("floret composite vertices", U::VERTEX),
            blur_buf: GrowBuffer::new("floret blur uniforms", U::UNIFORM),
            blur_bg: None,
            targets: None,
            text_textures: HashMap::new(),
            image_textures: HashMap::new(),
            transparent: false,
            fb: FrameBuilder::new(),
            text: TextRasterizer::new(),
            frame: 0,
            stats: RenderStats::default(),
        }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    /// Counts from the last frame.
    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// Clear frames to transparent black instead of the opaque background
    /// (rasterising layers into an image, like the web app's
    /// `renderLayersToCanvas(…, null)`). Off by default.
    pub fn set_transparent_background(&mut self, on: bool) {
        self.transparent = on;
    }

    /// Render `layers` (top-level scene nodes, painter's order) into
    /// `target`, a single-sample `size.0 × size.1` texture of the format
    /// given to [`Renderer::new`] with `RENDER_ATTACHMENT` usage.
    ///
    /// Scene coordinates are centre-origin (y down) over a `size`-unit view,
    /// like the web app. `background` is cleared opaque (its alpha is
    /// ignored, as in the web app). Records into `encoder`; buffer and
    /// texture uploads go through `queue` and land before the encoder's
    /// commands once it's submitted.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        layers: &[Arc<SceneNode>],
        background: [f64; 4],
        size: (u32, u32),
    ) -> RenderStats {
        let size = (size.0.max(1), size.1.max(1));
        self.frame += 1;

        // 1. Scene → batches.
        self.fb.reset();
        {
            let mut e = Emitter {
                fb: &mut self.fb,
                text: &mut self.text,
            };
            for node in layers {
                e.emit(node, &IDENTITY);
            }
        }

        // 2. Targets + uploads.
        let depth = self.fb.max_layer_depth();
        let has_blur = self
            .fb
            .cmds
            .iter()
            .any(|c| matches!(c, Cmd::PopLayerBlur { .. }));
        self.ensure_targets(device, size, depth, has_blur);
        queue.write_buffer(
            &self.view_buf,
            0,
            bytemuck::bytes_of(&ViewUniform {
                size: [size.0 as f32, size.1 as f32],
                _pad: [0.0; 2],
            }),
        );
        self.sdf_buf
            .write(device, queue, bytemuck::cast_slice(&self.fb.sdf));
        self.fill_buf
            .write(device, queue, bytemuck::cast_slice(&self.fb.fill));
        self.tex_buf
            .write(device, queue, bytemuck::cast_slice(&self.fb.tex_verts));
        // Composite quads: the whole view first, then rotated-clip masks.
        let (hw, hh) = (size.0 as f32 / 2.0, size.1 as f32 / 2.0);
        let mut comp: Vec<TexVertex> = [
            [-hw, -hh],
            [hw, -hh],
            [-hw, hh],
            [-hw, hh],
            [hw, -hh],
            [hw, hh],
        ]
        .iter()
        .map(|&pos| TexVertex {
            pos,
            uv: [0.0; 2],
            tint: [1.0; 4],
        })
        .collect();
        comp.extend_from_slice(&self.fb.composite);
        self.comp_buf
            .write(device, queue, bytemuck::cast_slice(&comp));
        let blurs: Vec<f32> = self
            .fb
            .cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::PopLayerBlur { sigma, .. } => Some(*sigma),
                _ => None,
            })
            .collect();
        if !blurs.is_empty() {
            let mut bytes = vec![0u8; blurs.len() * 2 * BLUR_STRIDE as usize];
            for (i, sigma) in blurs.iter().enumerate() {
                for (k, dir) in [[1.0, 0.0], [0.0, 1.0]].into_iter().enumerate() {
                    let off = (i * 2 + k) * BLUR_STRIDE as usize;
                    let u = BlurUniform {
                        dir,
                        sigma: *sigma,
                        _pad: 0.0,
                    };
                    bytes[off..off + std::mem::size_of::<BlurUniform>()]
                        .copy_from_slice(bytemuck::bytes_of(&u));
                }
            }
            self.blur_buf.write(device, queue, &bytes);
            let gen = self.blur_buf.generation;
            if self.blur_bg.as_ref().map(|(g, _)| *g) != Some(gen) {
                if let Some(buf) = &self.blur_buf.buf {
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("floret blur"),
                        layout: &self.blur_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: buf,
                                offset: 0,
                                size: wgpu::BufferSize::new(
                                    std::mem::size_of::<BlurUniform>() as u64
                                ),
                            }),
                        }],
                    });
                    self.blur_bg = Some((gen, bg));
                }
            }
        }
        self.upload_text(device, queue);
        self.upload_images(device, queue);

        // 3. Replay.
        self.execute(encoder, target, background, size);

        self.stats = RenderStats {
            sdf_instances: self.fb.sdf.len(),
            poly_fills: self
                .fb
                .cmds
                .iter()
                .filter(|c| matches!(c, Cmd::PolyFill { .. }))
                .count(),
            fill_vertices: self.fb.fill.len(),
            text_quads: self
                .fb
                .cmds
                .iter()
                .filter(|c| matches!(c, Cmd::Tex { .. }))
                .count(),
            image_quads: self
                .fb
                .cmds
                .iter()
                .filter(|c| matches!(c, Cmd::Image { .. }))
                .count(),
            commands: self.fb.cmds.len(),
            layers: depth,
            skipped: self.fb.skipped,
        };
        self.stats
    }

    fn ensure_targets(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        depth: usize,
        blur: bool,
    ) {
        if self.targets.as_ref().is_some_and(|t| t.size != size) {
            self.targets = None;
        }
        let n = self.sample_count;
        let format = self.format;
        let tex = |label: &str, samples: u32, format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let ra = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let targets = self.targets.get_or_insert_with(|| Targets {
            size,
            base_msaa: (n > 1).then(|| tex("floret base msaa", n, format, ra)),
            stencil: tex("floret stencil", n, STENCIL_FORMAT, ra),
            layers: Vec::new(),
            blur_tmp: None,
        });
        let make_layer = |label: &str, samples: u32| {
            let resolve = tex(label, 1, format, ra | wgpu::TextureUsages::TEXTURE_BINDING);
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &self.tex_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&resolve),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            Layer {
                msaa: (samples > 1).then(|| tex(label, samples, format, ra)),
                resolve,
                bind,
            }
        };
        while targets.layers.len() < depth {
            targets.layers.push(make_layer("floret layer", n));
        }
        if blur && targets.blur_tmp.is_none() {
            targets.blur_tmp = Some(make_layer("floret blur scratch", 1));
        }
    }

    fn upload_text(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let frame = self.frame;
        for cmd in &self.fb.cmds {
            let Cmd::Tex { texture, .. } = cmd else {
                continue;
            };
            if let Some(t) = self.text_textures.get_mut(&texture.id) {
                t.last_used = frame;
                continue;
            }
            if let Some(bind) = self.create_text_texture(device, queue, texture) {
                self.text_textures.insert(
                    texture.id,
                    TextTexture {
                        bind,
                        last_used: frame,
                    },
                );
            }
        }
        self.text_textures
            .retain(|_, t| t.last_used + TEXT_TTL_FRAMES >= frame);
    }

    /// Upload every image this frame draws that isn't on the GPU yet,
    /// recycling an unused texture of the same size when there is one.
    fn upload_images(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let frame = self.frame;
        for cmd in &self.fb.cmds {
            let Cmd::Image { image, .. } = cmd else {
                continue;
            };
            if let Some(t) = self.image_textures.get_mut(&image.id()) {
                t.last_used = frame;
                continue;
            }
            let size = (image.width(), image.height());
            if size.0 == 0 || size.1 == 0 || size.0 > self.max_texture || size.1 > self.max_texture
            {
                continue;
            }
            let stale = self
                .image_textures
                .iter()
                .find(|(_, t)| t.last_used < frame && t.size == size)
                .map(|(id, _)| *id);
            let mut tex = match stale.and_then(|id| self.image_textures.remove(&id)) {
                Some(t) => t,
                None => self.create_image_texture(device, size),
            };
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &image.premultiplied(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size.0 * 4),
                    rows_per_image: Some(size.1),
                },
                wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
            );
            tex.last_used = frame;
            self.image_textures.insert(image.id(), tex);
        }
        self.image_textures
            .retain(|_, t| t.last_used + IMAGE_TTL_FRAMES >= frame);
    }

    fn create_image_texture(&self, device: &wgpu::Device, size: (u32, u32)) -> ImageTexture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("floret image"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("floret image"),
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
        ImageTexture {
            texture,
            bind,
            size,
            last_used: 0,
        }
    }

    fn create_text_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        run: &TextKey,
    ) -> Option<wgpu::BindGroup> {
        if run.px_w == 0
            || run.px_h == 0
            || run.px_w > self.max_texture
            || run.px_h > self.max_texture
        {
            return None;
        }
        let size = wgpu::Extent3d {
            width: run.px_w,
            height: run.px_h,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("floret text"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &run.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(run.px_w * 4),
                rows_per_image: Some(run.px_h),
            },
            size,
        );
        let view = texture.create_view(&Default::default());
        Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("floret text"),
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
        }))
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        background: [f64; 4],
        size: (u32, u32),
    ) {
        let Some(t) = self.targets.as_ref() else {
            return;
        };
        let cmds = &self.fb.cmds;
        let bg = if self.transparent {
            wgpu::Color::TRANSPARENT
        } else {
            wgpu::Color {
                r: finite01(background[0]),
                g: finite01(background[1]),
                b: finite01(background[2]),
                a: 1.0,
            }
        };
        let mut stack: Vec<Level> = vec![Level::Base];
        let mut load = wgpu::LoadOp::Clear(bg);
        let mut pending: Option<PendingComposite> = None;
        let mut blur_index = 0u32;
        let mut i = 0;
        loop {
            let level = *stack.last().unwrap_or(&Level::Base);
            // Does this pass finish its level (a pop, or the end of the
            // frame) or get suspended by a nested layer? Only a finishing
            // pass needs to resolve its MSAA samples; only a suspended one
            // needs to keep them.
            let finishes = !matches!(
                cmds[i..].iter().find(|c| matches!(
                    c,
                    Cmd::PushLayer | Cmd::PopLayer { .. } | Cmd::PopLayerBlur { .. }
                )),
                Some(Cmd::PushLayer)
            );
            {
                let (view, resolve) = match level {
                    Level::Base => match &t.base_msaa {
                        Some(m) => (m, Some(target)),
                        None => (target, None),
                    },
                    Level::Layer(k) => {
                        let l = &t.layers[k];
                        match &l.msaa {
                            Some(m) => (m, Some(&l.resolve)),
                            None => (&l.resolve, None),
                        }
                    }
                };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("floret scene"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: resolve.filter(|_| finishes),
                        ops: wgpu::Operations {
                            load,
                            store: if resolve.is_some() && finishes {
                                wgpu::StoreOp::Discard
                            } else {
                                wgpu::StoreOp::Store
                            },
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &t.stencil,
                        depth_ops: None,
                        stencil_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0),
                            store: wgpu::StoreOp::Discard,
                        }),
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(0, &self.view_bg, &[]);
                if let Some(c) = pending.take() {
                    if let (true, Some(s)) = (
                        self.scissor(&mut pass, &Some(c.bounds), size),
                        self.comp_buf.slice(),
                    ) {
                        pass.set_pipeline(&self.composite_pipes[c.blend as usize]);
                        pass.set_bind_group(1, &t.layers[c.layer].bind, &[]);
                        pass.set_vertex_buffer(0, s);
                        pass.draw(c.first_vertex..c.first_vertex + 6, 0..1);
                    }
                }
                while let Some(cmd) = cmds.get(i) {
                    if matches!(
                        cmd,
                        Cmd::PushLayer | Cmd::PopLayer { .. } | Cmd::PopLayerBlur { .. }
                    ) {
                        break;
                    }
                    self.draw(&mut pass, cmd, size);
                    i += 1;
                }
            }
            let Some(cmd) = cmds.get(i) else {
                break;
            };
            i += 1;
            match cmd {
                Cmd::PushLayer => {
                    stack.push(Level::Layer(stack.len() - 1));
                    load = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
                }
                Cmd::PopLayer {
                    blend,
                    mask,
                    bounds,
                } => {
                    let Some(Level::Layer(k)) = stack.pop() else {
                        stack.push(Level::Base);
                        continue;
                    };
                    load = wgpu::LoadOp::Load;
                    pending = Some(PendingComposite {
                        layer: k,
                        blend: *blend,
                        first_vertex: mask.map_or(0, |m| (m + 1) * 6),
                        bounds: *bounds,
                    });
                }
                Cmd::PopLayerBlur { bounds, .. } => {
                    let Some(Level::Layer(k)) = stack.pop() else {
                        stack.push(Level::Base);
                        continue;
                    };
                    if let (Some(tmp), Some((_, blur_bg))) = (&t.blur_tmp, &self.blur_bg) {
                        let child = &t.layers[k];
                        let passes = [(&child.bind, &tmp.resolve), (&tmp.bind, &child.resolve)];
                        for (dir, (src, dst)) in passes.into_iter().enumerate() {
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("floret blur"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: dst,
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
                            });
                            // Only the content + kernel radius can be non-zero.
                            if !self.scissor(&mut pass, &Some(*bounds), size) {
                                continue;
                            }
                            pass.set_pipeline(&self.gauss_pipe);
                            pass.set_bind_group(0, &self.view_bg, &[]);
                            pass.set_bind_group(1, src, &[]);
                            let off = (blur_index * 2 + dir as u32) * BLUR_STRIDE as u32;
                            pass.set_bind_group(2, blur_bg, &[off]);
                            pass.draw(0..3, 0..1);
                        }
                    }
                    blur_index += 1;
                    load = wgpu::LoadOp::Load;
                    pending = Some(PendingComposite {
                        layer: k,
                        blend: Blend::SourceOver,
                        first_vertex: 0,
                        bounds: *bounds,
                    });
                }
                _ => {}
            }
        }
    }

    /// Set the scissor for a draw's clip; `false` = nothing visible.
    fn scissor(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        clip: &Option<[f32; 4]>,
        size: (u32, u32),
    ) -> bool {
        let (bw, bh) = (size.0 as f32, size.1 as f32);
        let Some(c) = clip else {
            pass.set_scissor_rect(0, 0, size.0, size.1);
            return true;
        };
        // View units = pixels; centre origin.
        let sx = |x: f32| x + bw * 0.5;
        let sy = |y: f32| y + bh * 0.5;
        let (x0, x1, y0, y1) = (sx(c[0]), sx(c[2]), sy(c[1]), sy(c[3]));
        // AABBs are min/max ordered: inverted (e.g. disjoint nested clips,
        // an empty layer) or NaN means nothing to draw. (TS swaps the edges
        // of an inverted intersection, drawing into the gap.)
        if !(x0 <= x1 && y0 <= y1) {
            return false;
        }
        let left = x0.floor().clamp(0.0, bw);
        let right = x1.ceil().clamp(0.0, bw);
        let top = y0.floor().clamp(0.0, bh);
        let bottom = y1.ceil().clamp(0.0, bh);
        if right <= left || bottom <= top {
            return false;
        }
        pass.set_scissor_rect(
            left as u32,
            top as u32,
            (right - left) as u32,
            (bottom - top) as u32,
        );
        true
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, cmd: &Cmd, size: (u32, u32)) {
        match cmd {
            Cmd::Sdf { start, count, clip } => {
                let Some(buf) = self.sdf_buf.slice() else {
                    return;
                };
                if !self.scissor(pass, clip, size) {
                    return;
                }
                pass.set_pipeline(&self.sdf_pipe);
                pass.set_vertex_buffer(0, buf);
                pass.draw(0..4, *start..start + count);
            }
            Cmd::PolyFill {
                fan_start,
                fan_count,
                cover_start,
                clip,
            } => {
                let Some(buf) = self.fill_buf.slice() else {
                    return;
                };
                if !self.scissor(pass, clip, size) {
                    return;
                }
                pass.set_vertex_buffer(0, buf);
                pass.set_stencil_reference(0);
                pass.set_pipeline(&self.winding_pipe);
                pass.draw(*fan_start..fan_start + fan_count, 0..1);
                pass.set_pipeline(&self.cover_pipe);
                pass.draw(*cover_start..cover_start + 6, 0..1);
            }
            Cmd::Tex {
                quad,
                texture,
                clip,
            } => {
                let (Some(buf), Some(tex)) =
                    (self.tex_buf.slice(), self.text_textures.get(&texture.id))
                else {
                    return;
                };
                if !self.scissor(pass, clip, size) {
                    return;
                }
                pass.set_pipeline(&self.tex_pipe);
                pass.set_bind_group(1, &tex.bind, &[]);
                pass.set_vertex_buffer(0, buf);
                pass.draw(quad * 6..quad * 6 + 6, 0..1);
            }
            Cmd::Image { quad, image, clip } => {
                let (Some(buf), Some(tex)) =
                    (self.tex_buf.slice(), self.image_textures.get(&image.id()))
                else {
                    return;
                };
                if !self.scissor(pass, clip, size) {
                    return;
                }
                pass.set_pipeline(&self.tex_pipe);
                pass.set_bind_group(1, &tex.bind, &[]);
                pass.set_vertex_buffer(0, buf);
                pass.draw(quad * 6..quad * 6 + 6, 0..1);
            }
            _ => {}
        }
    }
}

fn finite01(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
