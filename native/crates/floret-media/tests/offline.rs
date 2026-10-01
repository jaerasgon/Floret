//! The audio engine rendered offline (no device): frequencies, waveforms,
//! envelopes, filter responses, routing, analysis and file playback,
//! checked numerically.

use floret_engine::audio::{
    AudioBackend, AudioGraph, AudioNode, FilterType, MicState, NoiseColor, Oversample, SampleState,
    Wave,
};
use floret_media::dsp::{Biquad, BiquadKind, Bus, QUANTUM};
use floret_media::graph::Params;
use floret_media::nodes::Dsp;
use floret_media::AudioSystem;
use std::f64::consts::TAU;
use std::sync::Arc;

const SR: f32 = 48000.0;

fn id(s: &str) -> Arc<str> {
    Arc::from(s)
}

fn graph(nodes: Vec<(&str, AudioNode)>) -> AudioGraph {
    AudioGraph {
        live: nodes.iter().map(|(i, _)| id(i)).collect(),
        declared: nodes.into_iter().map(|(i, n)| (id(i), Some(n))).collect(),
    }
}

fn input(s: &str) -> Option<Arc<str>> {
    Some(id(s))
}

fn osc(wave: Wave, freq: f64, amp: f64) -> AudioNode {
    AudioNode::Oscillator { wave, freq, amp }
}

fn out(from: &str) -> AudioNode {
    AudioNode::Output {
        input: input(from),
        volume: 1.0,
    }
}

fn left(frames: &[[f32; 2]]) -> Vec<f32> {
    frames.iter().map(|f| f[0]).collect()
}

fn secs(s: f64) -> usize {
    (s * f64::from(SR)) as usize
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

/// Frequency from rising zero crossings (interpolated).
fn zero_cross_freq(x: &[f32]) -> f64 {
    let mut crossings = Vec::new();
    for i in 1..x.len() {
        let (a, b) = (f64::from(x[i - 1]), f64::from(x[i]));
        if a < 0.0 && b >= 0.0 {
            crossings.push(i as f64 - 1.0 + a / (a - b));
        }
    }
    let n = crossings.len();
    assert!(n > 2, "no signal");
    (n - 1) as f64 / (crossings[n - 1] - crossings[0]) * f64::from(SR)
}

/// Amplitude of the `freq` component (single-bin DFT, Hann windowed).
fn tone(x: &[f32], freq: f64) -> f64 {
    let n = x.len() as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (TAU * i as f64 / n).cos();
        let ph = TAU * freq * i as f64 / f64::from(SR);
        re += f64::from(v) * w * ph.cos();
        im -= f64::from(v) * w * ph.sin();
    }
    re.hypot(im) * 4.0 / n
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn oscillators_hit_frequency_amplitude_and_harmonics() {
    // (wave, RMS of a unit wave, 2nd and 3rd harmonic amplitudes)
    let cases = [
        (Wave::Sine, 1.0 / 2f64.sqrt(), 0.0, 0.0),
        (Wave::Square, 1.0, 0.0, 4.0 / (3.0 * std::f64::consts::PI)),
        (
            Wave::Sawtooth,
            1.0 / 3f64.sqrt(),
            2.0 / (2.0 * std::f64::consts::PI),
            2.0 / (3.0 * std::f64::consts::PI),
        ),
        (
            Wave::Triangle,
            1.0 / 3f64.sqrt(),
            0.0,
            8.0 / (9.0 * std::f64::consts::PI.powi(2)),
        ),
    ];
    for (wave, unit_rms, h2, h3) in cases {
        let sys = AudioSystem::offline(SR);
        sys.submit(graph(vec![("o", osc(wave, 440.0, 0.5)), ("out", out("o"))]));
        let x = left(&sys.render(secs(0.6)));
        let x = &x[secs(0.05)..];
        let f = zero_cross_freq(x);
        assert!(close(f, 440.0, 0.2), "{wave:?}: {f} Hz");
        let r = rms(x);
        assert!(close(r, 0.5 * unit_rms, 0.01), "{wave:?}: rms {r}");
        assert!(close(tone(x, 880.0), 0.5 * h2, 0.01), "{wave:?}: 2nd");
        assert!(close(tone(x, 1320.0), 0.5 * h3, 0.01), "{wave:?}: 3rd");
        // Mono sources reach both speakers.
        let frames = sys.render(256);
        assert!(frames.iter().all(|f| f[0] == f[1]));
    }
}

#[test]
fn parameter_changes_ramp_instead_of_clicking() {
    let sys = AudioSystem::offline(SR);
    sys.submit(graph(vec![
        ("o", osc(Wave::Sine, 100.0, 0.0)),
        ("out", out("o")),
    ]));
    sys.render(secs(0.05));
    sys.submit(graph(vec![
        ("o", osc(Wave::Sine, 100.0, 1.0)),
        ("out", out("o")),
    ]));
    let x = left(&sys.render(secs(0.1)));
    // A 100 Hz sine at full scale moves at most ~0.013 per sample; the 20 ms
    // ramp adds 1/960 per sample. A step would jump by up to 1.
    let max_step = x
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0f32, f32::max);
    assert!(max_step < 0.016, "step {max_step}");
    // …and the ramp completes in 20 ms.
    let peak = x[secs(0.03)..].iter().fold(0f32, |m, v| m.max(v.abs()));
    assert!(close(f64::from(peak), 1.0, 0.01), "peak {peak}");
    let early = x[..secs(0.005)].iter().fold(0f32, |m, v| m.max(v.abs()));
    assert!(early < 0.3, "early {early}");
}

