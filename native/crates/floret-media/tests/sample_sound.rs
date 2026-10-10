//! The Sound and Interactive Library samples make sound when played, and
//! Mix sums chains: each patch runs on the engine with the real audio
//! engine rendering offline (no device), driven by a scripted performance
//! (keys held, mouse pressed, a microphone tone), and what comes out of
//! the speakers is measured.

use floret_engine::audio::{AudioBackend, AudioGraph, AudioNode, Wave};
use floret_engine::{Engine, FrameCtx};
use floret_media::AudioSystem;
use std::f64::consts::TAU;
use std::sync::Arc;

const SR: f32 = 48000.0;
/// Audio frames per 60 fps engine tick.
const PER_TICK: usize = 800;

fn sample_text(id: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../samples")
        .join(format!("{id}.flora"));
    std::fs::read_to_string(path).expect("sample file")
}

fn sample(id: &str) -> Engine {
    Engine::load(&sample_text(id)).expect("sample loads")
}

#[derive(Default, Clone)]
struct Play {
    mouse: (f64, f64),
    pressed: bool,
    keys: Vec<&'static str>,
}

fn frame(f: usize, p: &Play) -> FrameCtx {
    FrameCtx {
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
    }
}

/// Run `frames` ticks of a sample with `play(f)` as the input, rendering
/// the audio in between; returns the left channel.
fn perform(
    engine: &mut Engine,
    sys: &Arc<AudioSystem>,
    frames: usize,
    play: impl Fn(usize) -> Play,
) -> Vec<f32> {
    let mut heard = Vec::with_capacity(frames * PER_TICK);
    for f in 0..frames {
        let r = engine.tick(&frame(f, &play(f)));
        let errors: Vec<_> = r
            .errors
            .iter()
            .filter(|(_, m)| !m.contains("microphone") && !m.contains("camera"))
            .collect();
        assert!(errors.is_empty(), "frame {f}: {errors:?}");
        heard.extend(sys.render(PER_TICK).iter().map(|s| s[0]));
    }
    heard
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn with_audio(id: &str) -> (Engine, Arc<AudioSystem>) {
    let mut engine = sample(id);
    let sys = Arc::new(AudioSystem::offline(SR));
    engine.set_audio(Some(sys.clone()));
    (engine, sys)
}

#[test]
fn mix_sums_two_chains_into_one() {
    let sys = AudioSystem::offline(SR);
    let id = |s: &str| Arc::<str>::from(s);
    let osc = |freq| AudioNode::Oscillator {
        wave: Wave::Sine,
        freq,
        amp: 0.5,
    };
    let nodes = vec![
        ("a", osc(440.0)),
        ("b", osc(660.0)),
        (
            "mix",
            AudioNode::Mix {
                inputs: vec![id("a"), id("b")],
                level: 0.5,
            },
        ),
        (
            "out",
            AudioNode::Output {
                input: Some(id("mix")),
                volume: 1.0,
            },
        ),
    ];
    sys.submit(AudioGraph {
        live: nodes.iter().map(|(i, _)| id(i)).collect(),
        declared: nodes.into_iter().map(|(i, n)| (id(i), Some(n))).collect(),
    });
    let x: Vec<f32> = sys.render(24000).iter().map(|s| s[0]).collect();
    let x = &x[4800..];
    // Each tone at 0.5 × 0.5, together sqrt(2 · 0.25² / 2).
    let tone = |freq: f64| {
        let n = x.len() as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &v) in x.iter().enumerate() {
            let w = 0.5 - 0.5 * (TAU * i as f64 / n).cos();
            let ph = TAU * freq * i as f64 / f64::from(SR);
            re += f64::from(v) * w * ph.cos();
            im -= f64::from(v) * w * ph.sin();
        }
        re.hypot(im) * 4.0 / n
    };
    assert!(
        (tone(440.0) - 0.25).abs() < 0.01,
        "440 Hz at {}",
        tone(440.0)
    );
    assert!(
        (tone(660.0) - 0.25).abs() < 0.01,
        "660 Hz at {}",
        tone(660.0)
    );
    assert!((rms(x) - 0.25).abs() < 0.01, "rms {}", rms(x));
}

