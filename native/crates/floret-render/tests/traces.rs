//! Render real scenes recorded from the TS app and sanity-check the pixels.
//! GPU tests skip (pass with a note) when no adapter is available.

mod common;

use common::*;
use floret_render::Headless;
use std::sync::{Mutex, OnceLock};

/// One device for the whole test binary (device creation is the slow part).
fn gpu() -> Option<std::sync::MutexGuard<'static, Headless>> {
    static GPU: OnceLock<Option<Mutex<Headless>>> = OnceLock::new();
    let g = GPU.get_or_init(|| {
        let h = Headless::new();
        match &h {
            Some(h) => eprintln!(
                "floret-render: adapter {} ({:?})",
                h.adapter_info.name, h.adapter_info.backend
            ),
            None => eprintln!("floret-render: no GPU adapter — skipping GPU tests"),
        }
        h.map(Mutex::new)
    });
    g.as_ref()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
}

fn render(s: &TraceScene) -> Option<Vec<u8>> {
    let mut g = gpu()?;
    let px = g.render(&s.layers, s.width, s.height, s.background);
    if let Some(dir) = out_dir() {
        write_png(
            &dir.join(format!("{}-f{}.png", s.sample, s.frame)),
            s.width,
            s.height,
            &px,
        );
    }
    Some(px)
}

/// `cargo test --release -p floret-render --test traces -- --ignored --nocapture`
/// CPU batching cost per frame, and a full offscreen frame (incl. readback).
#[test]
#[ignore = "benchmark"]
fn bench_heavy_samples() {
    use floret_engine::scene::IDENTITY;
    use floret_render::{Emitter, FrameBuilder, TextRasterizer};
    use std::time::Instant;
    for id in [
        "tut-43-gol",
        "webcam-mosaic",
        "curl-flow",
        "phyllotaxis",
        "type-kinetic-wave",
        "type-raven",
        "grid-ripple",
    ] {
        let Some(s) = load_trace(id).and_then(|t| scene(&t, 5)) else {
            continue;
        };
        let (mut fb, mut text) = (FrameBuilder::new(), TextRasterizer::new());
        let n = 200;
        let mut t0 = Instant::now();
        for i in 0..=n {
            if i == 1 {
                t0 = Instant::now(); // first frame warms the text cache
            }
            fb.reset();
            let mut e = Emitter {
                fb: &mut fb,
                text: &mut text,
            };
            for l in &s.layers {
                e.emit(l, &IDENTITY);
            }
        }
        let cpu = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
        let gpu_ms = gpu().map(|mut g| {
            g.render(&s.layers, s.width, s.height, s.background);
            let t0 = Instant::now();
            for _ in 0..10 {
                g.render(&s.layers, s.width, s.height, s.background);
            }
            t0.elapsed().as_secs_f64() * 100.0
        });
        eprintln!(
            "{id:20} {:5} sdf {:4} fills {:4} text  batch {cpu:6.3} ms  frame {:?} ms",
            fb.sdf.len(),
            fb.cmds
                .iter()
                .filter(|c| matches!(c, floret_render::Cmd::PolyFill { .. }))
                .count(),
            fb.tex_verts.len() / 6,
            gpu_ms.map(|v| (v * 10.0).round() / 10.0)
        );
    }
}

#[test]
fn renders_every_recorded_sample() {
    if gpu().is_none() {
        return;
    }
    for id in sample_ids() {
        let Some(trace) = load_trace(&id) else {
            continue;
        };
        let Some(s) = scene(&trace, 5).or_else(|| scene(&trace, 0)) else {
            continue;
        };
        let px = render(&s).unwrap();
        assert_eq!(px.len(), (s.width * s.height * 4) as usize, "{id}");
    }
}