fn envelope(gate: bool) -> AudioNode {
    AudioNode::Envelope {
        input: input("o"),
        attack: 0.1,
        decay: 0.1,
        sustain: 0.5,
        release: 0.1,
        level: 1.0,
        gate,
    }
}

#[test]
fn envelope_follows_attack_decay_sustain_release() {
    let sys = AudioSystem::offline(SR);
    // A 1 Hz square is +1 for half a second: a DC probe for the gain.
    let nodes = |gate| {
        graph(vec![
            ("o", osc(Wave::Square, 1.0, 1.0)),
            ("env", envelope(gate)),
            ("out", out("env")),
        ])
    };
    sys.submit(nodes(false));
    let pre = left(&sys.render(QUANTUM * 19));
    assert!(pre.iter().all(|&v| v == 0.0));
    sys.submit(nodes(true));
    let held = left(&sys.render(QUANTUM * 113)); // ~0.3 s
    sys.submit(nodes(false));
    let released = left(&sys.render(secs(0.15)));
    let at = |x: &[f32], t: f64| f64::from(x[secs(t)]);
    assert!(close(at(&held, 0.05), 0.5, 0.01), "mid attack");
    assert!(close(at(&held, 0.1), 1.0, 0.01), "peak");
    assert!(close(at(&held, 0.15), 0.75, 0.01), "mid decay");
    assert!(close(at(&held, 0.25), 0.5, 0.01), "sustain");
    let s = f64::from(held[held.len() - 1]);
    assert!(close(at(&released, 0.05), s * 0.5, 0.01), "mid release");
    assert!(at(&released, 0.11).abs() < 1e-6, "released");
}

#[test]
fn biquad_matches_web_audio_definitions() {
    let sr = f64::from(SR);
    // Web Audio's lowpass / highpass take Q in dB: |H(f0)| = 10^(Q/20).
    for kind in [BiquadKind::Lowpass, BiquadKind::Highpass] {
        let mut b = Biquad::default();
        b.set(kind, 1000.0, 6.0, sr);
        assert!(close(b.magnitude(1000.0, sr), 10f64.powf(6.0 / 20.0), 1e-9));
    }
    // Bandpass Q is linear and peaks at unity.
    let mut b = Biquad::default();
    b.set(BiquadKind::Bandpass, 2000.0, 4.0, sr);
    assert!(close(b.magnitude(2000.0, sr), 1.0, 1e-9));
    // −3 dB points at f0·(√(1+1/4Q²) ± 1/2Q) (bilinear warping aside).
    let half: f64 = 1.0 / (2.0 * 4.0);
    let upper = 2000.0 * ((1.0 + half * half).sqrt() + half);
    assert!(close(b.magnitude(upper, sr), 0.5f64.sqrt(), 0.02));

    // Processed signals follow the response: sines through a lowpass.
    for (f, tol) in [(200.0, 0.01), (1000.0, 0.01), (5000.0, 0.005)] {
        let sys = AudioSystem::offline(SR);
        sys.submit(graph(vec![
            ("o", osc(Wave::Sine, f, 0.5)),
            (
                "f",
                AudioNode::Filter {
                    input: input("o"),
                    kind: FilterType::Lowpass,
                    freq: 1000.0,
                    q: 1.0,
                },
            ),
            ("out", out("f")),
        ]));
        let x = left(&sys.render(secs(0.5)));
        let got = tone(&x[secs(0.1)..], f) / 0.5;
        let mut want = Biquad::default();
        want.set(BiquadKind::Lowpass, 1000.0, 1.0, sr);
        let want = want.magnitude(f, sr);
        assert!(close(got, want, tol), "{f} Hz: {got} vs {want}");
    }
}

