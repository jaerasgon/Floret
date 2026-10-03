//! Recording loops (#27): MP4 (H.264), GIF and PNG-sequence export. MP4s
//! go through Apple's encoder on macOS and OpenH264 elsewhere
//! (`floret_video::export`).
//!
//! The patch is re-run offline from a fresh start, frame by frame at a
//! fixed dt, so a recording is smooth and comes out the same however fast
//! the machine is. Each frame is rendered on the app's GPU at the chosen
//! size and encoded on a worker thread; the export window shows progress
//! and can cancel, and the app keeps running meanwhile.
//!
//! Recordings are whole loops of the patch's loop length (a patch setting,
//! edited in the export window or the toolbar's loop field, see
//! [`loop_length_field`]; the Time node's `loop` output runs 0..1
//! over it). A loop is `N = round(loop × fps)` frames at `dt = loop / N`,
//! so frame N would be frame 0 again: played on repeat, a one-loop
//! recording joins seamlessly.

use crate::export;
use crate::theme;
use eframe::egui::{self, RichText};
use eframe::egui_wgpu::{self, wgpu};
use floret_engine::graph::NodeIdx;
use floret_engine::patch::PatchFile;
use floret_engine::scene::SceneNode;
use floret_engine::{Engine, FrameCtx};
use floret_render::{RenderClock, Renderer};
use floret_video::export::{H264Settings, Mp4File};
use floret_video::gif_writer::{self, GifWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Mp4,
    Gif,
    PngSequence,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Mp4, Format::Gif, Format::PngSequence];

    pub fn label(self) -> &'static str {
        match self {
            Format::Mp4 => "MP4",
            Format::Gif => "GIF",
            Format::PngSequence => "PNG sequence",
        }
    }

    /// The entry in a preview's Export menu.
    pub fn menu_label(self) -> &'static str {
        match self {
            Format::Mp4 => "MP4 video (H.264)…",
            Format::Gif => "GIF…",
            Format::PngSequence => "PNG sequence…",
        }
    }

    /// Frame rates offered (GIFs don't play faster than 50 fps).
    pub fn rates(self) -> &'static [u32] {
        match self {
            Format::Gif => &[10, 12, 15, 20, 25, 30, 50],
            _ => &[24, 25, 30, 50, 60],
        }
    }
}

/// The toolbar's loop-length field: the patch's loop length in seconds,
/// shown as "off" while the patch doesn't set one (the Time node's `loop`
/// output and exports then use the default). Typing a blank, "off" or 0
/// clears the setting. Edits are undo steps, like the Export window's.
pub fn loop_length_field(
    ui: &mut egui::Ui,
    patch: &mut PatchFile,
    view: &mut crate::canvas::View,
    size: egui::Vec2,
) -> egui::Response {
    let mut secs = loop_field_value(patch);
    let drag = egui::DragValue::new(&mut secs)
        .range(0.0..=600.0)
        .speed(0.05)
        .custom_formatter(|v, _| loop_field_text(v))
        .custom_parser(parse_loop_field);
    let tip = format!(
        "Loop length: the Time node's loop output runs 0 → 1 over it, and video \
         exports record whole loops. Saved with the patch. Off: not set ({} s \
         default). Drag, or click to type; 0 or blank turns it off.",
        floret_engine::patch::DEFAULT_LOOP_LENGTH
    );
    // An explicit id: the toolbar gains a widget (the unsaved-changes dot)
    // on the first edit, which would otherwise change the field's id and end
    // the typing after one key.
    let fixed = egui::UiBuilder::new().id(egui::Id::new("floret-loop-length"));
    let r = ui
        .scope_builder(fixed, |ui| ui.add_sized(size, drag))
        .inner
        .on_hover_text(tip);
    if r.changed() && set_loop_length(patch, secs) {
        view.commit_setting(patch, "patch.loopLength");
    }
    r
}

/// The loop-length field's value: the patch's own loop length, 0 when unset.
pub fn loop_field_value(patch: &PatchFile) -> f64 {
    patch
        .loop_length
        .filter(|l| l.is_finite() && *l > 0.0)
        .unwrap_or(0.0)
}

