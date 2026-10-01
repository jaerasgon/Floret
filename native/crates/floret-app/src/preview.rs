//! Live preview (`PreviewPanel.tsx` / `Preview.tsx`): one view per
//! `render/renderer` node, each drawn by `floret-render` into its own
//! offscreen texture at the renderer's resolution and shown as its own
//! dock tab (`workspace.rs`), titled from the renderer's `label`;
//! deleting the node drops its view.
//!
//! Each view has its own camera — fit, zoom toward the pointer, pan, 1:1
//! and a pixel grid once texels are big — and a lock that ignores pinch /
//! pan. The transport pauses, steps and restarts the patch. Focus mode
//! shows one renderer alone (Esc exits); the output window sends one
//! fullscreen to a chosen display (Esc closes it).
//!
//! Renderers, offscreen textures and their egui registrations are kept
//! across frames; a texture is only reallocated when its canvas resizes.
//! [`PreviewPanel`] also owns the canvas size the engine sees (the first
//! renderer's, like the web canvas driver) and the patch's mouse /
//! keyboard input.

use crate::theme;
use eframe::egui::{self, pos2, vec2, Align, Color32, Layout, Pos2, Rect, RichText, Vec2};
use eframe::egui_wgpu::{self, wgpu};
use floret_engine::patch::PatchFile;
use floret_engine::scene::SceneNode;
use floret_engine::tick::Outputs;
use floret_engine::{Engine, FrameCtx, Value};
use floret_render::{RenderClock, RenderStats, Renderer};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Texel size (physical px) from which the pixel grid shows.
const GRID_FROM: f32 = 8.0;
/// Texel size from which the image is sampled nearest-neighbour.
const NEAREST_FROM: f32 = 2.0;
const MAX_TEXEL: f32 = 64.0;
/// Displays offered for the output window (winit's monitor order).
const DISPLAYS: usize = 4;

// ─── Transport ──────────────────────────────────────────────────────────────

/// Pause / step / restart, read by the app's tick.
#[derive(Default)]
pub struct Transport {
    pub paused: bool,
    step: bool,
    restart: bool,
}

/// What this frame's tick should do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Advance {
    /// Run normally with the real frame time.
    Run,
    /// Paused: don't tick, and hold the clock.
    Hold,
    /// Paused, but step one frame of this length.
    Step(f64),
}

/// One stepped frame (60 fps).
pub const STEP_DT: f64 = 1.0 / 60.0;

impl Transport {
    pub fn advance(&mut self) -> Advance {
        match (self.paused, std::mem::take(&mut self.step)) {
            (false, _) => Advance::Run,
            (true, true) => Advance::Step(STEP_DT),
            (true, false) => Advance::Hold,
        }
    }

    /// A restart was asked for since the last call.
    pub fn take_restart(&mut self) -> bool {
        std::mem::take(&mut self.restart)
    }

    pub fn toggle(&mut self) {
        self.paused = !self.paused;
    }

    pub fn step(&mut self) {
        self.paused = true;
        self.step = true;
    }

    pub fn restart(&mut self) {
        self.restart = true;
    }
}

// ─── Camera ─────────────────────────────────────────────────────────────────

/// A view's zoom (× fit) and pan (screen points from centred).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub zoom: f32,
    pub pan: Vec2,
    pub locked: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            zoom: 1.0,
            pan: Vec2::ZERO,
            locked: false,
        }
    }
}

/// Points per canvas pixel when `size` fits `area`.
pub fn fit_scale(area: Rect, size: (u32, u32)) -> f32 {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    (area.width() / w).min(area.height() / h)
}

impl Camera {
    /// Where the canvas image sits on screen.
    pub fn image_rect(&self, area: Rect, size: (u32, u32)) -> Rect {
        let s = fit_scale(area, size) * self.zoom;
        Rect::from_center_size(
            area.center() + self.pan,
            vec2(size.0.max(1) as f32 * s, size.1.max(1) as f32 * s),
        )
    }

    /// Zoom by `factor`, keeping the canvas point under `at` still.
    pub fn zoom_at(&mut self, factor: f32, at: Pos2, area: Rect, size: (u32, u32), ppp: f32) {
        let max = MAX_TEXEL / (fit_scale(area, size) * ppp).max(1e-6);
        let zoom = (self.zoom * factor).clamp(0.1, max.max(1.0));
        let c = area.center() + self.pan;
        self.pan += (at - c) * (1.0 - zoom / self.zoom);
        self.zoom = zoom;
    }

