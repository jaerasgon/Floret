//! Floret desktop app — UI spike.
//!
//! Loads the built-in samples, runs them on the Rust engine every frame, and
//! shows them on a native node canvas next to a live GPU preview of the
//! first renderer node's scene (drawn by `floret-render`). Nodes not yet
//! ported to Rust are marked "not ported".

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod canvas;
mod preview;
mod theme;

use eframe::egui::{self, vec2, Align, Layout, RichText};
use floret_engine::patch::{PatchFile, PatchNode, Position};
use floret_engine::scene::SceneNode;
use floret_engine::spec::Catalogue;
use floret_engine::tick::Outputs;
use floret_engine::{Engine, FrameCtx};
use std::sync::Arc;
use std::time::Instant;

pub struct Sample {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub flora: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/samples.rs"));

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("Floret")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("Floret", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

struct App {
    sample: usize,
    patch: PatchFile,
    engine: Option<Engine>,
    warnings: Vec<String>,
    error: Option<String>,
    outputs: Outputs,
    view: canvas::View,
    fit_pending: bool,
    canvas_rect: egui::Rect,
    /// GPU scene preview (`None` without a wgpu render state).
    preview: Option<preview::Preview>,
    /// Canvas size (renderer node's width × height).
    canvas: (u32, u32),
    /// Pointer over the preview, canvas pixels, centre origin.
    mouse: (f64, f64),
    mouse_pressed: bool,
    key: String,
    key_pressed: bool,
    start: Instant,
    last: Instant,
    frame: u64,
    fps: f32,
    query: String,
    show_palette: bool,
    next_id: u64,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> App {
        theme::apply(&cc.egui_ctx);
        let preview = cc.wgpu_render_state.as_ref().map(preview::Preview::new);
        // FLORET_SAMPLE=<id> picks the startup sample (handy for screenshots).
        let want = std::env::var("FLORET_SAMPLE").unwrap_or_else(|_| "tut-03-animate".into());
        let start = SAMPLES.iter().position(|s| s.id == want).unwrap_or(0);
        let mut app = App {
            sample: start,
            patch: empty_patch(),
            engine: None,
            warnings: Vec::new(),
            error: None,
            outputs: Vec::new(),
            view: canvas::View::default(),
            fit_pending: true,
            canvas_rect: egui::Rect::ZERO,
            preview,
            canvas: floret_render::DEFAULT_CANVAS,
            mouse: (0.0, 0.0),
            mouse_pressed: false,
            key: String::new(),
            key_pressed: false,
            start: Instant::now(),
            last: Instant::now(),
            frame: 0,
            fps: 0.0,
            query: String::new(),
            show_palette: true,
            next_id: 1,
        };
        app.load_sample(start);
        app
    }

    fn load_sample(&mut self, i: usize) {
        self.sample = i;
        match PatchFile::load(SAMPLES[i].flora) {
            Ok(p) => {
                self.patch = p;
                self.error = None;
            }
            Err(e) => {
                self.patch = empty_patch();
                self.error = Some(e.to_string());
            }
        }
        self.recompile();
        self.start = Instant::now();
        self.frame = 0;
        self.fit_pending = true;
        self.view.selected = None;
    }

    fn recompile(&mut self) {
        match Engine::from_patch(&self.patch) {
            Ok((engine, warnings)) => {
                self.engine = Some(engine);
                self.warnings = warnings.into_iter().map(|w| w.message).collect();
            }
            Err(e) => {
                self.engine = None;
                self.error = Some(e);
            }
        }
        self.outputs.clear();
    }

    fn add_node(&mut self, ty: &str) {
        let Some(spec) = Catalogue::builtin().get(ty) else {
            return;
        };
        let center = self.canvas_rect.center();
        let p = (center - self.canvas_rect.min) / self.view.zoom - self.view.pan;
        let id = format!("n{}", self.next_id);
        self.next_id += 1;
        self.patch.nodes.push(PatchNode {
            id: id.clone(),
            def_type: ty.to_owned(),
            position: Position {
                x: f64::from(p.x - 84.0),
                y: f64::from(p.y - 30.0),
            },
            params: spec.default_params.clone(),
            parent_id: None,
            port_values: None,
            ports: None,
            width: None,
            height: None,
        });
        self.view.selected = Some(id);
        self.recompile();
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.fps = self.fps * 0.9 + (1.0 / dt.max(1e-6)) as f32 * 0.1;
        // Canvas size comes from the renderer's params, read before the tick
        // like the web driver (a wired size settles one frame later).
        self.canvas = self.canvas_size();
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        let frame = FrameCtx {
            t: now.duration_since(self.start).as_secs_f64(),
            dt,
            frame_count: self.frame as f64,
            width: f64::from(self.canvas.0),
            height: f64::from(self.canvas.1),
            mouse_x: self.mouse.0,
            mouse_y: self.mouse.1,
            mouse_is_pressed: self.mouse_pressed,
            key: self.key.clone(),
            key_is_pressed: self.key_pressed,
        };
        self.outputs = engine.tick(&frame).outputs;
        self.frame += 1;
    }

    /// The first renderer's canvas size (TS driver rules), or the default.
    fn canvas_size(&self) -> (u32, u32) {
        let Some(engine) = &self.engine else {
            return self.canvas;
        };
        match engine.graph.renderer() {
            Some(r) => {
                let id = &engine.graph.nodes[r].id;
                floret_render::canvas_size(
                    engine.params.get(id, "width"),
                    engine.params.get(id, "height"),
                )
            }
            None => self.canvas,
        }
    }

    /// The first renderer's scene: its layers (flattened) and background.
    fn scene(&self) -> (Vec<Arc<SceneNode>>, [f64; 4]) {
        let out = self.engine.as_ref().and_then(|e| {
            let r = e.graph.renderer()?;
            self.outputs.get(r)?.clone()
        });
        match out {
            Some(out) => (
                out.first()
                    .map(|s| floret_render::flatten_layers(s))
                    .unwrap_or_default(),
                out.get(1)
                    .map(|s| floret_render::background(s))
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]),
            ),
            None => (Vec::new(), [0.0, 0.0, 0.0, 1.0]),
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

    fn preview_panel(&mut self, ui: &mut egui::Ui, rs: Option<&eframe::egui_wgpu::RenderState>) {
        let w = ui.available_width();
        let aspect = self.canvas.1 as f32 / self.canvas.0.max(1) as f32;
        let h = (w * aspect).clamp(120.0, (ui.available_height() * 0.62).max(120.0));
        let (area, response) = ui.allocate_exact_size(vec2(w, h), egui::Sense::click_and_drag());
        ui.painter()
            .rect_filled(area, egui::CornerRadius::same(6), theme::BG_0);
        let image = preview::fit(area, self.canvas);
        let (layers, bg) = self.scene();
        match (rs, self.preview.as_mut()) {
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
            if let Some(m) = preview::canvas_mouse(image, self.canvas, pos) {
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

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new("✿ Floret").color(theme::GOLD).strong().size(15.0));
            ui.separator();
            ui.menu_button("Samples", |ui| {
                let mut cats: Vec<&str> = Vec::new();
                for s in SAMPLES {
                    if !cats.contains(&s.category) {
                        cats.push(s.category);
                    }
                }
                for cat in cats {
                    ui.menu_button(cat, |ui| {
                        for (i, s) in SAMPLES.iter().enumerate().filter(|(_, s)| s.category == cat) {
                            if ui.selectable_label(i == self.sample, s.name).on_hover_text(s.description).clicked() {
                                self.load_sample(i);
                                ui.close();
                            }
                        }
                    });
                }
            });
            ui.label(RichText::new(SAMPLES[self.sample].name).color(theme::FG_1));
            ui.separator();
            ui.toggle_value(&mut self.show_palette, "Nodes");
            if ui.button("Fit").on_hover_text("Fit patch to view (or double-click the canvas)").clicked() {
                self.fit_pending = true;
            }
            if ui.button("Restart").clicked() {
                self.recompile();
                self.start = Instant::now();
                self.frame = 0;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{:.0} fps", self.fps)).color(theme::FG_2).monospace());
                if let Some(engine) = &self.engine {
                    let missing = engine.graph.nodes.iter().filter(|n| n.unimplemented).count();
                    if missing > 0 {
                        ui.label(RichText::new(format!("{missing} nodes not ported yet")).color(theme::FG_2))
                            .on_hover_text("These node types haven't been ported to the Rust engine yet; they output nothing.");
                    }
                }
            });
        });
    }

