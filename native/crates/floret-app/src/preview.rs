//! Live GPU preview. Draws directly into egui's wgpu render pass through a
//! paint callback — the integration path the real scene renderer will use.
//! Until that renderer lands (phase 2), it draws an animated placeholder
//! shader driven by the engine's clock.

use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    resolution: [f32; 2],
    mouse: [f32; 2],
    time: f32,
    _pad: [f32; 3],
}

struct Resources {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
}

const SHADER: &str = r#"
struct U { resolution: vec2<f32>, mouse: vec2<f32>, time: f32 };
@group(0) @binding(0) var<uniform> u: U;

struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VOut {
    // One triangle covering the viewport.
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VOut;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    var q = (in.uv - 0.5) * vec2<f32>(aspect, 1.0) * 2.0;
    let r = length(q);
    let a = atan2(q.y, q.x);
    let t = u.time;
    // A slowly turning floret: 8 petals, breathing.
    let petals = 0.55 + 0.12 * sin(t * 0.8) + 0.18 * cos(8.0 * a + t * 0.6);
    let edge = smoothstep(petals + 0.012, petals - 0.012, r);
    let rings = 0.5 + 0.5 * cos(r * 28.0 - t * 2.0);
    let gold = vec3<f32>(0.957, 0.773, 0.259);
    let ink = vec3<f32>(0.078, 0.075, 0.102);
    var col = mix(ink, gold * (0.55 + 0.45 * rings), edge);
    let core = smoothstep(0.14, 0.0, r);
    col = mix(col, vec3<f32>(1.0, 0.93, 0.7), core);
    let vignette = smoothstep(1.6, 0.4, r);
    return vec4<f32>(col * vignette, 1.0);
}
"#;

/// Create GPU resources once, at startup.
pub fn init(rs: &egui_wgpu::RenderState) {
    let device = &rs.device;
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("floret-preview"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("floret-preview-uniforms"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("floret-preview"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("floret-preview"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniforms.as_entire_binding(),
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("floret-preview"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("floret-preview"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(rs.target_format.into())],
        }),
        multiview_mask: None,
        cache: None,
    });
    rs.renderer.write().callback_resources.insert(Resources {
        pipeline,
        bind_group,
        uniforms,
    });
}

struct Callback {
    uniforms: Uniforms,
}

impl egui_wgpu::CallbackTrait for Callback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(r) = resources.get::<Resources>() {
            queue.write_buffer(&r.uniforms, 0, bytemuck::bytes_of(&self.uniforms));
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(r) = resources.get::<Resources>() else {
            return;
        };
        pass.set_pipeline(&r.pipeline);
        pass.set_bind_group(0, &r.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Draw the preview into `rect`, animated by `time` (seconds).
pub fn paint(ui: &egui::Ui, rect: egui::Rect, time: f32, mouse: egui::Vec2) {
    let ppp = ui.ctx().pixels_per_point();
    let cb = Callback {
        uniforms: Uniforms {
            resolution: [rect.width() * ppp, rect.height() * ppp],
            mouse: [mouse.x, mouse.y],
            time,
            _pad: [0.0; 3],
        },
    };
    ui.painter()
        .add(egui_wgpu::Callback::new_paint_callback(rect, cb));
}
