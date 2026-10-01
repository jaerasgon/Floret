use super::super::value::harness::{node, num};
use crate::audio::{
    begin, Analysis, AudioFeedback, AudioGraph, AudioNode, FilterType, MicState, RecordingBackend,
    SampleState, Wave,
};
use crate::particles::Handle;
use crate::value::Value;
use crate::Engine;
use serde_json::json;
use std::sync::Arc;

fn audio_ref(v: &Value) -> Option<(&str, &str)> {
    match v {
        Value::Handle(Handle::Audio(a)) => Some((&a.node, a.kind)),
        _ => None,
    }
}

fn with_backend<R>(fb: AudioFeedback, f: impl FnOnce() -> R) -> (R, AudioGraph) {
    let rec = Arc::new(RecordingBackend::default());
    *rec.feedback.lock().unwrap() = fb;
    let r = {
        let _g = begin(Some(rec.clone()), vec![]);
        f()
    };
    (r, rec.last().unwrap())
}

fn up(id: &str) -> Value {
    Value::Handle(Handle::Audio(crate::audio::AudioRef {
        node: Arc::from(id),
        kind: "oscillator",
    }))
}

#[test]
fn without_audio_nodes_fail_like_ts() {
    for ty in [
        "sound/oscillator",
        "sound/pulse",
        "sound/noise",
        "sound/gain",
        "sound/fft",
        "sound/amplitude",
        "sound/output",
        "sound/audio-in",
        "sound/envelope",
    ] {
        let mut n = node(ty, json!({}));
        assert!(n.eval(vec![vec![]; 8]).is_err(), "{ty}");
    }
    // No URL: SoundFile answers before touching the AudioContext.
    let mut sf = node("sound/sound-file", json!({}));
    let out = sf.eval1(&[Value::str(" "), Value::Undefined]);
    assert!(matches!(out[0], Value::Null));
    assert!(matches!(out[1], Value::Bool(false)));
    assert_eq!((num(&out[2]), num(&out[3])), (0.0, 0.0));
    let mut sf = node("sound/sound-file", json!({}));
    assert!(sf.eval(vec![vec![Value::str("a.wav")]]).is_err());
}

#[test]
fn midi_conversions() {
    let mut m = node("sound/midi-to-freq", json!({}));
    assert_eq!(num(&m.eval1(&[Value::Num(69.0)])[0]), 440.0);
    assert_eq!(num(&m.eval1(&[Value::Num(81.0)])[0]), 880.0);
    assert_eq!(
        num(&m.eval1(&[Value::str("x")])[0]),
        440.0 * 2f64.powf(-69.0 / 12.0)
    );
    let mut f = node("sound/freq-to-midi", json!({}));
    assert_eq!(num(&f.eval1(&[Value::Num(220.0)])[0]), 57.0);
    assert_eq!(num(&f.eval1(&[Value::Num(-3.0)])[0]), 0.0);
}

