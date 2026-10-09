//! Export Variations: render many versions of the patch at once to pick
//! favourites from.
//!
//! What varies is the patch seed (a range, or N random seeds drawn from a
//! base seed), the patch's presets (all or some), or both (every preset
//! with every seed). Each variation is a fresh offline run, as video export
//! does ([`recording::Offline`]: fixed time step, no mouse, the patch's own
//! sound engine), captured as a still at a chosen time or as a one-loop GIF,
//! so the same settings always give the same pictures. Files are named by
//! what made them (`<patch>-seed-1234.png`, `<patch>-sunset-seed-77.png`)
//! and go into a new folder together with a **contact sheet**: every
//! variation in a grid, labelled with its preset and seed.
//!
//! Coming back to a favourite: type its seed into the Presets panel's seed
//! field (and recall its preset), or click its thumbnail in the window's
//! results, which does both as one undo step.

use crate::export;
use crate::recording::{self, Gpu, Offline};
use crate::theme;
use eframe::egui::{self, RichText};
use eframe::egui_wgpu::{self, wgpu};
use floret_engine::image::Image;
use floret_engine::patch::PatchFile;
use floret_engine::scene::{self, Paint, RectF, ResolvedStyle, SceneNode};
use floret_engine::spec::Catalogue;
use floret_render::{RenderClock, Renderer};
use floret_video::gif_writer::GifWriter;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Instant;

/// The most variations one export makes.
pub const MAX_VARIATIONS: usize = 400;
/// Stills step the patch at this rate up to the capture time.
pub const STILL_FPS: u32 = 30;
/// Contact sheet: the box each thumbnail fits in, before shrinking to fit
/// the GPU's texture limit.
const CELL: u32 = 256;
const LABEL_H: u32 = 24;
const GAP: u32 = 12;
const HEADER_H: u32 = 40;

// ─── What varies ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedMode {
    /// Each variation keeps its seed (the patch's, or its preset's).
    Keep,
    /// `count` seeds counting up from `first`.
    Range,
    /// `count` seeds drawn from `first` (the base): the same base always
    /// gives the same seeds.
    Random,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vary {
    pub seeds: SeedMode,
    pub first: u32,
    pub count: u32,
    /// Presets to render (indices into the patch's presets); empty: the
    /// patch's current values.
    pub presets: Vec<usize>,
}

/// One variation to render.
#[derive(Clone, Debug, PartialEq)]
pub struct Variation {
    pub seed: u32,
    /// The preset recalled first, by name.
    pub preset: Option<String>,
    /// File name without extension, unique within the export.
    pub stem: String,
}

impl Variation {
    /// The contact sheet's label: "seed 1234", "Sunset · seed 77".
    pub fn label(&self) -> String {
        match &self.preset {
            Some(p) => format!("{p} · seed {}", self.seed),
            None => format!("seed {}", self.seed),
        }
    }
}

/// `n` distinct seeds drawn from `base` (1 ..= 999 999, never `base`
/// itself); the same base always gives the same seeds.
pub fn random_seeds(base: u32, n: usize) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::with_capacity(n);
    let mut k = 0u64;
    while out.len() < n {
        k += 1;
        let s = floret_engine::seed::reroll(base, (u64::from(base) << 32) ^ k);
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// The seeds asked for (`None` per variation for [`SeedMode::Keep`]).
fn seed_list(vary: &Vary) -> Vec<Option<u32>> {
    let n = vary.count.clamp(1, MAX_VARIATIONS as u32);
    match vary.seeds {
        SeedMode::Keep => vec![None],
        SeedMode::Range => (0..n).map(|i| Some(vary.first.wrapping_add(i))).collect(),
        SeedMode::Random => random_seeds(vary.first, n as usize)
            .into_iter()
            .map(Some)
            .collect(),
    }
}

/// `<stem>-seed-1234`, `<stem>-<preset>-seed-1234`.
pub fn file_stem(stem: &str, preset: Option<&str>, seed: u32) -> String {
    match preset {
        Some(p) => format!("{stem}-{}-seed-{seed}", export::slug(p)),
        None => format!("{stem}-seed-{seed}"),
    }
}

/// Every variation `vary` asks for: each chosen preset (or the current
/// values) with each seed, at most [`MAX_VARIATIONS`].
pub fn variations(patch: &PatchFile, vary: &Vary, stem: &str) -> Vec<Variation> {
    let presets: Vec<Option<&floret_engine::patch::Preset>> = if vary.presets.is_empty() {
        vec![None]
    } else {
        (vary.presets.iter())
            .filter_map(|&i| patch.presets.get(i))
            .map(Some)
            .collect()
    };
    let seeds = seed_list(vary);
    let mut out: Vec<Variation> = Vec::new();
    'all: for p in &presets {
        for s in &seeds {
            if out.len() >= MAX_VARIATIONS {
                break 'all;
            }
            let seed = s.unwrap_or_else(|| p.map_or(patch.seed, |p| p.seed));
            let preset = p.map(|p| p.name.clone());
            let base = file_stem(stem, preset.as_deref(), seed);
            // Two presets with the same name (and seed): number the rest.
            let mut name = base.clone();
            let mut k = 2;
            while out.iter().any(|v| v.stem == name) {
                name = format!("{base}-{k}");
                k += 1;
            }
            out.push(Variation {
                seed,
                preset,
                stem: name,
            });
        }
    }
    out
}

/// Put a variation into `patch`: recall its preset (if the patch still has
/// one by that name), then set its seed. True if anything changed.
pub fn apply(patch: &mut PatchFile, v: &Variation) -> bool {
    let mut changed = false;
    if let Some(p) = (v.preset.as_ref())
        .and_then(|name| patch.presets.iter().find(|p| &p.name == name))
        .cloned()
    {
        let (fields, _) = floret_engine::preset::morph(Catalogue::builtin(), patch, &p, &p, 0.0);
        changed |= floret_engine::preset::apply(patch, &fields);
    }
    if patch.seed != v.seed {
        patch.seed = v.seed;
        changed = true;
    }
    changed
}

// ─── Contact sheet ──────────────────────────────────────────────────────────