    pub fn fit(&mut self) {
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
    }

    /// One canvas pixel per physical screen pixel, centred.
    pub fn one_to_one(&mut self, area: Rect, size: (u32, u32), ppp: f32) {
        self.zoom = 1.0 / (fit_scale(area, size) * ppp).max(1e-6);
        self.pan = Vec2::ZERO;
    }

    /// Physical screen pixels per canvas pixel.
    pub fn texel(&self, area: Rect, size: (u32, u32), ppp: f32) -> f32 {
        fit_scale(area, size) * self.zoom * ppp
    }
}

/// Pointer position in canvas pixels, centre origin (the engine's mouse
/// convention), if it's over the image.
fn canvas_mouse(image: Rect, size: (u32, u32), pos: Pos2) -> Option<(f64, f64)> {
    if !image.contains(pos) || image.width() <= 0.0 || image.height() <= 0.0 {
        return None;
    }
    let u = f64::from((pos.x - image.min.x) / image.width());
    let v = f64::from((pos.y - image.min.y) / image.height());
    Some(((u - 0.5) * f64::from(size.0), (v - 0.5) * f64::from(size.1)))
}

/// Screen x (or y) of each texel edge of `image` inside `clip`, for the
/// pixel grid.
fn grid_lines(image_min: f32, texel: f32, clip_min: f32, clip_max: f32) -> Vec<f32> {
    if texel <= 0.0 {
        return Vec::new();
    }
    let first = ((clip_min - image_min) / texel).ceil().max(0.0);
    let mut out = Vec::new();
    let mut x = image_min + first * texel;
    while x <= clip_max && out.len() < 4096 {
        out.push(x);
        x += texel;
    }
    out
}

// ─── Panel ──────────────────────────────────────────────────────────────────

/// One renderer's view.
#[derive(Default)]
struct View {
    gpu: Option<Preview>,
    cam: Camera,
    size: (u32, u32),
    /// The renderer's `label` param, if set.
    label: Option<String>,
    /// Tab / window title: the label, else "Preview" (numbered when
    /// several are unlabeled).
    title: String,
}

/// The fullscreen output window.
#[derive(Clone, Debug, PartialEq)]
struct Output {
    renderer: String,
    display: usize,
}

/// The preview panel: one view per renderer, the transport, focus /
/// output state, the canvas size and the patch's input.
pub struct PreviewPanel {
    views: IndexMap<String, View>,
    /// Canvas size the engine sees (the first renderer's width × height).
    pub canvas: (u32, u32),
    /// Pointer over a preview, canvas pixels, centre origin.
    mouse: (f64, f64),
    mouse_pressed: bool,
    key: String,
    key_pressed: bool,
    export: crate::export::Export,
    /// What exports are named after: the renderer's label, else this
    /// (the patch name).
    pub doc_name: String,
    /// Shader built-ins for the next render (from the last frame context).
    pub clock: RenderClock,
    pub transport: Transport,
    /// Focus mode: this renderer alone, full window.
    focus: Option<String>,
    output: Option<Output>,
    thumbs: ProbeThumbs,
    /// `MSAA` sample count the adapter supports for previews.
    samples: Option<u32>,
}

impl PreviewPanel {
    pub fn new(rs: Option<&egui_wgpu::RenderState>) -> PreviewPanel {
        // FLORET_OUTPUT=<display> launches straight to the output window.
        let output = std::env::var("FLORET_OUTPUT").ok().map(|d| Output {
            renderer: String::new(),
            display: d.trim().parse::<usize>().unwrap_or(1).saturating_sub(1),
        });
        PreviewPanel {
            views: IndexMap::new(),
            canvas: floret_render::DEFAULT_CANVAS,
            mouse: (0.0, 0.0),
            mouse_pressed: false,
            key: String::new(),
            key_pressed: false,
            export: crate::export::Export::default(),
            doc_name: String::new(),
            clock: RenderClock::default(),
            transport: Transport::default(),
            focus: None,
            output,
            thumbs: ProbeThumbs::default(),
            samples: rs.map(msaa_samples),
        }
    }

    /// Shader compile errors from the last frame (the first view that
    /// reported any).
    pub fn shader_errors(&self) -> &crate::shader_panel::RuntimeErrors {
        self.views
            .values()
            .filter_map(|v| v.gpu.as_ref())
            .map(|p| p.renderer.shader_errors())
            .find(|e| !e.is_empty())
            .unwrap_or(&[])
    }

