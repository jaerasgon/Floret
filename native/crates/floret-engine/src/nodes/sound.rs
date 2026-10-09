//! `sound/*` nodes. Ported from `src/nodes/sound/`.
//!
//! Each node declares its audio to the tick's backend (`crate::audio`) and
//! wires out an [`AudioRef`] to itself; the app's audio engine does the
//! DSP. Inputs are read and clamped exactly like the TS nodes before they
//! reach Web Audio, and a value Web Audio would reject (a non-finite
//! `AudioParam` value) fails the node the same way.
//!
//! With no backend installed every node except `sound/midi-to-freq`,
//! `sound/freq-to-midi` and a URL-less `sound/sound-file` fails, as the TS
//! nodes do without an `AudioContext`: the runtime emits empty outputs.

use super::shared::first;
use super::Registrar;
use crate::audio::{
    self, AudioFeedback, AudioNode, AudioRef, FilterType, MicState, NoiseColor, Oversample,
    SampleState, Wave,
};
use crate::js;
use crate::node::{raw, scalar, EvalCtx, NodeImpl, NodeInit, NodeResult};
use crate::particles::Handle;
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Spread, Value};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("sound/oscillator", oscillator);
    r.node("sound/pulse", pulse);
    r.node("sound/noise", noise);
    r.node("sound/mono-synth", mono_synth);
    r.node("sound/sound-file", sound_file);
    r.node("sound/audio-in", audio_in);
    r.node("sound/gain", gain);
    r.node("sound/filter", filter);
    r.node("sound/delay", delay);
    r.node("sound/reverb", reverb);
    r.node("sound/distortion", distortion);
    r.node("sound/compressor", compressor);
    r.node("sound/envelope", envelope);
    r.node("sound/amplitude", amplitude);
    r.node("sound/fft", fft);
    r.node("sound/peak-detect", peak_detect);
    r.node("sound/output", output);
    r.node("sound/midi-to-freq", midi_to_freq);
    r.node("sound/freq-to-midi", freq_to_midi);
    r.node("sound/mix", mix);
    r.dynamic("sound/mix", mix_inputs);
    r.node("sound/scale", scale);
    r.dynamic("sound/scale", scale_inputs);
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// The tick's audio feedback, or the error the TS nodes hit without an
/// `AudioContext`.
fn context() -> Result<Arc<AudioFeedback>, String> {
    audio::feedback().ok_or_else(|| "no audio output: AudioContext is unavailable".to_owned())
}

/// `((inputs[i] ?? [])[0] as number | undefined) ?? fallback`, as the
/// number Web Audio (or `Math.min`) ends up seeing.
fn arg(inputs: &[Spread], i: usize, fallback: f64) -> f64 {
    let v = first(inputs, i);
    if v.is_nullish() {
        fallback
    } else {
        v.to_number()
    }
}

/// `!!((inputs[i] ?? [])[0])`.
fn flag(inputs: &[Spread], i: usize) -> bool {
    first(inputs, i).truthy()
}

/// `Math.max(lo, Math.min(hi, x))` (NaN stays NaN).
fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    js::max(lo, js::min(hi, x))
}

/// An `AudioParam` value: Web Audio throws a TypeError on non-finite ones.
fn finite(x: f64) -> Result<f64, String> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err("The provided float value is non-finite.".into())
    }
}

/// The upstream node on an `audio` input, if one is wired and live.
fn upstream(inputs: &[Spread], i: usize) -> Option<Arc<str>> {
    match first(inputs, i) {
        Value::Handle(Handle::Audio(a)) => Some(a.node),
        _ => None,
    }
}

fn handle(cx: &EvalCtx, kind: &'static str) -> Spread {
    one(Value::Handle(Handle::Audio(AudioRef {
        node: Arc::from(cx.node_id),
        kind,
    })))
}

/// Declare `node` and output the handle (the shape of most sound nodes).
fn declare(cx: &EvalCtx, kind: &'static str, node: AudioNode) -> NodeResult {
    audio::declare(cx.node_id, Some(node));
    Ok(vec![handle(cx, kind)])
}

