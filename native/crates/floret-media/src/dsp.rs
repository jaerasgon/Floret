//! DSP building blocks shared by the nodes: render-quantum buses, parameter
//! ramps, the Web Audio biquad and ADSR segments.

/// Samples per render quantum (Web Audio's block size).
pub const QUANTUM: usize = 128;

/// One channel of one render quantum.
pub type Chan = [f32; QUANTUM];

/// A node's output for one quantum: mono or stereo.
#[derive(Clone)]
pub struct Bus {
    pub ch: [Chan; 2],
    pub channels: usize,
}

pub static SILENCE: Bus = Bus::new();

impl Bus {
    pub const fn new() -> Bus {
        Bus {
            ch: [[0.0; QUANTUM]; 2],
            channels: 1,
        }
    }

    pub fn silence(&mut self) {
        self.channels = 1;
        self.ch[0] = [0.0; QUANTUM];
    }

    /// Channel `c` with Web Audio's "speakers" up-mix (mono → both).
    pub fn chan(&self, c: usize) -> &Chan {
        &self.ch[c.min(self.channels - 1)]
    }

    /// Add `other` in (a mono bus turns stereo when `other` is).
    pub fn add(&mut self, other: &Bus) {
        if other.channels == 2 && self.channels == 1 {
            self.ch[1] = self.ch[0];
            self.channels = 2;
        }
        for c in 0..self.channels {
            for (y, x) in self.ch[c].iter_mut().zip(other.chan(c)) {
                *y += x;
            }
        }
    }

    /// Down-mix to mono (`(L + R) / 2`).
    pub fn mono(&self, i: usize) -> f32 {
        if self.channels == 2 {
            0.5 * (self.ch[0][i] + self.ch[1][i])
        } else {
            self.ch[0][i]
        }
    }
}

impl Default for Bus {
    fn default() -> Bus {
        Bus::new()
    }
}

/// Parameter smoothing: what the TS nodes get from
/// `cancelScheduledValues(t); linearRampToValueAtTime(v, t + ramp)` every
/// tick — a linear glide from the current value to each new target.
#[derive(Clone, Debug)]
pub struct Ramp {
    value: f32,
    target: f32,
    step: f32,
    left: u32,
    len: u32,
}

/// The TS nodes' 20 ms parameter ramps.
pub const RAMP_SECONDS: f32 = 0.02;

impl Ramp {
    pub fn new(value: f32, seconds: f32, sample_rate: f32) -> Ramp {
        Ramp {
            value,
            target: value,
            step: 0.0,
            left: 0,
            len: ((seconds * sample_rate).round() as u32).max(1),
        }
    }

    /// Glide to `target` (restarting the glide if it changed).
    pub fn set(&mut self, target: f32) {
        if target != self.target {
            self.target = target;
            self.left = self.len;
            self.step = (target - self.value) / self.len as f32;
        }
    }

    /// Jump straight to `v`.
    pub fn jump(&mut self, v: f32) {
        self.value = v;
        self.target = v;
        self.left = 0;
    }