#[test]
fn step_sequencer_plays_its_pattern_and_your_cells() {
    let (mut engine, sys) = with_audio("sound-step-sequencer");
    let idle = perform(&mut engine, &sys, 120, |_| Play::default());
    assert!(
        rms(&idle[PER_TICK * 30..]) > 0.01,
        "the default pattern plays: {}",
        rms(&idle)
    );
    // C clears the grid: silence once the tails die away.
    let (mut engine, sys) = with_audio("sound-step-sequencer");
    let cleared = perform(&mut engine, &sys, 300, |f| Play {
        keys: if f == 2 { vec!["c"] } else { vec![] },
        ..Play::default()
    });
    assert!(
        rms(&cleared[PER_TICK * 260..]) < 1e-3,
        "cleared: {}",
        rms(&cleared[PER_TICK * 260..])
    );
}

#[test]
fn keyboard_synth_is_silent_until_a_key_is_held() {
    let (mut engine, sys) = with_audio("sound-keyboard-synth");
    let x = perform(&mut engine, &sys, 90, |f| Play {
        keys: if (30..70).contains(&f) {
            vec!["a"]
        } else {
            vec![]
        },
        mouse: (0.0, -200.0),
        ..Play::default()
    });
    let before = rms(&x[..PER_TICK * 28]);
    let during = rms(&x[PER_TICK * 40..PER_TICK * 68]);
    assert!(before < 1e-4, "silent before: {before}");
    assert!(during > 0.02, "a held key sounds: {during}");
}

#[test]
fn theremin_sounds_while_the_mouse_is_held() {
    let (mut engine, sys) = with_audio("sound-hand-theremin");
    let x = perform(&mut engine, &sys, 100, |f| Play {
        mouse: (-200.0 + f as f64 * 4.0, -100.0),
        pressed: (40..90).contains(&f),
        ..Play::default()
    });
    assert!(rms(&x[..PER_TICK * 38]) < 1e-3, "silent before");
    assert!(
        rms(&x[PER_TICK * 55..PER_TICK * 88]) > 0.02,
        "sounding while held"
    );
}

#[test]
fn music_box_plucks_its_tune() {
    let (mut engine, sys) = with_audio("sound-music-box");
    let x = perform(&mut engine, &sys, 400, |_| Play::default());
    assert!(
        rms(&x[PER_TICK * 60..]) > 0.005,
        "the tune plays: {}",
        rms(&x)
    );
}

#[test]
fn midi_garden_plays_keys_and_clicks() {
    let (mut engine, sys) = with_audio("play-midi-garden");
    let x = perform(&mut engine, &sys, 100, |f| Play {
        keys: if (20..50).contains(&f) {
            vec!["d"]
        } else {
            vec![]
        },
        pressed: (60..90).contains(&f),
        mouse: (100.0, -100.0),
    });
    assert!(rms(&x[..PER_TICK * 18]) < 1e-4, "silent before");
    assert!(rms(&x[PER_TICK * 25..PER_TICK * 50]) > 0.01, "a key plays");
    assert!(
        rms(&x[PER_TICK * 65..PER_TICK * 90]) > 0.01,
        "a click plays"
    );
}

#[test]
fn mic_bloom_plays_its_demo_and_listens_to_the_microphone() {
    let (mut engine, sys) = with_audio("sound-mic-bloom");
    let demo = perform(&mut engine, &sys, 90, |_| Play::default());
    assert!(rms(&demo[PER_TICK * 20..]) > 0.01, "the demo loop plays");
    let fft = engine.graph.idx("mb_fft").expect("fft node");

    // Microphone on: the demo goes quiet and a mic tone fills the spectrum.
    // (The Microphone toggle switched on in the file.)
    let text = sample_text("sound-mic-bloom");
    let at = text.find("\"id\": \"mb_micOn\"").expect("the toggle");
    let on = at + text[at..].find("\"value\": false").expect("its value");
    let text = format!("{}\"value\": true{}", &text[..on], &text[on + 14..]);
    let mut engine = Engine::load(&text).expect("sample loads");
    let sys = Arc::new(AudioSystem::offline(SR));
    engine.set_audio(Some(sys.clone()));
    let mut mic = sys.attach_test_mic(SR);
    let mut n = 0u64;
    let mut heard = Vec::new();
    let mut peak = 0.0f64;
    for f in 0..90 {
        for _ in 0..PER_TICK {
            let v = (TAU * 880.0 * n as f64 / f64::from(SR)).sin() as f32 * 0.6;
            let _ = mic.push(v);
            n += 1;
        }
        let r = engine.tick(&frame(f, &Play::default()));
        if let Some(Some(out)) = r.outputs.get(fft) {
            peak = out[0]
                .iter()
                .filter_map(|v| v.as_f64())
                .fold(peak, f64::max);
        }
        heard.extend(sys.render(PER_TICK).iter().map(|s| s[0]));
    }
    assert!(peak > 100.0, "the mic tone shows in the spectrum: {peak}");
    // The demo is muted and the microphone isn't sent to the speakers.
    assert!(
        rms(&heard[PER_TICK * 30..]) < 1e-3,
        "quiet with the mic on: {}",
        rms(&heard[PER_TICK * 30..])
    );
}