    /// Renderer node ids with a view, in patch order, and their titles.
    pub fn renderers(&self) -> Vec<(String, String)> {
        self.views
            .iter()
            .map(|(id, v)| (id.clone(), v.title.clone()))
            .collect()
    }

    /// Keep one view per renderer node (new ones appear, deleted ones go).
    fn sync(&mut self, engine: Option<&Engine>) {
        let Some(e) = engine else {
            return;
        };
        let ids: Vec<String> = e
            .graph
            .renderer_ids
            .iter()
            .map(|&i| e.graph.nodes[i].id.clone())
            .collect();
        self.views.retain(|id, _| ids.contains(id));
        let mut unlabeled = 0;
        for id in &ids {
            let v = self.views.entry(id.clone()).or_default();
            v.size =
                floret_render::canvas_size(e.params.get(id, "width"), e.params.get(id, "height"));
            v.label = e
                .params
                .get(id, "label")
                .and_then(|l| l.as_str())
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned);
            v.title = match &v.label {
                Some(l) => l.clone(),
                None => {
                    unlabeled += 1;
                    match unlabeled {
                        1 => "Preview".to_owned(),
                        n => format!("Preview {n}"),
                    }
                }
            };
        }
        if self
            .focus
            .as_ref()
            .is_some_and(|f| !self.views.contains_key(f))
        {
            self.focus = None;
        }
        if let Some(o) = self.output.as_mut() {
            if !self.views.contains_key(&o.renderer) {
                match ids.first() {
                    Some(first) if o.renderer.is_empty() => o.renderer = first.clone(),
                    _ => self.output = None,
                }
            }
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
        self.sync(engine);
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
        self.clock = RenderClock {
            time: t,
            time_delta: dt,
            frame: frame_count as f64,
            mouse_x: self.mouse.0,
            mouse_y: self.mouse.1,
        };
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

    /// Focus mode is on: the app shows only [`PreviewPanel::show_focus`].
    pub fn focused(&self) -> bool {
        self.focus.is_some()
    }

    /// One renderer's view filling `ui` (a dock tab).
    pub fn show_renderer(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        engine: Option<&Engine>,
        outputs: &Outputs,
        id: &str,
    ) {
        self.sync(engine);
        if !self.views.contains_key(id) {
            empty(ui, engine, ui.available_height());
            return;
        }
        self.header(ui, rs, engine, outputs, id);
        let size = ui.available_size();
        self.viewport(ui, rs, engine, outputs, id, size, false);
    }

    /// Focus mode: the featured renderer fills the window; Esc exits.
    pub fn show_focus(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        engine: Option<&Engine>,
        outputs: &Outputs,
    ) {
        self.sync(engine);
        let Some(id) = self.focus.clone() else {
            return;
        };
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.focus = None;
        }
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
        let size = ui.available_size();
        self.viewport(ui, rs, engine, outputs, &id, size, true);
        // A quiet hint, top-left.
        ui.painter().text(
            rect.left_top() + vec2(12.0, 10.0),
            egui::Align2::LEFT_TOP,
            "Esc to exit focus mode",
            egui::FontId::proportional(11.0),
            *theme::FG_2,
        );
        self.output_window(ui.ctx(), rs, engine, outputs);
    }