/// Where everything sits on the contact sheet, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetLayout {
    pub cols: u32,
    pub rows: u32,
    /// The box each thumbnail is fitted into.
    pub cell: (u32, u32),
    pub label_h: u32,
    pub gap: u32,
    pub header_h: u32,
    pub size: (u32, u32),
}

impl SheetLayout {
    /// `n` thumbnails of frames shaped like `frame`, in a grid about as
    /// wide as tall, shrunk until the sheet fits `max` pixels a side.
    pub fn new(n: usize, frame: (u32, u32), max: u32) -> SheetLayout {
        let n = n.max(1) as u32;
        let (fw, fh) = (f64::from(frame.0.max(1)), f64::from(frame.1.max(1)));
        let fit = |cell: f64| {
            let s = (cell / fw).min(cell / fh);
            (
                (fw * s).round().max(8.0) as u32,
                (fh * s).round().max(8.0) as u32,
            )
        };
        let mut cell = fit(f64::from(CELL));
        // Columns that make the sheet roughly square.
        let ratio = f64::from(cell.1 + LABEL_H) / f64::from(cell.0);
        let cols = ((f64::from(n) * ratio).sqrt().ceil() as u32).clamp(1, n);
        let rows = n.div_ceil(cols);
        let size = |cell: (u32, u32)| {
            (
                GAP + cols * (cell.0 + GAP),
                HEADER_H + rows * (cell.1 + LABEL_H + GAP),
            )
        };
        let mut s = size(cell);
        let mut box_ = f64::from(CELL);
        while (s.0 > max || s.1 > max) && box_ > 16.0 {
            box_ *= 0.9;
            cell = fit(box_);
            s = size(cell);
        }
        SheetLayout {
            cols,
            rows,
            cell,
            label_h: LABEL_H,
            gap: GAP,
            header_h: HEADER_H,
            size: s,
        }
    }

    /// Top-left corner of cell `i` (row-major).
    pub fn cell_at(&self, i: usize) -> (u32, u32) {
        let i = i as u32;
        let (c, r) = (i % self.cols, i / self.cols);
        (
            self.gap + c * (self.cell.0 + self.gap),
            self.header_h + r * (self.cell.1 + self.label_h + self.gap),
        )
    }

    /// Where a thumbnail of `size` sits in cell `i`: centred in its box.
    pub fn thumb_rect(&self, i: usize, size: (u32, u32)) -> (u32, u32, u32, u32) {
        let (x, y) = self.cell_at(i);
        let fit = |w: u32, h: u32| {
            let s = (f64::from(self.cell.0) / f64::from(w.max(1)))
                .min(f64::from(self.cell.1) / f64::from(h.max(1)));
            (
                ((f64::from(w) * s).round() as u32).clamp(1, self.cell.0),
                ((f64::from(h) * s).round() as u32).clamp(1, self.cell.1),
            )
        };
        let (w, h) = fit(size.0, size.1);
        (x + (self.cell.0 - w) / 2, y + (self.cell.1 - h) / 2, w, h)
    }
}

/// Box-filter `rgba` (`from`) down to `to` (each side no bigger).
pub fn downscale(rgba: &[u8], from: (u32, u32), to: (u32, u32)) -> Vec<u8> {
    let (fw, fh) = (from.0.max(1) as usize, from.1.max(1) as usize);
    let (tw, th) = (
        to.0.clamp(1, from.0.max(1)) as usize,
        to.1.clamp(1, from.1.max(1)) as usize,
    );
    let mut out = vec![0u8; tw * th * 4];
    for ty in 0..th {
        let (y0, y1) = (ty * fh / th, ((ty + 1) * fh / th).max(ty * fh / th + 1));
        for tx in 0..tw {
            let (x0, x1) = (tx * fw / tw, ((tx + 1) * fw / tw).max(tx * fw / tw + 1));
            let mut sum = [0u32; 4];
            for y in y0..y1.min(fh) {
                for x in x0..x1.min(fw) {
                    let p = (y * fw + x) * 4;
                    for (s, v) in sum.iter_mut().zip(&rgba[p..p + 4]) {
                        *s += u32::from(*v);
                    }
                }
            }
            let n = ((y1.min(fh) - y0) * (x1.min(fw) - x0)).max(1) as u32;
            let o = (ty * tw + tx) * 4;
            for (k, s) in sum.iter().enumerate() {
                out[o + k] = ((s + n / 2) / n) as u8;
            }
        }
    }
    out
}

/// A rendered variation, shrunk for the contact sheet and the window.
#[derive(Clone)]
pub struct Thumb {
    pub variation: Variation,
    pub size: (u32, u32),
    pub rgba: Vec<u8>,
}

fn text_style(size: f64, color: &str, align: &str) -> Arc<ResolvedStyle> {
    Arc::new(ResolvedStyle {
        fill: Some(Paint::css(color)),
        font_size: size,
        text_align: Arc::from(align),
        text_baseline: Arc::from("middle"),
        ..(**scene::default_style()).clone()
    })
}

/// The contact sheet as a scene (centre origin, y down, in sheet pixels).
pub fn sheet_scene(layout: &SheetLayout, thumbs: &[Thumb], title: &str) -> Vec<Arc<SceneNode>> {
    let (ox, oy) = (
        f64::from(layout.size.0) / 2.0,
        f64::from(layout.size.1) / 2.0,
    );
    let mut nodes = vec![Arc::new(SceneNode::Text {
        text: Arc::from(title),
        x: f64::from(layout.gap) - ox,
        y: f64::from(layout.header_h) / 2.0 - oy,
        style: text_style(15.0, "#e6e6e4", "left"),
    })];
    for (i, t) in thumbs.iter().enumerate() {
        let (x, y, w, h) = layout.thumb_rect(i, t.size);
        nodes.push(Arc::new(SceneNode::Image {
            image: Arc::new(Image::new(t.size.0, t.size.1, t.rgba.clone())),
            src: None,
            dst: RectF {
                x: f64::from(x) - ox,
                y: f64::from(y) - oy,
                w: f64::from(w),
                h: f64::from(h),
            },
            tint: None,
        }));
        let (cx, cy) = layout.cell_at(i);
        nodes.push(Arc::new(SceneNode::Text {
            text: Arc::from(t.variation.label()),
            x: f64::from(cx) + f64::from(layout.cell.0) / 2.0 - ox,
            y: f64::from(cy + layout.cell.1) + f64::from(layout.label_h) / 2.0 - oy,
            style: text_style(12.0, "#a3a3a0", "center"),
        }));
    }
    nodes
}

