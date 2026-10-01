//! Live preview: the first renderer node's scene, drawn by `floret-render`
//! into an offscreen texture at the renderer's resolution, then shown fitted
//! (aspect-correct) in the preview panel as an egui image.
//!
//! The renderer, the offscreen texture and its egui registration are kept
//! across frames; the texture is only reallocated when the canvas size
//! changes.
//!
//! [`PreviewPanel`] is everything the app needs: it owns the GPU preview,
//! the canvas size (from the renderer node, like the web canvas driver) and
//! the patch's mouse / keyboard input, and draws the panel.

use crate::theme;
use eframe::egui::{self, vec2, Align, Layout, RichText};
use eframe::egui_wgpu::{self, wgpu};
use floret_engine::scene::SceneNode;
use floret_engine::tick::Outputs;
use floret_engine::{Engine, FrameCtx};
use floret_render::{RenderStats, Renderer};
use std::sync::Arc;
use std::time::Instant;

/// The preview panel: GPU preview + canvas size + the patch's input state.
pub struct PreviewPanel {
    /// `None` without a wgpu render state.
    gpu: Option<Preview>,
    /// Canvas size (the renderer node's width × height).
    pub canvas: (u32, u32),
    /// Pointer over the preview, canvas pixels, centre origin.
    mouse: (f64, f64),
    mouse_pressed: bool,
    key: String,
    key_pressed: bool,
    export: crate::export::Export,
    /// What exports are named after: the renderer's label, else this
    /// (the patch name).
    pub doc_name: String,
}

impl PreviewPanel {
    pub fn new(rs: Option<&egui_wgpu::RenderState>) -> PreviewPanel {
        PreviewPanel {
            gpu: rs.map(Preview::new),
            canvas: floret_render::DEFAULT_CANVAS,
            mouse: (0.0, 0.0),
            mouse_pressed: false,
            key: String::new(),
            key_pressed: false,
            export: crate::export::Export::default(),
            doc_name: String::new(),
        }
    }

    /// The engine's frame context for this tick. Reads the canvas size from
    /// the first renderer's params before the tick, like the web driver (a
    /// wired size settles one frame later), and the keyboard.
    pub fn frame_ctx(
        &mut self,
        ctx: &egui::Context,
        engine: Option<&Engine>,
        t: f64,
        dt: f64,
        frame_count: u64,
    ) -> FrameCtx {
        if let Some(e) = engine {
            if let Some(r) = e.graph.renderer() {
                let id = &e.graph.nodes[r].id;
                self.canvas = floret_render::canvas_size(
                    e.params.get(id, "width"),
                    e.params.get(id, "height"),
                );
            }
        }
        self.read_keys(ctx);
        FrameCtx {
            t,
            dt,
            frame_count: frame_count as f64,
            width: f64::from(self.canvas.0),
            height: f64::from(self.canvas.1),
            mouse_x: self.mouse.0,
            mouse_y: self.mouse.1,
            mouse_is_pressed: self.mouse_pressed,
            key: self.key.clone(),
            key_is_pressed: self.key_pressed,
        }
    }

