//! The DSP behind each sound node. Everything here runs on the audio
//! thread: no allocation, no locks. A node reads its targets from its
//! [`Params`] once per render quantum and ramps toward them per sample.

use crate::analysis::Tap;
use crate::compressor::Compressor;
use crate::decode::SampleData;
use crate::dsp::{
    samples, wrap01, Adsr, Biquad, BiquadKind, Bus, Lowpass4, Ramp, QUANTUM, RAMP_SECONDS, SILENCE,
};
use crate::graph::Params;
use crate::reverb::Reverb;
use crate::wavetable::{PeriodicWave, Shape, WaveOsc};
use floret_engine::audio::{AudioNode, NoiseColor, Wave};
use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

/// What every node may read besides its own input.
pub struct Ctx<'a> {
    pub sample_rate: f32,
    /// This quantum's microphone samples, while the input stream is open.
    pub mic: Option<&'a [f32; QUANTUM]>,
}

pub enum Dsp {
    Osc(Osc),
    Pulse(Box<Pulse>),
    Noise(Noise),
    Synth(Synth),
    Player(Box<Player>),
    Mic(Ramp),
    Gain(Ramp),
    Filter(Filter),
    Delay(Box<Delay>),
    Reverb(Box<Reverb>),
    Shaper(Box<Shaper>),
    Compressor(Box<Compressor>),
    Envelope(Envelope),
    Analyser(Arc<Tap>),
    Output(Ramp),
}

/// Parameter slots, per node kind (see `graph::write_params`).
pub mod slot {
    pub const FREQ: usize = 0;
    pub const AMP: usize = 1;
    pub const WIDTH: usize = 2;
    pub const GATE: usize = 7;
}

impl Dsp {
    /// Build the DSP for a declaration (control thread: may allocate).
    pub fn new(
        node: &AudioNode,
        sample_rate: f32,
        sample: Option<Arc<SampleData>>,
        tap: Option<Arc<Tap>>,
    ) -> Dsp {
        let sr = sample_rate;
        let ramp = |v: f64| Ramp::new(v as f32, RAMP_SECONDS, sr);
        match node {
            AudioNode::Oscillator { wave, freq, .. } => Dsp::Osc(Osc {
                table: match wave {
                    Wave::Sine => None,
                    Wave::Square => Some(table(Shape::Square, sr)),
                    Wave::Sawtooth => Some(table(Shape::Sawtooth, sr)),
                    Wave::Triangle => Some(table(Shape::Triangle, sr)),
                },
                phase: 0.0,
                freq: ramp(*freq),
                amp: ramp(0.0),
            }),
            AudioNode::Pulse { freq, .. } => Dsp::Pulse(Box::new(Pulse {
                saw: table(Shape::Sawtooth, sr),
                freq: ramp(*freq),
                amp: ramp(0.0),
                width: f32::NAN,
                curve: [0.0; PULSE_CURVE],
            })),
            AudioNode::Noise { color, seed, .. } => Dsp::Noise(Noise {
                buffer: noise_buffer(*color, *seed, sr),
                pos: 0,
                amp: ramp(0.0),
            }),
            AudioNode::MonoSynth { freq, .. } => Dsp::Synth(Synth {
                saw: table(Shape::Sawtooth, sr),
                freq: Ramp::new(*freq as f32, 0.01, sr),
                env: Adsr::default(),
                gate: Gate::default(),
            }),
            AudioNode::SoundFile { pan, amp, .. } => Dsp::Player(Box::new(Player {
                sample,
                pos: 0.0,
                playing: false,
                ended: false,
                gate: Gate::default(),
                pan: ramp(*pan),
                amp: ramp(*amp),
            })),
            AudioNode::AudioIn { .. } => Dsp::Mic(ramp(0.0)),
            AudioNode::Gain { level, .. } | AudioNode::Mix { level, .. } => Dsp::Gain(ramp(*level)),
            AudioNode::Filter { freq, q, .. } => Dsp::Filter(Filter {
                biquad: Biquad::default(),
                freq: ramp(*freq),
                q: ramp(*q),
                kind: None,
            }),
            AudioNode::Delay {
                time,
                feedback,
                damping,
                ..
            } => Dsp::Delay(Box::new(Delay::new(sr, *time, *feedback, *damping))),
            AudioNode::Reverb {
                seconds,
                decay,
                reverse,
                wet,
                seed,
                ..
            } => Dsp::Reverb(Box::new(Reverb::new(
                sr, *seconds, *decay, *reverse, *seed, *wet,
            ))),
            AudioNode::Distortion { wet, .. } => Dsp::Shaper(Box::new(Shaper::new(sr, *wet))),
            AudioNode::Compressor { .. } => Dsp::Compressor(Box::new(Compressor::new(sr))),
            AudioNode::Envelope { .. } => Dsp::Envelope(Envelope {
                env: Adsr::default(),
                gate: Gate::default(),
            }),
            AudioNode::Analyser { .. } => Dsp::Analyser(tap.unwrap_or_default()),
            AudioNode::Output { volume, .. } => Dsp::Output(ramp(*volume)),
        }
    }

