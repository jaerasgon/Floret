//! Device nodes with a fake provider: camera frames, landmarks and MIDI
//! flow through the real samples (`body-tracking`, `webcam-mosaic`,
//! `tut-45-midi-cc`) into what their renderers draw.
//!
//! The provider is process-wide, so this lives in its own test binary and
//! every test installs the same fake.

use floret_engine::devices::{
    self, CameraFrame, CameraRequest, Devices, LandmarkLayer, LandmarksRequest, MidiNote,
};
use floret_engine::image::Image;
use floret_engine::landmarks::Tracker;
use floret_engine::scene::SceneNode;
use floret_engine::{Engine, FrameCtx, Spread, Value};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// A landmarks request as recorded: node, image id, start, trackers, count.
type LandmarkCall = (String, Option<u64>, bool, Vec<Tracker>, usize);

#[derive(Default)]
struct Fake {
    frame: OnceLock<Arc<Image>>,
    camera_calls: Mutex<Vec<(String, bool, bool)>>,
    landmark_calls: Mutex<Vec<LandmarkCall>>,
}

impl Fake {
    /// 64×48: left half white, right half black.
    fn frame(&self) -> Arc<Image> {
        self.frame
            .get_or_init(|| {
                let mut px = Vec::new();
                for _y in 0..48 {
                    for x in 0..64 {
                        let v = if x < 32 { 255 } else { 0 };
                        px.extend_from_slice(&[v, v, v, 255]);
                    }
                }
                Arc::new(Image::new(64, 48, px))
            })
            .clone()
    }
}

impl Devices for Fake {
    fn midi_cc(&self, device: &str, channel: u8, cc: u8) -> Option<u8> {
        (device.is_empty() && channel == 0 && cc == 1).then_some(64)
    }

    fn midi_note(&self, _: &str, _: u8) -> MidiNote {
        MidiNote {
            note: Some(69),
            velocity: 127,
            pressed: true,
        }
    }

    fn camera(&self, node: &str, req: &CameraRequest) -> Result<CameraFrame, String> {
        self.camera_calls
            .lock()
            .unwrap()
            .push((node.to_owned(), req.start, req.mirror));
        Ok(if req.start {
            CameraFrame {
                image: self.frame(),
                ready: true,
            }
        } else {
            CameraFrame {
                image: Image::placeholder(),
                ready: false,
            }
        })
    }

    fn landmarks(
        &self,
        node: &str,
        req: &LandmarksRequest,
    ) -> Result<Option<Vec<LandmarkLayer>>, String> {
        self.landmark_calls.lock().unwrap().push((
            node.to_owned(),
            req.image.as_ref().map(|i| i.id()),
            req.start,
            req.trackers.clone(),
            req.count,
        ));
        // One hand, id 0, its 21 points on a diagonal.
        Ok(Some(vec![LandmarkLayer {
            tracker: Tracker::Hands,
            dets: vec![(0..21)
                .map(|i| [0.25 + i as f32 / 40.0, 0.25 + i as f32 / 40.0, 0.0])
                .collect()],
            ids: vec![0],
        }]))
    }
}

fn fake() -> Arc<Fake> {
    static FAKE: OnceLock<Arc<Fake>> = OnceLock::new();
    let f = FAKE.get_or_init(|| Arc::new(Fake::default())).clone();
    devices::install(Some(f.clone()));
    f
}

fn load(id: &str) -> Engine {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../samples/{id}.flora"));
    Engine::load(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn frame(n: u32) -> FrameCtx {
    FrameCtx {
        t: f64::from(n) / 60.0,
        dt: 1.0 / 60.0,
        frame_count: f64::from(n),
        width: 520.0,
        height: 520.0,
        ..FrameCtx::default()
    }
}

fn port(engine: &Engine, out: &floret_engine::tick::Outputs, node: &str, port: usize) -> Spread {
    let idx = engine.graph.idx(node).unwrap();
    out[idx]
        .as_ref()
        .unwrap_or_else(|| panic!("{node} has no output"))[port]
        .clone()
}

fn kinds(layers: &Spread) -> (usize, usize, usize) {
    let mut k = (0, 0, 0);
    for v in layers.iter() {
        match v {
            Value::Scene(s) => match &**s {
                SceneNode::Line { .. } => k.0 += 1,
                SceneNode::Ellipse { .. } => k.1 += 1,
                SceneNode::Rect { .. } => k.2 += 1,
                _ => {}
            },
            other => panic!("not a layer: {other:?}"),
        }
    }
    k
}

#[test]
fn body_tracking_draws_the_providers_landmarks() {
    let fake = fake();
    let mut engine = load("body-tracking");
    let mut r = engine.tick(&frame(0));
    for n in 1..3 {
        r = engine.tick(&frame(n));
    }
    assert!(r.errors.is_empty(), "{:?}", r.errors);

    // The camera ran with the sample's settings and output our frame.
    let (node, start, mirror) = fake.camera_calls.lock().unwrap().last().cloned().unwrap();
    assert_eq!((node.as_str(), start, mirror), ("bt_cam", true, true));
    let out = &r.outputs;
    assert!(
        matches!(&port(&engine, out, "bt_cam", 0)[0], Value::Image(i) if i.id() == fake.frame().id())
    );
    assert!(matches!(
        port(&engine, out, "bt_cam", 1)[0],
        Value::Bool(true)
    ));
    assert!(matches!(port(&engine, out, "bt_cam", 2)[0], Value::Num(w) if w == 64.0));

    // The tracker was handed that frame.
    let call = fake.landmark_calls.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        call,
        (
            "bt_lm".into(),
            Some(fake.frame().id()),
            true,
            vec![Tracker::Hands],
            2
        )
    );

    // 21 points / edges, then one line per edge and a dot per point.
    assert_eq!(port(&engine, out, "bt_lm", 0).len(), 21);
    assert_eq!(port(&engine, out, "bt_lm", 4).len(), 21);
    assert!(matches!(port(&engine, out, "bt_lm", 10)[0], Value::Num(d) if d == 1.0));
    let layers = r.layers(engine.graph.idx("bt_renderer").unwrap());
    assert_eq!(kinds(&layers), (21, 21, 0));
    // Mirrored, centred on the 520 px canvas: the first point (x = 0.25)
    // lands right of centre.
    let Value::Scene(first) = &layers[21] else {
        panic!()
    };
    let SceneNode::Ellipse { x, .. } = &**first else {
        panic!()
    };
    assert!(*x > 0.0, "{x}");
}

#[test]
fn webcam_mosaic_samples_the_camera_frame() {
    fake();
    let mut engine = load("webcam-mosaic");
    let mut r = engine.tick(&frame(0));
    for n in 1..3 {
        r = engine.tick(&frame(n));
    }
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    // 36×36 cells: light cells (left half) become circles, dark squares.
    let layers = r.layers(engine.graph.idx("wm_renderer").unwrap());
    let (_, circles, squares) = kinds(&layers);
    assert_eq!(circles + squares, 36 * 36);
    assert_eq!(circles, squares);
}

#[test]
fn midi_values_reach_the_patch() {
    fake();
    let mut engine = load("tut-45-midi-cc");
    let r = engine.tick(&frame(0));
    let value = port(&engine, &r.outputs, "t45_cc", 0);
    let raw = port(&engine, &r.outputs, "t45_cc", 1);
    assert!(matches!(value[0], Value::Num(v) if (v - 64.0 / 127.0).abs() < 1e-12));
    assert!(matches!(raw[0], Value::Num(v) if v == 64.0));
}