    /// Keyboard state for the patch (`key` / `keyIsPressed`), unless a
    /// text field has focus. Names follow DOM `KeyboardEvent.key`.
    fn read_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            self.key_pressed = false;
            return;
        }
        ctx.input(|i| {
            for e in &i.events {
                if let egui::Event::Key {
                    key, pressed: true, ..
                } = e
                {
                    self.key = match key.name() {
                        "Space" => " ".to_owned(),
                        n if n.chars().count() == 1 => n.to_lowercase(),
                        n => n.to_owned(),
                    };
                }
            }
            self.key_pressed = !i.keys_down.is_empty();
        });
    }

    /// The panel: a header (size, shape count, encode time) and the first
    /// renderer's scene, aspect-fitted.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        engine: Option<&Engine>,
        outputs: &Outputs,
    ) {
        let (layers, bg) = scene(engine, outputs);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Preview").color(theme::FG_1).small());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = engine
                    .and_then(|e| {
                        let r = e.graph.renderer()?;
                        e.params.get(&e.graph.nodes[r].id, "label")?.as_str()
                    })
                    .filter(|l| !l.trim().is_empty());
                self.export.ui(
                    ui,
                    rs,
                    label.unwrap_or(&self.doc_name),
                    crate::export::Frame {
                        layers: &layers,
                        background: bg,
                        size: self.canvas,
                    },
                );
                let mut info = format!("{} × {}", self.canvas.0, self.canvas.1);
                if let Some(p) = &self.gpu {
                    let s = p.stats;
                    info.push_str(&format!(
                        " · {} shapes · {:.1} ms",
                        s.sdf_instances + s.poly_fills + s.text_quads,
                        p.encode_ms
                    ));
                }
                ui.label(RichText::new(info).color(theme::FG_2).small().monospace());
            });
        });

        let w = ui.available_width();
        let aspect = self.canvas.1 as f32 / self.canvas.0.max(1) as f32;
        let h = (w * aspect).clamp(120.0, (ui.available_height() * 0.62).max(120.0));
        let (area, response) = ui.allocate_exact_size(vec2(w, h), egui::Sense::click_and_drag());
        ui.painter()
            .rect_filled(area, egui::CornerRadius::same(6), theme::BG_0);
        let image = fit(area, self.canvas);
        match (rs, self.gpu.as_mut()) {
            (Some(rs), Some(p)) => {
                let id = p.render(rs, &layers, bg, self.canvas);
                ui.painter().image(
                    id,
                    image,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            _ => {
                ui.painter().text(
                    area.center(),
                    egui::Align2::CENTER_CENTER,
                    "No GPU renderer available",
                    egui::FontId::proportional(12.0),
                    theme::FG_2,
                );
            }
        }
        // Mouse in canvas pixels, centre origin; pressed while held on it.
        if let Some(pos) = response.hover_pos() {
            if let Some(m) = canvas_mouse(image, self.canvas, pos) {
                self.mouse = m;
            }
        }
        self.mouse_pressed = response.is_pointer_button_down_on();
        ui.painter().rect_stroke(
            area,
            egui::CornerRadius::same(6),
            egui::Stroke::new(1.0, theme::BORDER),
            egui::StrokeKind::Outside,
        );
    }
}

/// The first renderer's scene: its layers (first output, flattened) and
/// background (second output), like the web canvas driver.
fn scene(engine: Option<&Engine>, outputs: &Outputs) -> (Vec<Arc<SceneNode>>, [f64; 4]) {
    const BLACK: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
    let out = engine
        .and_then(|e| e.graph.renderer())
        .and_then(|r| outputs.get(r)?.as_ref());
    match out {
        Some(out) => (
            out.first()
                .map(|s| floret_render::flatten_layers(s))
                .unwrap_or_default(),
            out.get(1).map_or(BLACK, |s| floret_render::background(s)),
        ),
        None => (Vec::new(), BLACK),
    }
}

/// Offscreen colour format: non-sRGB, premultiplied, gamma-space values —
/// what egui expects from a native texture, and how WebGL blends.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const MSAA: u32 = 4;

struct Target {
    size: (u32, u32),
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct Preview {
    renderer: Renderer,
    target: Option<Target>,
    texture_id: Option<egui::TextureId>,
    stats: RenderStats,
    /// Smoothed CPU time spent batching + encoding a frame (ms).
    encode_ms: f32,
}

impl Preview {
    fn new(rs: &egui_wgpu::RenderState) -> Preview {
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
    fn render(
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
fn fit(area: egui::Rect, size: (u32, u32)) -> egui::Rect {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let scale = (area.width() / w).min(area.height() / h);
    egui::Rect::from_center_size(area.center(), egui::vec2(w * scale, h * scale))
}

/// Pointer position in canvas pixels, centre origin (the engine's mouse
/// convention), if it's over the fitted image.
fn canvas_mouse(image: egui::Rect, size: (u32, u32), pos: egui::Pos2) -> Option<(f64, f64)> {
    if !image.contains(pos) || image.width() <= 0.0 || image.height() <= 0.0 {
        return None;
    }
    let u = f64::from((pos.x - image.min.x) / image.width());
    let v = f64::from((pos.y - image.min.y) / image.height());
    Some(((u - 0.5) * f64::from(size.0), (v - 0.5) * f64::from(size.1)))
}
