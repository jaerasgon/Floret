//! The particle samples, run on the engine at a fixed 60 fps and drawn:
//! every live particle and trail segment becomes an SDF instance and
//! nothing is skipped. `FLORET_RENDER_OUT=<dir>` keeps the frames.
//! Skips when no GPU adapter is available.

mod common;

use common::*;
use floret_engine::{Engine, FrameCtx, PatchFile};
use floret_render::Headless;

const FRAMES: u32 = 90;

#[test]
fn particle_samples_draw_every_particle() {
    let Some(mut gpu) = Headless::new() else {
        eprintln!("floret-render: no GPU adapter — skipping particle samples");
        return;
    };
    let samples = reference_dir().join("../../../samples");
    let mut seen = 0;
    for entry in std::fs::read_dir(&samples).expect("samples") {
        let path = entry.expect("entry").path();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        if !name.starts_with("particles-") {
            continue;
        }
        let patch = PatchFile::load(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let (mut engine, _) = Engine::from_patch(&patch).expect("compiles");
        let r = engine.graph.renderer().expect("a renderer");
        let id = engine.graph.nodes[r].id.clone();
        let (w, h) = floret_render::canvas_size(
            engine.params.get(&id, "width"),
            engine.params.get(&id, "height"),
        );
        let mut outputs = Vec::new();
        for f in 0..FRAMES {
            let dt = 1.0 / 60.0;
            outputs = engine
                .tick(&FrameCtx {
                    t: f64::from(f) * dt,
                    dt,
                    frame_count: f64::from(f),
                    width: f64::from(w),
                    height: f64::from(h),
                    mouse_x: 0.0,
                    mouse_y: 0.0,
                    mouse_is_pressed: false,
                    key: String::new(),
                    key_is_pressed: false,
                })
                .outputs;
        }
        let out = outputs[r].as_ref().expect("renderer output");
        let layers = floret_render::flatten_layers(&out[0]);
        let bg = out
            .get(1)
            .map_or([0.0, 0.0, 0.0, 1.0], |s| floret_render::background(s));
        let (px, stats) = gpu.render_with_stats(&layers, w, h, bg);
        eprintln!("{name:24} {stats:?}");
        assert_eq!(stats.skipped, 0, "{name}: skipped shapes");
        assert!(stats.sdf_instances > 50, "{name}: too few instances drawn");
        if let Some(dir) = out_dir() {
            write_png(&dir.join(format!("{name}-f{FRAMES}.png")), w, h, &px);
        }
        seen += 1;
    }
    assert!(seen >= 4, "particle samples found: {seen}");
}
