//! The generative-algorithm features on the GPU: the Effect node's ASCII
//! mode, the Symmetry node's masked wedges, and every sample that shows
//! them (flow lines, subdivision, Truchet / WFC tiles, symmetry, growth,
//! reaction–diffusion, ASCII, palette from an image), drawn and checked
//! for a real picture. `FLORET_RENDER_OUT=<dir>` keeps the frames. Skips
//! when no GPU adapter is available.

mod common;

use common::{out_dir, reference_dir, write_png};
use floret_engine::host::Host;
use floret_engine::scene::{default_style, Paint, ResolvedStyle, SceneNode};
use floret_engine::{Engine, FrameCtx, PatchFile};
use floret_render::{Headless, RenderClock};
use std::sync::{Arc, Mutex, OnceLock};

fn gpu() -> Option<std::sync::MutexGuard<'static, Headless>> {
    static GPU: OnceLock<Option<Mutex<Headless>>> = OnceLock::new();
    GPU.get_or_init(|| Headless::new().map(Mutex::new))
        .as_ref()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Every URL serves a 64 × 64 picture in four colour blocks.
struct FakeNet;

impl Host for FakeNet {
    fn fetch(&self, _: &str) -> Result<Vec<u8>, String> {
        let mut rgba = Vec::with_capacity(64 * 64 * 4);
        for y in 0..64 {
            for x in 0..64 {
                rgba.extend_from_slice(&match (x < 32, y < 40) {
                    (true, true) => [230, 60, 50, 255],
                    (false, true) => [250, 220, 120, 255],
                    (true, false) => [30, 60, 120, 255],
                    (false, false) => [60, 160, 110, 255],
                });
            }
        }
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 64, 64);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(&rgba).map_err(|e| e.to_string())?;
        wr.finish().map_err(|e| e.to_string())?;
        Ok(out)
    }
}