fn run_dsp(node: &AudioNode, params: &[f32], input: &[f32]) -> Vec<f32> {
    let mut dsp = Dsp::new(node, SR, None, None);
    let p = Params::default();
    for (i, v) in params.iter().enumerate() {
        p.set(i, *v);
    }
    let cx = floret_media::nodes::Ctx {
        sample_rate: SR,
        mic: None,
    };
    let mut y = Vec::new();
    for q in input.chunks(QUANTUM) {
        let mut bus = Bus::new();
        bus.ch[0][..q.len()].copy_from_slice(q);
        let mut o = Bus::new();
        dsp.process(&p, Some(&bus), &mut o, &cx);
        y.extend_from_slice(&o.ch[0][..q.len()]);
    }
    y
}

#[test]
fn delay_echoes_with_feedback() {
    let node = AudioNode::Delay {
        input: None,
        time: 0.1,
        feedback: 0.5,
        damping: 20000.0,
    };
    let mut x = vec![0f32; secs(0.35)];
    x[0] = 1.0;
    let y = run_dsp(&node, &[0.1, 0.5, 20000.0], &x);
    assert_eq!(y[0], 1.0, "dry passes straight through");
    let peak = |from: usize, to: usize| {
        (from..to)
            .max_by(|&a, &b| y[a].abs().total_cmp(&y[b].abs()))
            .unwrap()
    };
    let e1 = peak(secs(0.05), secs(0.15));
    let e2 = peak(secs(0.15), secs(0.25));
    assert!(e1.abs_diff(4800) <= 3, "first echo at {e1}");
    assert!(e2.abs_diff(9600) <= 6, "second echo at {e2}");
    let ratio = y[e2].abs() / y[e1].abs();
    assert!(close(f64::from(ratio), 0.5, 0.1), "feedback ratio {ratio}");
}

#[test]
fn pulse_width_sets_the_duty_cycle() {
    for (width, duty) in [(0.0, 0.998), (0.5, 0.498), (0.75, 0.248)] {
        let sys = AudioSystem::offline(SR);
        sys.submit(graph(vec![
            (
                "p",
                AudioNode::Pulse {
                    freq: 100.0,
                    amp: 0.5,
                    width,
                },
            ),
            ("out", out("p")),
        ]));
        let x = left(&sys.render(secs(0.5)));
        let x = &x[secs(0.1)..];
        let high = x.iter().filter(|&&v| v > 0.0).count() as f64 / x.len() as f64;
        assert!(close(high, duty, 0.01), "width {width}: duty {high}");
    }
}

#[test]
fn noise_is_seeded_and_coloured() {
    let render = |color, seed| {
        let sys = AudioSystem::offline(SR);
        sys.submit(graph(vec![
            (
                "n",
                AudioNode::Noise {
                    color,
                    amp: 0.5,
                    seed,
                },
            ),
            ("out", out("n")),
        ]));
        left(&sys.render(secs(0.5)))[secs(0.05)..].to_vec()
    };
    let white = render(NoiseColor::White, 1);
    assert_eq!(white, render(NoiseColor::White, 1));
    assert_ne!(white, render(NoiseColor::White, 2));
    assert!(close(rms(&white), 0.5 / 3f64.sqrt(), 0.01));
    // Redder noise changes less from sample to sample.
    let roughness = |x: &[f32]| {
        let d: Vec<f32> = x.windows(2).map(|w| w[1] - w[0]).collect();
        rms(&d) / rms(x)
    };
    let (w, p, b) = (
        roughness(&white),
        roughness(&render(NoiseColor::Pink, 1)),
        roughness(&render(NoiseColor::Brown, 1)),
    );
    assert!(w > 1.3 && p < w * 0.6 && b < p * 0.5, "{w} {p} {b}");
}