    /// One row: size / stats, then Focus, Output and Export. The transport
    /// (pause / step / restart) is in the app toolbar: it drives the whole
    /// patch, not one view.
    fn header(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        engine: Option<&Engine>,
        outputs: &Outputs,
        id: &str,
    ) {
        let id = id.to_owned();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (layers, bg) = scene(engine, outputs, &id);
                let v = &self.views[&id];
                let name = v.label.clone().unwrap_or_else(|| self.doc_name.clone());
                let size = v.size;
                self.export.ui(
                    ui,
                    rs,
                    &name,
                    crate::export::Frame {
                        layers: &layers,
                        background: bg,
                        size,
                    },
                );
                let live = self.output.as_ref().is_some_and(|o| o.renderer == id);
                ui.menu_button(
                    RichText::new(if live { "● Output" } else { "Output…" }).small(),
                    |ui| {
                        ui.label(RichText::new("Fullscreen on display").small());
                        for d in 0..DISPLAYS {
                            let on = live && self.output.as_ref().is_some_and(|o| o.display == d);
                            if ui
                                .selectable_label(on, format!("Display {}", d + 1))
                                .clicked()
                            {
                                self.output = Some(Output {
                                    renderer: id.clone(),
                                    display: d,
                                });
                            }
                        }
                        if live {
                            ui.separator();
                            if ui.button("Close output (Esc)").clicked() {
                                self.output = None;
                            }
                        }
                    },
                )
                .response
                .on_hover_text("Show this renderer fullscreen on a display (Esc closes it)");
                if ui
                    .small_button("⛶ Focus")
                    .on_hover_text("Focus mode: this preview alone (Esc exits)")
                    .clicked()
                {
                    self.focus = Some(id.clone());
                }
                let mut info = format!("{}×{}", size.0, size.1);
                if let Some(p) = &self.views[&id].gpu {
                    let s = p.stats;
                    info.push_str(&format!(
                        " · {} shapes · {:.1}ms",
                        s.sdf_instances + s.poly_fills + s.text_quads,
                        p.encode_ms
                    ));
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(info).color(theme::FG_2).small().monospace(),
                        )
                        .truncate(),
                    );
                });
            });
        });
    }

    /// The view itself: camera input, the image (with the pixel grid when
    /// zoomed in), zoom controls, and the patch's mouse.
    #[allow(clippy::too_many_arguments)]
    fn viewport(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        engine: Option<&Engine>,
        outputs: &Outputs,
        id: &str,
        size: Vec2,
        bare: bool,
    ) {
        let samples = self.samples.unwrap_or(1);
        let (area, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let painter = ui.painter_at(area);
        if !bare {
            painter.rect_filled(area, egui::CornerRadius::same(6), theme::BG_0);
        }
        let (layers, bg) = scene(engine, outputs, id);
        let clock = self.clock;
        let view = self.views.get_mut(id).expect("synced");
        let canvas = view.size;

        // Camera: pinch / ⌘-scroll zooms toward the pointer, scroll and
        // middle-drag pan (ignored while locked).
        if response.hovered() && !view.cam.locked {
            let (scroll, zoom, at) =
                ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
            if zoom != 1.0 {
                if let Some(at) = at {
                    view.cam.zoom_at(zoom, at, area, canvas, ppp);
                }
            } else if scroll != Vec2::ZERO {
                view.cam.pan += scroll;
            }
        }
        if response.dragged_by(egui::PointerButton::Middle) && !view.cam.locked {
            view.cam.pan += response.drag_delta();
        }
        let image = view.cam.image_rect(area, canvas);
        let texel = view.cam.texel(area, canvas, ppp);
        match (rs, view.gpu.as_mut()) {
            (Some(rs), Some(p)) => {
                p.renderer.set_clock(clock);
                p.set_filter(rs, texel >= NEAREST_FROM);
                let tex = p.render(rs, &layers, bg, canvas);
                painter.image(
                    tex,
                    image,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            (Some(rs), None) => {
                view.gpu = Some(Preview::new(rs, samples));
                ui.ctx().request_repaint();
            }
            _ => {
                painter.text(
                    area.center(),
                    egui::Align2::CENTER_CENTER,
                    "No GPU renderer available",
                    egui::FontId::proportional(12.0),
                    *theme::FG_2,
                );
            }
        }
        if texel >= GRID_FROM {
            let step = texel / ppp;
            let clip = image.intersect(area);
            let stroke = egui::Stroke::new(
                1.0 / ppp,
                Color32::from_rgba_unmultiplied(128, 128, 128, 70),
            );
            for x in grid_lines(image.left(), step, clip.left(), clip.right()) {
                painter.line_segment([pos2(x, clip.top()), pos2(x, clip.bottom())], stroke);
            }
            for y in grid_lines(image.top(), step, clip.top(), clip.bottom()) {
                painter.line_segment([pos2(clip.left(), y), pos2(clip.right(), y)], stroke);
            }
        }

        // Mouse in canvas pixels, centre origin; pressed while held on it.
        if let Some(pos) = response.hover_pos() {
            if let Some(m) = canvas_mouse(image, canvas, pos) {
                self.mouse = m;
            }
        }
        self.mouse_pressed =
            response.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down());
        if response.hovered() && !ui.ctx().egui_wants_keyboard_input() {
            // Space toggles pause while the pointer is over a preview.
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
                self.transport.toggle();
            }
        }
        if !bare {
            painter.rect_stroke(
                area,
                egui::CornerRadius::same(6),
                egui::Stroke::new(1.0, theme::BORDER),
                egui::StrokeKind::Inside,
            );
        }
        let view = self.views.get_mut(id).expect("synced");
        controls(ui, area, &mut view.cam, canvas, ppp, texel);
    }

    /// The fullscreen output window, while one is open (call once a
    /// frame).
    pub fn output_window(
        &mut self,
        ctx: &egui::Context,
        rs: Option<&egui_wgpu::RenderState>,
        engine: Option<&Engine>,
        outputs: &Outputs,
    ) {
        let Some(out) = self.output.clone() else {
            return;
        };
        if !self.views.contains_key(&out.renderer) {
            return;
        }
        // The view's texture was rendered this frame by its panel / tab;
        // render here too when it isn't shown (e.g. a hidden dock tab).
        let (layers, bg) = scene(engine, outputs, &out.renderer);
        let clock = self.clock;
        let samples = self.samples.unwrap_or(1);
        let view = self.views.get_mut(&out.renderer).expect("checked");
        let canvas = view.size;
        let tex = match (rs, view.gpu.as_mut()) {
            (Some(rs), Some(p)) => {
                p.renderer.set_clock(clock);
                Some(p.cached_or_render(rs, &layers, bg, canvas))
            }
            (Some(rs), None) => {
                view.gpu = Some(Preview::new(rs, samples));
                None
            }
            _ => None,
        };
        let mut close = false;
        let mut mouse = None;
        let builder = egui::ViewportBuilder::default()
            .with_title(format!("Floret · {}", view.title))
            .with_fullscreen(true)
            .with_monitor(out.display)
            .with_inner_size([960.0, 540.0]);
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("floret-output"),
            builder,
            |ui, _class| {
                egui::CentralPanel::no_frame().show(ui, |ui| {
                    let rect = ui.max_rect();
                    ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
                    let image = Camera::default().image_rect(rect, canvas);
                    if let Some(tex) = tex {
                        ui.painter().image(
                            tex,
                            image,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                    if let Some(p) = resp.hover_pos() {
                        mouse = canvas_mouse(image, canvas, p)
                            .map(|m| (m, resp.is_pointer_button_down_on()));
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                    }
                });
                close = ui
                    .input(|i| i.key_pressed(egui::Key::Escape) || i.viewport().close_requested());
            },
        );
        if let Some((m, down)) = mouse {
            self.mouse = m;
            self.mouse_pressed = down;
        }
        if close {
            self.output = None;
        }
    }

    /// Probe pictures for the canvas: image wires uploaded as textures,
    /// layer wires rendered small (refreshed every 120 ms, like TS).
    pub fn probe_thumbs(
        &mut self,
        ctx: &egui::Context,
        rs: Option<&egui_wgpu::RenderState>,
        engine: Option<&Engine>,
        outputs: &Outputs,
        patch: &PatchFile,
    ) -> HashMap<String, egui::TextureId> {
        self.thumbs.update(ctx, rs, engine, outputs, patch)
    }
}

/// The view's floating zoom controls (`.preview-controls`), top-right.
fn controls(
    ui: &mut egui::Ui,
    area: Rect,
    cam: &mut Camera,
    size: (u32, u32),
    ppp: f32,
    texel: f32,
) {
    let w = 30.0;
    let mut stack = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                pos2(area.right() - w - 8.0, area.top() + 8.0),
                vec2(w, 5.0 * 22.0),
            ))
            .layout(Layout::top_down(Align::Center)),
    );
    stack.spacing_mut().item_spacing.y = 2.0;
    let items: [(&str, &str); 5] = [
        ("+", "Zoom in"),
        ("−", "Zoom out"),
        ("⛶", "Fit in view"),
        ("1:1", "Actual pixels"),
        (
            if cam.locked { "🔒" } else { "🔓" },
            if cam.locked {
                "Unlock view (allow pinch / pan)"
            } else {
                "Lock view (ignore pinch / pan)"
            },
        ),
    ];
    for (i, (label, tip)) in items.iter().enumerate() {
        let on = (i == 4 && cam.locked) || (i == 3 && (texel - 1.0).abs() < 0.01);
        let resp = stack
            .add_sized(
                vec2(w, 20.0),
                egui::Button::new(RichText::new(*label).small())
                    .selected(on)
                    .wrap_mode(egui::TextWrapMode::Extend)
                    .fill(theme::BG_1.gamma_multiply(0.92)),
            )
            .on_hover_text(*tip);
        if resp.clicked() {
            match i {
                0 => cam.zoom_at(1.2, area.center() + cam.pan, area, size, ppp),
                1 => cam.zoom_at(1.0 / 1.2, area.center() + cam.pan, area, size, ppp),
                2 => cam.fit(),
                3 => cam.one_to_one(area, size, ppp),
                _ => cam.locked = !cam.locked,
            }
        }
    }
    if (cam.zoom - 1.0).abs() > 1e-3 || cam.pan != Vec2::ZERO {
        ui.painter().text(
            area.left_bottom() + vec2(8.0, -6.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{:.0}%", texel / ppp * 100.0),
            egui::FontId::monospace(10.0),
            *theme::FG_2,
        );
    }
}

