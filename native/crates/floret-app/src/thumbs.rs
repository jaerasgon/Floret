//! Sample thumbnails (#75): each sample run on the engine to its cover
//! frame ([`FRAMES`], or its catalogue `coverFrame`), played by its
//! scripted performance when the catalogue says `coverPlay`, its scene
//! rendered offscreen by `floret-render` at thumbnail size, and cached as
//! PNGs in `<data dir>/thumbnails/` (keyed by the sample's contents, its
//! cover settings, [`COVER_VERSION`] and the app version). Samples that
//! draw what a camera, microphone or file gives them bundle a cover
//! instead (`samples/covers/<id>.png`). Rendering runs on a worker thread
//! with the app's GPU device; thumbnails appear as they finish.

use crate::{sample_scripts, Sample, SAMPLES};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use eframe::egui_wgpu::{self, wgpu};
use floret_engine::scene::SceneNode;
use floret_engine::Engine;
use floret_render::{RenderClock, Renderer};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;

/// The frame each thumbnail shows (at 60 fps) unless the sample's
/// `coverFrame` says otherwise.
pub const FRAMES: u32 = 90;
/// How covers are made. Part of the cache key: bump it when a change
/// would give a sample a different cover, so cached ones re-render.
/// 2: per-sample cover frames, scripted play and bundled covers.
const COVER_VERSION: u32 = 2;
/// Frames rendered up to the cover frame, so Echo trails have some
/// history. Patches with a GPU shader render every frame: their buffers
/// feed back into themselves frame by frame.
const HISTORY: u32 = 12;
/// Longest side, pixels.
pub const SIZE: u32 = 240;

type Rgba = (u32, u32, Vec<u8>);

#[derive(Default)]
pub struct Thumbnails {
    textures: HashMap<&'static str, TextureHandle>,
    /// Samples that couldn't be rendered.
    failed: Vec<&'static str>,
    rx: Option<mpsc::Receiver<(&'static str, Option<Rgba>)>>,
    started: bool,
}

impl Thumbnails {
    pub fn get(&self, id: &str) -> Option<&TextureHandle> {
        self.textures.get(id)
    }

    /// Still being made (as opposed to failed).
    pub fn pending(&self, id: &str) -> bool {
        self.started && !self.textures.contains_key(id) && !self.failed.contains(&id)
    }