/// A string param (`params.param<string>(key, fallback)`); non-strings are
/// whatever Web Audio does with an invalid enum: ignored.
fn param_str(cx: &EvalCtx, key: &str) -> Option<String> {
    match cx.param(key) {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Seed for a node's noise / impulse response (TS used `Math.random`).
fn seed_of(cx: &EvalCtx) -> u32 {
    crate::particles::fnv1a(cx.node_id.as_bytes())
}

fn midi_to_hz(midi: f64) -> f64 {
    440.0 * 2f64.powf((midi - 69.0) / 12.0)
}

// ─── Sources ─────────────────────────────────────────────────────────────────

fn oscillator(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let freq = arg(inputs, 0, 440.0);
        let amp = arg(inputs, 1, 0.5);
        // An unknown type is ignored by `OscillatorNode.type`: it stays sine.
        let wave = match param_str(cx, "type").as_deref() {
            Some("triangle") => Wave::Triangle,
            Some("sawtooth") => Wave::Sawtooth,
            Some("square") => Wave::Square,
            _ => Wave::Sine,
        };
        context()?;
        let node = AudioNode::Oscillator {
            wave,
            freq: finite(freq)?,
            amp: finite(clamp(amp, 0.0, 1.0))?,
        };
        declare(cx, "oscillator", node)
    })
}

fn pulse(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let freq = arg(inputs, 0, 220.0);
        let amp = arg(inputs, 1, 0.5);
        // A NaN width builds an all −1 curve: the same as width ≥ 1.
        let width = clamp(arg(inputs, 2, 0.0), 0.0, 0.99);
        context()?;
        let node = AudioNode::Pulse {
            freq: finite(freq)?,
            amp: finite(clamp(amp, 0.0, 1.0))?,
            width: if width.is_nan() { 1.0 } else { width },
        };
        declare(cx, "pulse", node)
    })
}

fn noise(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let amp = arg(inputs, 0, 0.5);
        // `buildBuffer` falls through to brown for anything else.
        let color = match param_str(cx, "type").as_deref() {
            Some("white") | None => NoiseColor::White,
            Some("pink") => NoiseColor::Pink,
            Some(_) => NoiseColor::Brown,
        };
        context()?;
        let node = AudioNode::Noise {
            color,
            amp: finite(clamp(amp, 0.0, 1.0))?,
            seed: seed_of(cx),
        };
        declare(cx, "noise", node)
    })
}

fn mono_synth(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let note = arg(inputs, 0, 60.0);
        let velocity = arg(inputs, 1, 0.7);
        let attack = arg(inputs, 2, 0.02);
        let decay = arg(inputs, 3, 0.25);
        let sustain = arg(inputs, 4, 0.05);
        let release = arg(inputs, 5, 0.35);
        let gate = flag(inputs, 6);
        context()?;
        let node = AudioNode::MonoSynth {
            freq: finite(midi_to_hz(clamp(note, 0.0, 127.0)))?,
            velocity: finite(clamp(velocity, 0.0, 1.0))?,
            attack: finite(clamp(attack, 0.0, 10.0))?,
            decay: finite(clamp(decay, 0.0, 10.0))?,
            sustain: finite(clamp(sustain, 0.0, 1.0))?,
            release: finite(clamp(release, 0.0, 10.0))?,
            gate,
        };
        declare(cx, "mono-synth", node)
    })
}

fn sound_file(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let url = match first(inputs, 0) {
            Value::Str(s) => s.trim().to_owned(),
            _ => String::new(),
        };
        let lower = url.get(..11).unwrap_or(&url).to_ascii_lowercase();
        let url = if lower.starts_with("data:") && !lower.starts_with("data:audio/") {
            String::new()
        } else {
            url
        };
        let idle = || {
            vec![
                one(Value::Null),
                one(false.into()),
                one(Value::Num(0.0)),
                one(Value::Num(0.0)),
            ]
        };
        if url.is_empty() {
            audio::declare(cx.node_id, None);
            return Ok(idle());
        }
        let rate = arg(inputs, 1, 1.0);
        let pan = arg(inputs, 2, 0.0);
        let amp = arg(inputs, 3, 1.0);
        let looping = flag(inputs, 4);
        let play = flag(inputs, 5);
        let fb = context()?;
        let url: Arc<str> = Arc::from(url);
        let duration = match fb.samples.get(&url) {
            Some(SampleState::Ready { duration }) => Some(*duration),
            _ => None,
        };
        let node = AudioNode::SoundFile {
            url,
            rate: finite(rate)?,
            pan: finite(clamp(pan, -1.0, 1.0))?,
            amp: finite(clamp(amp, 0.0, 2.0))?,
            looping,
            play,
        };
        audio::declare(cx.node_id, Some(node));
        let Some(duration) = duration else {
            return Ok(idle());
        };
        let position = fb.playheads.get(cx.node_id).copied().unwrap_or(0.0);
        Ok(vec![
            handle(cx, "sound-file"),
            one(true.into()),
            one(Value::Num(duration)),
            one(Value::Num(position)),
        ])
    })
}