/// `PreviewOverlay`: what to say when there's nothing to show.
fn empty(ui: &mut egui::Ui, engine: Option<&Engine>, h: f32) {
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), egui::Sense::hover());
    ui.painter()
        .rect_filled(area, egui::CornerRadius::same(6), theme::BG_0);
    let (title, line) = match engine {
        None => (
            "compile error",
            "The patch didn't compile — see the status bar.",
        ),
        Some(e) if e.graph.nodes.is_empty() => (
            "empty canvas",
            "Drag nodes from the palette, or pick a sample.",
        ),
        Some(_) => (
            "no Renderer",
            "Add a Renderer and wire your drawing into its layers port.",
        ),
    };
    ui.painter().text(
        area.center() - vec2(0.0, 9.0),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::monospace(11.0),
        *theme::GOLD_TEXT,
    );
    ui.painter().text(
        area.center() + vec2(0.0, 9.0),
        egui::Align2::CENTER_CENTER,
        line,
        egui::FontId::proportional(11.5),
        *theme::FG_1,
    );
}

/// A renderer's scene: its layers (first output, flattened) and
/// background (second output), like the web canvas driver.
fn scene(engine: Option<&Engine>, outputs: &Outputs, id: &str) -> (Vec<Arc<SceneNode>>, [f64; 4]) {
    const BLACK: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
    let out = engine
        .and_then(|e| e.graph.idx(id))
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

fn msaa_samples(rs: &egui_wgpu::RenderState) -> u32 {
    let ok = |f: wgpu::TextureFormat| {
        rs.adapter
            .get_texture_format_features(f)
            .flags
            .sample_count_supported(MSAA)
    };
    if ok(FORMAT) && ok(floret_render::STENCIL_FORMAT) {
        MSAA
    } else {
        1
    }
}

struct Target {
    size: (u32, u32),
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct Preview {
    renderer: Renderer,
    target: Option<Target>,
    texture_id: Option<egui::TextureId>,
    nearest: bool,
    stats: RenderStats,
    /// Smoothed CPU time spent batching + encoding a frame (ms).
    encode_ms: f32,
    /// When the texture was last rendered (the output window reuses it
    /// within a frame).
    rendered: Option<Instant>,
}

impl Preview {
    fn new(rs: &egui_wgpu::RenderState, samples: u32) -> Preview {
        Preview {
            renderer: Renderer::new(&rs.device, &rs.queue, FORMAT, samples),
            target: None,
            texture_id: None,
            nearest: false,
            stats: RenderStats::default(),
            encode_ms: 0.0,
            rendered: None,
        }
    }

    fn filter(&self) -> wgpu::FilterMode {
        if self.nearest {
            wgpu::FilterMode::Nearest
        } else {
            wgpu::FilterMode::Linear
        }
    }

    /// Sample nearest-neighbour (crisp texels) or linear.
    fn set_filter(&mut self, rs: &egui_wgpu::RenderState, nearest: bool) {
        if self.nearest == nearest {
            return;
        }
        self.nearest = nearest;
        if let (Some(id), Some(t)) = (self.texture_id, &self.target) {
            rs.renderer.write().update_egui_texture_from_wgpu_texture(
                &rs.device,
                &t.view,
                self.filter(),
                id,
            );
        }
    }

    /// This frame's texture: reuse it if a panel already rendered it.
    fn cached_or_render(
        &mut self,
        rs: &egui_wgpu::RenderState,
        layers: &[Arc<SceneNode>],
        background: [f64; 4],
        size: (u32, u32),
    ) -> egui::TextureId {
        match (self.texture_id, self.rendered) {
            (Some(id), Some(t))
                if t.elapsed() < Duration::from_millis(8)
                    && self.target.as_ref().is_some_and(|t| t.size == size) =>
            {
                id
            }
            _ => self.render(rs, layers, background, size),
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
            let (texture, view) = offscreen(device, size, "floret preview");
            let mut egui_renderer = rs.renderer.write();
            match self.texture_id {
                Some(id) => egui_renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &view,
                    self.filter(),
                    id,
                ),
                None => {
                    self.texture_id =
                        Some(egui_renderer.register_native_texture(device, &view, self.filter()));
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
        self.rendered = Some(Instant::now());
        self.texture_id.expect("registered")
    }
}

fn offscreen(
    device: &wgpu::Device,
    size: (u32, u32),
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.0.max(1),
            height: size.1.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    (texture, view)
}

// ─── Probe thumbnails ───────────────────────────────────────────────────────

/// Thumbnail size (2× the TS 132 × 100 canvas, for crispness).
const THUMB: (u32, u32) = (264, 200);
/// `LAYER_BASE`: the canvas size scenes are usually authored against.
const LAYER_BASE: f64 = 520.0;
const POLL: Duration = Duration::from_millis(120);

#[derive(Default)]
struct ProbeThumbs {
    last: Option<Instant>,
    /// Image probes: (image identity, texture).
    images: HashMap<String, (usize, egui::TextureHandle)>,
    /// Layer probes: offscreen target + egui texture.
    layers: HashMap<String, (wgpu::Texture, wgpu::TextureView, egui::TextureId)>,
    renderer: Option<Renderer>,
    out: HashMap<String, egui::TextureId>,
}

impl ProbeThumbs {
    fn update(
        &mut self,
        ctx: &egui::Context,
        rs: Option<&egui_wgpu::RenderState>,
        engine: Option<&Engine>,
        outputs: &Outputs,
        patch: &PatchFile,
    ) -> HashMap<String, egui::TextureId> {
        if self.last.is_some_and(|t| t.elapsed() < POLL) {
            return self.out.clone();
        }
        self.last = Some(Instant::now());
        let Some(engine) = engine else {
            self.out.clear();
            return HashMap::new();
        };
        let mut out = HashMap::new();
        for n in patch.nodes.iter().filter(|n| n.def_type == "meta/probe") {
            let port = engine
                .graph
                .idx(&n.id)
                .and_then(|i| outputs.get(i)?.as_ref()?.first().cloned());
            let Some(port) = port else {
                continue;
            };
            match port.first() {
                Some(Value::Image(img)) => {
                    let key = Arc::as_ptr(img) as usize;
                    let fresh = self.images.get(&n.id).is_none_or(|(k, _)| *k != key);
                    if fresh {
                        let tex = ctx.load_texture(
                            format!("probe-{}", n.id),
                            image_thumb(img),
                            egui::TextureOptions::LINEAR,
                        );
                        self.images.insert(n.id.clone(), (key, tex));
                    }
                    out.insert(n.id.clone(), self.images[&n.id].1.id());
                }
                Some(Value::Scene(_)) => {
                    if let Some(rs) = rs {
                        out.insert(n.id.clone(), self.render_layers(rs, &n.id, &port));
                    }
                }
                _ => {}
            }
        }
        self.images.retain(|id, _| out.contains_key(id));
        if let Some(rs) = rs {
            let mut gone = Vec::new();
            for (id, (_, _, tex)) in &self.layers {
                if !out.contains_key(id) {
                    gone.push((id.clone(), *tex));
                }
            }
            for (id, tex) in gone {
                rs.renderer.write().free_texture(&tex);
                self.layers.remove(&id);
            }
        }
        self.out = out.clone();
        out
    }

    /// Render a probe's layers scaled to fit the thumbnail
    /// (`drawLayerThumb`).
    fn render_layers(
        &mut self,
        rs: &egui_wgpu::RenderState,
        id: &str,
        port: &[Value],
    ) -> egui::TextureId {
        let device = &rs.device;
        let renderer = self
            .renderer
            .get_or_insert_with(|| Renderer::new(device, &rs.queue, FORMAT, 1));
        if !self.layers.contains_key(id) {
            let (texture, view) = offscreen(device, THUMB, "floret probe");
            let tex = rs.renderer.write().register_native_texture(
                device,
                &view,
                wgpu::FilterMode::Linear,
            );
            self.layers.insert(id.to_owned(), (texture, view, tex));
        }
        let (_, view, tex) = &self.layers[id];
        let s = f64::from(THUMB.0.min(THUMB.1)) / LAYER_BASE;
        let group = SceneNode::Group {
            children: Arc::from(floret_render::flatten_layers(port)),
            transform: Some([s, 0.0, 0.0, s, 0.0, 0.0]),
            blend_mode: None,
            clip: None,
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("floret probe"),
        });
        let bg = [13.0 / 255.0, 15.0 / 255.0, 20.0 / 255.0, 1.0];
        renderer.render(
            device,
            &rs.queue,
            &mut encoder,
            view,
            &[Arc::new(group)],
            bg,
            THUMB,
        );
        rs.queue.submit([encoder.finish()]);
        *tex
    }
}

/// An image wire downscaled (nearest) to fit the thumbnail, letterboxed.
fn image_thumb(img: &floret_engine::image::Image) -> egui::ColorImage {
    let (sw, sh) = (img.width().max(1), img.height().max(1));
    let scale = (THUMB.0 as f32 / sw as f32).min(THUMB.1 as f32 / sh as f32);
    let (w, h) = (
        ((sw as f32 * scale).round() as usize).max(1),
        ((sh as f32 * scale).round() as usize).max(1),
    );
    let mut out = egui::ColorImage::new([w, h], vec![Color32::BLACK; w * h]);
    for y in 0..h {
        for x in 0..w {
            let sx = ((x as f32 / scale) as u32).min(sw - 1);
            let sy = ((y as f32 / scale) as u32).min(sh - 1);
            let [r, g, b, a] = img.pixel(sx, sy);
            out.pixels[y * w + x] = Color32::from_rgba_unmultiplied(r, g, b, a);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 200.0))
    }

    #[test]
    fn transport_pauses_steps_and_restarts() {
        let mut t = Transport::default();
        assert_eq!(t.advance(), Advance::Run);
        t.toggle();
        assert_eq!(t.advance(), Advance::Hold);
        t.step();
        assert_eq!(t.advance(), Advance::Step(STEP_DT));
        assert_eq!(t.advance(), Advance::Hold);
        // Stepping while running pauses first.
        let mut r = Transport::default();
        r.step();
        assert!(r.paused);
        assert!(!r.take_restart());
        r.restart();
        assert!(r.take_restart());
        assert!(!r.take_restart());
    }

    #[test]
    fn camera_fits_and_zooms_toward_the_pointer() {
        let cam = Camera::default();
        let size = (100, 100);
        // 100² fits a 400×200 area at 2 pt / px, centred.
        assert_eq!(fit_scale(area(), size), 2.0);
        assert_eq!(
            cam.image_rect(area(), size),
            Rect::from_min_size(pos2(100.0, 0.0), vec2(200.0, 200.0))
        );
        let mut z = cam;
        let at = pos2(150.0, 50.0);
        let before = z.image_rect(area(), size);
        let u = (at - before.min) / before.size();
        z.zoom_at(2.0, at, area(), size, 1.0);
        let after = z.image_rect(area(), size);
        let u2 = (at - after.min) / after.size();
        assert!((u - u2).length() < 1e-5, "{u:?} {u2:?}");
        assert_eq!(after.width(), 400.0);
        z.fit();
        assert_eq!(z, Camera::default());
    }

    #[test]
    fn one_to_one_and_the_texel_size() {
        let mut cam = Camera::default();
        let size = (100, 100);
        cam.one_to_one(area(), size, 2.0);
        assert!((cam.texel(area(), size, 2.0) - 1.0).abs() < 1e-6);
        assert_eq!(cam.image_rect(area(), size).width(), 50.0);
        // Zoom stops at 64 screen px per texel.
        cam.zoom_at(1e6, area().center(), area(), size, 1.0);
        assert!((cam.texel(area(), size, 1.0) - MAX_TEXEL).abs() < 1e-3);
    }

    #[test]
    fn mouse_and_grid() {
        let image = Rect::from_min_size(pos2(10.0, 10.0), vec2(100.0, 50.0));
        assert_eq!(
            canvas_mouse(image, (200, 100), pos2(60.0, 35.0)),
            Some((0.0, 0.0))
        );
        assert_eq!(
            canvas_mouse(image, (200, 100), pos2(10.0, 10.0)),
            Some((-100.0, -50.0))
        );
        assert_eq!(canvas_mouse(image, (200, 100), pos2(0.0, 0.0)), None);
        assert_eq!(grid_lines(0.0, 10.0, 15.0, 42.0), [20.0, 30.0, 40.0]);
        assert_eq!(grid_lines(-5.0, 10.0, 0.0, 20.0), [5.0, 15.0]);
        assert!(grid_lines(0.0, 0.0, 0.0, 10.0).is_empty());
    }
}