    /// Start the worker (once) and collect finished thumbnails.
    pub fn update(&mut self, ctx: &egui::Context, rs: Option<&egui_wgpu::RenderState>) {
        if !self.started {
            self.started = true;
            let Some(rs) = rs else {
                self.failed = SAMPLES.iter().map(|s| s.id).collect();
                return;
            };
            let (tx, rx) = mpsc::channel();
            self.rx = Some(rx);
            let gpu = (rs.device.clone(), rs.queue.clone(), msaa(&rs.adapter));
            let dir = cache_dir();
            let repaint = ctx.clone();
            let spawned = std::thread::Builder::new()
                .name("floret-thumbnails".into())
                .spawn(move || worker(gpu, &dir, tx, repaint));
            if spawned.is_err() {
                self.failed = SAMPLES.iter().map(|s| s.id).collect();
            }
        }
        let Some(rx) = &self.rx else { return };
        while let Ok((id, img)) = rx.try_recv() {
            match img {
                Some((w, h, px)) => {
                    let image = ColorImage::from_rgba_premultiplied([w as usize, h as usize], &px);
                    let tex =
                        ctx.load_texture(format!("thumb-{id}"), image, TextureOptions::LINEAR);
                    self.textures.insert(id, tex);
                }
                None => self.failed.push(id),
            }
        }
    }
}

fn cache_dir() -> PathBuf {
    crate::files::data_dir().join("thumbnails")
}

fn msaa(adapter: &wgpu::Adapter) -> u32 {
    let ok = [
        floret_render::OFFSCREEN_FORMAT,
        floret_render::STENCIL_FORMAT,
    ]
    .iter()
    .all(|f| {
        adapter
            .get_texture_format_features(*f)
            .flags
            .sample_count_supported(4)
    });
    if ok {
        4
    } else {
        1
    }
}

/// The frame `s`'s cover shows (1-based, at 60 fps).
pub fn cover_frame(s: &Sample) -> u32 {
    if s.cover_frame > 0 {
        s.cover_frame
    } else {
        FRAMES
    }
}

/// FNV-1a over the sample, how its cover is made and the app version.
fn key(s: &Sample) -> String {
    let how = format!(
        "{COVER_VERSION}:{}:{}:{}",
        cover_frame(s),
        s.cover_play,
        env!("CARGO_PKG_VERSION")
    );
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.flora.bytes().chain(how.bytes()) {
        h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
    }
    format!("{}-{h:016x}.png", s.id)
}

fn worker(
    (device, queue, samples): (wgpu::Device, wgpu::Queue, u32),
    dir: &Path,
    tx: mpsc::Sender<(&'static str, Option<Rgba>)>,
    ctx: egui::Context,
) {
    let mut renderer = None;
    // Samples that download a picture go last: they may wait for it.
    let (late, early): (Vec<&'static Sample>, Vec<&'static Sample>) =
        SAMPLES.iter().partition(|s| downloads(s.flora));
    for s in early.into_iter().chain(late) {
        let path = dir.join(key(s));
        let bundled = s.cover.and_then(decode_png);
        let img = bundled.or_else(|| read_png(&path)).or_else(|| {
            let r = renderer.get_or_insert_with(|| {
                Renderer::new(&device, &queue, floret_render::OFFSCREEN_FORMAT, samples)
            });
            let (img, downloaded) = render_cover(&device, &queue, r, s, crate::host::shared())?;
            // Without its picture (offline), try again next launch.
            if downloaded {
                let _ = std::fs::create_dir_all(dir);
                let _ = write_png(&path, &img);
            }
            Some(img)
        });
        if tx.send((s.id, img)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

/// The patch downloads a picture (`io/image-url`).
pub fn downloads(flora: &str) -> bool {
    flora.contains("\"io/image-url\"")
}

/// `s`'s cover: its bundled one, else a render at its cover frame
/// (played by its script when `coverPlay`). `None` as [`render_sample`].
#[cfg(test)]
pub fn cover(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    s: &Sample,
) -> Option<Rgba> {
    match s.cover {
        Some(png) => decode_png(png),
        None => render_cover(device, queue, renderer, s, crate::host::shared()).map(|(img, _)| img),
    }
}

/// Render `s` as its cover (ignoring any bundled one) with `host`
/// loading its files, and whether every picture it downloads arrived in
/// time.
pub(crate) fn render_cover(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    s: &Sample,
    host: Arc<dyn floret_engine::host::Host>,
) -> Option<(Rgba, bool)> {
    let script = s
        .cover_play
        .then(|| sample_scripts::script_for(s.id))
        .flatten();
    run(
        device,
        queue,
        renderer,
        s.flora,
        cover_frame(s),
        script,
        host,
    )
}

/// Run a patch for [`FRAMES`] frames and render its scene at thumbnail
/// size. `None` when it has no renderer or anything panics / errors.
#[cfg(test)]
pub fn render_sample(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    flora: &str,
) -> Option<Rgba> {
    run(
        device,
        queue,
        renderer,
        flora,
        FRAMES,
        None,
        crate::host::shared(),
    )
    .map(|(img, _)| img)
}

/// Audio frames per 60 fps tick when sound runs offline for a cover.
const AUDIO_RATE: f32 = 48_000.0;
const AUDIO_PER_TICK: usize = 800;
/// How long a cover waits for the pictures its patch downloads.
const DOWNLOAD_WAIT: std::time::Duration = std::time::Duration::from_secs(8);

/// Run a patch for `frames` frames (with `script`'s input, if any) and
/// render its scene at thumbnail size, as [`Cover::render`]. A patch with
/// sound nodes runs with the audio engine offline (no device), so what it
/// analyses is there. `None` when it has no renderer or anything panics /
/// errors.
fn run(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    flora: &str,
    frames: u32,
    script: Option<sample_scripts::Script>,
    host: Arc<dyn floret_engine::host::Host>,
) -> Option<(Rgba, bool)> {
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Off the UI thread: video nodes may wait for their frames.
        let _wait = floret_engine::video::wait_for_frames();
        let (mut engine, _) = Engine::load_with_warnings(flora).ok()?;
        engine.set_host(Some(host));
        let audio = flora
            .contains("\"sound/")
            .then(|| Arc::new(floret_media::AudioSystem::offline(AUDIO_RATE)));
        if let Some(a) = &audio {
            engine.set_audio(Some(a.clone()));
        }
        let mut cover = Cover::new(&mut engine, frames, script, audio.as_deref());
        let img = cover.render(device, queue, renderer, &mut |_| {})?;
        Some((img, cover.downloaded))
    }));
    run.ok().flatten()
}

/// A loaded patch to run to its cover frame.
pub(crate) struct Cover<'a> {
    engine: &'a mut Engine,
    frames: u32,
    script: Option<sample_scripts::Script>,
    /// Rendered [`AUDIO_PER_TICK`] frames after each tick.
    audio: Option<&'a floret_media::AudioSystem>,
    /// After [`Cover::render`]: every image download arrived in time.
    pub downloaded: bool,
}

impl<'a> Cover<'a> {
    pub(crate) fn new(
        engine: &'a mut Engine,
        frames: u32,
        script: Option<sample_scripts::Script>,
        audio: Option<&'a floret_media::AudioSystem>,
    ) -> Cover<'a> {
        Cover {
            engine,
            frames,
            script,
            audio,
            downloaded: true,
        }
    }

