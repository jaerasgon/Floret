//! `video/*` nodes with a fake provider: what they ask for each tick and
//! how its frames flow on to `draw/image`.
//!
//! The provider is process-wide, so this lives in its own test binary.

use floret_engine::image::Image;
use floret_engine::scene::SceneNode;
use floret_engine::video::{self, Playhead, Seek, VideoFrame, VideoRequest, Videos};
use floret_engine::{Engine, FrameCtx, Value};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// What a request carried (minus the source).
#[derive(Clone, Debug, PartialEq)]
struct Call {
    node: String,
    source: String,
    play: bool,
    looping: bool,
    speed: f64,
    seek: Option<Seek>,
    restart: bool,
}

/// A 4-second video whose frames are 8×6 and grey-level = 60 × time.
#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<Call>>,
    heads: Mutex<HashMap<String, Playhead>>,
}

impl Videos for Fake {
    fn video(&self, node: &str, req: &VideoRequest) -> Result<VideoFrame, String> {
        self.calls.lock().unwrap().push(Call {
            node: node.into(),
            source: req.source.into(),
            play: req.play,
            looping: req.looping,
            speed: req.speed,
            seek: req.seek,
            restart: req.restart,
        });
        if req.source == "broken.mp4" {
            return Err("can't play this".into());
        }
        let mut heads = self.heads.lock().unwrap();
        let time = heads.entry(node.into()).or_default().advance(req, 4.0);
        let v = (time * 60.0) as u8;
        Ok(VideoFrame {
            image: Arc::new(Image::new(8, 6, [v, v, v, 255].repeat(48))),
            time,
            duration: 4.0,
            ready: true,
        })
    }
}

fn fake() -> Arc<Fake> {
    static FAKE: OnceLock<Arc<Fake>> = OnceLock::new();
    let f = FAKE.get_or_init(|| Arc::new(Fake::default())).clone();
    video::install(Some(f.clone()));
    f
}

fn calls_for(f: &Fake, node: &str) -> Vec<Call> {
    let calls = f.calls.lock().unwrap();
    calls.iter().filter(|c| c.node == node).cloned().collect()
}

fn frame(n: u32) -> FrameCtx {
    FrameCtx {
        t: f64::from(n) / 10.0,
        dt: 0.1,
        frame_count: f64::from(n),
        width: 200.0,
        height: 200.0,
        ..FrameCtx::default()
    }
}

/// `video/file` (with `extra` port values) → `draw/image` → renderer,
/// plus a `video/frame-at` at 2 s.
fn patch(id: &str, url: &str, ports: serde_json::Value, scrub: bool) -> Engine {
    let mut edges = vec![
        json!({ "id": "e1", "source": format!("{id}_v"), "target": format!("{id}_d"),
                "sourceHandle": "out-image", "targetHandle": "in-image" }),
        json!({ "id": "e2", "source": format!("{id}_d"), "target": format!("{id}_r"),
                "sourceHandle": "out-shape", "targetHandle": "in-layers" }),
    ];
    let mut nodes = vec![
        json!({ "id": format!("{id}_v"), "defType": "video/file", "position": { "x": 0, "y": 0 },
                "params": { "url": url }, "portValues": ports }),
        json!({ "id": format!("{id}_f"), "defType": "video/frame-at", "position": { "x": 0, "y": 300 },
                "params": { "url": url }, "portValues": { "time": 2 } }),
        json!({ "id": format!("{id}_d"), "defType": "draw/image", "position": { "x": 300, "y": 0 },
                "params": {}, "portValues": { "w": 80, "h": 60 } }),
        json!({ "id": format!("{id}_r"), "defType": "render/renderer", "position": { "x": 600, "y": 0 },
                "params": { "width": 200, "height": 200 }, "portValues": {} }),
    ];
    if scrub {
        nodes.push(json!({ "id": format!("{id}_s"), "defType": "io/slider",
            "position": { "x": -300, "y": 0 }, "params": { "value": 0.25 }, "portValues": {} }));
        edges.push(
            json!({ "id": "e3", "source": format!("{id}_s"), "target": format!("{id}_v"),
            "sourceHandle": "out-number", "targetHandle": "in-scrub" }),
        );
    }
    let text = json!({ "kind": "floret.patch", "version": 2, "appVersion": "0.0.1",
        "meta": { "name": id }, "nodes": nodes, "edges": edges })
    .to_string();
    let (engine, warnings) = Engine::load_with_warnings(&text).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    engine
}

