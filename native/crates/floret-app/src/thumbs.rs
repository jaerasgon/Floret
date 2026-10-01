//! Sample thumbnails (#75): each sample run for [`FRAMES`] frames on the
//! engine, its scene rendered offscreen by `floret-render` at thumbnail
//! size, and cached as PNGs in `<data dir>/thumbnails/` (keyed by the
//! sample's contents and the app version). Rendering runs on a worker
//! thread with the app's GPU device; thumbnails appear as they finish.

use crate::{Sample, SAMPLES};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use eframe::egui_wgpu::{self, wgpu};
use floret_engine::scene::SceneNode;
use floret_engine::{Engine, FrameCtx};
use floret_render::{RenderClock, Renderer};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;

/// The frame each thumbnail shows (at 60 fps).
pub const FRAMES: u32 = 90;
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

/// FNV-1a over the sample and the app version.
fn key(s: &Sample) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.flora.bytes().chain(env!("CARGO_PKG_VERSION").bytes()) {
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
    for s in SAMPLES {
        let path = dir.join(key(s));
        let img = read_png(&path).or_else(|| {
            let r = renderer.get_or_insert_with(|| {
                Renderer::new(&device, &queue, floret_render::OFFSCREEN_FORMAT, samples)
            });
            let img = render_sample(&device, &queue, r, s.flora)?;
            let _ = std::fs::create_dir_all(dir);
            let _ = write_png(&path, &img);
            Some(img)
        });
        if tx.send((s.id, img)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

/// Run a patch for [`FRAMES`] frames and render its scene at thumbnail
/// size. `None` when it has no renderer or anything panics / errors.
pub fn render_sample(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    flora: &str,
) -> Option<Rgba> {
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Off the UI thread: video nodes may wait for their frames.
        let _wait = floret_engine::video::wait_for_frames();
        let (mut engine, _) = Engine::load_with_warnings(flora).ok()?;
        engine.set_host(Some(crate::host::shared()));
        let r = engine.graph.renderer()?;
        let id = engine.graph.nodes[r].id.clone();
        let size = floret_render::canvas_size(
            engine.params.get(&id, "width"),
            engine.params.get(&id, "height"),
        );
        let dt = 1.0 / 60.0;
        let mut out = None;
        let mut t = 0.0;
        for f in 0..FRAMES {
            t = f64::from(f) * dt;
            let ctx = FrameCtx {
                t,
                dt,
                frame_count: f64::from(f),
                width: f64::from(size.0),
                height: f64::from(size.1),
                ..Default::default()
            };
            out = engine.tick(&ctx).outputs.get(r).cloned().flatten();
        }
        let out = out?;
        let layers = out
            .first()
            .map(|s| floret_render::flatten_layers(s))
            .unwrap_or_default();
        let bg = out
            .get(1)
            .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
        let scale = f64::from(SIZE) / f64::from(size.0.max(size.1));
        let w = ((f64::from(size.0) * scale).round() as u32).max(1);
        let h = ((f64::from(size.1) * scale).round() as u32).max(1);
        let layers = vec![Arc::new(SceneNode::Group {
            children: Arc::from(layers),
            transform: Some([scale, 0.0, 0.0, scale, 0.0, 0.0]),
            blend_mode: None,
            clip: None,
        })];
        renderer.set_clock(RenderClock {
            time: t,
            time_delta: dt,
            frame: f64::from(FRAMES - 1),
            ..Default::default()
        });
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let (px, _) = floret_render::render_and_read(device, queue, renderer, &layers, (w, h), bg);
        if pollster::block_on(scope.pop()).is_some() {
            return None;
        }
        Some((w, h, px))
    }));
    run.ok().flatten()
}

fn write_png(path: &Path, (w, h, px): &Rgba) -> Result<(), String> {
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
    let file = std::fs::File::open(path).ok()?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file))
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
}