    /// Tick to the cover frame (`before(f)` ahead of each tick) and render
    /// the last [`HISTORY`] frames (every frame for a GPU shader) at
    /// thumbnail size, so effects that keep past frames see them; read
    /// back the last.
    pub(crate) fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut Renderer,
        before: &mut dyn FnMut(u32),
    ) -> Option<Rgba> {
        let frames = self.frames.max(1);
        let r = self.engine.graph.renderer()?;
        let id = self.engine.graph.nodes[r].id.clone();
        let size = floret_render::canvas_size(
            self.engine.params.get(&id, "width"),
            self.engine.params.get(&id, "height"),
        );
        self.downloaded = self.wait_for_downloads(size);
        let shaders = self.engine.graph.nodes.iter().any(|n| n.ty == "gpu/shader");
        let history = if shaders { frames } else { HISTORY };
        let engine = &mut *self.engine;
        let dt = 1.0 / 60.0;
        let scale = f64::from(SIZE) / f64::from(size.0.max(size.1));
        let w = ((f64::from(size.0) * scale).round() as u32).max(1);
        let h = ((f64::from(size.1) * scale).round() as u32).max(1);
        let mut px = None;
        for f in 0..frames {
            before(f);
            let input = self.script.map(|s| s(f)).unwrap_or_default();
            let ctx = sample_scripts::frame_ctx(f, size, &input);
            let out = engine.tick(&ctx).outputs.get(r).cloned().flatten();
            if let Some(a) = self.audio {
                a.render(AUDIO_PER_TICK);
            }
            if f + history < frames {
                continue;
            }
            let Some(out) = out else {
                px = None;
                continue;
            };
            let layers = out
                .first()
                .map(|s| floret_render::flatten_layers(s))
                .unwrap_or_default();
            let bg = out
                .get(1)
                .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
            let layers = vec![Arc::new(SceneNode::Group {
                children: Arc::from(layers),
                transform: Some([scale, 0.0, 0.0, scale, 0.0, 0.0]),
                blend_mode: None,
                clip: None,
            })];
            renderer.set_clock(RenderClock {
                time: ctx.t,
                time_delta: dt,
                frame: f64::from(f),
                ..Default::default()
            });
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let (frame, _) =
                floret_render::render_and_read(device, queue, renderer, &layers, (w, h), bg);
            if pollster::block_on(scope.pop()).is_some() {
                return None;
            }
            px = Some(frame);
        }
        Some((w, h, px?))
    }

    /// Give the patch's image downloads up to [`DOWNLOAD_WAIT`] to arrive,
    /// ticking frame 0 meanwhile, so the cover shows the picture rather
    /// than its placeholder. False when one didn't arrive.
    fn wait_for_downloads(&mut self, size: (u32, u32)) -> bool {
        let engine = &mut *self.engine;
        let loaders: Vec<usize> = (0..engine.graph.nodes.len())
            .filter(|&i| engine.graph.nodes[i].ty == "io/image-url")
            .collect();
        if loaders.is_empty() {
            return true;
        }
        let start = std::time::Instant::now();
        loop {
            let res = engine.tick(&sample_scripts::frame_ctx(0, size, &Default::default()));
            // A loader's second output is "loaded".
            let loaded = loaders.iter().all(|&i| {
                res.outputs
                    .get(i)
                    .cloned()
                    .flatten()
                    .and_then(|o| o.get(1).and_then(|s| s.first().cloned()))
                    .is_some_and(|v| matches!(v, floret_engine::Value::Bool(true)))
            });
            if loaded || start.elapsed() >= DOWNLOAD_WAIT {
                return loaded;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

pub(crate) fn write_png(path: &Path, (w, h, px): &Rgba) -> Result<(), String> {
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, *w, *h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(px))
            .map_err(|e| e.to_string())?;
    }
    crate::files::write_atomic(path, buf).map_err(|e| e.to_string())
}

fn read_png(path: &Path) -> Option<Rgba> {
    decode_png(&std::fs::read(path).ok()?)
}

/// An 8-bit RGBA PNG's pixels (`None` for anything else).
pub(crate) fn decode_png(bytes: &[u8]) -> Option<Rgba> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .ok()?;
    let mut px = vec![0; dec.output_buffer_size()?];
    let info = dec.next_frame(&mut px).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    px.truncate(info.buffer_size());
    Some((info.width, info.height, px))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_sample_and_round_trips_the_cache() {
        let Some(mut hl) = floret_render::Headless::with_sample_count(1) else {
            eprintln!("no GPU adapter: skipping");
            return;
        };
        let s = SAMPLES
            .iter()
            .find(|s| s.id == "tut-01-hello-rect")
            .unwrap();
        let img =
            render_sample(&hl.device, &hl.queue, &mut hl.renderer, s.flora).expect("rendered");
        let (w, h, px) = &img;
        assert_eq!(w.max(h), &SIZE);
        assert_eq!(px.len(), (w * h * 4) as usize);
        // Not a blank frame: some pixel differs from the corner (background).
        let corner = &px[..4];
        assert!(
            px.chunks(4).any(|p| p != corner),
            "thumbnail shows the shape"
        );
        let dir = std::env::temp_dir().join(format!("floret-thumbs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(key(s));
        write_png(&path, &img).unwrap();
        assert_eq!(read_png(&path).as_ref(), Some(&img));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn keys_change_with_contents() {
        let a = &SAMPLES[0];
        let b = &SAMPLES[1];
        assert_ne!(key(a), key(b));
        assert!(key(a).starts_with(a.id));
    }

    #[test]
    fn keys_change_with_how_the_cover_is_made() {
        let s = &SAMPLES[0];
        let later = Sample {
            cover_frame: FRAMES + 60,
            ..*s
        };
        let played = Sample {
            cover_play: true,
            ..*s
        };
        assert_ne!(key(s), key(&later));
        assert_ne!(key(s), key(&played));
        assert_eq!(cover_frame(s), FRAMES);
        assert_eq!(cover_frame(&later), FRAMES + 60);
    }

    /// The app's host, but downloads take a while and give one picture.
    struct SlowNetwork(Vec<u8>);

    impl floret_engine::host::Host for SlowNetwork {
        fn fetch(&self, _: &str) -> Result<Vec<u8>, String> {
            std::thread::sleep(std::time::Duration::from_millis(400));
            Ok(self.0.clone())
        }
    }

    #[test]
    fn covers_wait_for_downloaded_pictures() {
        let Some(mut hl) = floret_render::Headless::with_sample_count(1) else {
            eprintln!("no GPU adapter: skipping");
            return;
        };
        // A bright picture with a dark square in the middle.
        let (w, h) = (64u32, 64u32);
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let dark = (16..48).contains(&x) && (16..48).contains(&y);
                let v = if dark { 30 } else { 230 };
                px.extend_from_slice(&[v, v / 2, 255 - v, 255]);
            }
        }
        let dir = std::env::temp_dir().join(format!("floret-thumbs-dl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("picture.png");
        write_png(&png, &(w, h, px)).unwrap();
        let s = SAMPLES.iter().find(|s| s.id == "tut-37-image-url").unwrap();
        assert!(downloads(s.flora));
        let (mut engine, _) = Engine::load_with_warnings(s.flora).unwrap();
        engine.set_host(Some(Arc::new(SlowNetwork(std::fs::read(&png).unwrap()))));
        let mut cover = Cover::new(&mut engine, cover_frame(s), None, None);
        let (_, _, img) = cover
            .render(&hl.device, &hl.queue, &mut hl.renderer, &mut |_| {})
            .expect("rendered");
        assert!(cover.downloaded, "the picture arrived");
        // The picture shows: both its colours are on the cover.
        let near = |p: &[u8; 4], c: [u8; 3]| (0..3).all(|i| p[i].abs_diff(c[i]) < 40);
        let (pixels, _) = img.as_chunks::<4>();
        assert!(pixels.iter().any(|p| near(p, [230, 115, 25])));
        assert!(pixels.iter().any(|p| near(p, [30, 15, 225])));
        let _ = std::fs::remove_dir_all(dir);
    }
}