    pub fn tick(&mut self) -> f32 {
        if self.left > 0 {
            self.left -= 1;
            self.value = if self.left == 0 {
                self.target
            } else {
                self.value + self.step
            };
        }
        self.value
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn is_moving(&self) -> bool {
        self.left > 0
    }
}

/// `x mod 1` into [0, 1).
pub fn wrap01(x: f64) -> f64 {
    let w = x - x.floor();
    if w >= 1.0 {
        0.0
    } else {
        w
    }
}

/// A `BiquadFilterNode` with Web Audio's coefficient formulas (Audio EQ
/// cookbook; lowpass / highpass take Q in dB). Two channels of state.
#[derive(Clone, Debug, Default)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: [f64; 2],
    z2: [f64; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BiquadKind {
    Lowpass,
    Highpass,
    Bandpass,
}

impl Biquad {
    pub fn set(&mut self, kind: BiquadKind, freq: f64, q: f64, sample_rate: f64) {
        let nyquist = sample_rate / 2.0;
        let f = (freq / nyquist).clamp(0.0, 1.0);
        let (b0, b1, b2, a0, a1, a2);
        if f >= 1.0 || f <= 0.0 {
            // The spec's limiting cases: a pass-through or a block.
            let pass = match kind {
                BiquadKind::Lowpass => f >= 1.0,
                BiquadKind::Highpass => f <= 0.0,
                BiquadKind::Bandpass => false,
            };
            (b0, b1, b2, a0, a1, a2) = (if pass { 1.0 } else { 0.0 }, 0.0, 0.0, 1.0, 0.0, 0.0);
        } else {
            let w0 = std::f64::consts::PI * f;
            let (sin, cos) = w0.sin_cos();
            match kind {
                BiquadKind::Lowpass | BiquadKind::Highpass => {
                    let alpha = sin / (2.0 * 10f64.powf(q / 20.0));
                    let k = if kind == BiquadKind::Lowpass {
                        1.0 - cos
                    } else {
                        1.0 + cos
                    };
                    b0 = k / 2.0;
                    b1 = if kind == BiquadKind::Lowpass { k } else { -k };
                    b2 = k / 2.0;
                    a0 = 1.0 + alpha;
                    a1 = -2.0 * cos;
                    a2 = 1.0 - alpha;
                }
                BiquadKind::Bandpass => {
                    let alpha = sin / (2.0 * q.max(1e-4));
                    b0 = alpha;
                    b1 = 0.0;
                    b2 = -alpha;
                    a0 = 1.0 + alpha;
                    a1 = -2.0 * cos;
                    a2 = 1.0 - alpha;
                }
            }
        }
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    pub fn process(&mut self, c: usize, x: f32) -> f32 {
        let x = f64::from(x);
        let y = self.b0 * x + self.z1[c];
        self.z1[c] = self.b1 * x - self.a1 * y + self.z2[c];
        self.z2[c] = self.b2 * x - self.a2 * y;
        // Flush denormals so a silent tail doesn't cost CPU.
        if self.z1[c].abs() < 1e-30 {
            self.z1[c] = 0.0;
        }
        if self.z2[c].abs() < 1e-30 {
            self.z2[c] = 0.0;
        }
        y as f32
    }

    /// Magnitude response at `freq` (Hz), like `getFrequencyResponse`.
    pub fn magnitude(&self, freq: f64, sample_rate: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * freq / sample_rate;
        let (c1, s1, c2, s2) = (w.cos(), -w.sin(), (2.0 * w).cos(), -(2.0 * w).sin());
        let num = (
            self.b0 + self.b1 * c1 + self.b2 * c2,
            self.b1 * s1 + self.b2 * s2,
        );
        let den = (
            1.0 + self.a1 * c1 + self.a2 * c2,
            self.a1 * s1 + self.a2 * s2,
        );
        (num.0.hypot(num.1)) / (den.0.hypot(den.1))
    }
}

/// Linear gain segments driven by a gate: the TS nodes' AudioParam
/// schedules (`linearRampToValueAtTime` to the peak over `attack`, to
/// `peak × sustain` over `decay`, hold; on release to 0 over `release`),
/// each starting from wherever the gain is.
#[derive(Clone, Debug, Default)]
pub struct Adsr {
    value: f32,
    seg: Option<Segment>,
    then: Option<(f32, u32)>,
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    from: f32,
    to: f32,
    len: u32,
    pos: u32,
}

impl Adsr {
    /// Rising edge. Lengths are in samples.
    pub fn attack(&mut self, peak: f32, attack: u32, sustain: f32, decay: u32) {
        self.then = Some((sustain, decay));
        self.start(peak, attack);
    }

    /// Falling edge.
    pub fn release(&mut self, release: u32) {
        self.then = None;
        self.start(0.0, release);
    }

    fn start(&mut self, to: f32, len: u32) {
        if len == 0 {
            self.value = to;
            self.seg = None;
            if let Some((t, l)) = self.then.take() {
                self.start(t, l);
            }
        } else {
            self.seg = Some(Segment {
                from: self.value,
                to,
                len,
                pos: 0,
            });
        }
    }

    pub fn tick(&mut self) -> f32 {
        if let Some(s) = &mut self.seg {
            s.pos += 1;
            if s.pos >= s.len {
                self.value = s.to;
                self.seg = None;
                if let Some((t, l)) = self.then.take() {
                    self.start(t, l);
                }
            } else {
                self.value = s.from + (s.to - s.from) * (s.pos as f32 / s.len as f32);
            }
        }
        self.value
    }

    pub fn value(&self) -> f32 {
        self.value
    }
}

/// Seconds → whole samples (≥ 0).
pub fn samples(seconds: f32, sample_rate: f32) -> u32 {
    (seconds.max(0.0) * sample_rate).round() as u32
}

/// Two cascaded Butterworth biquads: the anti-aliasing filter around the
/// waveshaper when it oversamples.
#[derive(Clone, Debug, Default)]
pub struct Lowpass4([Biquad; 2]);

impl Lowpass4 {
    pub fn new(cutoff: f64, sample_rate: f64) -> Lowpass4 {
        let mut f = Lowpass4::default();
        // Butterworth Qs for 4th order, given in dB as Web Audio's lowpass
        // takes them.
        for (b, q) in f.0.iter_mut().zip([0.541_196_1f64, 1.306_563]) {
            b.set(BiquadKind::Lowpass, cutoff, 20.0 * q.log10(), sample_rate);
        }
        f
    }

    pub fn process(&mut self, c: usize, x: f32) -> f32 {
        let y = self.0[0].process(c, x);
        self.0[1].process(c, y)
    }
}