fn out(engine: &Engine, r: &floret_engine::TickResult, node: &str, port: usize) -> Value {
    let i = engine.graph.idx(node).unwrap();
    r.outputs[i].as_ref().unwrap()[port]
        .first()
        .cloned()
        .unwrap_or(Value::Undefined)
}

#[test]
fn plays_from_the_provider_into_draw_image() {
    let f = fake();
    let mut e = patch("a", " clip.mp4 ", json!({}), false);
    let mut last = None;
    for n in 0..4 {
        last = Some(e.tick(&frame(n)));
    }
    let r = last.unwrap();
    // Defaults: playing, looping, speed 1, no seek; restart on frame 0.
    let calls = calls_for(&f, "a_v");
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls[0],
        Call {
            node: "a_v".into(),
            source: "clip.mp4".into(),
            play: true,
            looping: true,
            speed: 1.0,
            seek: None,
            restart: true,
        }
    );
    assert!(!calls[1].restart);
    // Three ticks of 0.1 s after the restart.
    let Value::Num(time) = out(&e, &r, "a_v", 1) else {
        panic!()
    };
    assert!((time - 0.3).abs() < 1e-9, "{time}");
    assert_eq!(out(&e, &r, "a_v", 2).as_f64(), Some(4.0));
    assert_eq!(out(&e, &r, "a_v", 3).as_f64(), Some(8.0));
    assert_eq!(out(&e, &r, "a_v", 4).as_f64(), Some(6.0));
    assert!(matches!(out(&e, &r, "a_v", 5), Value::Bool(true)));
    let Value::Image(img) = out(&e, &r, "a_v", 0) else {
        panic!("no image")
    };
    assert_eq!(img.pixel(0, 0)[0], 18);
    // The frame reaches the scene.
    let Value::Scene(shape) = out(&e, &r, "a_d", 0) else {
        panic!("no shape")
    };
    assert!(matches!(&*shape, SceneNode::Image { image, .. } if image.id() == img.id()));

    // frame-at: a paused seek to 2 s.
    let at = calls_for(&f, "a_f");
    assert!(at
        .iter()
        .all(|c| !c.play && c.seek == Some(Seek::Seconds(2.0))));
    assert_eq!(out(&e, &r, "a_f", 1).as_f64(), Some(4.0));
    let Value::Image(img) = out(&e, &r, "a_f", 0) else {
        panic!()
    };
    assert_eq!(img.pixel(0, 0)[0], 120);
}

#[test]
fn scrub_overrides_and_pins_reach_the_request() {
    let f = fake();
    let mut e = patch(
        "b",
        "clip.mp4",
        json!({ "play": false, "loop": false, "speed": -2 }),
        true,
    );
    let r = e.tick(&frame(5));
    let c = calls_for(&f, "b_v").pop().unwrap();
    assert!(!c.play && !c.looping);
    assert_eq!(c.speed, -2.0);
    assert_eq!(c.seek, Some(Seek::Position(0.25)));
    assert_eq!(out(&e, &r, "b_v", 1).as_f64(), Some(1.0));
}

#[test]
fn errors_and_empty_sources() {
    let f = fake();
    let mut e = patch("c", "broken.mp4", json!({}), false);
    let r = e.tick(&frame(0));
    let v = e.graph.idx("c_v").unwrap();
    assert!(r
        .errors
        .iter()
        .any(|(i, msg)| *i == v && msg.contains("can't play")));

    // No file picked: not ready, and the provider isn't asked.
    let mut e = patch("d", "", json!({}), false);
    let r = e.tick(&frame(0));
    assert!(calls_for(&f, "d_v").is_empty());
    assert!(matches!(out(&e, &r, "d_v", 5), Value::Bool(false)));
    assert_eq!(out(&e, &r, "d_v", 3).as_f64(), Some(0.0));
    let Value::Image(img) = out(&e, &r, "d_v", 0) else {
        panic!()
    };
    assert!(!img.is_ready());
}
