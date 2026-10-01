//! Live preview: the first renderer node's scene, drawn by `floret-render`
//! into an offscreen texture at the renderer's resolution, then shown fitted
//! (aspect-correct) in the preview panel as an egui image.
//!
//! The renderer, the offscreen texture and its egui registration are kept
//! across frames; the texture is only reallocated when the canvas size
//! changes.

use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};
use floret_engine::scene::SceneNode;
use floret_render::{RenderStats, Renderer};
use std::sync::Arc;
use std::time::Instant;

/// Offscreen colour format: non-sRGB, premultiplied, gamma-space values —
/// what egui expects from a native texture, and how WebGL blends.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const MSAA: u32 = 4;

struct Target {
    size: (u32, u32),
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

pub struct Preview {
    renderer: Renderer,
    target: Option<Target>,
    texture_id: Option<egui::TextureId>,
    pub stats: RenderStats,
    /// Smoothed CPU time spent batching + encoding a frame (ms).
    pub encode_ms: f32,
}

impl Preview {
    pub fn new(rs: &egui_wgpu::RenderState) -> Preview {
        let samples = if rs
            .adapter
            .get_texture_format_features(FORMAT)
            .flags
            .sample_count_supported(MSAA)
            && rs
                .adapter
                .get_texture_format_features(floret_render::STENCIL_FORMAT)
                .flags
                .sample_count_supported(MSAA)
        {
            MSAA
        } else {
            1
        };
        Preview {
            renderer: Renderer::new(&rs.device, &rs.queue, FORMAT, samples),
            target: None,
            texture_id: None,
            stats: RenderStats::default(),
            encode_ms: 0.0,
        }
    }

    /// Render one frame of the scene at `size` and return its egui texture.
    pub fn render(
        &mut self,
        rs: &egui_wgpu::RenderState,
        layers: &[Arc<SceneNode>],
        background: [f64; 4],
        size: (u32, u32),
    ) -> egui::TextureId {
        let t0 = Instant::now();
        let device = &rs.device;
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("floret preview"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let mut egui_renderer = rs.renderer.write();
            match self.texture_id {
                Some(id) => egui_renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &view,
                    wgpu::FilterMode::Linear,
                    id,
                ),
                None => {
                    self.texture_id = Some(egui_renderer.register_native_texture(
                        device,
                        &view,
                        wgpu::FilterMode::Linear,
                    ));
                }
            }
            self.target = Some(Target {
                size,
                _texture: texture,
                view,
            });
        }
        let target = self.target.as_ref().expect("target");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("floret preview"),
        });
        self.stats = self.renderer.render(
            device,
            &rs.queue,
            &mut encoder,
            &target.view,
            layers,
            background,
            size,
        );
        rs.queue.submit([encoder.finish()]);
        let ms = t0.elapsed().as_secs_f32() * 1000.0;
        self.encode_ms = if self.encode_ms == 0.0 {
            ms
        } else {
            self.encode_ms * 0.9 + ms * 0.1
        };
        self.texture_id.expect("registered")
    }
}

/// The largest `size`-shaped rect centred in `area`.
pub fn fit(area: egui::Rect, size: (u32, u32)) -> egui::Rect {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let scale = (area.width() / w).min(area.height() / h);
    egui::Rect::from_center_size(area.center(), egui::vec2(w * scale, h * scale))
}

/// Pointer position in canvas pixels, centre origin (the engine's mouse
/// convention), if it's over the fitted image.
pub fn canvas_mouse(image: egui::Rect, size: (u32, u32), pos: egui::Pos2) -> Option<(f64, f64)> {
    if !image.contains(pos) || image.width() <= 0.0 || image.height() <= 0.0 {
        return None;
    }
    let u = f64::from((pos.x - image.min.x) / image.width());
    let v = f64::from((pos.y - image.min.y) / image.height());
    Some(((u - 0.5) * f64::from(size.0), (v - 0.5) * f64::from(size.1)))
}