#[test]
fn analyser_feedback_sees_the_tone() {
    let sys = AudioSystem::offline(SR);
    let g = || {
        graph(vec![
            ("o", osc(Wave::Sine, 1500.0, 0.01)),
            (
                "fft",
                AudioNode::Analyser {
                    input: input("o"),
                    fft_size: 256,
                    smoothing: 0.0,
                },
            ),
        ])
    };
    sys.submit(g());
    sys.render(secs(0.1));
    let fb = sys.feedback();
    let a = &fb.analysers["fft"];
    assert_eq!((a.time.len(), a.bytes.len()), (256, 128));
    let peak = (0..128).max_by_key(|&i| a.bytes[i]).unwrap();
    assert_eq!(peak, 8, "1500 Hz / 187.5 Hz bins");
    let max = a.time.iter().fold(0f32, |m, v| m.max(v.abs()));
    assert!(close(f64::from(max), 0.01, 0.001));
    // Analysers aren't outputs: nothing reaches the speakers.
    assert!(sys.render(512).iter().all(|f| f[0] == 0.0));
}

#[test]
fn engine_patch_drives_the_audio_engine() {
    // fft-bloom: oscillator → output, and → fft → the visuals.
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/fft-bloom.flora"),
    )
    .unwrap();
    let mut engine = floret_engine::Engine::load(&text).unwrap();
    let sys = Arc::new(AudioSystem::offline(SR));
    engine.set_audio(Some(sys.clone()));
    let mut heard = Vec::new();
    let mut spectrum_peak = 0.0;
    for frame in 0..30 {
        let r = engine.tick(&floret_engine::FrameCtx {
            t: frame as f64 / 60.0,
            dt: 1.0 / 60.0,
            frame_count: frame as f64,
            width: 800.0,
            height: 600.0,
            ..Default::default()
        });
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let fft = engine.graph.idx("fb_fft").unwrap();
        let spectrum = &r.outputs[fft].as_ref().unwrap()[0];
        spectrum_peak = spectrum
            .iter()
            .filter_map(|v| v.as_f64())
            .fold(spectrum_peak, f64::max);
        heard.extend(left(&sys.render(800)));
    }
    assert!(spectrum_peak > 200.0, "spectrum peak {spectrum_peak}");
    assert!(rms(&heard[secs(0.1)..]) > 0.05, "silent output");
}

#[test]
fn microphone_reaches_the_graph() {
    let sys = AudioSystem::offline(SR);
    let mut mic = sys.attach_test_mic(44100.0);
    sys.submit(graph(vec![
        (
            "mic",
            AudioNode::AudioIn {
                amp: 1.0,
                start: true,
            },
        ),
        ("out", out("mic")),
    ]));
    assert_eq!(sys.feedback().mic, MicState::Live);
    let mut x = Vec::new();
    let mut n = 0u64;
    for _ in 0..20 {
        // 10 ms of a 300 Hz tone at the input device's rate, then 10 ms out.
        for _ in 0..441 {
            let v = (TAU * 300.0 * n as f64 / 44100.0).sin() as f32 * 0.5;
            let _ = mic.push(v);
            n += 1;
        }
        x.extend(left(&sys.render(480)));
    }
    let f = zero_cross_freq(&x[secs(0.08)..]);
    assert!(close(f, 300.0, 1.0), "mic tone at {f} Hz");
}