#[test]
fn sources_declare_clamped_params() {
    let (out, g) = with_backend(AudioFeedback::default(), || {
        let mut osc = node("sound/oscillator", json!({"type": "square"}));
        osc.eval1(&[Value::Num(330.0), Value::Num(3.0)])
    });
    assert_eq!(audio_ref(&out[0]), Some(("n1", "oscillator")));
    assert_eq!(
        g.declared["n1"],
        Some(AudioNode::Oscillator {
            wave: Wave::Square,
            freq: 330.0,
            amp: 1.0
        })
    );

    // Web Audio rejects a non-finite frequency.
    let (r, _) = with_backend(AudioFeedback::default(), || {
        node("sound/oscillator", json!({})).eval(vec![vec![Value::Num(f64::NAN)], vec![]])
    });
    assert!(r.is_err());

    let (_, g) = with_backend(AudioFeedback::default(), || {
        let mut f = node("sound/filter", json!({"type": "bandpass"}));
        f.eval1(&[up("osc"), Value::Num(5.0), Value::Num(80.0)])
    });
    assert_eq!(
        g.declared["n1"],
        Some(AudioNode::Filter {
            input: Some(Arc::from("osc")),
            kind: FilterType::Bandpass,
            freq: 20.0,
            q: 50.0
        })
    );

    let (_, g) = with_backend(AudioFeedback::default(), || {
        node("sound/mono-synth", json!({})).eval1(&[
            Value::Num(81.0),
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Bool(true),
        ])
    });
    match &g.declared["n1"] {
        Some(AudioNode::MonoSynth {
            freq,
            velocity,
            gate,
            ..
        }) => assert_eq!((*freq, *velocity, *gate), (880.0, 0.7, true)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn mic_output_waits_for_the_stream() {
    let args = [Value::Num(0.5), Value::Bool(true)];
    let (out, g) = with_backend(AudioFeedback::default(), || {
        node("sound/audio-in", json!({})).eval1(&args)
    });
    assert!(matches!(out[0], Value::Null));
    assert_eq!(
        g.declared["n1"],
        Some(AudioNode::AudioIn {
            amp: 0.5,
            start: true
        })
    );
    let live = AudioFeedback {
        mic: MicState::Live,
        ..Default::default()
    };
    let (out, _) = with_backend(live, || node("sound/audio-in", json!({})).eval1(&args));
    assert_eq!(audio_ref(&out[0]), Some(("n1", "audio-in")));
}

#[test]
fn sound_file_reports_loading_then_ready() {
    let args = [
        Value::str("clip.wav"),
        Value::Num(1.0),
        Value::Num(0.0),
        Value::Num(1.0),
        Value::Bool(false),
        Value::Bool(true),
    ];
    let (out, g) = with_backend(AudioFeedback::default(), || {
        node("sound/sound-file", json!({})).eval1(&args)
    });
    assert!(matches!(out[1], Value::Bool(false)));
    assert!(matches!(
        g.declared["n1"],
        Some(AudioNode::SoundFile { play: true, .. })
    ));
    let mut fb = AudioFeedback::default();
    fb.samples
        .insert(Arc::from("clip.wav"), SampleState::Ready { duration: 2.5 });
    fb.playheads.insert(Arc::from("n1"), 0.75);
    let (out, _) = with_backend(fb, || node("sound/sound-file", json!({})).eval1(&args));
    assert_eq!(audio_ref(&out[0]), Some(("n1", "sound-file")));
    assert!(matches!(out[1], Value::Bool(true)));
    assert_eq!((num(&out[2]), num(&out[3])), (2.5, 0.75));
}

fn analysis(time: Vec<f32>, bytes: Vec<u8>) -> AudioFeedback {
    let mut fb = AudioFeedback {
        sample_rate: 48000.0,
        ..Default::default()
    };
    fb.analysers.insert(
        Arc::from("n1"),
        Analysis {
            time: Arc::from(time),
            bytes: Arc::from(bytes),
        },
    );
    fb
}

#[test]
fn fft_outputs_analyser_data() {
    // No data yet: a fresh analyser reads zeros.
    let (out, g) = with_backend(AudioFeedback::default(), || {
        node("sound/fft", json!({"bins": "32"}))
            .eval(vec![vec![up("osc")], vec![]])
            .unwrap()
    });
    assert_eq!((out[0].len(), out[1].len()), (32, 64));
    assert_eq!(
        g.declared["n1"],
        Some(AudioNode::Analyser {
            input: Some(Arc::from("osc")),
            fft_size: 64,
            smoothing: 0.8
        })
    );
    let fb = analysis(vec![0.5; 64], (0..32).collect());
    let (out, _) = with_backend(fb, || {
        node("sound/fft", json!({"bins": "32"}))
            .eval(vec![vec![], vec![]])
            .unwrap()
    });
    assert_eq!(num(&out[0][31]), 31.0);
    assert_eq!(num(&out[1][0]), 0.5);
    // Not a power of two.
    let (r, _) = with_backend(AudioFeedback::default(), || {
        node("sound/fft", json!({"bins": "100"})).eval(vec![vec![], vec![]])
    });
    assert!(r.is_err());
}

#[test]
fn amplitude_smooths_rms() {
    let fb = analysis(vec![0.5; 2048], vec![0; 1024]);
    let (levels, _) = with_backend(fb, || {
        let mut a = node("sound/amplitude", json!({}));
        (0..2)
            .map(|_| num(&a.eval1(&[Value::Null, Value::Num(0.5)])[0]))
            .collect::<Vec<_>>()
    });
    assert_eq!(levels, vec![0.25, 0.375]);
}

#[test]
fn peak_detect_respects_band_and_gap() {
    let mut bytes = vec![0u8; 256];
    bytes[2] = 255; // 187.5 Hz at 48 kHz / 512
    let fb = analysis(vec![0.0; 512], bytes);
    let (hits, _) = with_backend(fb, || {
        let mut p = node("sound/peak-detect", json!({}));
        let mut hits = Vec::new();
        for _ in 0..45 {
            let o = p.eval1(&[
                Value::Null,
                Value::Num(100.0),
                Value::Num(200.0),
                Value::Num(0.9),
            ]);
            hits.push(matches!(o[0], Value::Bool(true)));
        }
        hits
    });
    let at: Vec<usize> = (0..45).filter(|&i| hits[i]).collect();
    assert_eq!(at, vec![19, 39]);
}

#[test]
fn engine_submits_the_patch_graph() {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../samples/tut-47-envelope.flora"),
    )
    .unwrap();
    let mut engine = Engine::load(&text).unwrap();
    // No backend: every sound node fails, as in the golden trace.
    let r = engine.tick(&Default::default());
    assert_eq!(r.errors.len(), 3);

    let rec = Arc::new(RecordingBackend::default());
    engine.set_audio(Some(rec.clone()));
    let r = engine.tick(&Default::default());
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let g = rec.last().unwrap();
    let mut live: Vec<&str> = g.live.iter().map(|s| &**s).collect();
    live.sort();
    assert_eq!(live, vec!["t47_env", "t47_osc", "t47_out"]);
    assert!(matches!(
        &g.declared["t47_env"],
        Some(AudioNode::Envelope { input: Some(i), .. }) if &**i == "t47_osc"
    ));
    assert!(matches!(
        &g.declared["t47_out"],
        Some(AudioNode::Output { input: Some(i), .. }) if &**i == "t47_env"
    ));
}