    /// Render one quantum into `out`.
    pub fn process(&mut self, p: &Params, input: Option<&Bus>, out: &mut Bus, cx: &Ctx) {
        let input = input.unwrap_or(&SILENCE);
        let sr = cx.sample_rate;
        match self {
            Dsp::Osc(o) => o.process(p, out, sr),
            Dsp::Pulse(o) => o.process(p, out, sr),
            Dsp::Noise(n) => {
                n.amp.set(p.get(0));
                out.channels = 1;
                for y in out.ch[0].iter_mut() {
                    *y = n.buffer[n.pos] * n.amp.tick();
                    n.pos = (n.pos + 1) % n.buffer.len();
                }
            }
            Dsp::Synth(s) => s.process(p, out, sr),
            Dsp::Player(pl) => pl.process(p, out, sr),
            Dsp::Mic(amp) => {
                amp.set(p.get(0));
                out.channels = 1;
                for (i, y) in out.ch[0].iter_mut().enumerate() {
                    let x = cx.mic.map_or(0.0, |m| m[i]);
                    *y = x * amp.tick();
                }
            }
            Dsp::Gain(g) | Dsp::Output(g) => {
                g.set(p.get(0));
                apply_gain(g, input, out);
            }
            Dsp::Filter(f) => f.process(p, input, out, sr),
            Dsp::Delay(d) => d.process(p, input, out, sr),
            Dsp::Reverb(r) => r.process(p, input, out),
            Dsp::Shaper(s) => s.process(p, input, out, sr),
            Dsp::Compressor(c) => c.process(p, input, out),
            Dsp::Envelope(e) => e.process(p, input, out, sr),
            Dsp::Analyser(tap) => {
                tap.push(input);
                out.silence();
            }
        }
    }
}

fn apply_gain(g: &mut Ramp, input: &Bus, out: &mut Bus) {
    out.channels = input.channels;
    for i in 0..QUANTUM {
        let k = g.tick();
        for c in 0..input.channels {
            out.ch[c][i] = input.ch[c][i] * k;
        }
    }
}

// ─── Sources ─────────────────────────────────────────────────────────────────

fn table(shape: Shape, sample_rate: f32) -> WaveOsc {
    WaveOsc::new(PeriodicWave::basic(shape, sample_rate))
}

pub struct Osc {
    /// Chromium's band-limited table; `None` for a sine.
    table: Option<WaveOsc>,
    phase: f64,
    freq: Ramp,
    amp: Ramp,
}

