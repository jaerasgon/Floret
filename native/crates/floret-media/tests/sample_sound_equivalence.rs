//! Rebuilt Sound samples sound as before.
//!
//! The no-code rebuild replaced the Sound samples' Expression nodes with
//! visual nodes, so their node ids changed and nodes can't be compared one
//! to one. This plays the old and the new file through the real audio
//! engine, rendering offline, with the same scripted performance (keys,
//! the mouse, a fake hand, a microphone tone) and compares what comes out
//! of the speakers block by block, and the renderer's layers frame by
//! frame (numbers within a small tolerance).
//!
//! `FLORET_SAMPLES_BEFORE=<dir> cargo test -p floret-media --test
//! sample_sound_equivalence -- --ignored --nocapture` (`<dir>`: copies of
//! the samples before the rebuild).

use floret_engine::devices::{self, CameraFrame, CameraRequest, Devices, LandmarkLayer};
use floret_engine::devices::LandmarksRequest;
use floret_engine::image::Image;
use floret_engine::landmarks::Tracker;
use floret_engine::{Engine, FrameCtx, Value};
use floret_media::AudioSystem;
use std::f64::consts::TAU;
use std::path::Path;
use std::sync::{Arc, Mutex};

const SR: f32 = 48000.0;
/// Audio frames per 60 fps engine tick.
const PER_TICK: usize = 800;
const FRAMES: usize = 240;

#[derive(Default, Clone)]
struct Play {
    mouse: (f64, f64),
    pressed: bool,
    keys: Vec<&'static str>,
    hand: Option<[f32; 2]>,
}

type Script = fn(usize) -> Play;

fn keyboard(f: usize) -> Play {
    let held: &[&'static str] = match f {
        10..=24 => &["a"],
        25..=39 => &["d"],
        40..=44 => &["x"],
        45..=54 => &["g"],
        55..=64 => &["z"],
        70..=140 => &["a", "d", "g"],
        150..=170 => &["k"],
        180..=200 => &["w", "t"],
        _ => &[],
    };
    Play {
        mouse: (0.0, -240.0 + f as f64 * 2.0),
        keys: held.to_vec(),
        ..Play::default()
    }
}

fn theremin(f: usize) -> Play {
    Play {
        mouse: (
            -330.0 + f as f64 * 3.0,
            -150.0 + 120.0 * (f as f64 * 0.07).sin(),
        ),
        pressed: (15..120).contains(&f),
        hand: (140..230).contains(&f).then(|| {
            let s = (f - 140) as f64 / 90.0;
            [(0.85 - 0.7 * s) as f32, (0.2 + 0.5 * s) as f32]
        }),
        ..Play::default()
    }
}

fn still(_: usize) -> Play {
    Play::default()
}

/// (sample, performance, toggles switched in both files, a microphone tone).
type Case = (&'static str, Script, &'static [(&'static str, &'static str)], bool);

const CASES: &[Case] = &[
    ("sound-keyboard-synth", keyboard, &[], false),
    ("sound-keyboard-synth", keyboard, &[("ks_arpOn", "true")], false),
    ("sound-hand-theremin", theremin, &[], false),
    ("sound-hand-theremin", theremin, &[("th_snap", "false")], false),
    ("sound-hand-theremin", theremin, &[("th_camOn", "true")], false),
    ("sound-mic-bloom", still, &[], false),
    ("sound-mic-bloom", still, &[("mb_micOn", "true")], true),
];

#[derive(Default)]
struct Fake(Mutex<Option<[f32; 2]>>);

impl Devices for Fake {
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
        let tip = *self.0.lock().unwrap();
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

fn edited(text: &str, edits: &[(&str, &str)]) -> String {
    let mut t = text.to_owned();
    for (id, value) in edits {
        let at = t.find(&format!("\"id\": \"{id}\"")).expect("the toggle");
        let from = if *value == "true" { "false" } else { "true" };
        let on = at + t[at..].find(&format!("\"value\": {from}")).expect("its value");
        t = format!("{}\"value\": {value}{}", &t[..on], &t[on + 9 + from.len()..]);
    }
    t
}

/// What one run gives: the left channel and the renderer's layers per frame.
fn perform(fake: &Fake, flora: &str, script: Script, mic: bool) -> (Vec<f32>, Vec<String>) {
    let mut engine = Engine::load(flora).expect("sample loads");
    let sys = Arc::new(AudioSystem::offline(SR));
    engine.set_audio(Some(sys.clone()));
    let mut tone = mic.then(|| sys.attach_test_mic(SR));
    let r = engine.graph.renderer().expect("a renderer");
    let mut n = 0u64;
    let (mut heard, mut layers) = (Vec::new(), Vec::new());
    for f in 0..FRAMES {
        if let Some(m) = tone.as_mut() {
            for _ in 0..PER_TICK {
                let v = (TAU * 880.0 * n as f64 / f64::from(SR)).sin() as f32 * 0.6;
                let _ = m.push(v);
                n += 1;
            }
        }
        let p = script(f);
        *fake.0.lock().unwrap() = p.hand;
        let ctx = FrameCtx {
            t: f as f64 / 60.0,
            dt: 1.0 / 60.0,
            frame_count: f as f64,
            width: 640.0,
            height: 640.0,
            mouse_x: p.mouse.0,
            mouse_y: p.mouse.1,
            mouse_is_pressed: p.pressed,
            key: p.keys.last().map(|k| (*k).to_owned()).unwrap_or_default(),
            key_is_pressed: !p.keys.is_empty(),
            keys_down: p.keys.iter().map(|k| (*k).to_owned()).collect(),
            ..FrameCtx::default()
        };
        let res = engine.tick(&ctx);
        let errors: Vec<_> = res
            .errors
            .iter()
            .filter(|(_, m)| !m.contains("microphone") && !m.contains("camera"))
            .collect();
        assert!(errors.is_empty(), "frame {f}: {errors:?}");
        let text = res
            .outputs
            .get(r)
            .cloned()
            .flatten()
            .map(|o| {
                o.iter()
                    .flat_map(|s| s.iter())
                    .map(canon)
                    .collect::<String>()
            })
            .unwrap_or_default();
        layers.push(text);
        heard.extend(sys.render(PER_TICK).iter().map(|s| s[0]));
    }
    (heard, layers)
}

/// A value as text, scene nodes in full (their `Debug` is just a kind),
/// groups' children included.
fn canon(v: &Value) -> String {
    match v {
        Value::Scene(n) => canon(&n.to_value()),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(canon).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) => {
            let parts: Vec<String> = map.iter().map(|(k, v)| format!("{k}: {}", canon(v))).collect();
            format!("{{{}}}", parts.join(", "))
        }
        // Image ids come from a global counter.
        Value::Image(i) => format!("Image({}×{})", i.width(), i.height()),
        other => format!("{other:?}"),
    }
}

