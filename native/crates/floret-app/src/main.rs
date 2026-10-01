//! Floret desktop app — UI spike.
//!
//! Loads the built-in samples, runs them on the Rust engine every frame, and
//! shows them on a native node canvas next to a live GPU preview of the
//! first renderer node's scene (drawn by `floret-render`). Nodes not yet
//! ported to Rust are marked "not ported".

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod bus;
mod canvas;
mod crash;
mod editor;
mod export;
mod gpu_support;
mod host;
mod palette;
mod preview;
mod shader_panel;
mod theme;

use eframe::egui::{self, Align, Layout, RichText};
use floret_engine::patch::PatchFile;
use floret_engine::spec::Catalogue;
use floret_engine::tick::{Outputs, RegionStats};
use floret_engine::{CompileWarning, Engine};
use std::collections::HashMap;
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
    crash::install_hook();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: gpu_support::options(),
        run_and_return: true,
        viewport: egui::ViewportBuilder::default()
            .with_title("Floret")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    let result = eframe::run_native("Floret", options, Box::new(|cc| Ok(Box::new(App::new(cc)))));
    if let Err(e) = &result {
        gpu_support::report_startup_failure(e);
    }
    result
}

struct App {
    sample: usize,
    patch: PatchFile,
    engine: Option<Engine>,
    warnings: Vec<CompileWarning>,
    error: Option<String>,
    outputs: Outputs,
    region_stats: HashMap<usize, RegionStats>,
    tick_errors: Vec<(usize, String)>,
    view: canvas::View,
    preview: preview::PreviewPanel,
    start: Instant,
    last: Instant,
    frame: u64,
    fps: f32,
    query: String,
    show_palette: bool,
    crash: crash::Guard,
    shader_panel: shader_panel::ShaderPanel,
    bus: bus::BusHost,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> App {
        theme::apply(&cc.egui_ctx);
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
            region_stats: HashMap::new(),
            tick_errors: Vec::new(),
            view: canvas::View::default(),
            preview: preview::PreviewPanel::new(cc.wgpu_render_state.as_ref()),
            start: Instant::now(),
            last: Instant::now(),
            frame: 0,
            fps: 0.0,
            query: String::new(),
            show_palette: true,
            crash: crash::Guard::default(),
            shader_panel: shader_panel::ShaderPanel::default(),
            bus: bus::BusHost::new(),
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
        self.view.on_load(&mut self.patch);
        audio::clear();
        self.recompile();
        self.start = Instant::now();
        self.frame = 0;
    }

    fn recompile(&mut self) {
        match Engine::from_patch(&self.patch) {
            Ok((mut engine, warnings)) => {
                engine.set_host(Some(host::shared()));
                audio::attach(&mut engine);
                self.engine = Some(engine);
                self.warnings = warnings;
            }
            Err(e) => {
                self.engine = None;
                self.error = Some(e);
                audio::clear();
            }
        }
        self.outputs.clear();
    }

    fn tick(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.fps = self.fps * 0.9 + (1.0 / dt.max(1e-6)) as f32 * 0.1;
        let t = now.duration_since(self.start).as_secs_f64();
        let frame = self
            .preview
            .frame_ctx(ctx, self.engine.as_ref(), t, dt, self.frame);
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        if self.crash.paused() {
            return;
        }
        if let Some(r) = self
            .crash
            .run(crash::Stage::Tick, &self.patch, || engine.tick(&frame))
        {
            self.outputs = r.outputs;
            self.region_stats = r.region_stats;
            self.tick_errors = r.errors;
            self.frame += 1;
        }
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
                self.view.request_fit();
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
        palette::show(ui, &mut self.query, &mut self.view, &mut self.patch);
        // Palette clicks add nodes; recompile picks them up after the canvas.
    }

    fn inspector(&self, ui: &mut egui::Ui) {
        let Some(id) = self.view.primary() else {
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
        self.tick(ui.ctx());
        ui.ctx().request_repaint();
        self.bus
            .process(frame.wgpu_render_state(), self.preview.clock);
        let mut warnings: Vec<String> = self.warnings.iter().map(|w| w.message.clone()).collect();
        warnings.extend(
            self.shader_panel
                .warnings(&self.patch, self.preview.shader_errors()),
        );

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
                    if !warnings.is_empty() {
                        ui.label(
                            RichText::new(format!("⚠ {} warnings", warnings.len()))
                                .color(theme::DANGER)
                                .small(),
                        )
                        .on_hover_text(warnings.join("\n"));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        self.bus.status(ui);
                        audio::status(ui);
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
                let rs = frame.wgpu_render_state();
                if self.crash.render_ok() {
                    let (preview, engine, outputs) =
                        (&mut self.preview, self.engine.as_ref(), &self.outputs);
                    preview.doc_name = self
                        .patch
                        .meta
                        .as_ref()
                        .and_then(|m| m.name.clone())
                        .unwrap_or_default();
                    self.crash.run(crash::Stage::Render, &self.patch, || {
                        preview.show(ui, rs, engine, outputs);
                    });
                }
                ui.add_space(10.0);
                ui.separator();
                ui.label(RichText::new("Inspector").color(theme::FG_1).small());
                egui::ScrollArea::vertical().show(ui, |ui| self.inspector(ui));
            });

        egui::CentralPanel::no_frame().show(ui, |ui| {
            let cat = Catalogue::builtin();
            if let Some(engine) = &self.engine {
                let live = canvas::Live {
                    engine,
                    outputs: &self.outputs,
                    region_stats: &self.region_stats,
                    warnings: &self.warnings,
                    errors: &self.tick_errors,
                };
                canvas::show(ui, &mut self.patch, &live, cat, &mut self.view);
            }
            // Inline value edits reach the running engine without a recompile.
            let edits = self.view.take_live_edits();
            if let Some(engine) = self.engine.as_mut() {
                editor::values::apply_live(engine, cat, &self.patch, edits);
            }
            let selected = self.view.primary().cloned();
            if self.shader_panel.show(
                ui.ctx(),
                &mut self.patch,
                selected.as_deref(),
                self.preview.shader_errors(),
            ) {
                self.view.commit(&mut self.patch);
            }
            // Structural edits (wires, nodes, undo) rebuild the engine.
            if self.view.take_recompile() {
                self.recompile();
            }
        });

        match self.crash.show(ui.ctx(), &self.patch) {
            Some(crash::Action::Reload) => {
                self.recompile();
                self.start = Instant::now();
                self.frame = 0;
            }
            Some(crash::Action::ClearOutputs) => self.outputs.clear(),
            None => {}
        }
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