const SHEET_BG: [f64; 4] = [
    0x19 as f64 / 255.0,
    0x19 as f64 / 255.0,
    0x19 as f64 / 255.0,
    1.0,
];

// ─── Rendering ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// A PNG of the frame at the capture time.
    Still,
    /// A one-loop GIF.
    Gif,
}

pub struct Spec {
    pub patch: PatchFile,
    /// Renderer node id ("" for the first).
    pub renderer: String,
    pub variations: Vec<Variation>,
    pub output: Output,
    /// Capture time in seconds (stills; the contact sheet's frame for GIFs).
    pub at: f64,
    pub scale: f64,
    pub gif_fps: u32,
    /// The folder the files go in (created if missing).
    pub dir: PathBuf,
    /// The contact sheet's file stem and title, or `None` for no sheet.
    pub sheet: Option<(String, String)>,
}

impl Spec {
    /// Progress steps: one per variation, one for the sheet.
    pub fn steps(&self) -> usize {
        self.variations.len() + usize::from(self.sheet.is_some())
    }
}

pub struct Done {
    pub message: String,
    pub dir: PathBuf,
    pub thumbs: Vec<Thumb>,
}

const CANCELLED: &str = "Export cancelled";

/// Frame index of the capture time `at` in a run at `fps` (clamped to
/// `frames` when given).
pub fn capture_frame(at: f64, fps: u32, frames: Option<usize>) -> usize {
    let i = (at.max(0.0) * f64::from(fps)).round() as usize;
    frames.map_or(i, |n| i.min(n.saturating_sub(1)))
}

struct GpuFrames<'a> {
    gpu: &'a Gpu,
    renderer: Option<(u32, Renderer)>,
}

impl GpuFrames<'_> {
    fn render(
        &mut self,
        layers: &[Arc<SceneNode>],
        bg: [f64; 4],
        canvas: (u32, u32),
        size: (u32, u32),
        clock: RenderClock,
    ) -> Result<Vec<u8>, String> {
        let gpu = self.gpu;
        let samples = export::samples(&gpu.adapter, size);
        if self.renderer.as_ref().is_none_or(|(s, _)| *s != samples) {
            let r = Renderer::new(
                &gpu.device,
                &gpu.queue,
                floret_render::OFFSCREEN_FORMAT,
                samples,
            );
            self.renderer = Some((samples, r));
        }
        let (_, renderer) = self.renderer.as_mut().expect("just made");
        renderer.set_clock(clock);
        let sx = f64::from(size.0) / f64::from(canvas.0.max(1));
        let sy = f64::from(size.1) / f64::from(canvas.1.max(1));
        let oom = gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let (pixels, _) = floret_render::render_and_read(
            &gpu.device,
            &gpu.queue,
            renderer,
            &export::scaled(layers, sx, sy),
            size,
            bg,
        );
        if let Some(e) = pollster::block_on(validation.pop()) {
            return Err(e.to_string());
        }
        if let Some(e) = pollster::block_on(oom.pop()) {
            return Err(e.to_string());
        }
        Ok(pixels)
    }
}

fn clock(i: usize, dt: f64) -> RenderClock {
    RenderClock {
        time: i as f64 * dt,
        time_delta: dt,
        frame: i as f64,
        ..RenderClock::default()
    }
}

/// A variation's file, and its frame for the contact sheet (size, RGBA).
type Shot = (PathBuf, (u32, u32), Vec<u8>);

/// Render one variation into `dir`; returns its file and the frame for the
/// contact sheet (`size`, RGBA).
fn render_one(
    spec: &Spec,
    v: &Variation,
    gpu: &mut GpuFrames<'_>,
    cancel: &AtomicBool,
) -> Result<Shot, String> {
    let mut patch = spec.patch.clone();
    apply(&mut patch, v);
    let mut off = Offline::new(&patch, &spec.renderer)?;
    let canvas = off.size;
    let max = gpu.gpu.device.limits().max_texture_dimension_2d;
    match spec.output {
        Output::Still => {
            let size = recording::out_size(canvas, spec.scale, recording::Format::PngSequence);
            if let Some(p) =
                recording::problem(size, STILL_FPS, recording::Format::PngSequence, max)
            {
                return Err(p);
            }
            let dt = 1.0 / f64::from(STILL_FPS);
            let k = capture_frame(spec.at, STILL_FPS, None);
            let mut last = (Vec::new(), [0.0, 0.0, 0.0, 1.0]);
            for i in 0..=k {
                if cancel.load(Ordering::Relaxed) {
                    return Err(CANCELLED.to_owned());
                }
                last = off.frame(i, dt);
                off.sound(i, STILL_FPS);
            }
            let pixels = gpu.render(&last.0, last.1, canvas, size, clock(k, dt))?;
            let path = spec.dir.join(format!("{}.png", v.stem));
            export::write_png(&path, size.0, size.1, &pixels)?;
            Ok((path, size, pixels))
        }
        Output::Gif => {
            let fps = spec.gif_fps;
            let size = recording::out_size(canvas, spec.scale, recording::Format::Gif);
            if let Some(p) = recording::problem(size, fps, recording::Format::Gif, max) {
                return Err(p);
            }
            let plan = recording::plan(patch.loop_seconds(), 1, fps);
            let pick = capture_frame(spec.at, fps, Some(plan.frames));
            let path = spec.dir.join(format!("{}.gif", v.stem));
            let file = File::create(&path)
                .map(BufWriter::new)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let mut gif = GifWriter::new(file, size.0, size.1, fps)?;
            let mut keep = Vec::new();
            for i in 0..plan.frames {
                if cancel.load(Ordering::Relaxed) {
                    drop(gif);
                    let _ = std::fs::remove_file(&path);
                    return Err(CANCELLED.to_owned());
                }
                let (layers, bg) = off.frame(i, plan.dt);
                off.sound(i, fps);
                let mut pixels = gpu.render(&layers, bg, canvas, size, clock(i, plan.dt))?;
                if i == pick {
                    keep = pixels.clone();
                }
                gif.push_rgba(&mut pixels)?;
            }
            gif.finish()?;
            Ok((path, size, keep))
        }
    }
}