/// Ten seconds of a sample's sound with its controls moved on the way:
/// each slider to a quarter of its range at 3 s and its top at 6 s, each
/// toggle flipped at 4.5 s.
fn play_with_controls(text: &str) -> Vec<f32> {
    let patch = floret_engine::load_patch(text).expect("loads");
    let mut moves: Vec<(usize, String, serde_json::Value)> = Vec::new();
    for n in &patch.nodes {
        let num = |k: &str, d: f64| n.params.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
        match n.def_type.as_str() {
            "io/slider" => {
                let (lo, hi) = (num("min", 0.0), num("max", 1.0));
                moves.push((180, n.id.clone(), (lo + 0.25 * (hi - lo)).into()));
                moves.push((360, n.id.clone(), hi.into()));
            }
            "io/toggle" => {
                let on = n
                    .params
                    .get("value")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                moves.push((270, n.id.clone(), (!on).into()));
            }
            _ => {}
        }
    }
    let mut engine = Engine::load(text).expect("sample loads");
    let sys = Arc::new(AudioSystem::offline(SR));
    engine.set_audio(Some(sys.clone()));
    let mut heard = Vec::new();
    for f in 0..600 {
        for (at, id, v) in &moves {
            if *at == f {
                engine.params.set(id, "value", v.clone());
            }
        }
        engine.tick(&frame(f, &Play::default()));
        heard.extend(sys.render(PER_TICK).iter().map(|s| s[0]));
    }
    heard
}

/// `FLORET_SAMPLES_BEFORE=<dir of old .flora files> cargo test -p
/// floret-media --test sample_sound -- --ignored
/// rebuilt_samples_sound_as_before --nocapture`: a sample rebuilt with
/// other nodes (`FLORET_SOUND_SAMPLES`, default `sys-music-viz`) plays the
/// same audio as its old version, sample for sample, over ten seconds with
/// its controls moved (see [`play_with_controls`]).
#[test]
#[ignore = "needs FLORET_SAMPLES_BEFORE"]
fn rebuilt_samples_sound_as_before() {
    let Some(before_dir) = std::env::var_os("FLORET_SAMPLES_BEFORE") else {
        eprintln!("FLORET_SAMPLES_BEFORE not set: skipping");
        return;
    };
    let ids = std::env::var("FLORET_SOUND_SAMPLES").unwrap_or_else(|_| "sys-music-viz".into());
    let largest = |x: &[f32], y: &[f32]| {
        x.iter()
            .zip(y)
            .map(|(p, q)| (p - q).abs())
            .fold(0.0f32, f32::max)
    };
    let mut failures = Vec::new();
    for id in ids.split(',') {
        let old = std::path::Path::new(&before_dir).join(format!("{id}.flora"));
        let before = std::fs::read_to_string(old).expect("old sample");
        let (a, b) = (play_with_controls(&before), play_with_controls(&before));
        let c = play_with_controls(&sample_text(id));
        let (noise, change) = (largest(&a, &b), largest(&a, &c));
        eprintln!(
            "{id}: rms {:.4} → {:.4}; largest sample difference {change:e} (old vs old {noise:e})",
            rms(&a),
            rms(&c)
        );
        if a.len() != c.len() || change > noise.max(1e-6) {
            failures.push(format!(
                "{id}: sounds different (largest difference {change})"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