/// Outputs `null` until the microphone stream is live.
fn audio_in(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let amp = arg(inputs, 0, 0.5);
        let start = flag(inputs, 1);
        let fb = context()?;
        audio::declare(
            cx.node_id,
            Some(AudioNode::AudioIn {
                amp: finite(clamp(amp, 0.0, 1.0))?,
                start,
            }),
        );
        if start && fb.mic == MicState::Live {
            Ok(vec![handle(cx, "audio-in")])
        } else {
            Ok(vec![one(Value::Null)])
        }
    })
}

// ─── Processors ──────────────────────────────────────────────────────────────

fn gain(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let input = upstream(inputs, 0);
        let level = arg(inputs, 1, 1.0);
        context()?;
        let level = finite(clamp(level, 0.0, 2.0))?;
        declare(cx, "gain", AudioNode::Gain { input, level })
    })
}

fn filter(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let input = upstream(inputs, 0);
        let freq = arg(inputs, 1, 1000.0);
        let res = arg(inputs, 2, 1.0);
        let kind = match param_str(cx, "type").as_deref() {
            Some("highpass") => FilterType::Highpass,
            Some("bandpass") => FilterType::Bandpass,
            _ => FilterType::Lowpass,
        };
        context()?;
        let node = AudioNode::Filter {
            input,
            kind,
            freq: finite(clamp(freq, 20.0, 20000.0))?,
            q: finite(clamp(res, 0.0001, 50.0))?,
        };
        declare(cx, "filter", node)
    })
}

fn delay(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let input = upstream(inputs, 0);
        let time = arg(inputs, 1, 0.25);
        let feedback = arg(inputs, 2, 0.4);
        let damping = arg(inputs, 3, 2300.0);
        context()?;
        let node = AudioNode::Delay {
            input,
            time: finite(clamp(time, 0.0, 2.0))?,
            feedback: finite(clamp(feedback, 0.0, 0.99))?,
            damping: finite(clamp(damping, 20.0, 20000.0))?,
        };
        declare(cx, "delay", node)
    })
}

fn reverb(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let input = upstream(inputs, 0);
        let seconds = arg(inputs, 1, 3.0);
        let decay = arg(inputs, 2, 2.0);
        let wet = arg(inputs, 3, 0.5);
        let reverse = cx.param_value("reverse").truthy();
        context()?;
        let node = AudioNode::Reverb {
            input,
            seconds: finite(clamp(seconds, 0.001, 10.0))?,
            decay: finite(clamp(decay, 0.0, 100.0))?,
            reverse,
            wet: finite(clamp(wet, 0.0, 1.0))?,
            seed: seed_of(cx),
        };
        declare(cx, "reverb", node)
    })
}

fn distortion(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let input = upstream(inputs, 0);
        let amount = arg(inputs, 1, 0.25);
        let wet = arg(inputs, 2, 1.0);
        let oversample = match param_str(cx, "oversample").as_deref() {
            Some("2x") => Oversample::X2,
            Some("4x") => Oversample::X4,
            _ => Oversample::None,
        };
        context()?;
        let amount = clamp(amount, 0.0, 1.0);
        let node = AudioNode::Distortion {
            input,
            // A NaN amount builds an all-NaN curve; silence is the sane reading.
            amount: if amount.is_nan() { 0.0 } else { amount },
            wet: finite(clamp(wet, 0.0, 1.0))?,
            oversample,
        };
        declare(cx, "distortion", node)
    })
}