/// Render every variation (and the contact sheet) into `spec.dir`,
/// counting finished steps in `progress`; stops and removes what it wrote
/// when `cancel` is set.
pub fn run(
    spec: &Spec,
    gpu: &Gpu,
    progress: &AtomicUsize,
    cancel: &AtomicBool,
) -> Result<Done, String> {
    let _wait = floret_engine::video::wait_for_frames();
    let made_dir = !spec.dir.exists();
    std::fs::create_dir_all(&spec.dir).map_err(|e| format!("{}: {e}", spec.dir.display()))?;
    let mut written: Vec<PathBuf> = Vec::new();
    let mut thumbs: Vec<Thumb> = Vec::new();
    let mut gpu2 = GpuFrames {
        gpu,
        renderer: None,
    };
    let max = gpu.device.limits().max_texture_dimension_2d;
    let mut go = || -> Result<(), String> {
        let mut layout = None;
        for (i, v) in spec.variations.iter().enumerate() {
            let (path, size, pixels) = render_one(spec, v, &mut gpu2, cancel)?;
            written.push(path);
            let layout = *layout.get_or_insert_with(|| {
                SheetLayout::new(spec.variations.len(), size, max.min(8192))
            });
            // Shrunk to its cell (small exports keep their own size and
            // are drawn larger).
            let (_, _, w, h) = layout.thumb_rect(i, size);
            let (w, h) = (w.min(size.0), h.min(size.1));
            thumbs.push(Thumb {
                variation: v.clone(),
                size: (w, h),
                rgba: downscale(&pixels, size, (w, h)),
            });
            progress.store(i + 1, Ordering::Relaxed);
        }
        if let (Some((stem, title)), Some(layout)) = (&spec.sheet, layout) {
            if cancel.load(Ordering::Relaxed) {
                return Err(CANCELLED.to_owned());
            }
            let nodes = sheet_scene(&layout, &thumbs, title);
            let size = layout.size;
            let pixels = gpu2.render(&nodes, SHEET_BG, size, size, RenderClock::default())?;
            let path = spec.dir.join(format!("{stem}.png"));
            export::write_png(&path, size.0, size.1, &pixels)?;
            written.push(path);
            progress.store(spec.steps(), Ordering::Relaxed);
        }
        Ok(())
    };
    match go() {
        Ok(()) => {
            let n = spec.variations.len();
            let what = match spec.output {
                Output::Still => "PNG",
                Output::Gif => "GIF",
            };
            let sheet = if spec.sheet.is_some() {
                " and a contact sheet"
            } else {
                ""
            };
            let plural = if n == 1 { "" } else { "s" };
            let folder = spec
                .dir
                .file_name()
                .map_or_else(String::new, |f| f.to_string_lossy().into_owned());
            Ok(Done {
                message: format!("Saved {n} {what}{plural}{sheet} in {folder}"),
                dir: spec.dir.clone(),
                thumbs,
            })
        }
        Err(e) => {
            for p in written {
                let _ = std::fs::remove_file(p);
            }
            if made_dir {
                let _ = std::fs::remove_dir(&spec.dir);
            }
            Err(e)
        }
    }
}

/// A folder in `parent` that doesn't exist yet: `<stem>-variations`,
/// then `-2`, `-3`, …
pub fn fresh_dir(parent: &Path, stem: &str) -> PathBuf {
    let base = format!("{stem}-variations");
    let mut p = parent.join(&base);
    let mut k = 2;
    while p.exists() {
        p = parent.join(format!("{base}-{k}"));
        k += 1;
    }
    p
}

// ─── Window ─────────────────────────────────────────────────────────────────

struct Job {
    progress: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    steps: usize,
    started: Instant,
    done: mpsc::Receiver<Result<Done, String>>,
}

/// The last export's thumbnails, as textures for the results grid.
struct Results {
    dir: PathBuf,
    cells: Vec<(Variation, egui::TextureHandle)>,
    /// The variation last put into the patch from here.
    used: Option<usize>,
}

/// The Export Variations window.
pub struct Variations {
    open: bool,
    renderer: Option<String>,
    vary_seeds: bool,
    seed_mode: SeedMode,
    first: u32,
    count: u32,
    vary_presets: bool,
    /// Per preset: render it (resized to the patch's presets).
    picked: Vec<bool>,
    output: Output,
    at: f64,
    scale: f64,
    gif_fps: u32,
    sheet: bool,
    job: Option<Job>,
    status: Option<Result<String, String>>,
    results: Option<Results>,
}

impl Default for Variations {
    fn default() -> Self {
        Variations {
            open: false,
            renderer: None,
            vary_seeds: true,
            seed_mode: SeedMode::Random,
            first: 0,
            count: 12,
            vary_presets: false,
            picked: Vec::new(),
            output: Output::Still,
            at: 2.0,
            scale: 1.0,
            gif_fps: 15,
            sheet: true,
            job: None,
            status: None,
            results: None,
        }
    }
}

impl Variations {
    /// Show the window (for `renderer`, else the one picked last). Seeds
    /// start from the patch's current one.
    pub fn open(&mut self, renderer: Option<String>, patch: &PatchFile) {
        if !self.open && self.job.is_none() {
            self.first = patch.seed;
        }
        self.open = true;
        if renderer.is_some() {
            self.renderer = renderer;
        }
    }