/// A sample after `frames` ticks at 60 fps, drawn: (pixels, width, height).
fn draw_sample(g: &mut Headless, name: &str, frames: u32) -> (Vec<u8>, u32, u32) {
    let path = reference_dir().join(format!("../../../samples/{name}.flora"));
    let patch = PatchFile::load(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let (mut engine, _) = Engine::from_patch(&patch).expect("compiles");
    engine.set_host(Some(Arc::new(FakeNet)));
    let r = engine.graph.renderer().expect("a renderer");
    let id = engine.graph.nodes[r].id.clone();
    let (w, h) = floret_render::canvas_size(
        engine.params.get(&id, "width"),
        engine.params.get(&id, "height"),
    );
    let dt = 1.0 / 60.0;
    let frame = |f: u32| FrameCtx {
        t: f64::from(f) * dt,
        dt,
        frame_count: f64::from(f),
        width: f64::from(w),
        height: f64::from(h),
        ..FrameCtx::default()
    };
    let mut outputs = Vec::new();
    for f in 0..frames {
        let t = engine.tick(&frame(f));
        assert!(t.errors.is_empty(), "{name}: {:?}", t.errors);
        outputs = t.outputs;
        // Give a picture loading off the tick time to arrive.
        if f < 20 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let out = outputs[r].as_ref().expect("renderer output");
    let layers = floret_render::flatten_layers(&out[0]);
    let bg = out
        .get(1)
        .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
    let f = frame(frames);
    g.renderer.set_clock(RenderClock {
        time: f.t,
        frame: f.frame_count,
        ..RenderClock::default()
    });
    let (px, stats) = g.render_with_stats(&layers, w, h, bg);
    assert_eq!(stats.skipped, 0, "{name}: skipped shapes");
    assert!(
        g.renderer.shader_errors().is_empty(),
        "{name}: shader errors {:?}",
        g.renderer.shader_errors()
    );
    if let Some(dir) = out_dir() {
        write_png(&dir.join(format!("{name}-f{frames}.png")), w, h, &px);
    }
    (px, w, h)
}

/// Distinct colours (to 4 bits a channel) in a frame.
fn colours(px: &[u8]) -> usize {
    let mut set = std::collections::HashSet::new();
    for p in px.chunks(4) {
        set.insert((p[0] >> 4, p[1] >> 4, p[2] >> 4));
    }
    set.len()
}

/// Share of pixels that differ from the corner (background) colour.
fn covered(px: &[u8]) -> f64 {
    let bg = [px[0], px[1], px[2]];
    let n = px
        .chunks(4)
        .filter(|p| (0..3).any(|i| p[i].abs_diff(bg[i]) > 24))
        .count();
    n as f64 / (px.len() / 4) as f64
}

#[test]
fn every_generative_sample_draws_a_picture() {
    let Some(mut g) = gpu() else { return };
    // (sample, frames, least coverage, least colours)
    let cases = [
        ("flow-lines", 2, 0.12, 5),
        ("mondrian-splits", 2, 0.6, 4),
        ("truchet-tiles", 2, 0.15, 3),
        ("wfc-circuits", 2, 0.05, 4),
        ("kaleido-slime", 120, 0.05, 3),
        ("wallpaper-groups", 2, 0.15, 4),
        ("sim-differential-growth", 240, 0.05, 4),
        ("sim-reaction-diffusion", 180, 0.05, 3),
        ("ascii-cells", 120, 0.02, 3),
        ("palette-from-image", 40, 0.3, 4),
    ];
    for (name, frames, cover, distinct) in cases {
        let (px, _, _) = draw_sample(&mut g, name, frames);
        let (c, d) = (covered(&px), colours(&px));
        assert!(c >= cover, "{name}: covers {c:.3}");
        assert!(d >= distinct, "{name}: {d} colours");
    }
}

#[test]
fn ascii_draws_characters_by_brightness() {
    let Some(mut g) = gpu() else { return };
    // A left-to-right grey ramp under the effect.
    let (w, h) = (240u32, 120u32);
    let ramp: Vec<Arc<SceneNode>> = (0..24)
        .map(|i| {
            let v = (i as f64 / 23.0 * 255.0).round() as u8;
            let st = {
                let mut s: ResolvedStyle = (**default_style()).clone();
                s.fill = Some(Paint::css(&format!("rgb({v},{v},{v})")));
                s.stroke = None;
                Arc::new(s)
            };
            Arc::new(SceneNode::Rect {
                x: -120.0 + i as f64 * 10.0,
                y: -60.0,
                w: 10.0,
                h: 120.0,
                mode: Arc::from("corner"),
                style: st,
            })
        })
        .collect();
    let patch = serde_json::json!({
        "kind": "floret.patch",
        "version": 2,
        "nodes": [{
            "id": "fx", "defType": "gpu/effect", "position": {"x": 0, "y": 0},
            "params": { "mode": "ascii" },
            "portValues": { "size": 20.0 },
        }],
        "edges": [],
    });
    let patch = PatchFile::load(&patch.to_string()).unwrap();
    let (mut engine, _) = Engine::from_patch(&patch).unwrap();
    let t = engine.tick(&FrameCtx::default());
    let fx = engine
        .graph
        .nodes
        .iter()
        .position(|n| n.id == "fx")
        .unwrap();
    let Some(floret_engine::Value::Scene(s)) = t.outputs[fx].as_ref().map(|o| o[0][0].clone())
    else {
        panic!("no layer")
    };
    let SceneNode::Shader(sh) = &*s else {
        panic!("not a shader")
    };
    let mut sh = (**sh).clone();
    sh.input = Some(Arc::new(SceneNode::group(ramp)));
    g.renderer.set_clock(RenderClock::default());
    let px = g.render(&[Arc::new(SceneNode::Shader(Arc::new(sh)))], w, h, [0.0; 4]);
    if let Some(dir) = out_dir() {
        write_png(&dir.join("ascii-ramp.png"), w, h, &px);
    }
    // Ink per 10-px-wide cell column: none on black, the most on white,
    // and never pure blocks (it's characters, with gaps).
    let ink = |x0: u32| {
        let mut sum = 0u32;
        for y in 0..h {
            for x in x0..x0 + 10 {
                sum += u32::from(px[((y * w + x) * 4) as usize]);
            }
        }
        sum
    };
    assert_eq!(ink(0), 0, "black is a space");
    let (dim, bright) = (ink(60), ink(230));
    assert!(bright > dim && dim > 0, "{dim} < {bright}");
    assert!(bright < 10 * h * 255 * 3 / 4, "characters, not blocks");
}