fn compressor(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let input = upstream(inputs, 0);
        let threshold = arg(inputs, 1, -24.0);
        let ratio = arg(inputs, 2, 12.0);
        let knee = arg(inputs, 3, 30.0);
        let attack = arg(inputs, 4, 0.003);
        let release = arg(inputs, 5, 0.25);
        context()?;
        let node = AudioNode::Compressor {
            input,
            threshold: finite(clamp(threshold, -100.0, 0.0))?,
            ratio: finite(clamp(ratio, 1.0, 20.0))?,
            knee: finite(clamp(knee, 0.0, 40.0))?,
            attack: finite(clamp(attack, 0.0, 1.0))?,
            release: finite(clamp(release, 0.0, 1.0))?,
        };
        declare(cx, "compressor", node)
    })
}

fn envelope(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let input = upstream(inputs, 0);
        let attack = arg(inputs, 1, 0.05);
        let decay = arg(inputs, 2, 0.1);
        let sustain = arg(inputs, 3, 0.5);
        let release = arg(inputs, 4, 0.5);
        let level = arg(inputs, 5, 1.0);
        let gate = flag(inputs, 6);
        context()?;
        let node = AudioNode::Envelope {
            input,
            attack: finite(clamp(attack, 0.0, 10.0))?,
            decay: finite(clamp(decay, 0.0, 10.0))?,
            sustain: finite(clamp(sustain, 0.0, 1.0))?,
            release: finite(clamp(release, 0.0, 10.0))?,
            level: finite(clamp(level, 0.0, 2.0))?,
            gate,
        };
        declare(cx, "envelope", node)
    })
}

fn output(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let input = upstream(inputs, 0);
        let volume = match first(inputs, 1) {
            Value::Num(v) => v,
            _ => 1.0,
        };
        context()?;
        let volume = finite(clamp(volume, 0.0, 2.0))?;
        audio::declare(cx.node_id, Some(AudioNode::Output { input, volume }));
        Ok(vec![])
    })
}

/// Most inputs a Mix takes (the editor's `− n +` goes to 32).
const MAX_MIX_SLOTS: usize = 32;

/// `sound/mix`'s pins: `slotCount` audio inputs `in1`, `in2`, … and
/// `level` (`DynamicInputsFn`).
fn mix_inputs(params: &Params) -> Vec<PortSpec> {
    let raw = params
        .get("slotCount")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(2.0);
    let n = if raw.is_finite() && raw >= 1.0 {
        (raw.floor() as usize).min(MAX_MIX_SLOTS)
    } else {
        2
    };
    let mut pins: Vec<PortSpec> = (1..=n)
        .map(|i| PortSpec::new(format!("in{i}"), PortType::Audio).label(i.to_string()))
        .collect();
    pins.push(PortSpec::new("level", PortType::Number).default(Value::Num(1.0)));
    pins
}

/// Mix: the sum of every wired input, × `level`. Lets several voices
/// share one effect chain, analyser or output.
fn mix(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let n = inputs.len().saturating_sub(1);
        let mut sources: Vec<Arc<str>> = Vec::new();
        for i in 0..n {
            if let Some(id) = upstream(inputs, i) {
                if !sources.contains(&id) {
                    sources.push(id);
                }
            }
        }
        let level = arg(inputs, n, 1.0);
        context()?;
        let level = finite(clamp(level, 0.0, 2.0))?;
        declare(
            cx,
            "mix",
            AudioNode::Mix {
                inputs: sources,
                level,
            },
        )
    })
}

// ─── Analysers ───────────────────────────────────────────────────────────────

/// Declare an analyser tap and return its data from the start of the tick
/// (`None` until it has any: a fresh `AnalyserNode` reads zeros).
fn analyse(
    cx: &EvalCtx,
    fb: &AudioFeedback,
    input: Option<Arc<str>>,
    fft_size: usize,
    smoothing: f64,
) -> Option<audio::Analysis> {
    audio::declare(
        cx.node_id,
        Some(AudioNode::Analyser {
            input,
            fft_size,
            smoothing,
        }),
    );
    fb.analysers
        .get(cx.node_id)
        .filter(|a| a.time.len() == fft_size && a.bytes.len() == fft_size / 2)
        .cloned()
}