    /// What the settings ask for.
    pub fn vary(&self, patch: &PatchFile) -> Vary {
        Vary {
            seeds: if self.vary_seeds {
                self.seed_mode
            } else {
                SeedMode::Keep
            },
            first: self.first,
            count: self.count,
            presets: if self.vary_presets {
                (0..patch.presets.len())
                    .filter(|&i| self.picked.get(i).copied().unwrap_or(true))
                    .collect()
            } else {
                Vec::new()
            },
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else {
            return;
        };
        match job.done.try_recv() {
            Ok(Ok(done)) => {
                self.status = Some(Ok(done.message));
                let cells = (done.thumbs.into_iter())
                    .enumerate()
                    .map(|(i, t)| {
                        let img = egui::ColorImage::from_rgba_unmultiplied(
                            [t.size.0 as usize, t.size.1 as usize],
                            &t.rgba,
                        );
                        let tex = ctx.load_texture(
                            format!("floret-variation-{i}"),
                            img,
                            egui::TextureOptions::LINEAR,
                        );
                        (t.variation, tex)
                    })
                    .collect();
                self.results = Some(Results {
                    dir: done.dir,
                    cells,
                    used: None,
                });
                self.job = None;
            }
            Ok(Err(e)) => {
                self.status = Some(Err(e));
                self.job = None;
            }
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint(),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.status = Some(Err("Export failed".into()));
                self.job = None;
            }
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        rs: Option<&egui_wgpu::RenderState>,
        patch: &mut PatchFile,
        view: &mut crate::canvas::View,
        renderers: &[(String, String)],
        doc_name: &str,
    ) {
        self.poll(ctx);
        if !self.open {
            return;
        }
        self.picked.resize(patch.presets.len(), true);
        let targets = recording::targets(patch, renderers, doc_name);
        let mut open = true;
        let running = self.job.is_some();
        let mut window = egui::Window::new("Export Variations")
            .id(egui::Id::new("floret-variations"))
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center());
        if !running {
            window = window.open(&mut open);
        }
        window.show(ctx, |ui| {
            ui.add_enabled_ui(!running, |ui| self.settings(ui, rs, patch, &targets));
            ui.separator();
            self.footer(ui, rs, patch, &targets);
            self.results_grid(ui, patch, view);
        });
        if !open {
            self.open = false;
        }
    }

