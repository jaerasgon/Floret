//! The two camera-driven particle samples with a fake provider: `hand-sparks`
//! (the tracked hands' speed sets how many sparks fly) and `camera-embers`
//! (the camera's bright parts are where the embers start).
//!
//! The provider is process-wide, so this is its own test binary and the
//! tests take turns.

use floret_engine::devices::{
    self, CameraFrame, CameraRequest, Devices, LandmarkLayer, LandmarksRequest,
};
use floret_engine::image::Image;
use floret_engine::landmarks::Tracker;
use floret_engine::particles::Handle;
use floret_engine::{Engine, FrameCtx, Value};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// What the fake devices report right now.
#[derive(Default)]
struct State {
    /// Hands seen: (wrist x, wrist y) in normalised image coordinates.
    hands: Vec<[f32; 2]>,
    /// Camera frame, 64 × 48 RGBA; `None` = camera off.
    picture: Option<Arc<Image>>,
}

#[derive(Default)]
struct Fake(Mutex<State>);

/// 21 points of a small hand about `(x, y)`.
fn hand(x: f32, y: f32) -> Vec<[f32; 3]> {
    (0..21)
        .map(|i| {
            let (c, r) = ((i % 5) as f32, (i / 5) as f32);
            [x + c * 0.01, y + r * 0.01, 0.0]
        })
        .collect()
}

impl Devices for Fake {
    fn camera(&self, _: &str, req: &CameraRequest) -> Result<CameraFrame, String> {
        let picture = self.0.lock().unwrap().picture.clone();
        Ok(match (req.start, picture) {
            (true, Some(image)) => CameraFrame { image, ready: true },
            _ => CameraFrame {
                image: Image::placeholder(),
                ready: false,
            },
        })
    }

    fn landmarks(
        &self,
        _: &str,
        req: &LandmarksRequest,
    ) -> Result<Option<Vec<LandmarkLayer>>, String> {
        let hands = self.0.lock().unwrap().hands.clone();
        if !req.start || hands.is_empty() {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(vec![LandmarkLayer {
            tracker: Tracker::Hands,
            ids: (0..hands.len() as u32).collect(),
            dets: hands.iter().map(|h| hand(h[0], h[1])).collect(),
        }]))
    }
}

fn fake() -> Arc<Fake> {
    static FAKE: OnceLock<Arc<Fake>> = OnceLock::new();
    let f = FAKE.get_or_init(|| Arc::new(Fake::default())).clone();
    devices::install(Some(f.clone()));
    f
}

/// Tests share the provider: one at a time.
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn load(id: &str) -> Engine {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../samples/{id}.flora"));
    let (engine, warnings) =
        Engine::load_with_warnings(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(warnings.is_empty(), "{id}: {warnings:?}");
    engine
}

fn frame(n: u32) -> FrameCtx {
    FrameCtx {
        t: f64::from(n) / 60.0,
        dt: 1.0 / 60.0,
        frame_count: f64::from(n),
        width: 600.0,
        height: 600.0,
        ..FrameCtx::default()
    }
}

fn number(engine: &Engine, out: &floret_engine::tick::Outputs, node: &str, port: usize) -> f64 {
    let idx = engine.graph.idx(node).unwrap();
    out[idx].as_ref().unwrap()[port]
        .first()
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("{node} has no number on port {port}"))
}

/// Live particles an Emitter holds.
fn alive(engine: &Engine, out: &floret_engine::tick::Outputs, node: &str) -> i64 {
    let idx = engine.graph.idx(node).unwrap();
    match out[idx].as_ref().unwrap()[0].first() {
        Some(Value::Handle(Handle::Particles(b))) => b.read().count,
        other => panic!("{node}: not particles: {other:?}"),
    }
}

/// Run `frames` frames with the hands moved by `step` (normalised image
/// units) every frame, back and forth; the emit rate at the end and the
/// sparks alive.
fn wave(step: f32, frames: u32) -> (f64, i64) {
    let f = fake();
    let mut engine = load("hand-sparks");
    let mut out = Default::default();
    for n in 0..frames {
        let dx = if n % 2 == 0 { step } else { -step };
        f.0.lock().unwrap().hands = vec![[0.3 + dx, 0.5], [0.7 - dx, 0.5]];
        let r = engine.tick(&frame(n));
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        out = r.outputs;
    }
    (
        number(&engine, &out, "hs_cap", 0),
        alive(&engine, &out, "hs_emit"),
    )
}

#[test]
fn hands_held_still_throw_almost_nothing() {
    let _g = lock();
    let (rate, sparks) = wave(0.0, 90);
    assert_eq!(rate, 0.0);
    assert_eq!(sparks, 0);
}

