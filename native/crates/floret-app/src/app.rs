//! The app window: the open document, the running engine, and the shell
//! around the editor (menus, toolbar, status bar, side panels, dialogs).
//!
//! Layout, top to bottom: toolbar ([`App::toolbar`]), status bar
//! ([`App::status_bar`]), the node palette (left), the docked previews,
//! inspector, help, library and shader panels (right,
//! [`App::side_panel`], see [`crate::workspace`]) and the node canvas
//! (centre).

use crate::files::{self, Document, Origin};
use crate::menu::{self, Command, MenuState};
use crate::prefs::{Prefs, PrefsWindow, Startup};
use crate::{
    audio, canvas, crash, devices, editor, help, host, palette, preview, recording, recovery,
    shader_panel, theme, welcome, workspace, SAMPLES,
};
use eframe::egui::{self, Align, Layout, RichText};
use egui_phosphor::regular as icons;
use floret_engine::patch::PatchFile;
use floret_engine::spec::Catalogue;
use floret_engine::tick::{Outputs, RegionStats};
use floret_engine::{CompileWarning, Engine};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Sample loaded by "On startup: Starter sample".
const STARTER: &str = "grid-ripple";

/// How the app was launched.
#[derive(Default)]
pub struct Launch {
    /// A file from the command line (or Finder / Explorer "Open with").
    pub file: Option<PathBuf>,
    /// `FLORET_SAMPLE=<id>`: a scripted launch (screenshots); no welcome
    /// screen or recovery prompt.
    pub sample: Option<String>,
}

/// Something that would discard unsaved changes, held while the user is
/// asked about them.
#[derive(Clone, Debug)]
enum After {
    New,
    /// Open a file (`None`: ask which).
    Open(Option<PathBuf>),
    Sample(usize),
    /// A patch from the library panel.
    Patch(Box<PatchFile>, String),
    Recover(recovery::Candidate),
    Quit,
}