impl Osc {
    fn process(&mut self, p: &Params, out: &mut Bus, sr: f32) {
        self.freq.set(p.get(slot::FREQ));
        self.amp.set(p.get(slot::AMP));
        let nyquist = sr / 2.0;
        out.channels = 1;
        for y in out.ch[0].iter_mut() {
            let f = self.freq.tick().clamp(-nyquist, nyquist);
            let s = match &mut self.table {
                Some(t) => t.next(f),
                None => {
                    let ph = self.phase;
                    self.phase = wrap01(ph + f64::from(f) / f64::from(sr));
                    (std::f64::consts::TAU * ph).sin() as f32
                }
            };
            *y = s * self.amp.tick();
        }
    }
}

/// Points in the TS Pulse's waveshaper curve.
const PULSE_CURVE: usize = 257;

/// The TS Pulse: a sawtooth through a 257-point step curve that is +1
/// where the input exceeds `2·width − 1`, applied like a `WaveShaperNode`
/// (linear interpolation between curve points, no oversampling).
pub struct Pulse {
    saw: WaveOsc,
    freq: Ramp,
    amp: Ramp,
    width: f32,
    curve: [f32; PULSE_CURVE],
}

impl Pulse {
    fn process(&mut self, p: &Params, out: &mut Bus, sr: f32) {
        self.freq.set(p.get(slot::FREQ));
        self.amp.set(p.get(slot::AMP));
        let width = p.get(slot::WIDTH);
        if width.to_bits() != self.width.to_bits() {
            self.width = width;
            let thresh = 2.0 * width - 1.0;
            for (i, c) in self.curve.iter_mut().enumerate() {
                let x = (i as f32 / (PULSE_CURVE - 1) as f32) * 2.0 - 1.0;
                *c = if x > thresh { 1.0 } else { -1.0 };
            }
        }
        let nyquist = sr / 2.0;
        out.channels = 1;
        for y in out.ch[0].iter_mut() {
            let f = self.freq.tick().clamp(-nyquist, nyquist);
            let x = self.saw.next(f);
            *y = shape(&self.curve, x) * self.amp.tick();
        }
    }
}

/// `WaveShaperNode`'s curve lookup.
fn shape(curve: &[f32], x: f32) -> f32 {
    let last = curve.len() - 1;
    let v = last as f32 * 0.5 * (x + 1.0);
    if v.is_nan() || v <= 0.0 {
        curve[0]
    } else if v >= last as f32 {
        curve[last]
    } else {
        let k = v as usize;
        let f = v - k as f32;
        (1.0 - f) * curve[k] + f * curve[k + 1]
    }
}

pub struct Noise {
    buffer: Vec<f32>,
    pos: usize,
    amp: Ramp,
}

/// The TS node's looping 2 s buffer, seeded instead of `Math.random`.
pub fn noise_buffer(color: NoiseColor, seed: u32, sample_rate: f32) -> Vec<f32> {
    let len = ((2.0 * sample_rate) as usize).max(1);
    let mut rng = floret_engine::js::Mulberry32::new(seed);
    let mut white = move || (rng.next_f64() * 2.0 - 1.0) as f32;
    let mut out = Vec::with_capacity(len);
    match color {
        NoiseColor::White => out.extend((0..len).map(|_| white())),
        NoiseColor::Pink => {
            // Paul Kellet's approximation, as in TS.
            let mut b = [0f32; 7];
            for _ in 0..len {
                let w = white();
                b[0] = 0.99886 * b[0] + w * 0.0555179;
                b[1] = 0.99332 * b[1] + w * 0.0750759;
                b[2] = 0.969 * b[2] + w * 0.153852;
                b[3] = 0.8665 * b[3] + w * 0.3104856;
                b[4] = 0.55 * b[4] + w * 0.5329522;
                b[5] = -0.7616 * b[5] - w * 0.016898;
                let pink = b.iter().sum::<f32>() + w * 0.5362;
                b[6] = w * 0.115926;
                out.push(pink * 0.11);
            }
        }
        NoiseColor::Brown => {
            let mut last = 0f32;
            for _ in 0..len {
                last = (last + 0.02 * white()) / 1.02;
                out.push(last * 3.5);
            }
        }
    }
    out
}

