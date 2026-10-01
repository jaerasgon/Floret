//! The `video-pixelate` sample end to end with the real decoder: its
//! embedded clip plays through `image/pixelate` into the renderer.
#![cfg(feature = "openh264")]

use floret_engine::scene::SceneNode;
use floret_engine::video::wait_for_frames;
use floret_engine::{Engine, FrameCtx, Value};
use std::path::Path;

#[test]
fn video_pixelate_sample_plays() {
    floret_video::install();
    let _wait = wait_for_frames();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/video-pixelate.flora");
    let (mut engine, warnings) =
        Engine::load_with_warnings(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let video = engine.graph.idx("vp_video").unwrap();
    let draw = engine.graph.idx("vp_draw").unwrap();
    let mut ids = Vec::new();
    for n in 0..3 {
        let r = engine.tick(&FrameCtx {
            t: f64::from(n) / 24.0,
            dt: 1.0 / 24.0,
            frame_count: f64::from(n),
            width: 640.0,
            height: 360.0,
            ..FrameCtx::default()
        });
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let out = r.outputs[video].as_ref().unwrap();
        let Some(Value::Image(img)) = out[0].first() else {
            panic!("no frame")
        };
        assert_eq!((img.width(), img.height()), (192, 108));
        assert!(matches!(out[2].first(), Some(Value::Num(d)) if (d - 4.0).abs() < 1e-9));
        assert!(matches!(out[5].first(), Some(Value::Bool(true))));
        ids.push(img.id());
        let Some(Value::Scene(shape)) = r.outputs[draw].as_ref().unwrap()[0].first().cloned()
        else {
            panic!("no shape")
        };
        assert!(matches!(&*shape, SceneNode::Image { image, .. } if image.is_ready()));
    }
    // A new frame every tick at the clip's own rate.
    ids.dedup();
    assert_eq!(ids.len(), 3);
}