pub struct App {
    // ── Document
    patch: PatchFile,
    doc: Document,
    recent: files::Recent,
    autosave: recovery::Autosaver,
    /// Recovery files offered on launch.
    recovery: Vec<recovery::Candidate>,
    /// Asking whether to save before doing this.
    confirm: Option<After>,
    allow_close: bool,
    title: String,
    /// A transient message for the status bar (text, error?, when).
    notice: Option<(String, bool, Instant)>,
    // ── Engine
    engine: Option<Engine>,
    warnings: Vec<CompileWarning>,
    error: Option<String>,
    outputs: Outputs,
    region_stats: HashMap<usize, RegionStats>,
    tick_errors: Vec<(usize, String)>,
    start: Instant,
    last: Instant,
    frame: u64,
    fps: f32,
    // ── Editor and panels
    view: canvas::View,
    preview: preview::PreviewPanel,
    /// The Export Video window and any recording in progress.
    recorder: recording::Recorder,
    /// When the monitor list for the output window was last read.
    monitors_read: Option<Instant>,
    query: String,
    crash: crash::Guard,
    shader_panel: shader_panel::ShaderPanel,
    workspace: workspace::Workspace,
    // ── Shell
    prefs: Prefs,
    prefs_window: PrefsWindow,
    welcome: welcome::Welcome,
    show_shortcuts: bool,
    /// The `?` overlay over the canvas.
    shortcuts_overlay: bool,
    show_about: bool,
    gpu_name: String,
    /// Menu commands waiting for the next frame.
    commands: Vec<Command>,
    /// Editing keys replayed into egui's input (see [`menu::inject`]).
    inject: Vec<Command>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    native_menu: Option<menu::native::NativeMenu>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, launch: Launch) -> App {
        let prefs = Prefs::load();
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, prefs.theme.egui());
        devices::install();
        floret_video::install();
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        cc.egui_ctx.set_zoom_factor(prefs.ui_scale);
        let gpu_name = cc.wgpu_render_state.as_ref().map_or_else(
            || "No GPU renderer".to_owned(),
            |rs| {
                let i = rs.adapter.get_info();
                format!("{} · {:?}", i.name, i.backend)
            },
        );
        let mut view = canvas::View::default();
        apply_editor_prefs(&mut view, &prefs);
        let scripted = launch.sample.is_some();
        let mut app = App {
            patch: empty_patch(),
            doc: Document::untitled(0),
            recent: files::Recent::load(),
            autosave: recovery::Autosaver::default(),
            recovery: if scripted {
                Vec::new()
            } else {
                recovery::scan()
            },
            confirm: None,
            allow_close: false,
            title: String::new(),
            notice: None,
            engine: None,
            warnings: Vec::new(),
            error: None,
            outputs: Vec::new(),
            region_stats: HashMap::new(),
            tick_errors: Vec::new(),
            start: Instant::now(),
            last: Instant::now(),
            frame: 0,
            fps: 0.0,
            view,
            preview: preview::PreviewPanel::new(cc.wgpu_render_state.as_ref()),
            recorder: recording::Recorder::default(),
            monitors_read: None,
            query: String::new(),
            crash: crash::Guard::default(),
            shader_panel: shader_panel::ShaderPanel::default(),
            workspace: workspace::Workspace::load(),
            prefs,
            prefs_window: PrefsWindow::default(),
            welcome: welcome::Welcome::default(),
            show_shortcuts: false,
            shortcuts_overlay: false,
            show_about: false,
            gpu_name,
            commands: Vec::new(),
            inject: Vec::new(),
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            native_menu: None,
        };
        app.startup(launch);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if std::env::var_os("FLORET_EGUI_MENU").is_none() {
            app.native_menu = menu::native::NativeMenu::new(cc, &app.menu_state());
        }
        app
    }

    /// The first document: a file from the command line, `FLORET_SAMPLE`,
    /// else the startup preference.
    fn startup(&mut self, launch: Launch) {
        self.new_patch();
        if let Some(id) = &launch.sample {
            let i = SAMPLES.iter().position(|s| s.id == id).unwrap_or(0);
            self.load_sample(i);
            return;
        }
        if let Some(path) = launch.file {
            self.open_path(&path);
        } else {
            match self.prefs.startup_patch {
                Startup::Last => {
                    if let Some(p) = self.recent.paths().first().filter(|p| p.exists()).cloned() {
                        self.open_path(&p);
                    }
                }
                Startup::Starter => {
                    if let Some(i) = SAMPLES.iter().position(|s| s.id == STARTER) {
                        self.load_sample(i);
                    }
                }
                Startup::Blank => {}
            }
        }
        if self.patch.nodes.is_empty() && self.prefs.show_welcome && self.recovery.is_empty() {
            self.welcome.show_home();
        }
    }

    fn rev(&self) -> u64 {
        self.view.revision()
    }

    fn dirty(&self) -> bool {
        self.doc.is_dirty(self.rev())
    }

    fn menu_state(&self) -> MenuState<'_> {
        MenuState {
            recent: self.recent.paths(),
            palette_visible: self.prefs.palette_visible,
            theme: self.prefs.theme,
        }
    }

    fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.notice = Some((text.into(), error, Instant::now()));
    }

    // ── Documents ───────────────────────────────────────────────────────

    /// Replace the open patch; `doc` is made with the new undo revision.
    fn set_patch(&mut self, patch: PatchFile, doc: impl FnOnce(u64) -> Document) {
        self.patch = patch;
        self.error = None;
        self.view.on_load(&mut self.patch);
        audio::clear();
        self.doc = doc(self.view.revision());
        recovery::set_current(self.doc.path(), &self.doc.name);
        host::set_document(self.doc.path());
        self.recompile();
        self.restart();
        if !self.patch.presets.is_empty() {
            self.workspace.show_presets();
        }
    }

    fn new_patch(&mut self) {
        self.set_patch(empty_patch(), Document::untitled);
    }

    fn load_sample(&mut self, i: usize) {
        let s = &SAMPLES[i];
        match floret_engine::load_patch(s.flora) {
            Ok(p) => self.set_patch(p, |rev| Document::sample(s.id, s.name, rev)),
            Err(e) => self.notify(format!("Couldn't load {}: {e}", s.name), true),
        }
    }

    fn open_path(&mut self, path: &Path) {
        match files::read(path) {
            Ok(p) => {
                let path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
                self.recent.push(&path);
                self.set_patch(p, |rev| Document::file(path, rev));
                self.welcome.open = false;
            }
            Err(e) => {
                if !path.exists() {
                    self.recent.remove(path);
                }
                self.notify(format!("Couldn't open {}: {e}", path.display()), true);
            }
        }
    }

    /// Save in place, or ask where. True when saved.
    fn save(&mut self) -> bool {
        match self.doc.path().map(Path::to_owned) {
            Some(p) => self.save_to(p),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        match files::save_dialog(&self.doc) {
            Some(p) => self.save_to(p),
            None => false,
        }
    }

    fn save_to(&mut self, path: PathBuf) -> bool {
        match files::save(&self.patch, &path) {
            Ok(()) => {
                let name = files::file_stem(&path);
                self.patch.meta.get_or_insert_with(Default::default).name = Some(name.clone());
                self.recent.push(&path);
                self.doc.saved_as(path, self.rev());
                recovery::set_current(self.doc.path(), &self.doc.name);
                host::set_document(self.doc.path());
                self.notify(format!("Saved {name}"), false);
                true
            }
            Err(e) => {
                self.notify(format!("Couldn't save {}: {e}", path.display()), true);
                false
            }
        }
    }

    /// Do `after`, first asking about unsaved changes if there are any.
    fn request(&mut self, ctx: &egui::Context, after: After) {
        if self.dirty() {
            self.confirm = Some(after);
        } else {
            self.perform(ctx, after);
        }
    }

    fn perform(&mut self, ctx: &egui::Context, after: After) {
        match after {
            After::New => self.new_patch(),
            After::Open(Some(p)) => self.open_path(&p),
            After::Open(None) => {
                if let Some(p) = files::open_dialog(self.doc.path()) {
                    self.open_path(&p);
                }
            }
            After::Sample(i) => self.load_sample(i),
            After::Patch(p, name) => self.set_patch(*p, |rev| {
                let mut d = Document::untitled(rev);
                d.name = name;
                d
            }),
            After::Recover(c) => self.recover(&c),
            After::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn recover(&mut self, c: &recovery::Candidate) {
        match c.read() {
            Ok(p) => {
                let origin = match &c.meta.source {
                    Some(s) if s.exists() => Origin::File(s.clone()),
                    _ => Origin::Untitled,
                };
                let name = c.meta.name.clone();
                self.set_patch(p, |rev| Document::recovered(origin, name, rev));
                c.discard();
                self.autosave.soon();
                self.notify("Restored unsaved changes — save to keep them", false);
            }
            Err(e) => self.notify(format!("Couldn't restore: {e}"), true),
        }
        self.recovery.retain(|x| x != c);
    }

    // ── Engine ──────────────────────────────────────────────────────────

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

    /// Restart time (fresh frame count; the engine keeps its graph).
    fn restart(&mut self) {
        self.start = Instant::now();
        self.frame = 0;
    }

    fn tick(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let mut dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.fps = self.fps * 0.9 + (1.0 / dt.max(1e-6)) as f32 * 0.1;
        // Preview transport: restart, and pause / step (time holds while paused).
        if self.preview.transport.take_restart() {
            self.recompile();
            self.restart();
        }
        match self.preview.transport.advance() {
            preview::Advance::Run => {}
            preview::Advance::Hold => {
                self.start += Duration::from_secs_f64(dt);
                return;
            }
            preview::Advance::Step(step) => {
                self.start += Duration::from_secs_f64((dt - step).max(0.0));
                dt = step;
            }
        }
        let t = now.duration_since(self.start).as_secs_f64();
        let frame = self
            .preview
            .frame_ctx(ctx, self.engine.as_ref(), t, dt, self.frame);
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        // The loop length is a patch setting, edited without a recompile.
        engine.loop_length = self.patch.loop_seconds();
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

    // ── Commands ────────────────────────────────────────────────────────

    fn run(&mut self, ctx: &egui::Context, c: Command) {
        if c.is_editing() {
            self.inject.push(c);
            ctx.request_repaint();
            return;
        }
        match c {
            Command::New => self.request(ctx, After::New),
            Command::Open => self.request(ctx, After::Open(None)),
            Command::OpenRecent(p) => self.request(ctx, After::Open(Some(p))),
            Command::ClearRecent => self.recent.clear(),
            Command::Save => {
                self.save();
            }
            Command::SaveAs => {
                self.save_as();
            }
            Command::BrowseSamples => self.workspace.show_library(),
            Command::ExportVideo => self.recorder.open(None, None),
            Command::Quit => self.request(ctx, After::Quit),
            Command::ToggleNodes => self.prefs.palette_visible ^= true,
            Command::ZoomIn => self.prefs.step_scale(1),
            Command::ZoomOut => self.prefs.step_scale(-1),
            Command::ZoomReset => self.prefs.ui_scale = 1.0,
            Command::Theme(t) => self.prefs.theme = t,
            Command::Restart => {
                self.recompile();
                self.restart();
            }
            Command::Preferences => self.prefs_window.open = true,
            Command::Shortcuts => self.show_shortcuts = true,
            Command::Welcome => self.welcome.show_home(),
            Command::About => self.show_about = true,
            Command::NewVariation => {
                crate::presets_panel::new_variation(&mut self.patch, &mut self.view);
                self.notify(format!("Variation · seed {}", self.patch.seed), false);
            }
            _ => {}
        }
    }

    /// Apply preferences after they change (menu, Preferences window).
    fn prefs_changed(&mut self, ctx: &egui::Context, before: &Prefs) {
        if self.prefs == *before {
            return;
        }
        if self.prefs.theme != before.theme {
            theme::apply(ctx, self.prefs.theme.egui());
        }
        if self.prefs.ui_scale != before.ui_scale {
            ctx.set_zoom_factor(self.prefs.ui_scale);
        }
        apply_editor_prefs(&mut self.view, &self.prefs);
        self.prefs.save();
    }

    // ── Panels ──────────────────────────────────────────────────────────

    pub fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            let egui_menu = self.native_menu.is_none();
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let egui_menu = true;
            if egui_menu {
                // The menu bar takes all the width it's given.
                let size = egui::vec2(180.0, ui.available_height());
                let layout = Layout::left_to_right(Align::Center);
                ui.allocate_ui_with_layout(size, layout, |ui| {
                    if let Some(c) = menu::bar(ui, &self.menu_state()) {
                        self.commands.push(c);
                    }
                });
                cluster_gap(ui);
            }
            // Document.
            let (logo, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
            theme::paint_logo(ui.painter(), logo.center(), 16.0, *theme::GOLD);
            let dirty = self.dirty();
            let name = RichText::new(&self.doc.name)
                .color(theme::FG_0)
                .font(theme::medium(12.5));
            let hover = match &self.doc.origin {
                Origin::File(p) => p.display().to_string(),
                Origin::Sample(_) => "Built-in sample — Save As to keep your changes".into(),
                Origin::Untitled => "Not saved yet".into(),
            };
            ui.label(name).on_hover_text(hover);
            if dirty {
                dot(ui, *theme::FG_2).on_hover_text("Unsaved changes");
            }
            cluster_gap(ui);
            // View.
            toggle(
                ui,
                &mut self.prefs.palette_visible,
                icons::SIDEBAR_SIMPLE,
                &shortcut_hint(&Command::ToggleNodes),
            );
            if tool(ui, icons::FRAME_CORNERS, &shortcut_hint(&Command::FitView)).clicked() {
                self.view.request_fit();
            }
            if tool(
                ui,
                icons::TREE_STRUCTURE,
                "Arrange: auto-arrange the patch left → right (undo with Ctrl/⌘+Z)",
            )
            .clicked()
            {
                self.view.request_arrange();
            }
            cluster_gap(ui);
            // Canvas toggles.
            let snap_tip = format!("Snap nodes to the {} px grid", self.view.grid);
            toggle(ui, &mut self.view.snap, icons::MAGNET, &snap_tip);
            toggle(
                ui,
                &mut self.view.wire_flow,
                icons::FLOW_ARROW,
                "Flow: animate data flowing along wires",
            );
            cluster_gap(ui);
            // Transport: the whole patch's clock.
            let t = &mut self.preview.transport;
            let (glyph, tip) = if t.paused {
                (icons::PLAY, "Play (Space over the preview)")
            } else {
                (icons::PAUSE, "Pause: time stops (Space over the preview)")
            };
            if tool(ui, glyph, tip).clicked() {
                t.toggle();
            }
            if tool(ui, icons::SKIP_FORWARD, "Step one frame (pauses)").clicked() {
                t.step();
            }
            if tool(
                ui,
                icons::ARROW_COUNTER_CLOCKWISE,
                &shortcut_hint(&Command::Restart),
            )
            .clicked()
            {
                t.restart();
            }
            // The patch's loop length (also in the Export window).
            ui.label(RichText::new("Loop").color(theme::FG_2));
            recording::loop_length_field(
                ui,
                &mut self.patch,
                &mut self.view,
                egui::vec2(52.0, TOOL_SIZE.y),
            );
            cluster_gap(ui);
            // Variations: re-roll the patch seed.
            let tip = format!(
                "{}: re-roll the patch seed (now {})",
                shortcut_hint(&Command::NewVariation),
                self.patch.seed
            );
            let vary = egui::Button::new((icon(icons::DICE_FIVE, *theme::FG_1), "Vary"))
                .min_size(TOOL_SIZE)
                .frame_when_inactive(false);
            if ui.add(vary).on_hover_text(tip).clicked() {
                self.commands.push(Command::NewVariation);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if tool(ui, icons::GEAR_SIX, &shortcut_hint(&Command::Preferences)).clicked() {
                    self.prefs_window.open = true;
                }
                toggle(
                    ui,
                    &mut self.shortcuts_overlay,
                    icons::QUESTION,
                    "Keyboard shortcuts (?)",
                );
                ui.add_space(4.0);
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

    pub fn status_bar(&mut self, ui: &mut egui::Ui, warnings: &[(String, Vec<String>)]) {
        ui.horizontal_centered(|ui| {
            let (dot_color, text) = match (&self.error, self.engine.is_some(), self.crash.paused())
            {
                (Some(e), _, _) => (theme::DANGER, e.clone()),
                (None, true, true) => (theme::FG_2, "Paused".to_owned()),
                (None, true, false) => (theme::GOLD, "Live".to_owned()),
                (None, false, _) => (theme::FG_2, "No patch".to_owned()),
            };
            dot(ui, *dot_color);
            ui.label(RichText::new(text).color(theme::FG_1).small());
            for segment in [
                format!("frame {}", self.frame),
                format!("{} nodes", self.patch.nodes.len()),
            ] {
                ui.label(
                    RichText::new(segment)
                        .color(theme::FG_2)
                        .font(egui::FontId::monospace(10.5)),
                );
            }
            if !warnings.is_empty() {
                let label = (
                    theme::icon(icons::WARNING, 12.0, theme::DANGER),
                    RichText::new(format!("{} warnings", warnings.len()))
                        .color(theme::DANGER)
                        .small(),
                );
                ui.menu_button(label, |ui| {
                    ui.set_max_width(420.0);
                    for (w, nodes) in warnings {
                        let b = ui.add_enabled(!nodes.is_empty(), egui::Button::new(w).wrap());
                        if b.clicked() {
                            if let Some(id) = nodes.first() {
                                self.view.select_only(id);
                            }
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("Click a warning to select its node");
            }
            if let Some((text, err, at)) = &self.notice {
                if at.elapsed() < Duration::from_secs(6) {
                    let col = if *err { theme::DANGER } else { theme::FG_1 };
                    ui.separator();
                    ui.label(RichText::new(text).color(col).small());
                } else {
                    self.notice = None;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                        .color(theme::FG_2)
                        .small(),
                );
                audio::status(ui);
                ui.label(
                    RichText::new("Tab add node · Q favorites · ? shortcuts")
                        .color(theme::FG_2)
                        .small(),
                );
            });
        });
    }

    /// The side dock: previews, inspector, help, library, shader, profiler.
    fn side_panel(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        self.preview.doc_name = self.doc.name.clone();
        let sample = match &self.doc.origin {
            Origin::Sample(id) => SAMPLES.iter().position(|s| s.id == *id),
            _ => None,
        };
        let action = self.workspace.show(
            ui,
            &mut workspace::Shell {
                patch: &mut self.patch,
                view: &mut self.view,
                preview: &mut self.preview,
                shader: &mut self.shader_panel,
                crash: &mut self.crash,
                engine: self.engine.as_ref(),
                outputs: &self.outputs,
                rs: frame.wgpu_render_state(),
                samples: SAMPLES,
                sample,
            },
        );
        if let Some(engine) = self.engine.as_mut() {
            engine.set_profiling(self.workspace.profiling());
        }
        if let Some((renderer, format)) = self.preview.take_record() {
            self.recorder.open(Some(renderer), Some(format));
        }
        match action {
            Some(workspace::ShellAction::LoadSample(i)) => self.request(ui.ctx(), After::Sample(i)),
            Some(workspace::ShellAction::LoadPatch(p)) => {
                let name = p
                    .meta
                    .as_ref()
                    .and_then(|m| m.name.clone())
                    .unwrap_or_else(|| "Untitled".into());
                self.request(ui.ctx(), After::Patch(p, name));
            }
            None => {}
        }
    }

    /// Run editor / panel code so a panic in it shows the crash screen:
    /// the half-done edit is rolled back and the app keeps running.
    fn guarded(&mut self, f: impl FnOnce(&mut Self)) {
        if let Err(caught) = crash::catch(crash::Stage::Editor, || f(self)) {
            self.view.recover(&mut self.patch);
            self.crash.report(caught, &self.patch);
        }
    }

    fn canvas(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let cat = Catalogue::builtin();
        self.view.thumbs = self.preview.probe_thumbs(
            ui.ctx(),
            frame.wgpu_render_state(),
            self.engine.as_ref(),
            &self.outputs,
            &self.patch,
        );
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
        // Structural edits (wires, nodes, undo) rebuild the engine.
        if self.view.take_recompile() {
            self.recompile();
        }
        if self.patch.nodes.is_empty() && !self.welcome.open {
            empty_hint(ui);
        }
    }

    // ── Dialogs ─────────────────────────────────────────────────────────

    /// "Save changes before …?" Returns what to do now.
    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(after) = self.confirm.clone() else {
            return;
        };
        let doing = match &after {
            After::Quit => "quitting",
            After::New => "starting a new patch",
            After::Open(_) => "opening another file",
            After::Sample(_) => "opening the sample",
            After::Patch(..) => "opening the patch",
            After::Recover(_) => "restoring",
        };
        enum Choice {
            Save,
            Discard,
            Cancel,
        }
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::new("floret-unsaved")).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.label(
                RichText::new(format!("Save changes to “{}”?", self.doc.name))
                    .strong()
                    .size(15.0),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "Your changes will be lost if you don't save them before {doing}."
                ))
                .color(theme::FG_1),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Don't Save").clicked() {
                    choice = Some(Choice::Discard);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let save = egui::Button::new(RichText::new("Save…").strong());
                    if ui.add(save).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        choice = Some(Choice::Save);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(Choice::Cancel);
                    }
                });
            });
        });
        if modal.should_close() && choice.is_none() {
            choice = Some(Choice::Cancel);
        }
        match choice {
            Some(Choice::Save) => {
                self.confirm = None;
                if self.save() {
                    self.perform(ctx, after);
                }
            }
            Some(Choice::Discard) => {
                self.confirm = None;
                self.perform(ctx, after);
            }
            Some(Choice::Cancel) => self.confirm = None,
            None => {}
        }
    }

    /// Offer autosaves / crash copies from sessions that didn't exit cleanly.
    fn recovery_dialog(&mut self, ctx: &egui::Context) {
        if self.recovery.is_empty() || self.confirm.is_some() {
            return;
        }
        let mut restore = None;
        let mut discard = None;
        let mut later = false;
        egui::Modal::new(egui::Id::new("floret-recovery")).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.label(RichText::new("Restore unsaved work?").strong().size(15.0));
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Floret didn't close normally last time. These changes weren't saved:",
                )
                .color(theme::FG_1),
            );
            ui.add_space(8.0);
            for c in &self.recovery {
                egui::Frame::new()
                    .fill(*theme::BG_2)
                    .corner_radius(6)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&c.meta.name).strong());
                                let from = c
                                    .meta
                                    .source
                                    .as_ref()
                                    .map_or("never saved".to_owned(), |p| p.display().to_string());
                                let kind = match c.meta.kind {
                                    recovery::Kind::Autosave => "autosaved",
                                    recovery::Kind::Crash => "saved after an error",
                                };
                                ui.label(
                                    RichText::new(format!("{kind} {} · {from}", c.age()))
                                        .color(theme::FG_2)
                                        .small(),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button(RichText::new("Restore").strong()).clicked() {
                                    restore = Some(c.clone());
                                }
                                if ui.button("Discard").clicked() {
                                    discard = Some(c.clone());
                                }
                            });
                        });
                    });
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Kept in {}", recovery::dir().display()))
                        .color(theme::FG_2)
                        .small(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Not now").clicked() {
                        later = true;
                    }
                });
            });
        });
        if let Some(c) = discard {
            c.discard();
            self.recovery.retain(|x| *x != c);
        }
        if let Some(c) = restore {
            self.request(ctx, After::Recover(c));
        }
        if later {
            self.recovery.clear();
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        for c in self.inject.drain(..) {
            if let Some(ev) = menu::event_for(&c) {
                raw.events.push(ev);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        theme::follow(&ctx);
        let prefs_before = self.prefs.clone();

        // Commands: native menu, app shortcuts, last frame's menu clicks.
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if let Some(m) = &mut self.native_menu {
            self.commands.extend(m.poll());
        }
        if self.confirm.is_none() {
            self.commands.extend(menu::shortcuts(&ctx));
        }
        for c in std::mem::take(&mut self.commands) {
            self.run(&ctx, c);
        }
        // `?` toggles the shortcuts overlay; Esc closes it before the
        // canvas sees the key.
        if self.confirm.is_none() && help::question_typed(&ctx) {
            self.shortcuts_overlay ^= true;
        }
        if self.shortcuts_overlay
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.shortcuts_overlay = false;
        }

        // Closing the window asks about unsaved changes first.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(After::Quit);
        }

        self.tick(&ctx);
        ctx.request_repaint();
        if self
            .monitors_read
            .is_none_or(|t| t.elapsed() > Duration::from_secs(2))
        {
            self.preview.monitors = preview::monitors(frame);
            self.monitors_read = Some(Instant::now());
        }
        let rev = self.rev();
        self.autosave.tick(
            Instant::now(),
            self.prefs.autosave_interval(),
            &self.doc,
            rev,
            &self.patch,
        );
        let title = self.doc.title(rev);
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        if self.preview.focused() {
            // Focus mode: the featured preview alone.
            let rs = frame.wgpu_render_state();
            let (preview, engine, outputs) =
                (&mut self.preview, self.engine.as_ref(), &self.outputs);
            egui::CentralPanel::no_frame().show(ui, |ui| {
                self.crash.run(crash::Stage::Render, &self.patch, || {
                    preview.show_focus(ui, rs, engine, outputs);
                });
            });
            return;
        }

        let mut warnings: Vec<(String, Vec<String>)> = self
            .warnings
            .iter()
            .map(|w| (w.message.clone(), w.node_ids.clone()))
            .collect();
        warnings.extend(
            self.shader_panel
                .warnings(&self.patch, self.preview.shader_errors())
                .into_iter()
                .map(|w| (w, Vec::new())),
        );

        let bar = |fill| {
            egui::Frame::new()
                .fill(fill)
                .inner_margin(egui::Margin::symmetric(10, 0))
        };
        egui::Panel::top("toolbar")
            .exact_size(38.0)
            .frame(bar(theme::BG_1.get()))
            .show(ui, |ui| self.guarded(|app| app.toolbar(ui)));
        egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(bar(theme::BG_1.get()))
            .show(ui, |ui| self.guarded(|app| app.status_bar(ui, &warnings)));

        if self.prefs.palette_visible {
            egui::Panel::left("palette")
                .resizable(true)
                .default_size(220.0)
                .size_range(160.0..=360.0)
                .frame(
                    egui::Frame::new()
                        .fill(*theme::BG_1)
                        .inner_margin(egui::Margin::symmetric(10, 4)),
                )
                .show(ui, |ui| {
                    self.guarded(|app| {
                        palette::show(ui, &mut app.query, &mut app.view, &mut app.patch);
                    });
                });
        }

        egui::Panel::right("side")
            .resizable(true)
            .default_size(400.0)
            .size_range(260.0..=900.0)
            .frame(egui::Frame::new().fill(*theme::BG_1))
            .show(ui, |ui| self.guarded(|app| app.side_panel(ui, frame)));

        let canvas_rect = egui::CentralPanel::no_frame()
            .show(ui, |ui| self.guarded(|app| app.canvas(ui, frame)))
            .response
            .rect;
        help::overlay(&ctx, canvas_rect, &mut self.shortcuts_overlay);
        // The toolbar's Snap / Flow toggles act on the view.
        self.prefs.snap_to_grid = self.view.snap;
        self.prefs.flow_animation = self.view.wire_flow;

        match self.crash.show(&ctx, &self.patch) {
            Some(crash::Action::Reload) => {
                self.recompile();
                self.restart();
            }
            Some(crash::Action::ClearOutputs) => self.outputs.clear(),
            None => {}
        }

        // ── Windows and dialogs.
        let rs = frame.wgpu_render_state();
        let mut show_welcome = self.prefs.show_welcome;
        if let Some(a) = self
            .welcome
            .show(&ctx, rs, self.recent.paths(), &mut show_welcome)
        {
            match a {
                welcome::Action::New => self.request(&ctx, After::New),
                welcome::Action::Open => self.request(&ctx, After::Open(None)),
                welcome::Action::OpenRecent(p) => self.request(&ctx, After::Open(Some(p))),
                welcome::Action::Sample(i) => self.request(&ctx, After::Sample(i)),
                welcome::Action::Library => self.workspace.show_library(),
            }
        }
        self.prefs.show_welcome = show_welcome;
        self.prefs_window.show(&ctx, &mut self.prefs);
        help::shortcuts_window(&ctx, &mut self.show_shortcuts);
        help::about_window(&ctx, &mut self.show_about, &self.gpu_name);
        self.recorder.show(
            &ctx,
            rs,
            &mut self.patch,
            &mut self.view,
            &self.preview.renderers(),
            &self.doc.name,
        );
        self.recovery_dialog(&ctx);
        self.confirm_dialog(&ctx);

        self.prefs_changed(&ctx, &prefs_before);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if let Some(m) = &mut self.native_menu {
            m.sync(&MenuState {
                recent: self.recent.paths(),
                palette_visible: self.prefs.palette_visible,
                theme: self.prefs.theme,
            });
        }
    }

    fn on_exit(&mut self) {
        self.autosave.clean_exit();
    }
}