    fn palette(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Search nodes…")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(4.0);
        let q = self.query.to_lowercase();
        let mut add = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut by_cat: Vec<(&str, Vec<&floret_engine::spec::NodeSpec>)> = Vec::new();
                for s in Catalogue::builtin().iter().filter(|s| !s.hidden) {
                    if !q.is_empty() && !s.label.to_lowercase().contains(&q) && !s.ty.contains(&q) {
                        continue;
                    }
                    match by_cat.iter_mut().find(|(c, _)| *c == s.category) {
                        Some((_, v)) => v.push(s),
                        None => by_cat.push((&s.category, vec![s])),
                    }
                }
                by_cat.sort_by_key(|(c, _)| *c);
                for (cat, specs) in by_cat {
                    let header = RichText::new(cat).color(theme::category_color(cat));
                    egui::CollapsingHeader::new(header)
                        .default_open(!q.is_empty())
                        .show(ui, |ui| {
                            for s in specs {
                                let r = ui.selectable_label(false, &s.label);
                                let r = match &s.description {
                                    Some(d) => r.on_hover_text(d),
                                    None => r,
                                };
                                if r.clicked() {
                                    add = Some(s.ty.clone());
                                }
                            }
                        });
                }
            });
        if let Some(ty) = add {
            self.add_node(&ty);
        }
    }

    fn inspector(&self, ui: &mut egui::Ui) {
        let Some(id) = &self.view.selected else {
            ui.label(RichText::new("Select a node to inspect it.").color(theme::FG_2));
            return;
        };
        let Some(n) = self.patch.nodes.iter().find(|n| &n.id == id) else {
            return;
        };
        let spec = Catalogue::builtin().get(&n.def_type);
        ui.label(RichText::new(spec.map_or(n.def_type.as_str(), |s| s.label.as_str())).strong());
        ui.label(RichText::new(&n.def_type).color(theme::FG_2).monospace());
        if let Some(d) = spec.and_then(|s| s.description.as_ref()) {
            ui.label(RichText::new(d).color(theme::FG_1));
        }
        if let (Some(engine), Some(spec)) = (&self.engine, spec) {
            if let Some(out) = engine
                .graph
                .idx(id)
                .and_then(|i| self.outputs.get(i))
                .and_then(|o| o.as_ref())
            {
                ui.add_space(6.0);
                for (p, v) in spec.outputs.iter().zip(out.iter()) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(p.label.as_deref().unwrap_or(&p.name)).color(theme::FG_1),
                        );
                        let text = format!("{:?}", v.iter().take(8).collect::<Vec<_>>());
                        ui.label(RichText::new(text).monospace().size(11.0));
                    });
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.read_keys(ui.ctx());
        self.tick();
        let rs = frame.wgpu_render_state().cloned();
        ui.ctx().request_repaint();

        egui::Panel::top("toolbar")
            .exact_size(38.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_1)
                    .inner_margin(egui::Margin::symmetric(10, 0)),
            )
            .show(ui, |ui| self.toolbar(ui));

        egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_1)
                    .inner_margin(egui::Margin::symmetric(10, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let (dot, text) = match (&self.error, self.engine.is_some()) {
                        (Some(e), _) => (theme::DANGER, e.clone()),
                        (None, true) => (theme::GOLD, "Live".to_owned()),
                        (None, false) => (theme::FG_2, "No patch".to_owned()),
                    };
                    ui.label(RichText::new("●").color(dot));
                    ui.label(RichText::new(text).color(theme::FG_1).small());
                    ui.label(
                        RichText::new(format!("frame {}", self.frame))
                            .color(theme::FG_2)
                            .small()
                            .monospace(),
                    );
                    ui.label(
                        RichText::new(format!("{} nodes", self.patch.nodes.len()))
                            .color(theme::FG_2)
                            .small(),
                    );
                    if !self.warnings.is_empty() {
                        ui.label(
                            RichText::new(format!("⚠ {} warnings", self.warnings.len()))
                                .color(theme::DANGER)
                                .small(),
                        )
                        .on_hover_text(self.warnings.join("\n"));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new("native spike · engine + renderer in Rust")
                                .color(theme::FG_3)
                                .small(),
                        );
                    });
                });
            });

        if self.show_palette {
            egui::Panel::left("palette")
                .resizable(true)
                .default_size(220.0)
                .size_range(160.0..=360.0)
                .frame(
                    egui::Frame::new()
                        .fill(theme::BG_1)
                        .inner_margin(egui::Margin::symmetric(10, 4)),
                )
                .show(ui, |ui| self.palette(ui));
        }

        egui::Panel::right("side")
            .resizable(true)
            .default_size(400.0)
            .size_range(260.0..=720.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_1)
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Preview").color(theme::FG_1).small());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut info = format!("{} × {}", self.canvas.0, self.canvas.1);
                        if let Some(p) = &self.preview {
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
                self.preview_panel(ui, rs.as_ref());
                ui.add_space(10.0);
                ui.separator();
                ui.label(RichText::new("Inspector").color(theme::FG_1).small());
                egui::ScrollArea::vertical().show(ui, |ui| self.inspector(ui));
            });

        egui::CentralPanel::no_frame().show(ui, |ui| {
            self.canvas_rect = ui.max_rect();
            if self.fit_pending {
                self.view.fit(&self.patch, self.canvas_rect);
                self.fit_pending = false;
            }
            let cat = Catalogue::builtin();
            if let Some(engine) = &self.engine {
                canvas::show(
                    ui,
                    &mut self.patch,
                    engine,
                    &self.outputs,
                    cat,
                    &mut self.view,
                );
            }
        });
    }
}

fn empty_patch() -> PatchFile {
    PatchFile {
        kind: floret_engine::patch::PATCH_KIND.into(),
        version: floret_engine::patch::CURRENT_PATCH_VERSION,
        app_version: None,
        meta: None,
        nodes: Vec::new(),
        edges: Vec::new(),
    }
}