/// Gate edges as the control thread reports them: a level plus a count of
/// rising edges, so an on/off pair between two quanta isn't lost.
#[derive(Default)]
struct Gate {
    epoch: u32,
    open: bool,
}

enum Edge {
    Rise,
    Fall,
    None,
}

impl Gate {
    fn poll(&mut self, p: &Params) -> (Edge, bool) {
        let epoch = p.epoch();
        let open = p.get(slot::GATE) > 0.5;
        let edge = if epoch != self.epoch {
            Edge::Rise
        } else if self.open && !open {
            Edge::Fall
        } else {
            Edge::None
        };
        self.epoch = epoch;
        self.open = open;
        (edge, open)
    }
}

pub struct Synth {
    saw: WaveOsc,
    freq: Ramp,
    env: Adsr,
    gate: Gate,
}

impl Synth {
    fn process(&mut self, p: &Params, out: &mut Bus, sr: f32) {
        self.freq.set(p.get(0));
        let (edge, open) = self.gate.poll(p);
        let (vel, a, d, s, r) = (p.get(1), p.get(2), p.get(3), p.get(4), p.get(5));
        match edge {
            Edge::Rise => {
                self.env
                    .attack(vel, samples(a, sr), vel * s, samples(d, sr));
                if !open {
                    self.env.release(samples(r, sr));
                }
            }
            Edge::Fall => self.env.release(samples(r, sr)),
            Edge::None => {}
        }
        let nyquist = sr / 2.0;
        out.channels = 1;
        for y in out.ch[0].iter_mut() {
            let f = self.freq.tick().clamp(0.0, nyquist);
            *y = self.saw.next(f) * self.env.tick();
        }
    }
}

/// `sound/envelope`: the input times the ADSR gain.
pub struct Envelope {
    env: Adsr,
    gate: Gate,
}

impl Envelope {
    fn process(&mut self, p: &Params, input: &Bus, out: &mut Bus, sr: f32) {
        let (edge, open) = self.gate.poll(p);
        let (a, d, s, r, peak) = (p.get(0), p.get(1), p.get(2), p.get(3), p.get(4));
        match edge {
            Edge::Rise => {
                self.env
                    .attack(peak, samples(a, sr), peak * s, samples(d, sr));
                if !open {
                    self.env.release(samples(r, sr));
                }
            }
            Edge::Fall => self.env.release(samples(r, sr)),
            Edge::None => {}
        }
        out.channels = input.channels;
        for i in 0..QUANTUM {
            let k = self.env.tick();
            for c in 0..input.channels {
                out.ch[c][i] = input.ch[c][i] * k;
            }
        }
    }
}

/// `sound/sound-file`: buffer playback → `StereoPannerNode` → gain.
pub struct Player {
    sample: Option<Arc<SampleData>>,
    /// Read position in source frames.
    pos: f64,
    playing: bool,
    /// A one-shot reached its end while `play` is still held: silent, but
    /// the reported position keeps wrapping, as the TS node's
    /// `elapsed % duration` did until `play` fell.
    ended: bool,
    gate: Gate,
    pan: Ramp,
    amp: Ramp,
}