/// "off" for 0, else the seconds ("2.5 s").
pub fn loop_field_text(secs: f64) -> String {
    if secs > 0.0 {
        let s = format!("{secs:.2}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        format!("{s} s")
    } else {
        "off".to_owned()
    }
}

/// Typed text → seconds: blank or "off" is 0, a trailing "s" is allowed.
pub fn parse_loop_field(text: &str) -> Option<f64> {
    let t = text.trim();
    let t = t.strip_suffix('s').unwrap_or(t).trim();
    if t.is_empty() || t.eq_ignore_ascii_case("off") {
        return Some(0.0);
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Set the patch's loop length from the field (0 or less clears it). True
/// when that changed the patch.
pub fn set_loop_length(patch: &mut PatchFile, secs: f64) -> bool {
    let next = (secs.is_finite() && secs > 0.0).then_some(secs);
    if next == patch.loop_length {
        return false;
    }
    patch.loop_length = next;
    true
}

/// How many frames a recording has, and the fixed time step between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub frames: usize,
    pub dt: f64,
    /// Frames per loop.
    pub per_loop: usize,
}

/// `loops` loops of `loop_s` seconds at `fps`: a whole number of frames
/// per loop, with dt stretched a hair if `loop_s × fps` isn't whole, so the
/// loop still closes exactly.
pub fn plan(loop_s: f64, loops: u32, fps: u32) -> Plan {
    let per_loop = (loop_s * f64::from(fps)).round().max(1.0) as usize;
    Plan {
        frames: per_loop * loops.max(1) as usize,
        dt: loop_s / per_loop as f64,
        per_loop,
    }
}

/// Frame `i` of an offline run: `t = i · dt` from a fresh start, with no
/// mouse or keys. The engine fills in its patch's loop length.
pub fn offline_frame(i: usize, dt: f64, canvas: (u32, u32)) -> FrameCtx {
    FrameCtx {
        t: i as f64 * dt,
        dt,
        frame_count: i as f64,
        width: f64::from(canvas.0),
        height: f64::from(canvas.1),
        ..FrameCtx::default()
    }
}

/// The recording's size: the canvas × `scale`, rounded (to even sizes for
/// H.264).
pub fn out_size(canvas: (u32, u32), scale: f64, format: Format) -> (u32, u32) {
    let dim = |c: u32| (f64::from(c) * scale).round().max(1.0) as u32;
    let (w, h) = (dim(canvas.0), dim(canvas.1));
    match format {
        Format::Mp4 => (w.max(2) & !1, h.max(2) & !1),
        _ => (w, h),
    }
}

/// Why this recording can't be made, if it can't.
pub fn problem(size: (u32, u32), fps: u32, format: Format, max_texture: u32) -> Option<String> {
    if size.0 > max_texture || size.1 > max_texture {
        return Some(format!(
            "Larger than this GPU's {max_texture} px texture limit"
        ));
    }
    match format {
        Format::Mp4 => H264Settings::export(size.0, size.1, fps).check().err(),
        Format::Gif if size.0 > 65535 || size.1 > 65535 => Some("Too large for a GIF".into()),
        Format::Gif if fps > gif_writer::MAX_FPS => {
            Some(format!("GIFs play at up to {} fps", gif_writer::MAX_FPS))
        }
        _ => None,
    }
}

// ─── Offline run ────────────────────────────────────────────────────────────

/// A fresh engine for the patch, run frame by frame for one renderer.
pub struct Offline {
    engine: Engine,
    renderer: NodeIdx,
    /// The recorded renderer's canvas size.
    pub size: (u32, u32),
}

fn renderer_size(engine: &Engine, r: NodeIdx) -> (u32, u32) {
    let id = &engine.graph.nodes[r].id;
    floret_render::canvas_size(
        engine.params.get(id, "width"),
        engine.params.get(id, "height"),
    )
}

impl Offline {
    /// Compile `patch` from scratch; record renderer `renderer` (else the
    /// first).
    pub fn new(patch: &PatchFile, renderer: &str) -> Result<Offline, String> {
        let (mut engine, _) = Engine::from_patch(patch)?;
        engine.set_host(Some(crate::host::shared()));
        let r = (engine.graph.idx(renderer))
            .filter(|r| engine.graph.renderer_ids.contains(r))
            .or_else(|| engine.graph.renderer())
            .ok_or("The patch has no Renderer to record")?;
        let size = renderer_size(&engine, r);
        Ok(Offline {
            engine,
            renderer: r,
            size,
        })
    }

    /// Tick frame `i` and return the recorded renderer's scene and
    /// background. Like the live preview, the patch sees the first
    /// renderer's canvas size.
    pub fn frame(&mut self, i: usize, dt: f64) -> (Vec<Arc<SceneNode>>, [f64; 4]) {
        let canvas =
            (self.engine.graph.renderer()).map_or(self.size, |r| renderer_size(&self.engine, r));
        let result = self.engine.tick(&offline_frame(i, dt, canvas));
        let out = result.outputs.get(self.renderer).cloned().flatten();
        let layers = (out.as_ref().and_then(|o| o.first()))
            .map(|s| floret_render::flatten_layers(s))
            .unwrap_or_default();
        let bg = (out.as_ref().and_then(|o| o.get(1)))
            .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
        (layers, bg)
    }
}

// ─── Output ─────────────────────────────────────────────────────────────────

/// `<stem>-0001.png`: frames numbered from 1, at least 4 digits.
pub fn frame_file(stem: &str, i: usize, frames: usize) -> String {
    let digits = frames.to_string().len().max(4);
    format!("{stem}-{:0digits$}.png", i + 1)
}

enum Sink {
    Mp4(Mp4File, PathBuf),
    Gif(GifWriter<BufWriter<File>>, PathBuf),
    Png {
        dir: PathBuf,
        stem: String,
        size: (u32, u32),
        frames: usize,
        written: Vec<PathBuf>,
    },
}

impl Sink {
    fn create(spec: &Spec, size: (u32, u32)) -> Result<Sink, String> {
        let create = |p: &Path| {
            File::create(p)
                .map(BufWriter::new)
                .map_err(|e| format!("{}: {e}", p.display()))
        };
        let path = spec.path.clone();
        Ok(match spec.format {
            Format::Mp4 => {
                let settings = H264Settings::export(size.0, size.1, spec.fps);
                Sink::Mp4(Mp4File::create(&path, settings)?, path)
            }
            Format::Gif => {
                let gif = GifWriter::new(create(&path)?, size.0, size.1, spec.fps)?;
                Sink::Gif(gif, path)
            }
            Format::PngSequence => Sink::Png {
                dir: path.parent().map(Path::to_owned).unwrap_or_default(),
                stem: path
                    .file_name()
                    .map_or_else(|| "floret".to_owned(), |n| n.to_string_lossy().into_owned()),
                size,
                frames: spec.plan.frames,
                written: Vec::new(),
            },
        })
    }

    fn push(&mut self, mut rgba: Vec<u8>) -> Result<(), String> {
        match self {
            Sink::Mp4(enc, _) => enc.push_rgba(&rgba),
            Sink::Gif(gif, _) => gif.push_rgba(&mut rgba),
            Sink::Png {
                dir,
                stem,
                size,
                frames,
                written,
            } => {
                let p = dir.join(frame_file(stem, written.len(), *frames));
                export::write_png(&p, size.0, size.1, &rgba)?;
                written.push(p);
                Ok(())
            }
        }
    }

    /// Finish the file(s): the name to report.
    fn finish(self) -> Result<String, String> {
        let name = |p: &Path| {
            p.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        };
        match self {
            Sink::Mp4(enc, p) => enc.finish().map(|()| name(&p)),
            Sink::Gif(gif, p) => gif.finish().map(|_| name(&p)),
            Sink::Png { written, .. } => Ok(match (written.first(), written.last()) {
                (Some(a), Some(b)) if written.len() > 1 => format!("{} … {}", name(a), name(b)),
                (Some(a), _) => name(a),
                _ => String::new(),
            }),
        }
    }

    /// Cancelled or failed: remove what was written.
    fn abort(self) {
        match self {
            Sink::Mp4(enc, p) => {
                drop(enc);
                let _ = std::fs::remove_file(p);
            }
            Sink::Gif(gif, p) => {
                drop(gif);
                let _ = std::fs::remove_file(p);
            }
            Sink::Png { written, .. } => {
                for p in written {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
    }
}

/// The app's GPU, shared with the worker thread.
#[derive(Clone)]
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: wgpu::Adapter,
}

/// One recording.
pub struct Spec {
    pub patch: PatchFile,
    /// Renderer node id.
    pub renderer: String,
    pub format: Format,
    /// The MP4 / GIF file, or for a PNG sequence the folder joined with
    /// the frames' file-name stem.
    pub path: PathBuf,
    pub scale: f64,
    pub fps: u32,
    pub plan: Plan,
}

const CANCELLED: &str = "Export cancelled";

/// Make the recording, counting finished frames in `progress` and
/// stopping when `cancel` is set. Ok: what was saved.
pub fn record(
    spec: &Spec,
    gpu: &Gpu,
    progress: &AtomicUsize,
    cancel: &AtomicBool,
) -> Result<String, String> {
    // Off the UI thread: video nodes wait for their frames, as offline
    // rendering needs.
    let _wait = floret_engine::video::wait_for_frames();
    let mut offline = Offline::new(&spec.patch, &spec.renderer)?;
    let canvas = offline.size;
    let size = out_size(canvas, spec.scale, spec.format);
    let max = gpu.device.limits().max_texture_dimension_2d;
    if let Some(p) = problem(size, spec.fps, spec.format, max) {
        return Err(p);
    }
    let mut sink = Sink::create(spec, size)?;
    let mut run = || {
        let mut renderer = Renderer::new(
            &gpu.device,
            &gpu.queue,
            floret_render::OFFSCREEN_FORMAT,
            export::samples(&gpu.adapter, size),
        );
        let sx = f64::from(size.0) / f64::from(canvas.0);
        let sy = f64::from(size.1) / f64::from(canvas.1);
        let dt = spec.plan.dt;
        for i in 0..spec.plan.frames {
            if cancel.load(Ordering::Relaxed) {
                return Err(CANCELLED.to_owned());
            }
            let (layers, bg) = offline.frame(i, dt);
            renderer.set_clock(RenderClock {
                time: i as f64 * dt,
                time_delta: dt,
                frame: i as f64,
                ..RenderClock::default()
            });
            let oom = gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
            let validation = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
            let (pixels, _) = floret_render::render_and_read(
                &gpu.device,
                &gpu.queue,
                &mut renderer,
                &export::scaled(&layers, sx, sy),
                size,
                bg,
            );
            if let Some(e) = pollster::block_on(validation.pop()) {
                return Err(e.to_string());
            }
            if let Some(e) = pollster::block_on(oom.pop()) {
                return Err(e.to_string());
            }
            sink.push(pixels)?;
            progress.store(i + 1, Ordering::Relaxed);
        }
        Ok(())
    };
    match run() {
        Ok(()) => {
            let name = sink.finish()?;
            Ok(format!(
                "Saved {name} ({} × {}, {} frames)",
                size.0, size.1, spec.plan.frames
            ))
        }
        Err(e) => {
            sink.abort();
            Err(e)
        }
    }
}

// ─── Window ─────────────────────────────────────────────────────────────────

struct Job {
    progress: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    frames: usize,
    started: Instant,
    done: mpsc::Receiver<Result<String, String>>,
}

/// The Export Video window and the recording in progress.
pub struct Recorder {
    open: bool,
    /// Renderer node id (`None`: the first).
    renderer: Option<String>,
    format: Format,
    scale: f64,
    fps: u32,
    loops: u32,
    job: Option<Job>,
    /// How the last recording went.
    status: Option<Result<String, String>>,
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder {
            open: false,
            renderer: None,
            format: Format::Mp4,
            scale: 1.0,
            fps: 30,
            loops: 1,
            job: None,
            status: None,
        }
    }
}

/// A renderer the window can record: id, title, canvas size, and the name
/// files are called after.
struct Target {
    id: String,
    title: String,
    size: (u32, u32),
    name: String,
}

fn targets(patch: &PatchFile, renderers: &[(String, String)], doc_name: &str) -> Vec<Target> {
    renderers
        .iter()
        .filter_map(|(id, title)| {
            let n = patch.nodes.iter().find(|n| n.id == *id)?;
            let label = (n.params.get("label").and_then(|l| l.as_str()))
                .map(str::trim)
                .filter(|l| !l.is_empty());
            Some(Target {
                id: id.clone(),
                title: title.clone(),
                size: floret_render::canvas_size(n.params.get("width"), n.params.get("height")),
                name: label.unwrap_or(doc_name).to_owned(),
            })
        })
        .collect()
}

impl Recorder {
    /// Show the window for `renderer` (else the one picked last), with
    /// `format` selected (else the last).
    pub fn open(&mut self, renderer: Option<String>, format: Option<Format>) {
        self.open = true;
        if renderer.is_some() {
            self.renderer = renderer;
        }
        if let Some(f) = format {
            self.set_format(f);
        }
        if self.job.is_none() {
            self.status = None;
        }
    }

    fn set_format(&mut self, f: Format) {
        self.format = f;
        if !f.rates().contains(&self.fps) {
            self.fps = 30;
        }
    }

    fn poll(&mut self) {
        let Some(job) = &self.job else {
            return;
        };
        match job.done.try_recv() {
            Ok(r) => {
                self.status = Some(r);
                self.job = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.status = Some(Err("Export failed".into()));
                self.job = None;
            }
        }
    }

    /// The window. Editing the loop length here edits the patch (an undo
    /// step on `view`).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        rs: Option<&egui_wgpu::RenderState>,
        patch: &mut PatchFile,
        view: &mut crate::canvas::View,
        renderers: &[(String, String)],
        doc_name: &str,
    ) {
        self.poll();
        if !self.open {
            return;
        }
        let targets = targets(patch, renderers, doc_name);
        let mut open = true;
        let running = self.job.is_some();
        let mut window = egui::Window::new("Export Video")
            .id(egui::Id::new("floret-record"))
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center());
        // While recording, Cancel is the way out.
        if !running {
            window = window.open(&mut open);
        }
        window.show(ctx, |ui| {
            ui.add_enabled_ui(!running, |ui| self.settings(ui, rs, patch, view, &targets));
            ui.separator();
            self.footer(ui, rs, patch, &targets);
        });
        if !open {
            self.open = false;
        }
    }

    fn target<'a>(&self, targets: &'a [Target]) -> Option<&'a Target> {
        (self.renderer.as_ref())
            .and_then(|id| targets.iter().find(|t| t.id == *id))
            .or_else(|| targets.first())
    }

    fn settings(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        patch: &mut PatchFile,
        view: &mut crate::canvas::View,
        targets: &[Target],
    ) {
        let canvas = self
            .target(targets)
            .map_or(floret_render::DEFAULT_CANVAS, |t| t.size);
        egui::Grid::new("floret-record-grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Format");
                ui.horizontal(|ui| {
                    for f in Format::ALL {
                        if ui.selectable_label(self.format == f, f.label()).clicked() {
                            self.set_format(f);
                        }
                    }
                });
                ui.end_row();

                if targets.len() > 1 {
                    ui.label("Preview");
                    let current = self.target(targets).map_or("", |t| &t.title).to_owned();
                    egui::ComboBox::from_id_salt("floret-record-renderer")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for t in targets {
                                let on = self.target(targets).is_some_and(|c| c.id == t.id);
                                if ui.selectable_label(on, &t.title).clicked() {
                                    self.renderer = Some(t.id.clone());
                                }
                            }
                        });
                    ui.end_row();
                }

                ui.label("Size");
                ui.horizontal(|ui| {
                    for (s, l) in [(0.5, "½×"), (1.0, "1×"), (2.0, "2×")] {
                        if ui.selectable_label(self.scale == s, l).clicked() {
                            self.scale = s;
                        }
                    }
                    let mut width = (f64::from(canvas.0) * self.scale).round();
                    let max = rs.map_or(8192, |rs| rs.device.limits().max_texture_dimension_2d);
                    let drag = egui::DragValue::new(&mut width)
                        .range(16.0..=f64::from(max))
                        .speed(4.0)
                        .suffix(" px wide");
                    if ui.add(drag).changed() {
                        self.scale = width / f64::from(canvas.0.max(1));
                    }
                });
                ui.end_row();

                ui.label("Frame rate");
                egui::ComboBox::from_id_salt("floret-record-fps")
                    .selected_text(format!("{} fps", self.fps))
                    .show_ui(ui, |ui| {
                        for &r in self.format.rates() {
                            ui.selectable_value(&mut self.fps, r, format!("{r} fps"));
                        }
                    });
                ui.end_row();

                ui.label("Loop length")
                    .on_hover_text("A patch setting, saved with it. The Time node's loop output runs 0 → 1 over it.");
                let mut secs = patch.loop_seconds();
                let drag = egui::DragValue::new(&mut secs)
                    .range(0.1..=600.0)
                    .speed(0.05)
                    .max_decimals(2)
                    .suffix(" s");
                if ui.add(drag).changed() {
                    patch.loop_length = Some(secs);
                    view.commit_setting(patch, "patch.loopLength");
                }
                ui.end_row();

                ui.label("Length");
                ui.horizontal(|ui| {
                    let suffix = if self.loops == 1 { " loop" } else { " loops" };
                    ui.add(
                        egui::DragValue::new(&mut self.loops)
                            .range(1..=100)
                            .speed(0.05)
                            .suffix(suffix),
                    );
                    let p = plan(secs, self.loops, self.fps);
                    let total = p.frames as f64 * p.dt;
                    ui.label(
                        RichText::new(format!("{total:.2} s · {} frames", p.frames))
                            .color(theme::FG_2),
                    );
                });
                ui.end_row();
            });
    }

    fn footer(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        patch: &PatchFile,
        targets: &[Target],
    ) {
        if let Some(job) = &self.job {
            let done = job.progress.load(Ordering::Relaxed);
            let frac = done as f32 / job.frames.max(1) as f32;
            let mut text = format!("Frame {done} of {}", job.frames);
            if done > 0 {
                let per = job.started.elapsed().as_secs_f64() / done as f64;
                let left = per * (job.frames - done) as f64;
                text.push_str(&format!(" · {left:.0} s left"));
            }
            ui.add(egui::ProgressBar::new(frac).text(text));
            ui.horizontal(|ui| {
                let cancelling = job.cancel.load(Ordering::Relaxed);
                if ui
                    .add_enabled(!cancelling, egui::Button::new("Cancel"))
                    .clicked()
                {
                    job.cancel.store(true, Ordering::Relaxed);
                }
                if cancelling {
                    ui.label(RichText::new("Stopping…").color(theme::FG_2));
                }
            });
            return;
        }
        let target = self.target(targets);
        let size = target.map(|t| out_size(t.size, self.scale, self.format));
        let max = rs.map_or(0, |rs| rs.device.limits().max_texture_dimension_2d);
        let blocked = match (target, size, rs) {
            (None, _, _) | (_, None, _) => Some("The patch has no Renderer to record".to_owned()),
            (_, _, None) => Some("No GPU renderer".to_owned()),
            (Some(_), Some(size), Some(_)) => problem(size, self.fps, self.format, max),
        };
        ui.horizontal(|ui| {
            if let Some(size) = size {
                ui.label(
                    RichText::new(format!("{} × {}", size.0, size.1))
                        .monospace()
                        .color(theme::FG_1),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let go = ui.add_enabled(
                    blocked.is_none(),
                    egui::Button::new(RichText::new("Export…").strong()),
                );
                if go.clicked() {
                    if let (Some(rs), Some(t)) = (rs, target) {
                        let (id, name) = (t.id.clone(), t.name.clone());
                        self.start(rs, patch, id, &name);
                    }
                }
            });
        });
        if let Some(p) = blocked {
            ui.label(RichText::new(p).color(theme::DANGER).small());
        }
        match &self.status {
            Some(Ok(m)) => {
                ui.label(
                    RichText::new(format!("✓ {m}"))
                        .color(theme::GOLD_TEXT)
                        .small(),
                );
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(theme::DANGER).small());
            }
            None => {}
        }
    }

    /// Ask where, then record on a worker thread.
    fn start(
        &mut self,
        rs: &egui_wgpu::RenderState,
        patch: &PatchFile,
        renderer: String,
        name: &str,
    ) {
        let slug = export::slug(name);
        let path = match self.format {
            Format::Mp4 => rfd::FileDialog::new()
                .set_title("Export MP4 video")
                .add_filter("MP4 video", &["mp4"])
                .set_file_name(format!("{slug}.mp4"))
                .save_file(),
            Format::Gif => rfd::FileDialog::new()
                .set_title("Export GIF")
                .add_filter("GIF image", &["gif"])
                .set_file_name(format!("{slug}.gif"))
                .save_file(),
            Format::PngSequence => rfd::FileDialog::new()
                .set_title("Choose a folder for the PNG frames")
                .pick_folder()
                .map(|d| d.join(&slug)),
        };
        let Some(path) = path else {
            return;
        };
        let plan = plan(patch.loop_seconds(), self.loops, self.fps);
        let spec = Spec {
            patch: patch.clone(),
            renderer,
            format: self.format,
            path,
            scale: self.scale,
            fps: self.fps,
            plan,
        };
        let gpu = Gpu {
            device: rs.device.clone(),
            queue: rs.queue.clone(),
            adapter: rs.adapter.clone(),
        };
        let progress = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (p, c) = (progress.clone(), cancel.clone());
        let spawned = std::thread::Builder::new()
            .name("floret-export".into())
            .spawn(move || {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    record(&spec, &gpu, &p, &c)
                }))
                .unwrap_or_else(|_| Err("Export failed: the patch crashed while recording".into()));
                let _ = tx.send(r);
            });
        if let Err(e) = spawned {
            self.status = Some(Err(format!("Export failed: {e}")));
            return;
        }
        self.status = None;
        self.job = Some(Job {
            progress,
            cancel,
            frames: plan.frames,
            started: Instant::now(),
            done: rx,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../samples/loop-bloom.flora");

    #[test]
    fn the_loop_field_reads_and_writes_the_patch_setting() {
        let mut patch = floret_engine::load_patch(SAMPLE).unwrap();
        assert_eq!(loop_field_value(&patch), 5.0);
        assert_eq!(loop_field_text(5.0), "5 s");
        assert_eq!(loop_field_text(2.5), "2.5 s");
        assert_eq!(loop_field_text(0.0), "off");
        // Same value: no edit.
        assert!(!set_loop_length(&mut patch, 5.0));
        assert!(set_loop_length(&mut patch, 2.5));
        assert_eq!(patch.loop_length, Some(2.5));
        // 0 turns it off: unset, so the default applies again.
        assert!(set_loop_length(&mut patch, 0.0));
        assert_eq!(patch.loop_length, None);
        assert_eq!(loop_field_value(&patch), 0.0);
        assert_eq!(
            patch.loop_seconds(),
            floret_engine::patch::DEFAULT_LOOP_LENGTH
        );
        assert!(!set_loop_length(&mut patch, -1.0));
    }

    #[test]
    fn the_loop_field_parses_seconds_off_and_blank() {
        assert_eq!(parse_loop_field("3"), Some(3.0));
        assert_eq!(parse_loop_field(" 2.5 s "), Some(2.5));
        assert_eq!(parse_loop_field("4s"), Some(4.0));
        assert_eq!(parse_loop_field(""), Some(0.0));
        assert_eq!(parse_loop_field("off"), Some(0.0));
        assert_eq!(parse_loop_field("Off"), Some(0.0));
        assert_eq!(parse_loop_field("abc"), None);
        assert_eq!(parse_loop_field("inf"), None);
    }

    #[test]
    fn a_loop_field_edit_is_an_undo_step() {
        let mut patch = floret_engine::load_patch(SAMPLE).unwrap();
        let mut view = crate::canvas::View::default();
        view.on_load(&mut patch);
        let before = view.revision();
        assert!(set_loop_length(&mut patch, 0.0));
        view.commit_setting(&patch, "patch.loopLength");
        assert_ne!(view.revision(), before);
    }

    #[test]
    fn plans_whole_loops() {
        let p = plan(4.0, 1, 30);
        assert_eq!((p.frames, p.per_loop), (120, 120));
        assert!((p.dt - 1.0 / 30.0).abs() < 1e-15);
        assert_eq!(plan(2.5, 3, 24).frames, 180);
        // 1.01 s at 30 fps: 30 frames, dt stretched so the loop closes.
        let p = plan(1.01, 1, 30);
        assert_eq!(p.frames, 30);
        assert!((p.frames as f64 * p.dt - 1.01).abs() < 1e-12);
        assert_eq!(plan(0.001, 1, 30).frames, 1);
    }

    #[test]
    fn sizes_round_and_stay_even_for_h264() {
        assert_eq!(out_size((1080, 1920), 1.0, Format::Mp4), (1080, 1920));
        assert_eq!(out_size((801, 601), 1.0, Format::Mp4), (800, 600));
        assert_eq!(out_size((801, 601), 1.0, Format::Gif), (801, 601));
        assert_eq!(out_size((1080, 1920), 0.5, Format::PngSequence), (540, 960));
        assert!(problem((3840, 2160), 30, Format::Mp4, 8192).is_none());
        assert!(problem((4000, 2000), 30, Format::Mp4, 8192).is_some());
        assert!(problem((4000, 2000), 30, Format::PngSequence, 2048).is_some());
        assert!(problem((400, 400), 60, Format::Gif, 8192).is_some());
    }

    #[test]
    fn frame_files_are_numbered_in_order() {
        assert_eq!(frame_file("loop", 0, 120), "loop-0001.png");
        assert_eq!(frame_file("loop", 119, 120), "loop-0120.png");
        assert_eq!(frame_file("x", 0, 12345), "x-00001.png");
    }

    /// Scenes of frames `0..=n` of an offline run.
    fn scenes(patch: &PatchFile, n: usize, dt: f64) -> Vec<String> {
        let mut off = Offline::new(patch, "").unwrap();
        (0..=n).map(|i| format!("{:?}", off.frame(i, dt))).collect()
    }

    #[test]
    fn offline_runs_are_deterministic() {
        let patch = floret_engine::load_patch(SAMPLE).unwrap();
        let p = plan(patch.loop_seconds(), 1, 30);
        let a = scenes(&patch, 12, p.dt);
        let b = scenes(&patch, 12, p.dt);
        assert_eq!(a, b);
        // And it moves.
        assert_ne!(a[0], a[6]);
    }

    #[test]
    fn the_loop_sample_joins_seamlessly() {
        let patch = floret_engine::load_patch(SAMPLE).unwrap();
        assert!(patch.loop_length.is_some(), "the sample sets its loop");
        for fps in [24, 30] {
            let p = plan(patch.loop_seconds(), 1, fps);
            let s = scenes(&patch, p.frames, p.dt);
            // Frame N (one past the recording) is frame 0 again.
            assert_eq!(s[p.frames], s[0], "{fps} fps");
        }
    }

    /// The whole path on a real GPU (skipped without an adapter): a GIF and
    /// a PNG sequence of the sample, read back.
    /// A device of our own, as `floret_render::Headless` makes one.
    fn gpu() -> Option<Gpu> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("floret recording test"),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .ok()?;
        Some(Gpu {
            device,
            queue,
            adapter,
        })
    }

    #[test]
    fn records_gif_and_png_sequence_on_the_gpu() {
        let Some(gpu) = gpu() else {
            eprintln!("no GPU adapter; skipping");
            return;
        };
        let patch = floret_engine::load_patch(SAMPLE).unwrap();
        let dir = std::env::temp_dir().join(format!("floret-rec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let plan = Plan {
            frames: 6,
            ..plan(patch.loop_seconds(), 1, 10)
        };
        let spec = |format, path: PathBuf| Spec {
            patch: patch.clone(),
            renderer: String::new(),
            format,
            path,
            scale: 0.1,
            fps: 10,
            plan,
        };
        let (progress, cancel) = (AtomicUsize::new(0), AtomicBool::new(false));

        let gif = dir.join("loop.gif");
        let msg = record(&spec(Format::Gif, gif.clone()), &gpu, &progress, &cancel).unwrap();
        assert!(msg.contains("6 frames"), "{msg}");
        assert_eq!(progress.load(Ordering::Relaxed), 6);
        let file = File::open(&gif).unwrap();
        let mut dec = gif::DecodeOptions::new().read_info(file).unwrap();
        let canvas = Offline::new(&patch, "").unwrap().size;
        let want = out_size(canvas, 0.1, Format::Gif);
        assert_eq!((u32::from(dec.width()), u32::from(dec.height())), want);
        let mut frames = 0;
        while dec.read_next_frame().unwrap().is_some() {
            frames += 1;
        }
        assert_eq!(frames, 6);

        let stem = dir.join("seq");
        record(&spec(Format::PngSequence, stem), &gpu, &progress, &cancel).unwrap();
        for i in 0..6 {
            assert!(dir.join(frame_file("seq", i, 6)).exists(), "frame {i}");
        }

        // Cancelling stops and cleans up.
        cancel.store(true, Ordering::Relaxed);
        let gone = dir.join("cancelled.gif");
        let r = record(&spec(Format::Gif, gone.clone()), &gpu, &progress, &cancel);
        assert_eq!(r, Err(CANCELLED.to_owned()));
        assert!(!gone.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