fn apply_editor_prefs(view: &mut canvas::View, p: &Prefs) {
    view.snap = p.snap_to_grid;
    view.gravity = p.node_gravity;
    view.wire_flow = p.flow_animation;
    view.grid = f64::from(p.grid_size);
    view.show_grid = p.show_grid;
    view.show_minimap = p.show_minimap;
    view.ambient_motion = p.ambient_motion;
    view.reduce_motion = p.reduce_motion;
    editor::ops::set_default_render_size(p.default_render_width, p.default_render_height);
}

/// Toolbar buttons share one size so the clusters line up.
const TOOL_SIZE: egui::Vec2 = egui::vec2(26.0, 24.0);

/// A Phosphor icon at the toolbar's size.
fn icon(glyph: &str, color: egui::Color32) -> RichText {
    theme::icon(glyph, 16.0, color)
}

/// A toolbar icon button (framed while hovered or pressed).
fn tool(ui: &mut egui::Ui, glyph: &str, tip: &str) -> egui::Response {
    let b = egui::Button::new(icon(glyph, *theme::FG_1))
        .min_size(TOOL_SIZE)
        .frame_when_inactive(false);
    ui.add(b).on_hover_text(tip)
}

/// A toolbar on/off icon button: a gold icon on the selected fill while
/// on.
fn toggle(ui: &mut egui::Ui, on: &mut bool, glyph: &str, tip: &str) -> egui::Response {
    let color = if *on { theme::GOLD_TEXT } else { theme::FG_1 };
    let b = egui::Button::new(icon(glyph, *color))
        .selected(*on)
        .min_size(TOOL_SIZE)
        .frame_when_inactive(*on);
    let r = ui.add(b).on_hover_text(tip);
    if r.clicked() {
        *on = !*on;
    }
    r
}