fn temp_wav(name: &str, rate: u32, samples: &[i16]) -> String {
    let path = std::env::temp_dir().join(format!("floret-{}-{name}.wav", std::process::id()));
    let data_len = (samples.len() * 2) as u32;
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    for x in [16u32, 1 | (1 << 16), rate, rate * 2, 2 | (16 << 16)] {
        v.extend_from_slice(&x.to_le_bytes());
    }
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        v.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(&path, v).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn sound_file_loads_plays_and_reports_its_playhead() {
    let rate = 22050;
    let tone: Vec<i16> = (0..rate / 4)
        .map(|n| ((TAU * 1000.0 * f64::from(n) / f64::from(rate)).sin() * 16000.0) as i16)
        .collect();
    let url: Arc<str> = Arc::from(temp_wav("tone", rate, &tone));
    let sys = AudioSystem::offline(SR);
    let g = |play| {
        graph(vec![
            (
                "sf",
                AudioNode::SoundFile {
                    url: url.clone(),
                    rate: 1.0,
                    pan: 0.0,
                    amp: 1.0,
                    looping: false,
                    play,
                },
            ),
            ("out", out("sf")),
        ])
    };
    // play is held from the start: it starts once the file has loaded.
    let mut ready = None;
    for _ in 0..500 {
        sys.submit(g(true));
        if let Some(SampleState::Ready { duration }) = sys.feedback().samples.get(&url) {
            ready = Some(*duration);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // 5512 frames at 22.05 kHz.
    assert_eq!(ready, Some(5512.0 / 22050.0));
    sys.submit(g(true));
    let frames = sys.render(secs(0.1));
    let x = left(&frames);
    assert!(close(zero_cross_freq(&x[secs(0.01)..]), 1000.0, 1.0));
    // Centre pan is equal-power: cos(π/4) on each side.
    let peak = x.iter().fold(0f32, |m, v| m.max(v.abs()));
    assert!(
        close(
            f64::from(peak),
            16000.0 / 32768.0 * std::f64::consts::FRAC_1_SQRT_2,
            0.01
        ),
        "{peak}"
    );
    assert!(frames.iter().all(|f| (f[0] - f[1]).abs() < 1e-6));
    let head = sys.feedback().playheads.get("sf").copied().unwrap();
    assert!(close(head, 0.1, 0.005), "playhead {head}");
    // Past the end it stops.
    let rest = left(&sys.render(secs(0.2)));
    assert!(rest[secs(0.17)..].iter().all(|&v| v == 0.0));
    assert!(!sys.feedback().playheads.contains_key("sf"));
    let _ = std::fs::remove_file(&*url);
}

#[test]
fn fan_out_and_nodes_that_skip_a_tick() {
    let sys = AudioSystem::offline(SR);
    let full = graph(vec![
        ("o", osc(Wave::Sine, 440.0, 0.25)),
        ("a", out("o")),
        ("b", out("o")),
    ]);
    let live = full.live.clone();
    sys.submit(full);
    let x = left(&sys.render(secs(0.2)));
    let peak = x[secs(0.05)..].iter().fold(0f32, |m, v| m.max(v.abs()));
    assert!(close(f64::from(peak), 0.5, 0.01), "both outputs: {peak}");
    // Live but not evaluated this tick: keeps playing.
    sys.submit(AudioGraph {
        declared: Default::default(),
        live,
    });
    assert!(rms(&left(&sys.render(secs(0.05)))) > 0.3);
    // Gone from the patch: silent.
    sys.submit(AudioGraph::default());
    // (What was already rendered of the current quantum still plays.)
    let x = left(&sys.render(secs(0.05)));
    assert!(x[QUANTUM..].iter().all(|&v| v == 0.0));
}

#[test]
fn rewiring_keeps_running_dsp_state() {
    let sys = AudioSystem::offline(SR);
    sys.submit(graph(vec![
        ("o", osc(Wave::Sine, 440.0, 0.8)),
        ("out", out("o")),
    ]));
    let mut x = left(&sys.render(secs(0.1)));
    // Insert a unity gain: the oscillator keeps its phase, no click.
    sys.submit(graph(vec![
        ("o", osc(Wave::Sine, 440.0, 0.8)),
        (
            "g",
            AudioNode::Gain {
                input: input("o"),
                level: 1.0,
            },
        ),
        ("out", out("g")),
    ]));
    x.extend(left(&sys.render(secs(0.1))));
    let max_step = x[secs(0.05)..]
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0f32, f32::max);
    let smooth = (TAU * 440.0 / f64::from(SR) * 0.8) as f32 + 1e-3;
    assert!(max_step <= smooth, "{max_step}");
}

#[test]
fn compressor_distortion_and_reverb() {
    let chain = |node: AudioNode, src: AudioNode| {
        let sys = AudioSystem::offline(SR);
        sys.submit(graph(vec![("o", src), ("fx", node), ("out", out("fx"))]));
        left(&sys.render(secs(0.6)))[secs(0.2)..].to_vec()
    };
    let loud = osc(Wave::Sine, 200.0, 1.0);
    let comp = |amp| {
        let c = AudioNode::Compressor {
            input: input("o"),
            threshold: -24.0,
            ratio: 12.0,
            knee: 30.0,
            attack: 0.003,
            release: 0.25,
        };
        rms(&chain(c, osc(Wave::Sine, 200.0, amp)))
    };
    // 20 dB in, ~14 dB out (the default 30 dB knee is soft); make-up gain
    // keeps the level up.
    let (hi, lo) = (comp(1.0), comp(0.1));
    assert!(hi / lo < 6.0, "compression {hi} / {lo}");
    assert!(hi > 0.4 && hi < 0.7, "loud rms {hi}");

    let dist = chain(
        AudioNode::Distortion {
            input: input("o"),
            amount: 1.0,
            wet: 1.0,
            oversample: Oversample::X4,
        },
        loud.clone(),
    );
    assert!(
        tone(&dist, 600.0) > 0.1 * tone(&dist, 200.0),
        "odd harmonics"
    );
    assert!(
        tone(&dist, 400.0) < 0.01,
        "symmetric curve: no even harmonics"
    );

    // Reverb: the tail keeps ringing after the source stops.
    let sys = AudioSystem::offline(SR);
    let g = |gate| {
        graph(vec![
            ("o", osc(Wave::Sine, 300.0, 1.0)),
            (
                "env",
                AudioNode::Envelope {
                    input: input("o"),
                    attack: 0.0,
                    decay: 0.0,
                    sustain: 1.0,
                    release: 0.0,
                    level: 1.0,
                    gate,
                },
            ),
            (
                "rv",
                AudioNode::Reverb {
                    input: input("env"),
                    seconds: 1.0,
                    decay: 2.0,
                    reverse: false,
                    wet: 1.0,
                    seed: 9,
                },
            ),
            ("out", out("rv")),
        ])
    };
    sys.submit(g(true));
    let on = left(&sys.render(secs(0.3)));
    sys.submit(g(false));
    let tail = left(&sys.render(secs(1.1)));
    let (a, b) = (rms(&tail[..secs(0.2)]), rms(&tail[secs(0.6)..secs(0.8)]));
    assert!(rms(&on) > 0.01 && a > 0.01, "wet {} tail {a}", rms(&on));
    assert!(b < a * 0.5, "decays: {a} → {b}");
    assert!(rms(&tail[secs(1.05)..]) < 1e-4, "ends with the IR");
}

#[test]
fn mono_synth_plays_its_note() {
    let sys = AudioSystem::offline(SR);
    let synth = |gate| AudioNode::MonoSynth {
        freq: 440.0,
        velocity: 0.7,
        attack: 0.01,
        decay: 0.05,
        sustain: 0.5,
        release: 0.05,
        gate,
    };
    sys.submit(graph(vec![("s", synth(false)), ("out", out("s"))]));
    assert!(left(&sys.render(1024)).iter().all(|&v| v == 0.0));
    sys.submit(graph(vec![("s", synth(true)), ("out", out("s"))]));
    let x = left(&sys.render(secs(0.3)));
    assert!(close(zero_cross_freq(&x[secs(0.1)..]), 440.0, 0.5));
    // Sustained at velocity × sustain: a saw of amplitude 0.35.
    assert!(close(rms(&x[secs(0.1)..]), 0.35 / 3f64.sqrt(), 0.01));
    sys.submit(graph(vec![("s", synth(false)), ("out", out("s"))]));
    let x = left(&sys.render(secs(0.1)));
    assert!(x[secs(0.06)..].iter().all(|&v| v == 0.0));
}
