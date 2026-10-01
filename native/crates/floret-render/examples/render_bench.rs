//! Tick and render every sample offscreen; print median ms per frame for
//! the tick, the renderer's CPU work (batching + encoding) and submitting
//! plus waiting for the GPU, slowest first.
//!
//! ```sh
//! cargo run --release -p floret-render --example render_bench -- [--ticks N] [sample …]
//! ```
//!
//! On a software adapter (lavapipe, WARP) the GPU column is CPU time too.

use floret_engine::{Engine, FrameCtx};
use floret_render::{background, canvas_size, flatten_layers, Headless, OFFSCREEN_FORMAT};
use std::path::Path;
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

fn main() {
    let mut ticks = 60usize;
    let mut only = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--ticks" => ticks = args.next().and_then(|v| v.parse().ok()).unwrap_or(ticks),
            "--bench" => {}
            _ => only.push(a),
        }
    }
    let Some(mut gpu) = Headless::new() else {
        eprintln!("no GPU adapter");
        return;
    };
    println!("adapter: {}", gpu.adapter_info.name);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut ids: Vec<String> = std::fs::read_dir(&dir)
        .expect("samples dir")
        .filter_map(|e| {
            e.ok()?
                .file_name()
                .to_str()?
                .strip_suffix(".flora")
                .map(str::to_owned)
        })
        .filter(|id| only.is_empty() || only.iter().any(|o| id.contains(o.as_str())))
        .collect();
    ids.sort();

    let mut rows = Vec::new();
    for id in ids {
        let text = std::fs::read_to_string(dir.join(format!("{id}.flora"))).expect("read");
        let Ok(mut engine) = Engine::load(&text) else {
            continue;
        };
        let Some(r) = engine.graph.renderer() else {
            continue;
        };
        let rid = engine.graph.nodes[r].id.clone();
        let (mut tick_ms, mut encode_ms, mut gpu_ms) = (Vec::new(), Vec::new(), Vec::new());
        let mut shapes = 0;
        for i in 0..ticks.max(2) {
            let size = canvas_size(
                engine.params.get(&rid, "width"),
                engine.params.get(&rid, "height"),
            );
            let frame = FrameCtx {
                t: i as f64 / 60.0,
                dt: 1.0 / 60.0,
                frame_count: i as f64,
                width: f64::from(size.0),
                height: f64::from(size.1),
                ..FrameCtx::default()
            };
            let t0 = Instant::now();
            let result = engine.tick(&frame);
            let t1 = Instant::now();
            let out = result.outputs[r].clone().unwrap_or_default();
            let layers = flatten_layers(out.first().map_or(&[][..], |s| &s[..]));
            let bg = background(out.get(1).map_or(&[][..], |s| &s[..]));
            let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bench target"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: OFFSCREEN_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = target.create_view(&Default::default());
            let t2 = Instant::now();
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            let stats = gpu.renderer.render(
                &gpu.device,
                &gpu.queue,
                &mut encoder,
                &view,
                &layers,
                bg,
                size,
            );
            let t3 = Instant::now();
            gpu.queue.submit([encoder.finish()]);
            let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
            let t4 = Instant::now();
            shapes = stats.sdf_instances + stats.poly_fills + stats.text_quads + stats.image_quads;
            if i > 0 {
                tick_ms.push((t1 - t0).as_secs_f64() * 1e3);
                encode_ms.push((t3 - t2).as_secs_f64() * 1e3);
                gpu_ms.push((t4 - t3).as_secs_f64() * 1e3);
            }
        }
        rows.push((
            id,
            median(tick_ms),
            median(encode_ms),
            median(gpu_ms),
            shapes,
        ));
    }
    rows.sort_by(|a, b| (b.1 + b.2 + b.3).total_cmp(&(a.1 + a.2 + a.3)));
    println!(
        "{:32} {:>9} {:>9} {:>9} {:>9}",
        "sample", "tick ms", "encode ms", "gpu ms", "shapes"
    );
    for (id, t, e, g, s) in rows {
        println!("{id:32} {t:>9.3} {e:>9.3} {g:>9.3} {s:>9}");
    }
}