/// Numbers in `a` and `b` within `tol` (relative past 1), the rest equal.
fn near_text(a: &str, b: &str, tol: f64) -> bool {
    fn tokens(s: &str) -> Vec<Result<f64, &str>> {
        let bytes = s.as_bytes();
        let (mut out, mut i, mut start) = (Vec::new(), 0, 0);
        while i < bytes.len() {
            let c = bytes[i] as char;
            let prev_alnum = i > 0 && (bytes[i - 1] as char).is_ascii_alphanumeric();
            let num = !prev_alnum
                && (c.is_ascii_digit()
                    || (c == '-' && bytes.get(i + 1).is_some_and(|d| d.is_ascii_digit())));
            if !num {
                i += 1;
                continue;
            }
            if start < i {
                out.push(Err(&s[start..i]));
            }
            let mut j = i + 1;
            while j < bytes.len() {
                let d = bytes[j] as char;
                let sign = (d == '-' || d == '+') && matches!(bytes[j - 1] as char, 'e' | 'E');
                if d.is_ascii_digit() || matches!(d, '.' | 'e' | 'E') || sign {
                    j += 1;
                } else {
                    break;
                }
            }
            out.push(s[i..j].parse::<f64>().map_err(|_| &s[i..j]));
            (i, start) = (j, j);
        }
        if start < s.len() {
            out.push(Err(&s[start..]));
        }
        out
    }
    let (ta, tb) = (tokens(a), tokens(b));
    ta.len() == tb.len()
        && ta.iter().zip(&tb).all(|(x, y)| match (x, y) {
            (Ok(x), Ok(y)) => (x - y).abs() <= tol * x.abs().max(1.0),
            (Err(x), Err(y)) => x == y,
            _ => false,
        })
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

#[test]
#[ignore = "needs FLORET_SAMPLES_BEFORE"]
fn rebuilt_sound_samples_sound_as_before() {
    let Some(before_dir) = std::env::var_os("FLORET_SAMPLES_BEFORE") else {
        eprintln!("FLORET_SAMPLES_BEFORE not set: skipping");
        return;
    };
    let fake = Arc::new(Fake::default());
    devices::install(Some(fake.clone()));
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut failures = Vec::new();
    for (id, script, edits, mic) in CASES {
        let read = |d: &Path| edited(&std::fs::read_to_string(d.join(format!("{id}.flora"))).unwrap(), edits);
        let (before, after) = (read(Path::new(&before_dir)), read(&dir));
        let name = format!("{id} {edits:?}");
        let (a, la) = perform(&fake, &before, *script, *mic);
        let (b, lb) = perform(&fake, &after, *script, *mic);
        // Same samples where nothing changed; where an event comes a frame
        // sooner the waves drift out of phase, so behaviour is compared by
        // loudness: each tick's level must follow the old one.
        let level = |x: &[f32], f: usize| rms(&x[f * PER_TICK..(f + 1) * PER_TICK]);
        let identical = (0..FRAMES)
            .filter(|&f| (level(&a, f) - level(&b, f)).abs() < 1e-6 && {
                let r = f * PER_TICK..(f + 1) * PER_TICK;
                a[r.clone()] == b[r]
            })
            .count();
        let louder: Vec<usize> = (0..FRAMES)
            .filter(|&f| {
                let (x, y) = (level(&a, f), level(&b, f));
                (x - y).abs() > 0.005 + 0.15 * x.max(y)
            })
            .collect();
        let layer_frames = (0..FRAMES).filter(|&f| near_text(&la[f], &lb[f], 1e-6)).count();
        eprintln!(
            "{name}: audio rms {:.4} → {:.4}; {identical} of {FRAMES} ticks sample-identical; \
             level differs on {} ticks {:?}; layers equal (±1e-6) on {layer_frames}/{FRAMES} frames",
            rms(&a),
            rms(&b),
            louder.len(),
            &louder[..louder.len().min(12)],
        );
        if louder.len() * 20 > FRAMES {
            failures.push(format!("{name}: loudness follows a different course on {} ticks", louder.len()));
        }
    }
    devices::install(None);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