/// RMS of the last 2048 samples, smoothed per tick (EMA).
fn amplitude(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut prev = 0.0;
    raw(move |inputs, cx| {
        let input = upstream(inputs, 0);
        let smoothing = arg(inputs, 1, 0.4);
        let fb = context()?;
        let rms = match analyse(cx, &fb, input, 2048, 0.0) {
            Some(a) => {
                let sum: f64 = a.time.iter().map(|&x| f64::from(x) * f64::from(x)).sum();
                (sum / a.time.len() as f64).sqrt()
            }
            None => 0.0,
        };
        let k = clamp(smoothing, 0.0, 0.999);
        prev = prev * k + rms * (1.0 - k);
        Ok(vec![one(Value::Num(prev))])
    })
}

/// `bins` byte-magnitude bins and `2 × bins` waveform samples.
fn fft(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let input = upstream(inputs, 0);
        let smoothing = arg(inputs, 1, 0.8);
        let bins = match cx.param_value("bins") {
            Value::Undefined => 128.0,
            Value::Str(s) => js::parse_int(&s),
            Value::Num(n) => js::parse_int(&js::num_to_string(n)),
            _ => f64::NAN,
        };
        let fb = context()?;
        // `AnalyserNode.fftSize` takes a power of two in 32..=32768.
        let size = bins * 2.0;
        if !(32.0..=32768.0).contains(&size)
            || (size as u32).count_ones() != 1
            || size.fract() != 0.0
        {
            return Err(format!(
                "Failed to set the 'fftSize' property on 'AnalyserNode': The value provided ({}) is not a power of two in [32, 32768].",
                js::num_to_string(size)
            ));
        }
        let smoothing = clamp(smoothing, 0.0, 0.999);
        if smoothing.is_nan() {
            return Err("The provided double value is non-finite.".into());
        }
        let size = size as usize;
        let (spectrum, waveform): (Vec<Value>, Vec<Value>) =
            match analyse(cx, &fb, input, size, smoothing) {
                Some(a) => (
                    a.bytes.iter().map(|&b| Value::Num(f64::from(b))).collect(),
                    a.time.iter().map(|&x| Value::Num(f64::from(x))).collect(),
                ),
                None => (vec![Value::Num(0.0); size / 2], vec![Value::Num(0.0); size]),
            };
        Ok(vec![Arc::from(spectrum), Arc::from(waveform)])
    })
}

const MIN_FRAMES_BETWEEN_PEAKS: u32 = 20;

/// True on a tick where the band's loudest bin crosses `threshold`, at
/// most once every 20 ticks.
fn peak_detect(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut since_peak: u32 = 0;
    raw(move |inputs, cx| {
        let input = upstream(inputs, 0);
        let freq1 = arg(inputs, 1, 40.0);
        let freq2 = arg(inputs, 2, 120.0);
        let threshold = arg(inputs, 3, 0.9);
        let fb = context()?;
        let bytes = analyse(cx, &fb, input, 512, 0.5).map(|a| a.bytes);
        let bin_hz = fb.sample_rate / 512.0;
        let i1 = js::max(0.0, (freq1 / bin_hz).floor());
        let i2 = js::min(255.0, (freq2 / bin_hz).ceil());
        let mut max = 0u8;
        let mut i = i1;
        while i <= i2 {
            let v = bytes
                .as_ref()
                .and_then(|b| b.get(i as usize))
                .copied()
                .unwrap_or(0);
            max = max.max(v);
            i += 1.0;
        }
        let normalised = f64::from(max) / 255.0;
        since_peak = since_peak.saturating_add(1);
        let detected = normalised >= threshold && since_peak >= MIN_FRAMES_BETWEEN_PEAKS;
        if detected {
            since_peak = 0;
        }
        Ok(vec![one(Value::Bool(detected))])
    })
}

// ─── MIDI ↔ frequency (pure) ─────────────────────────────────────────────────

/// `typeof v === 'number' ? v : Number(v) || 0`.
fn number_or_zero(v: &Value) -> f64 {
    match v {
        Value::Num(n) => *n,
        other => {
            let n = other.to_number();
            if n.is_nan() || n == 0.0 {
                0.0
            } else {
                n
            }
        }
    }
}