#[test]
fn a_waving_hand_throws_sparks_and_a_faster_one_throws_more() {
    let _g = lock();
    // 0.003 of the picture is 1.8 px of the 600 px canvas a frame each way:
    // a slow drift. 0.04 is 24 px a frame each way: a hard shake.
    let (slow_rate, slow) = wave(0.003, 90);
    let (fast_rate, fast) = wave(0.04, 90);
    assert!(slow_rate < 100.0, "slow wave rate {slow_rate}");
    assert!(fast_rate > 300.0, "fast wave rate {fast_rate}");
    assert!(fast > 2 * slow.max(1), "{slow} sparks, then {fast}");
    assert!(fast > 300, "{fast}");
}

#[test]
fn a_hand_walking_into_view_is_not_a_burst() {
    let _g = lock();
    let f = fake();
    let mut engine = load("hand-sparks");
    let mut out = Default::default();
    // No hands, then one hand appears, then a second hand appears, all of
    // them holding still.
    for n in 0..120 {
        f.0.lock().unwrap().hands = match n {
            0..=29 => vec![],
            30..=59 => vec![[0.3, 0.5]],
            _ => vec![[0.3, 0.5], [0.8, 0.4]],
        };
        let r = engine.tick(&frame(n));
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        out = r.outputs;
        let rate = number(&engine, &out, "hs_cap", 0);
        assert!(rate.is_finite() && rate == 0.0, "frame {n}: rate {rate}");
    }
    assert_eq!(alive(&engine, &out, "hs_emit"), 0);
}

#[test]
fn sparks_leave_from_the_hands_and_stop_with_them() {
    let _g = lock();
    let f = fake();
    let mut engine = load("hand-sparks");
    let mut out = Default::default();
    for n in 0..60 {
        let dx = if n % 2 == 0 { 0.04 } else { -0.04 };
        f.0.lock().unwrap().hands = vec![[0.5 + dx, 0.5]];
        let r = engine.tick(&frame(n));
        out = r.outputs;
    }
    assert!(alive(&engine, &out, "hs_emit") > 50);
    // Hands gone: the rate falls to nothing and the sparks burn out.
    f.0.lock().unwrap().hands = vec![];
    for n in 60..60 + 360 {
        out = engine.tick(&frame(n)).outputs;
    }
    assert_eq!(number(&engine, &out, "hs_cap", 0), 0.0);
    assert_eq!(alive(&engine, &out, "hs_emit"), 0);
}

/// A 64 × 48 frame: bright on its left half, dark on its right.
fn half_lit() -> Arc<Image> {
    let mut px = Vec::new();
    for _y in 0..48 {
        for x in 0..64 {
            let v = if x < 32 { 240 } else { 8 };
            px.extend_from_slice(&[v, v, v, 255]);
        }
    }
    Arc::new(Image::new(64, 48, px))
}

fn embers_start(engine: &Engine, out: &floret_engine::tick::Outputs) -> Vec<f64> {
    let idx = engine.graph.idx("ce_pick").unwrap();
    out[idx].as_ref().unwrap()[0]
        .iter()
        .filter_map(|v| match v {
            Value::Vec2(p) => Some(p.x),
            _ => None,
        })
        .collect()
}

#[test]
fn embers_start_where_the_camera_is_bright() {
    let _g = lock();
    let f = fake();
    f.0.lock().unwrap().picture = Some(half_lit());
    let mut engine = load("camera-embers");
    let mut out = Default::default();
    for n in 0..30 {
        let r = engine.tick(&frame(n));
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        out = r.outputs;
    }
    let xs = embers_start(&engine, &out);
    assert!(xs.len() >= 400, "{} points", xs.len());
    // The camera is mirrored in the patch, not in the picture: the bright
    // half of the frame is the left half of the area, x < 0.
    let left = xs.iter().filter(|&&x| x < 0.0).count();
    assert!(
        left * 10 > xs.len() * 9,
        "{left} of {} on the left",
        xs.len()
    );
    assert!(alive(&engine, &out, "ce_emit") > 300);
    f.0.lock().unwrap().picture = None;
}

#[test]
fn without_a_camera_a_stand_in_glow_sheds_embers() {
    let _g = lock();
    let f = fake();
    f.0.lock().unwrap().picture = None;
    let mut engine = load("camera-embers");
    let mut out = Default::default();
    for n in 0..60 {
        let r = engine.tick(&frame(n));
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        out = r.outputs;
    }
    let xs = embers_start(&engine, &out);
    assert!(xs.len() >= 400, "{} points", xs.len());
    // Head and shoulders about the middle of the 720 px area.
    assert!(xs.iter().all(|x| x.abs() < 330.0));
    assert!(alive(&engine, &out, "ce_emit") > 300);
}
