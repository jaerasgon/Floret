//! Native-only samples have no TypeScript trace to compare against
//! (`parity.rs` skips them): check each compiles cleanly, ticks without
//! node errors and draws something.

use floret_engine::scene::SceneNode;
use floret_engine::{Engine, FrameCtx, Value};
use std::path::Path;

fn native_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Drawn leaves (shapes, characters, particle batches) under a layer.
fn leaves(n: &SceneNode) -> usize {
    match n {
        SceneNode::Group { children, .. } => children.iter().map(|c| leaves(c)).sum(),
        SceneNode::Glyphs(run) => run.glyphs.len(),
        SceneNode::Text { text, .. } => text.chars().count(),
        _ => 1,
    }
}

#[test]
fn native_samples_compile_tick_and_draw() {
    let mut checked = 0;
    for entry in std::fs::read_dir(native_dir().join("samples")).unwrap() {
        let path = entry.unwrap().path();
        let Some(id) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".flora"))
        else {
            continue;
        };
        if native_dir()
            .join(format!("fixtures/traces/{id}.json.gz"))
            .exists()
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let (mut engine, warnings) = Engine::load_with_warnings(&text).unwrap();
        let msgs: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(msgs.is_empty(), "{id}: {msgs:?}");
        let renderer = engine.graph.renderer().expect("renderer");
        let mut drawn = 0;
        for f in 0..4 {
            let frame = FrameCtx {
                t: f as f64 * 0.5,
                dt: 0.5,
                frame_count: f as f64,
                width: 640.0,
                height: 480.0,
                ..FrameCtx::default()
            };
            let r = engine.tick(&frame);
            assert!(r.errors.is_empty(), "{id}: {:?}", r.errors);
            drawn = r
                .layers(renderer)
                .iter()
                .map(|l| match l {
                    Value::Scene(n) => leaves(n),
                    _ => 0,
                })
                .sum::<usize>();
        }
        assert!(drawn > 10, "{id}: only {drawn} drawn");
        checked += 1;
    }
    assert!(checked >= 3, "checked {checked}");
}