impl Player {
    fn process(&mut self, p: &Params, out: &mut Bus, sr: f32) {
        let (rate, looping) = (f64::from(p.get(0)), p.get(3) > 0.5);
        self.pan.set(p.get(1));
        self.amp.set(p.get(2));
        let (edge, open) = self.gate.poll(p);
        match edge {
            Edge::Rise => {
                self.pos = 0.0;
                self.playing = open;
                self.ended = false;
            }
            Edge::Fall => {
                self.playing = false;
                self.ended = false;
            }
            Edge::None => {}
        }
        out.channels = 2;
        let Some(data) = self.sample.as_ref().filter(|d| d.frames() > 0) else {
            out.ch = [[0.0; QUANTUM]; 2];
            p.set_playhead(None);
            return;
        };
        let frames = data.frames() as f64;
        let step = rate * f64::from(data.sample_rate) / f64::from(sr);
        let stereo = data.channels.len() > 1;
        for i in 0..QUANTUM {
            let (mut l, mut r) = (0.0, 0.0);
            if self.playing {
                (l, r) = data.frame_at(self.pos);
                self.pos += step;
                if self.pos >= frames || self.pos < 0.0 {
                    if looping {
                        self.pos = self.pos.rem_euclid(frames);
                    } else {
                        self.playing = false;
                        self.ended = true;
                    }
                }
            } else if self.ended {
                self.pos = (self.pos + step).rem_euclid(frames);
            }
            let pan = self.pan.tick();
            let (ol, or) = if stereo {
                if pan <= 0.0 {
                    let x = (pan + 1.0) * FRAC_PI_2;
                    (l + r * x.cos(), r * x.sin())
                } else {
                    let x = pan * FRAC_PI_2;
                    (l * x.cos(), r + l * x.sin())
                }
            } else {
                let x = (pan + 1.0) * 0.5 * FRAC_PI_2;
                (l * x.cos(), l * x.sin())
            };
            let g = self.amp.tick();
            out.ch[0][i] = ol * g;
            out.ch[1][i] = or * g;
        }
        let reported = self.playing || self.ended;
        p.set_playhead(reported.then(|| self.pos / f64::from(data.sample_rate)));
    }
}

// ─── Processors ──────────────────────────────────────────────────────────────

pub struct Filter {
    biquad: Biquad,
    freq: Ramp,
    q: Ramp,
    kind: Option<BiquadKind>,
}

impl Filter {
    fn process(&mut self, p: &Params, input: &Bus, out: &mut Bus, sr: f32) {
        self.freq.set(p.get(0));
        self.q.set(p.get(1));
        let kind = match p.get(2) as u32 {
            1 => BiquadKind::Highpass,
            2 => BiquadKind::Bandpass,
            _ => BiquadKind::Lowpass,
        };
        let mut dirty = self.kind != Some(kind);
        self.kind = Some(kind);
        out.channels = input.channels;
        for i in 0..QUANTUM {
            dirty |= self.freq.is_moving() || self.q.is_moving();
            let (f, q) = (self.freq.tick(), self.q.tick());
            if dirty {
                self.biquad
                    .set(kind, f64::from(f), f64::from(q), f64::from(sr));
                dirty = false;
            }
            for c in 0..input.channels {
                out.ch[c][i] = self.biquad.process(c, input.ch[c][i]);
            }
        }
    }
}

/// `sound/delay`: dry + lowpass(delay line), the lowpassed signal fed back.
pub struct Delay {
    lines: [Vec<f32>; 2],
    write: usize,
    time: Ramp,
    feedback: Ramp,
    damping: Ramp,
    lowpass: Biquad,
    fresh: bool,
}

impl Delay {
    fn new(sr: f32, time: f64, feedback: f64, damping: f64) -> Delay {
        let len = (2.0 * sr) as usize + 2 * QUANTUM + 4;
        Delay {
            lines: [vec![0.0; len], vec![0.0; len]],
            write: 0,
            time: Ramp::new(time as f32, RAMP_SECONDS, sr),
            feedback: Ramp::new(feedback as f32, RAMP_SECONDS, sr),
            damping: Ramp::new(damping as f32, RAMP_SECONDS, sr),
            lowpass: Biquad::default(),
            fresh: true,
        }
    }