    fn target<'a>(&self, targets: &'a [recording::Target]) -> Option<&'a recording::Target> {
        (self.renderer.as_ref())
            .and_then(|id| targets.iter().find(|t| t.id == *id))
            .or_else(|| targets.first())
    }

    fn settings(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        patch: &PatchFile,
        targets: &[recording::Target],
    ) {
        let canvas = self
            .target(targets)
            .map_or(floret_render::DEFAULT_CANVAS, |t| t.size);
        egui::Grid::new("floret-variations-grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Vary");
                ui.horizontal(|ui| {
                    ui.toggle_value(&mut self.vary_seeds, "Seed")
                        .on_hover_text("Render the patch with different seeds");
                    ui.toggle_value(&mut self.vary_presets, "Presets")
                        .on_hover_text("Render the patch's saved presets");
                });
                ui.end_row();

                if self.vary_seeds {
                    ui.label("Seeds");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("floret-variations-seeds")
                            .width(90.0)
                            .selected_text(match self.seed_mode {
                                SeedMode::Range => "Range",
                                _ => "Random",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.seed_mode,
                                    SeedMode::Random,
                                    "Random",
                                )
                                .on_hover_text("Seeds drawn from a base seed");
                                ui.selectable_value(&mut self.seed_mode, SeedMode::Range, "Range")
                                    .on_hover_text("Seeds counting up from the first");
                            });
                        ui.add(
                            egui::DragValue::new(&mut self.count)
                                .range(1..=MAX_VARIATIONS as u32)
                                .speed(0.1),
                        );
                        let from = if self.seed_mode == SeedMode::Range {
                            "from"
                        } else {
                            "from base"
                        };
                        ui.label(RichText::new(from).color(theme::FG_2));
                        ui.add(
                            egui::DragValue::new(&mut self.first)
                                .range(0..=u32::MAX)
                                .speed(1.0),
                        )
                        .on_hover_text(
                            "The same base always draws the same seeds; \
                             the patch's seed to start with",
                        );
                    });
                    ui.end_row();
                }

                if self.vary_presets {
                    ui.label("Presets");
                    ui.vertical(|ui| {
                        if patch.presets.is_empty() {
                            theme::empty_state(
                                ui,
                                "No presets yet",
                                "Save some in the Presets panel to render each one",
                            );
                        } else {
                            let all = self.picked.iter().all(|p| *p);
                            let mut every = all;
                            if ui.checkbox(&mut every, "All").changed() {
                                self.picked.iter_mut().for_each(|p| *p = every);
                            }
                            egui::ScrollArea::vertical()
                                .id_salt("floret-variations-presets")
                                .max_height(110.0)
                                .show(ui, |ui| {
                                    for (p, on) in patch.presets.iter().zip(self.picked.iter_mut())
                                    {
                                        ui.checkbox(on, &p.name);
                                    }
                                });
                        }
                    });
                    ui.end_row();
                }

                if targets.len() > 1 {
                    ui.label("Preview");
                    let current = self.target(targets).map_or("", |t| &t.title).to_owned();
                    egui::ComboBox::from_id_salt("floret-variations-renderer")
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

                ui.label("Capture");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.output, Output::Still, "Still (PNG)");
                    ui.selectable_value(&mut self.output, Output::Gif, "Loop (GIF)")
                        .on_hover_text(format!(
                            "One loop of the patch ({:.2} s) per variation",
                            patch.loop_seconds()
                        ));
                });
                ui.end_row();

                let at_label = match self.output {
                    Output::Still => "At",
                    Output::Gif => "Sheet frame",
                };
                ui.label(at_label);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut self.at)
                            .range(0.0..=600.0)
                            .speed(0.05)
                            .max_decimals(2)
                            .suffix(" s"),
                    )
                    .on_hover_text(
                        "How far into the patch to capture, run from a fresh start \
                         with no mouse or keys",
                    );
                    if self.output == Output::Gif {
                        egui::ComboBox::from_id_salt("floret-variations-fps")
                            .width(70.0)
                            .selected_text(format!("{} fps", self.gif_fps))
                            .show_ui(ui, |ui| {
                                for &r in recording::Format::Gif.rates() {
                                    ui.selectable_value(&mut self.gif_fps, r, format!("{r} fps"));
                                }
                            });
                    }
                });
                ui.end_row();

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

                ui.label("");
                ui.checkbox(&mut self.sheet, "Contact sheet")
                    .on_hover_text("Also save every variation in one labelled grid");
                ui.end_row();
            });
    }

    fn footer(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        patch: &PatchFile,
        targets: &[recording::Target],
    ) {
        if let Some(job) = &self.job {
            let done = job.progress.load(Ordering::Relaxed);
            let frac = done as f32 / job.steps.max(1) as f32;
            let mut text = format!("{done} of {}", job.steps);
            if done > 0 {
                let per = job.started.elapsed().as_secs_f64() / done as f64;
                let left = per * (job.steps - done.min(job.steps)) as f64;
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
        let name = target.map_or("floret", |t| t.name.as_str());
        let list = variations(patch, &self.vary(patch), &export::slug(name));
        let format = match self.output {
            Output::Still => recording::Format::PngSequence,
            Output::Gif => recording::Format::Gif,
        };
        let size = target.map(|t| recording::out_size(t.size, self.scale, format));
        let max = rs.map_or(0, |rs| rs.device.limits().max_texture_dimension_2d);
        let fps = match self.output {
            Output::Still => STILL_FPS,
            Output::Gif => self.gif_fps,
        };
        let blocked = match (target, size, rs) {
            (None, _, _) | (_, None, _) => Some("The patch has no Renderer to record".to_owned()),
            (_, _, None) => Some("No GPU renderer".to_owned()),
            _ if !self.vary_seeds && !self.vary_presets => {
                Some("Choose what varies: the seed, presets or both".to_owned())
            }
            _ if self.vary_presets && self.vary(patch).presets.is_empty() => {
                Some("Pick at least one preset".to_owned())
            }
            (Some(_), Some(size), Some(_)) => recording::problem(size, fps, format, max),
        };
        ui.horizontal(|ui| {
            if let Some(size) = size {
                let n = list.len();
                let plural = if n == 1 { "" } else { "s" };
                ui.label(
                    RichText::new(format!("{n} variation{plural} · {} × {}", size.0, size.1))
                        .monospace()
                        .color(theme::FG_1),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let text = RichText::new("Export…").strong().color(theme::ON_GOLD);
                let button = egui::Button::new(text)
                    .fill(theme::GOLD_FILL)
                    .stroke(egui::Stroke::NONE);
                let go = ui.add_enabled(blocked.is_none(), button);
                if go.clicked() {
                    if let (Some(rs), Some(t)) = (rs, target) {
                        let (id, name) = (t.id.clone(), t.name.clone());
                        self.start(rs, patch, id, &name, list);
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

    /// The last export's thumbnails: click one to put its preset and seed
    /// into the patch.
    fn results_grid(
        &mut self,
        ui: &mut egui::Ui,
        patch: &mut PatchFile,
        view: &mut crate::canvas::View,
    ) {
        let Some(res) = &mut self.results else {
            return;
        };
        if self.job.is_some() {
            return;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Results").strong());
            ui.label(
                RichText::new("Click one to use its seed")
                    .color(theme::FG_2)
                    .small(),
            );
        });
        // A fixed grid, so the window keeps its width.
        const COLS: usize = 4;
        let cell = 80.0;
        let mut use_ = None;
        egui::ScrollArea::vertical()
            .id_salt("floret-variations-results")
            .max_height(260.0)
            .show(ui, |ui| {
                egui::Grid::new("floret-variations-results-grid")
                    .spacing([8.0, 8.0])
                    .show(ui, |ui| {
                        for (i, (v, tex)) in res.cells.iter().enumerate() {
                            let size = tex.size_vec2();
                            let s = (cell / size.x).min(cell / size.y);
                            ui.vertical(|ui| {
                                ui.set_width(cell + 8.0);
                                let img = egui::Image::new((tex.id(), size * s));
                                let mut b = egui::Button::image(img).frame(true);
                                if res.used == Some(i) {
                                    b = b.stroke(egui::Stroke::new(2.0, theme::GOLD.get()));
                                }
                                let tip = match &v.preset {
                                    Some(p) => format!("Use “{p}” with seed {}", v.seed),
                                    None => format!("Use seed {}", v.seed),
                                };
                                if ui.add(b).on_hover_text(tip).clicked() {
                                    use_ = Some(i);
                                }
                                if let Some(p) = &v.preset {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(p).small().color(theme::FG_1),
                                        )
                                        .truncate(),
                                    );
                                }
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("seed {}", v.seed))
                                            .small()
                                            .color(theme::FG_2),
                                    )
                                    .truncate(),
                                );
                            });
                            if i % COLS == COLS - 1 {
                                ui.end_row();
                            }
                        }
                    });
            });
        let folder = res.dir.file_name().map_or_else(
            || res.dir.display().to_string(),
            |f| f.to_string_lossy().into_owned(),
        );
        ui.add(
            egui::Label::new(
                RichText::new(format!("In {folder}"))
                    .small()
                    .color(theme::FG_2),
            )
            .truncate(),
        )
        .on_hover_text(res.dir.display().to_string());
        if let Some(i) = use_ {
            let v = res.cells[i].0.clone();
            if apply(patch, &v) {
                view.commit(patch);
            }
            res.used = Some(i);
        }
    }

    /// Ask for a folder, then render on a worker thread.
    fn start(
        &mut self,
        rs: &egui_wgpu::RenderState,
        patch: &PatchFile,
        renderer: String,
        name: &str,
        list: Vec<Variation>,
    ) {
        // Debug builds only: `FLORET_EXPORT_DIR` skips the folder dialog, for
        // scripted visual checks where no dialog can open (Xvfb).
        let scripted = (cfg!(debug_assertions))
            .then(|| std::env::var_os("FLORET_EXPORT_DIR").map(PathBuf::from))
            .flatten();
        let Some(parent) = scripted.or_else(|| {
            rfd::FileDialog::new()
                .set_title("Choose where to save the variations")
                .pick_folder()
        }) else {
            return;
        };
        let slug = export::slug(name);
        let title = format!(
            "{name} · {} variation{}{}",
            list.len(),
            if list.len() == 1 { "" } else { "s" },
            match self.output {
                Output::Still => format!(" · at {:.2} s", self.at),
                Output::Gif => String::new(),
            }
        );
        let spec = Spec {
            patch: patch.clone(),
            renderer,
            variations: list,
            output: self.output,
            at: self.at,
            scale: self.scale,
            gif_fps: self.gif_fps,
            dir: fresh_dir(&parent, &slug),
            sheet: self.sheet.then(|| (format!("{slug}-contact-sheet"), title)),
        };
        let gpu = Gpu {
            device: rs.device.clone(),
            queue: rs.queue.clone(),
            adapter: rs.adapter.clone(),
        };
        let steps = spec.steps();
        let progress = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (p, c) = (progress.clone(), cancel.clone());
        let spawned = std::thread::Builder::new()
            .name("floret-variations".into())
            .spawn(move || {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&spec, &gpu, &p, &c)
                }))
                .unwrap_or_else(|_| Err("Export failed: the patch crashed while rendering".into()));
                let _ = tx.send(r);
            });
        if let Err(e) = spawned {
            self.status = Some(Err(format!("Export failed: {e}")));
            return;
        }
        self.status = None;
        self.results = None;
        self.job = Some(Job {
            progress,
            cancel,
            steps,
            started: Instant::now(),
            done: rx,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../samples/variations.flora");

    fn sample() -> PatchFile {
        floret_engine::load_patch(SAMPLE).unwrap()
    }

    #[test]
    fn random_seeds_are_distinct_repeatable_and_short() {
        let a = random_seeds(42, 50);
        assert_eq!(a, random_seeds(42, 50));
        assert_eq!(a.len(), 50);
        let mut sorted = a.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 50, "distinct");
        assert!(a.iter().all(|&s| (1..=999_999).contains(&s) && s != 42));
        // Another base, other seeds; a longer list starts the same way.
        assert_ne!(random_seeds(43, 5), a[..5]);
        assert_eq!(random_seeds(42, 60)[..50], a[..]);
    }

    #[test]
    fn variations_cross_presets_with_seeds_and_name_files() {
        let patch = sample();
        let range = Vary {
            seeds: SeedMode::Range,
            first: 1234,
            count: 3,
            presets: Vec::new(),
        };
        let v = variations(&patch, &range, "variations");
        let stems: Vec<&str> = v.iter().map(|v| v.stem.as_str()).collect();
        assert_eq!(
            stems,
            [
                "variations-seed-1234",
                "variations-seed-1235",
                "variations-seed-1236"
            ]
        );
        assert_eq!(v[0].label(), "seed 1234");

        // Presets keep their own seeds unless the seed varies too.
        let presets = Vary {
            seeds: SeedMode::Keep,
            presets: vec![0, 1],
            ..range.clone()
        };
        let v = variations(&patch, &presets, "x");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].stem, "x-calm-seed-0");
        assert_eq!(v[1].stem, "x-storm-seed-2718");
        assert_eq!(v[1].label(), "Storm · seed 2718");

        let both = Vary {
            seeds: SeedMode::Random,
            first: 7,
            count: 4,
            presets: vec![0, 1],
        };
        let v = variations(&patch, &both, "x");
        assert_eq!(v.len(), 8);
        let seeds = random_seeds(7, 4);
        assert_eq!(v[5].preset.as_deref(), Some("Storm"));
        assert_eq!(v[5].seed, seeds[1]);
        assert_eq!(v[5].stem, format!("x-storm-seed-{}", seeds[1]));

        // Never more than the cap.
        let many = Vary {
            count: 10_000,
            ..both
        };
        assert_eq!(variations(&patch, &many, "x").len(), MAX_VARIATIONS);
    }

    #[test]
    fn same_named_presets_get_numbered_files() {
        let mut patch = sample();
        patch.presets[1].name = "Calm".into();
        patch.presets[1].seed = 0;
        let v = variations(
            &patch,
            &Vary {
                seeds: SeedMode::Keep,
                first: 0,
                count: 1,
                presets: vec![0, 1],
            },
            "x",
        );
        assert_eq!(v[0].stem, "x-calm-seed-0");
        assert_eq!(v[1].stem, "x-calm-seed-0-2");
    }

    #[test]
    fn applying_a_variation_recalls_its_preset_and_seed() {
        let mut patch = sample();
        let storm = variations(
            &patch,
            &Vary {
                seeds: SeedMode::Range,
                first: 99,
                count: 1,
                presets: vec![1],
            },
            "x",
        )[0]
        .clone();
        let mut want = patch.clone();
        let p = want.presets[1].clone();
        let (fields, _) = floret_engine::preset::morph(Catalogue::builtin(), &want, &p, &p, 0.0);
        floret_engine::preset::apply(&mut want, &fields);
        want.seed = 99;
        assert!(apply(&mut patch, &storm));
        assert_eq!(
            serde_json::to_value(&patch).unwrap(),
            serde_json::to_value(&want).unwrap()
        );
        // Already there: nothing changes.
        assert!(!apply(&mut patch, &storm));
    }

    #[test]
    fn contact_sheet_layout_is_a_tidy_grid() {
        let l = SheetLayout::new(12, (1080, 1080), 8192);
        assert_eq!((l.cols, l.rows), (4, 3));
        assert_eq!(l.cell, (CELL, CELL));
        assert_eq!(
            l.size,
            (
                GAP + 4 * (CELL + GAP),
                HEADER_H + 3 * (CELL + LABEL_H + GAP)
            )
        );
        assert_eq!(l.cell_at(0), (GAP, HEADER_H));
        assert_eq!(
            l.cell_at(5),
            (GAP + CELL + GAP, HEADER_H + CELL + LABEL_H + GAP)
        );
        // Every cell (and its label) inside the sheet, none overlapping.
        for i in 0..12 {
            let (x, y) = l.cell_at(i);
            assert!(x + l.cell.0 + GAP <= l.size.0 && y + l.cell.1 + LABEL_H + GAP <= l.size.1);
            for j in 0..i {
                let (a, b) = l.cell_at(j);
                assert!(
                    x >= a + l.cell.0 || y >= b + l.cell.1 + LABEL_H,
                    "{i} vs {j}"
                );
            }
        }
        // A tall canvas: tall cells, at least as many columns as rows.
        let t = SheetLayout::new(12, (1080, 1920), 8192);
        assert!(t.cell.1 > t.cell.0 && t.cols >= t.rows, "{t:?}");
        assert_eq!(t.cell.1, CELL);
        // Thumbnails of another shape are centred in the box.
        let (x, y, w, h) = l.thumb_rect(0, (200, 100));
        assert_eq!((w, h), (CELL, CELL / 2));
        assert_eq!((x, y), (GAP, HEADER_H + CELL / 4));
        // Hundreds of variations shrink to fit the texture limit.
        let big = SheetLayout::new(400, (1920, 1080), 4096);
        assert!(big.size.0 <= 4096 && big.size.1 <= 4096, "{big:?}");
        assert!(big.cols * big.rows >= 400);
        // One variation: one cell.
        assert_eq!(SheetLayout::new(1, (100, 100), 8192).cols, 1);
    }

    #[test]
    fn downscale_averages_boxes() {
        // 2 × 2 → 1 × 1: the mean.
        let px = [
            0, 0, 0, 255, 100, 100, 100, 255, 200, 200, 200, 255, 100, 100, 100, 255,
        ];
        assert_eq!(downscale(&px, (2, 2), (1, 1)), vec![100, 100, 100, 255]);
        // Same size: unchanged.
        assert_eq!(downscale(&px, (2, 2), (2, 2)), px.to_vec());
        // Never upscales.
        assert_eq!(downscale(&px, (2, 2), (4, 4)).len(), 16);
    }

    #[test]
    fn capture_frames_round_and_clamp() {
        assert_eq!(capture_frame(2.0, 30, None), 60);
        assert_eq!(capture_frame(0.01, 30, None), 0);
        assert_eq!(capture_frame(-1.0, 30, None), 0);
        assert_eq!(capture_frame(10.0, 15, Some(60)), 59);
    }

    #[test]
    fn fresh_dirs_never_reuse_a_folder() {
        let root = std::env::temp_dir().join(format!("floret-var-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let a = fresh_dir(&root, "x");
        assert_eq!(a, root.join("x-variations"));
        std::fs::create_dir(&a).unwrap();
        assert_eq!(fresh_dir(&root, "x"), root.join("x-variations-2"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Scene of frame `k` of a variation's offline run.
    fn scene_at(patch: &PatchFile, v: &Variation, k: usize) -> String {
        let mut p = patch.clone();
        apply(&mut p, v);
        let mut off = Offline::new(&p, "").unwrap();
        let dt = 1.0 / f64::from(STILL_FPS);
        let mut last = String::new();
        for i in 0..=k {
            last = format!("{:?}", off.frame(i, dt));
        }
        last
    }

    #[test]
    fn seeds_give_distinct_repeatable_pictures() {
        let patch = sample();
        let vary = Vary {
            seeds: SeedMode::Random,
            first: 1,
            count: 3,
            presets: Vec::new(),
        };
        let v = variations(&patch, &vary, "x");
        let a: Vec<String> = v.iter().map(|v| scene_at(&patch, v, 3)).collect();
        let b: Vec<String> = v.iter().map(|v| scene_at(&patch, v, 3)).collect();
        assert_eq!(a, b, "deterministic");
        assert_ne!(a[0], a[1]);
        assert_ne!(a[1], a[2]);
        assert_ne!(a[0], a[2]);
    }

    /// The whole export on a real GPU (skipped without an adapter).
    #[test]
    fn exports_stills_and_a_contact_sheet_on_the_gpu() {
        let Some(gpu) = recording::tests::gpu() else {
            eprintln!("no GPU adapter; skipping");
            return;
        };
        let patch = sample();
        let root = std::env::temp_dir().join(format!("floret-var-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let vary = Vary {
            seeds: SeedMode::Range,
            first: 10,
            count: 3,
            presets: Vec::new(),
        };
        let spec = |dir: PathBuf| Spec {
            patch: patch.clone(),
            renderer: String::new(),
            variations: variations(&patch, &vary, "variations"),
            output: Output::Still,
            at: 0.2,
            scale: 0.1,
            gif_fps: 15,
            dir,
            sheet: Some(("variations-contact-sheet".into(), "Variations".into())),
        };
        let (progress, cancel) = (AtomicUsize::new(0), AtomicBool::new(false));
        let dir = root.join("a");
        let done = run(&spec(dir.clone()), &gpu, &progress, &cancel).unwrap();
        assert_eq!(progress.load(Ordering::Relaxed), 4);
        assert!(
            done.message.contains("3 PNGs and a contact sheet"),
            "{}",
            done.message
        );
        assert_eq!(done.thumbs.len(), 3);
        let read = |p: &Path| {
            let dec = png::Decoder::new(std::io::BufReader::new(File::open(p).unwrap()));
            let mut r = dec.read_info().unwrap();
            let mut buf = vec![0; r.output_buffer_size().unwrap()];
            let info = r.next_frame(&mut buf).unwrap();
            ((info.width, info.height), buf)
        };
        let shots: Vec<_> = (10..13)
            .map(|s| read(&dir.join(format!("variations-seed-{s}.png"))))
            .collect();
        assert_ne!(shots[0].1, shots[1].1, "seeds differ");
        let (sheet_size, _) = read(&dir.join("variations-contact-sheet.png"));
        let max = gpu.device.limits().max_texture_dimension_2d.min(8192);
        assert_eq!(sheet_size, SheetLayout::new(3, shots[0].0, max).size);

        // The same export again: the same pictures.
        let again = root.join("b");
        run(&spec(again.clone()), &gpu, &progress, &cancel).unwrap();
        assert_eq!(read(&again.join("variations-seed-11.png")), shots[1]);

        // One-loop GIFs: a whole loop per variation, no sheet.
        let gifs = root.join("gif");
        let gif_spec = Spec {
            output: Output::Gif,
            gif_fps: 10,
            sheet: None,
            variations: spec(gifs.clone()).variations[..1].to_vec(),
            ..spec(gifs.clone())
        };
        let done = run(&gif_spec, &gpu, &progress, &cancel).unwrap();
        assert!(done.message.contains("1 GIF in"), "{}", done.message);
        let file = File::open(gifs.join("variations-seed-10.gif")).unwrap();
        let mut dec = gif::DecodeOptions::new().read_info(file).unwrap();
        let mut frames = 0;
        while dec.read_next_frame().unwrap().is_some() {
            frames += 1;
        }
        assert_eq!(frames, recording::plan(patch.loop_seconds(), 1, 10).frames);

        // Cancelled: nothing left behind.
        cancel.store(true, Ordering::Relaxed);
        let gone = root.join("c");
        assert_eq!(
            run(&spec(gone.clone()), &gpu, &progress, &cancel).err(),
            Some(CANCELLED.to_owned())
        );
        assert!(!gone.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
