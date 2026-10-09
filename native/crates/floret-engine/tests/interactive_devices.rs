//! The device-driven Library samples with a fake provider: MIDI notes and
//! knobs drive `play-midi-garden`, a tracked hand plays
//! `sound-hand-theremin`, and both fall back to the mouse / sliders when the
//! devices are silent.
//!
//! The provider is process-wide, so this is its own test binary.

use floret_engine::devices::{
    self, CameraFrame, CameraRequest, Devices, LandmarkLayer, LandmarksRequest, MidiNote,
};
use floret_engine::image::Image;
use floret_engine::landmarks::Tracker;
use floret_engine::{Engine, FrameCtx, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// What the fake devices report right now.
#[derive(Default)]
struct State {
    cc: HashMap<u8, u8>,
    note: Option<(u8, u8)>,
    /// A hand's index fingertip (landmark 8), normalised image coords.
    tip: Option<[f32; 2]>,
}

#[derive(Default)]
struct Fake(Mutex<State>);

impl Devices for Fake {
    fn midi_cc(&self, _: &str, _: u8, cc: u8) -> Option<u8> {
        self.0.lock().unwrap().cc.get(&cc).copied()
    }

    fn midi_note(&self, _: &str, _: u8) -> MidiNote {
        match self.0.lock().unwrap().note {
            Some((note, velocity)) => MidiNote {
                note: Some(note),
                velocity,
                pressed: true,
            },
            None => MidiNote::default(),
        }
    }

    fn camera(&self, _: &str, req: &CameraRequest) -> Result<CameraFrame, String> {
        Ok(CameraFrame {
            image: if req.start {
                Arc::new(Image::new(64, 48, vec![90; 64 * 48 * 4]))
            } else {
                Image::placeholder()
            },
            ready: req.start,
        })
    }

    fn landmarks(
        &self,
        _: &str,
        req: &LandmarksRequest,
    ) -> Result<Option<Vec<LandmarkLayer>>, String> {
        let tip = self.0.lock().unwrap().tip;
        Ok(match (req.start, tip) {
            (true, Some([x, y])) => Some(vec![LandmarkLayer {
                tracker: Tracker::Hands,
                dets: vec![(0..21)
                    .map(|i| if i == 8 { [x, y, 0.0] } else { [0.5, 0.8, 0.0] })
                    .collect()],
                ids: vec![0],
            }]),
            _ => Some(Vec::new()),
        })
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

fn text(id: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../samples/{id}.flora"));
    std::fs::read_to_string(path).unwrap()
}

fn frame(n: u32, mouse: (f64, f64), pressed: bool) -> FrameCtx {
    FrameCtx {
        t: f64::from(n) / 60.0,
        dt: 1.0 / 60.0,
        frame_count: f64::from(n),
        width: 640.0,
        height: 640.0,
        mouse_x: mouse.0,
        mouse_y: mouse.1,
        mouse_is_pressed: pressed,
        ..FrameCtx::default()
    }
}

fn first(engine: &Engine, out: &floret_engine::tick::Outputs, node: &str, port: usize) -> Value {
    let idx = engine
        .graph
        .idx(node)
        .unwrap_or_else(|| panic!("{node} in the graph"));
    out[idx]
        .as_ref()
        .unwrap_or_else(|| panic!("{node} has no output"))[port]
        .first()
        .cloned()
        .unwrap_or(Value::Undefined)
}

fn list(engine: &Engine, out: &floret_engine::tick::Outputs, node: &str, port: usize) -> Vec<f64> {
    let idx = engine.graph.idx(node).unwrap();
    out[idx].as_ref().unwrap()[port]
        .iter()
        .filter_map(Value::as_f64)
        .collect()
}

#[test]
fn midi_notes_plant_flowers_and_knobs_turn_the_garden() {
    let _g = lock();
    let f = fake();
    *f.0.lock().unwrap() = State::default();
    let mut engine = Engine::load(&text("play-midi-garden")).unwrap();
    let mut out = Default::default();
    for n in 0..40 {
        {
            let mut s = f.0.lock().unwrap();
            s.note = match n {
                5..=10 => Some((60, 100)),
                20..=25 => Some((67, 127)),
                _ => None,
            };
            if n == 15 {
                s.cc.insert(21, 127);
            }
        }
        let r = engine.tick(&frame(n, (0.0, 0.0), false));
        // Sound nodes need an audio engine (none here).
        let errors: Vec<_> = r
            .errors
            .iter()
            .filter(|(_, m)| !m.starts_with("no audio output"))
            .collect();
        assert!(errors.is_empty(), "frame {n}: {errors:?}");
        out = r.outputs;
    }
    // Two notes planted after the five flowers it starts with, newest last.
    let pitches = list(&engine, &out, "mg_pitchNext", 0);
    assert_eq!(pitches.len(), 7, "{pitches:?}");
    assert_eq!(&pitches[5..], [60.0, 67.0]);
    let loud = list(&engine, &out, "mg_loudNext", 0);
    assert!((loud[6] - 1.0).abs() < 1e-9, "velocity 127 → 1: {loud:?}");
    // CC 21 moved: it wins over the Hue slider; CC 22 never sent: the slider.
    assert_eq!(first(&engine, &out, "mg_hue", 0).as_f64(), Some(1.0));
    assert_eq!(first(&engine, &out, "mg_petals", 0).as_f64(), Some(0.45));
}

#[test]
fn a_tracked_hand_plays_the_theremin_and_the_mouse_stands_in() {
    let _g = lock();
    let f = fake();
    *f.0.lock().unwrap() = State::default();
    // The Camera toggle switched on, as a player would.
    let t = text("sound-hand-theremin");
    let at = t.find("\"id\": \"th_camOn\"").unwrap();
    let on = at + t[at..].find("\"value\": false").unwrap();
    let t = format!("{}\"value\": true{}", &t[..on], &t[on + 14..]);
    let mut engine = Engine::load(&t).unwrap();

    // No hand: the mouse plays, only while pressed.
    let r = engine.tick(&frame(0, (-300.0, 0.0), false));
    assert_eq!(
        first(&engine, &r.outputs, "th_sounding", 0).as_f64(),
        Some(0.0)
    );
    let r = engine.tick(&frame(1, (-300.0, 0.0), true));
    assert_eq!(
        first(&engine, &r.outputs, "th_sounding", 0).as_f64(),
        Some(1.0)
    );
    let low = first(&engine, &r.outputs, "th_pitch", 0).as_f64().unwrap();

    // A hand up and to the right (of the mirrored picture: the left of the
    // camera's frame): sounding without a press, higher, louder.
    f.0.lock().unwrap().tip = Some([0.1, 0.15]);
    let mut out = Default::default();
    for n in 2..30 {
        let r = engine.tick(&frame(n, (-300.0, 0.0), false));
        out = r.outputs;
    }
    assert!(
        first(&engine, &out, "th_hands", 12).truthy(),
        "hand detected"
    );
    assert_eq!(first(&engine, &out, "th_sounding", 0).as_f64(), Some(1.0));
    let high = first(&engine, &out, "th_pitch", 0).as_f64().unwrap();
    assert!(high > low + 12.0, "pitch {low} → {high}");
    let loud = first(&engine, &out, "th_loud", 0).as_f64().unwrap();
    assert!(loud > 0.6, "loud {loud}");
    // Snapped to the scale: C major pentatonic from C3 (48).
    let pc = (high.round() as i64 - 48).rem_euclid(12);
    assert!(
        [0, 2, 4, 7, 9].contains(&pc),
        "{high} is in the pentatonic scale"
    );
}