fn midi_to_freq(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        Ok(vec![Value::Num(midi_to_hz(number_or_zero(&a[0])))])
    })
}

fn freq_to_midi(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        let f = number_or_zero(&a[0]);
        if f <= 0.0 {
            return Ok(vec![Value::Num(0.0)]);
        }
        Ok(vec![Value::Num(69.0 + 12.0 * (f / 440.0).log2())])
    })
}

// ─── sound/scale ─────────────────────────────────────────────────────────────

/// The scales `sound/scale` knows: name and semitone steps from the root.
pub const SCALES: &[(&str, &[f64])] = &[
    ("major", &[0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 11.0]),
    ("minor", &[0.0, 2.0, 3.0, 5.0, 7.0, 8.0, 10.0]),
    ("harmonic-minor", &[0.0, 2.0, 3.0, 5.0, 7.0, 8.0, 11.0]),
    ("dorian", &[0.0, 2.0, 3.0, 5.0, 7.0, 9.0, 10.0]),
    ("phrygian", &[0.0, 1.0, 3.0, 5.0, 7.0, 8.0, 10.0]),
    ("lydian", &[0.0, 2.0, 4.0, 6.0, 7.0, 9.0, 11.0]),
    ("mixolydian", &[0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 10.0]),
    ("pentatonic", &[0.0, 2.0, 4.0, 7.0, 9.0]),
    ("minor-pentatonic", &[0.0, 3.0, 5.0, 7.0, 10.0]),
    ("blues", &[0.0, 3.0, 5.0, 6.0, 7.0, 10.0]),
    ("whole-tone", &[0.0, 2.0, 4.0, 6.0, 8.0, 10.0]),
    (
        "chromatic",
        &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0],
    ),
];

/// `sound/scale`'s `mode`: what its first pin means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScaleMode {
    /// A step of the scale (0 = the root, 7 = the root an octave up in a
    /// seven-note scale, −1 the note below the root).
    Degree,
    /// Any MIDI note, moved to the nearest note of the scale.
    Quantize,
    /// 0..1 across `octaves` octaves of the scale from the root.
    Range,
}

impl ScaleMode {
    fn of(params: &Params) -> ScaleMode {
        match params.get("mode").and_then(|v| v.as_str()) {
            Some("quantize") => ScaleMode::Quantize,
            Some("range") => ScaleMode::Range,
            _ => ScaleMode::Degree,
        }
    }
}

/// `sound/scale`'s pins for its `mode` (`DynamicInputsFn`); `steps`
/// shows while `scale` is `custom`.
fn scale_inputs(params: &Params) -> Vec<PortSpec> {
    let n = |name: &str, d: f64| PortSpec::new(name, PortType::Number).default(Value::Num(d));
    let mut pins = match ScaleMode::of(params) {
        ScaleMode::Degree => {
            let mut octave = n("octave", 0.0);
            octave.integer = true;
            vec![
                n("degree", 0.0).kind(PortKind::Spread),
                n("root", 60.0),
                octave,
            ]
        }
        ScaleMode::Quantize => vec![n("note", 60.0).kind(PortKind::Spread), n("root", 60.0)],
        ScaleMode::Range => vec![
            n("amount", 0.0).kind(PortKind::Spread),
            n("root", 60.0),
            n("octaves", 2.0),
        ],
    };
    let mut scale = PortSpec::new("scale", PortType::String).default(Value::str("major"));
    scale.param = Some("scale".into());
    pins.push(scale);
    if params.get("scale").and_then(|v| v.as_str()) == Some("custom") {
        pins.push(PortSpec::new("steps", PortType::String).default(Value::str("0 2 4 7 9")));
    }
    pins
}