    fn process(&mut self, p: &Params, input: &Bus, out: &mut Bus, sr: f32) {
        self.time.set(p.get(0));
        self.feedback.set(p.get(1));
        self.damping.set(p.get(2));
        let len = self.lines[0].len();
        out.channels = input.channels;
        for i in 0..QUANTUM {
            if self.fresh || self.damping.is_moving() {
                // A BiquadFilterNode's default Q (1 dB).
                let f = f64::from(self.damping.tick());
                self.lowpass.set(BiquadKind::Lowpass, f, 1.0, f64::from(sr));
                self.fresh = false;
            }
            // In a cycle Web Audio delays by at least one render quantum.
            let d = f64::from(self.time.tick() * sr).max(QUANTUM as f64);
            let fb = self.feedback.tick();
            let read = (self.write as f64 - d).rem_euclid(len as f64);
            let (i0, frac) = (read.floor() as usize, (read - read.floor()) as f32);
            let i1 = (i0 + 1) % len;
            for c in 0..input.channels {
                let line = &mut self.lines[c];
                let y = line[i0] + (line[i1] - line[i0]) * frac;
                let z = self.lowpass.process(c, y);
                let x = input.ch[c][i];
                line[self.write] = x + fb * z;
                out.ch[c][i] = x + z;
            }
            self.write = (self.write + 1) % len;
        }
    }
}

/// `sound/distortion`: the TS soft-clip curve in a `WaveShaperNode`, mixed
/// with the dry signal.
pub struct Shaper {
    curve: [f32; 256],
    amount: f32,
    factor: usize,
    up: Lowpass4,
    down: Lowpass4,
    wet: Ramp,
    dry: Ramp,
}

impl Shaper {
    fn new(sr: f32, wet: f64) -> Shaper {
        Shaper {
            curve: [0.0; 256],
            amount: f32::NAN,
            factor: 0,
            up: Lowpass4::default(),
            down: Lowpass4::default(),
            wet: Ramp::new(wet as f32, RAMP_SECONDS, sr),
            dry: Ramp::new(1.0 - wet as f32, RAMP_SECONDS, sr),
        }
    }

    /// `buildCurve(amount)` from distortion.ts.
    pub fn build_curve(amount: f32, curve: &mut [f32; 256]) {
        let k = f64::from(amount) * 100.0 + 0.001;
        let deg = std::f64::consts::PI / 180.0;
        for (i, c) in curve.iter_mut().enumerate() {
            let x = (i as f64 * 2.0) / 256.0 - 1.0;
            *c = (((3.0 + k) * x * 20.0 * deg) / (std::f64::consts::PI + k * x.abs())) as f32;
        }
    }

    /// The `WaveShaperNode` curve lookup (linear interpolation).
    fn shape(&self, x: f32) -> f32 {
        let n = self.curve.len();
        let v = (n as f32 - 1.0) / 2.0 * (x + 1.0);
        if v.is_nan() {
            return 0.0;
        }
        if v <= 0.0 {
            return self.curve[0];
        }
        if v >= (n - 1) as f32 {
            return self.curve[n - 1];
        }
        let k = v as usize;
        let f = v - k as f32;
        (1.0 - f) * self.curve[k] + f * self.curve[k + 1]
    }

    fn process(&mut self, p: &Params, input: &Bus, out: &mut Bus, sr: f32) {
        let amount = p.get(0);
        if amount != self.amount {
            self.amount = amount;
            Self::build_curve(amount, &mut self.curve);
        }
        self.wet.set(p.get(1));
        self.dry.set(1.0 - p.get(1));
        let factor = match p.get(2) as u32 {
            2 => 2,
            4 => 4,
            _ => 1,
        };
        if factor != self.factor {
            self.factor = factor;
            if factor > 1 {
                let rate = f64::from(sr) * factor as f64;
                self.up = Lowpass4::new(0.45 * f64::from(sr), rate);
                self.down = Lowpass4::new(0.45 * f64::from(sr), rate);
            }
        }
        out.channels = input.channels;
        for i in 0..QUANTUM {
            let (w, d) = (self.wet.tick(), self.dry.tick());
            for c in 0..input.channels {
                let x = input.ch[c][i];
                let shaped = if factor == 1 {
                    self.shape(x)
                } else {
                    let mut y = 0.0;
                    for j in 0..factor {
                        let u = if j == 0 { x * factor as f32 } else { 0.0 };
                        let up = self.up.process(c, u);
                        let s = self.shape(up);
                        y = self.down.process(c, s);
                    }
                    y
                };
                out.ch[c][i] = x * d + shaped * w;
            }
        }
    }
}