/// The divider between toolbar clusters: a short hairline.
fn cluster_gap(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 16.0), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x.round() + 0.5,
        rect.y_range(),
        egui::Stroke::new(1.0, theme::BORDER),
    );
}

/// A small status dot.
fn dot(ui: &mut egui::Ui, color: egui::Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
    resp
}

fn shortcut_hint(c: &Command) -> String {
    match c.shortcut() {
        Some(sc) => format!("{} ({})", c.label(), menu::shortcut_text(&sc)),
        None => c.label().to_owned(),
    }
}

/// Centred hint on an empty canvas.
fn empty_hint(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let p = ui.painter();
    p.text(
        rect.center() - egui::vec2(0.0, 12.0),
        egui::Align2::CENTER_CENTER,
        "Empty patch",
        egui::FontId::proportional(16.0),
        theme::FG_2.get(),
    );
    p.text(
        rect.center() + egui::vec2(0.0, 12.0),
        egui::Align2::CENTER_CENTER,
        "Double-click or press Tab to add a node · ? for shortcuts · Library for samples",
        egui::FontId::proportional(12.0),
        theme::FG_2.get(),
    );
}

pub fn empty_patch() -> PatchFile {
    PatchFile {
        kind: floret_engine::patch::PATCH_KIND.into(),
        version: floret_engine::patch::CURRENT_PATCH_VERSION,
        app_version: None,
        meta: None,
        nodes: Vec::new(),
        edges: Vec::new(),
        loop_length: None,
        seed: 0,
        presets: Vec::new(),
    }
}