/// The steps of a scale by name, or of a `custom` list (semitones from
/// the root, separated by spaces or commas, folded into one octave and
/// sorted). Unknown names read as major; an empty custom list as the root.
fn scale_steps(name: &str, custom: &str) -> Vec<f64> {
    if name == "custom" {
        let mut steps: Vec<f64> = custom
            .split([' ', ',', '\t', ';'])
            .filter_map(|s| s.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .map(|v| v.rem_euclid(12.0))
            .collect();
        steps.sort_by(f64::total_cmp);
        steps.dedup();
        if steps.is_empty() {
            steps.push(0.0);
        }
        return steps;
    }
    SCALES
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap_or(&SCALES[0])
        .1
        .to_vec()
}

/// The MIDI note of scale `degree` (rounded to a whole step) above
/// `root`, and its index in the scale.
fn degree_note(steps: &[f64], root: f64, degree: f64) -> (f64, usize) {
    let len = steps.len() as i64;
    let d = degree.round() as i64;
    let (oct, i) = (d.div_euclid(len), d.rem_euclid(len) as usize);
    (root + 12.0 * oct as f64 + steps[i], i)
}

/// The note of the scale nearest `note` (ties go up), and its index.
fn quantize_note(steps: &[f64], root: f64, note: f64) -> (f64, usize) {
    let oct = ((note - root) / 12.0).floor();
    let mut best = (f64::INFINITY, 0.0, 0);
    for o in [oct - 1.0, oct, oct + 1.0] {
        for (i, s) in steps.iter().enumerate() {
            let cand = root + 12.0 * o + s;
            let d = (cand - note).abs();
            if d < best.0 || (d == best.0 && cand > best.1) {
                best = (d, cand, i);
            }
        }
    }
    (best.1, best.2)
}

/// Scale: snap numbers to the notes of a musical scale. `degree` counts
/// steps of the scale up from `root` (+ `octave` octaves); `quantize`
/// moves any MIDI note to the nearest note of the scale; `range` spreads
/// 0..1 over `octaves` octaves of it, so a mouse, a slider or noise plays
/// in key. Per element over the first pin's spread. Outputs the MIDI
/// `note`, its `freq` in Hz and its `index` in the scale (0 = the root).
fn scale(init: &NodeInit) -> Box<dyn NodeImpl> {
    let mode = ScaleMode::of(init.params);
    let names: Vec<String> = init.inputs.iter().map(|p| p.name.clone()).collect();
    raw(move |inputs, cx| {
        let pin = |name: &str| names.iter().position(|n| n == name);
        let single = |name: &str, d: f64| {
            let v = pin(name).map_or(Value::Undefined, |i| first(inputs, i));
            let x = if v.is_nullish() { d } else { v.to_number() };
            if x.is_finite() {
                x
            } else {
                d
            }
        };
        let name = match pin("scale").map(|i| first(inputs, i)) {
            Some(Value::Str(s)) => s.to_string(),
            _ => param_str(cx, "scale").unwrap_or_else(|| "major".into()),
        };
        let custom = pin("steps")
            .map(|i| super::convert::js_string_or_empty(&first(inputs, i)))
            .unwrap_or_default();
        let steps = scale_steps(&name, &custom);
        let root = single("root", 60.0);
        let values = inputs.first().cloned().unwrap_or_else(crate::value::empty);
        let mut notes = Vec::with_capacity(values.len());
        let mut freqs = Vec::with_capacity(values.len());
        let mut index = Vec::with_capacity(values.len());
        for v in values.iter() {
            let x = v.to_number();
            if !x.is_finite() {
                notes.push(Value::Num(f64::NAN));
                freqs.push(Value::Num(f64::NAN));
                index.push(Value::Num(f64::NAN));
                continue;
            }
            let (note, i) = match mode {
                ScaleMode::Degree => {
                    let octave = single("octave", 0.0).round();
                    degree_note(&steps, root + 12.0 * octave, x)
                }
                ScaleMode::Quantize => quantize_note(&steps, root, x),
                ScaleMode::Range => {
                    let octaves = single("octaves", 2.0).clamp(0.0, 10.0);
                    let top = (steps.len() as f64 * octaves).round();
                    let d = (x.clamp(0.0, 1.0) * (top + 1.0)).floor().min(top);
                    degree_note(&steps, root, d)
                }
            };
            notes.push(Value::Num(note));
            freqs.push(Value::Num(midi_to_hz(note)));
            index.push(Value::Num(i as f64));
        }
        Ok(vec![Arc::from(notes), Arc::from(freqs), Arc::from(index)])
    })
}

#[cfg(test)]
mod tests;
