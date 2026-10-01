//! `DynamicsCompressorNode`, ported from Chromium's `DynamicsCompressor`:
//! the static curve (linear below the threshold, an exponential knee, then
//! a constant ratio; `(1 / curve(1))^0.6` make-up gain), a 6 ms look-ahead,
//! a detector with a fast "saturation" release, and the adaptive release
//! whose speed depends on how much gain is being recovered, updated every
//! 32 frames.

use crate::dsp::{Bus, QUANTUM};
use crate::graph::Params;
use std::f32::consts::FRAC_PI_2;

const PRE_DELAY_SECONDS: f32 = 0.006;
const MAX_PRE_DELAY: usize = 1024;
const DEFAULT_PRE_DELAY: usize = 256;
const DIVISION_FRAMES: usize = 32;
const SAT_RELEASE_SECONDS: f32 = 0.0025;
const RELEASE_ZONES: [f32; 4] = [0.09, 0.16, 0.42, 0.98];

/// Coefficients of the 4th-order polynomial through the release zones
/// (Chromium's constants, digits as written there).
#[allow(clippy::excessive_precision)]
fn release_poly() -> [f32; 5] {
    const C: [[f32; 4]; 5] = [
        [
            0.999_999_999_999_999_8,
            1.843_221_968_432_392_3e-16,
            -1.937_339_435_167_642_3e-16,
            8.824_516_011_816_245e-18,
        ],
        [
            -1.578_832_035_284_588_8,
            2.330_583_703_207_428_6,
            -0.914_119_420_484_042_9,
            0.162_367_752_561_203_2,
        ],
        [
            0.533_414_286_910_642_4,
            -1.272_736_789_213_631,
            0.925_885_604_220_751_2,
            -0.186_563_101_917_762_26,
        ],
        [
            0.087_834_631_382_072_34,
            -0.169_416_296_792_562_2,
            0.085_880_579_515_952_72,
            -0.004_298_914_105_462_83,
        ],
        [
            -0.042_416_883_008_123_074,
            0.111_569_382_798_760_2,
            -0.097_646_763_252_658_72,
            0.028_494_263_462_021_576,
        ],
    ];
    C.map(|row| row.iter().zip(RELEASE_ZONES).map(|(c, z)| c * z).sum())
}

fn db_to_lin(db: f32) -> f32 {
    10f32.powf(0.05 * db)
}

fn lin_to_db(x: f32) -> f32 {
    20.0 * x.log10()
}

fn finite_or(x: f32, default: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        default
    }
}

/// The static curve for one (threshold, knee, ratio).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    linear_threshold: f32,
    knee_threshold: f32,
    knee_threshold_db: f32,
    y_knee_threshold_db: f32,
    slope: f32,
    k: f32,
    /// Make-up gain (linear).
    pub makeup: f32,
}

impl Curve {
    pub fn new(threshold_db: f32, knee_db: f32, ratio: f32) -> Curve {
        let linear_threshold = db_to_lin(threshold_db);
        let mut c = Curve {
            linear_threshold,
            knee_threshold: 0.0,
            knee_threshold_db: threshold_db + knee_db,
            y_knee_threshold_db: 0.0,
            slope: 1.0 / ratio,
            k: 5.0,
            makeup: 1.0,
        };
        c.k = c.k_at_slope(1.0 / ratio, threshold_db + knee_db);
        c.knee_threshold = db_to_lin(c.knee_threshold_db);
        c.y_knee_threshold_db = lin_to_db(c.knee_curve(c.knee_threshold, c.k));
        c.makeup = (1.0 / c.saturate(1.0)).powf(0.6);
        c
    }

    fn knee_curve(&self, x: f32, k: f32) -> f32 {
        if x < self.linear_threshold {
            return x;
        }
        self.linear_threshold + (1.0 - (-k * (x - self.linear_threshold)).exp()) / k
    }

    /// `KAtSlope`: the knee sharpness whose slope at the knee's end is
    /// `desired` (in dB/dB).
    fn k_at_slope(&self, desired: f32, knee_threshold_db: f32) -> f32 {
        let db_x = knee_threshold_db;
        let x = db_to_lin(db_x);
        let above = x >= self.linear_threshold;
        let x2 = x * 1.001;
        let db_x2 = lin_to_db(x2);
        let (mut min_k, mut max_k, mut k) = (0.1f32, 10000f32, 5f32);
        for _ in 0..15 {
            let slope = if above {
                let db_y = lin_to_db(self.knee_curve(x, k));
                let db_y2 = lin_to_db(self.knee_curve(x2, k));
                (db_y2 - db_y) / (db_x2 - db_x)
            } else {
                1.0
            };
            if slope < desired {
                max_k = k;
            } else {
                min_k = k;
            }
            k = (min_k * max_k).sqrt();
        }
        k
    }

    /// Output level for input level `x` (both linear).
    pub fn saturate(&self, x: f32) -> f32 {
        if x < self.knee_threshold {
            self.knee_curve(x, self.k)
        } else {
            let y_db =
                self.y_knee_threshold_db + self.slope * (lin_to_db(x) - self.knee_threshold_db);
            db_to_lin(y_db)
        }
    }
}

pub struct Compressor {
    sample_rate: f32,
    settings: [f32; 3],
    curve: Curve,
    release_poly: [f32; 5],
    look_ahead: [Box<[f32; MAX_PRE_DELAY]>; 2],
    read: usize,
    write: usize,
    pre_delay_frames: usize,
    detector_average: f32,
    compressor_gain: f32,
    /// −1 while not attacking.
    max_attack_diff_db: f32,
    /// Frames into the current 32-frame division (envelope rate updates).
    division: usize,
    envelope_rate: f32,
    scaled_desired_gain: f32,
}

impl Compressor {
    pub fn new(sample_rate: f32) -> Compressor {
        Compressor {
            sample_rate,
            settings: [f32::NAN; 3],
            curve: Curve::new(-24.0, 30.0, 12.0),
            release_poly: release_poly(),
            look_ahead: [
                Box::new([0.0; MAX_PRE_DELAY]),
                Box::new([0.0; MAX_PRE_DELAY]),
            ],
            read: 0,
            write: DEFAULT_PRE_DELAY,
            pre_delay_frames: DEFAULT_PRE_DELAY,
            detector_average: 0.0,
            compressor_gain: 1.0,
            max_attack_diff_db: -1.0,
            division: 0,
            envelope_rate: 1.0,
            scaled_desired_gain: 0.0,
        }
    }

    fn set_pre_delay(&mut self) {
        let frames = ((PRE_DELAY_SECONDS * self.sample_rate) as usize).min(MAX_PRE_DELAY - 1);
        if frames != self.pre_delay_frames {
            self.pre_delay_frames = frames;
            for b in &mut self.look_ahead {
                b.fill(0.0);
            }
            self.read = 0;
            self.write = frames;
        }
    }

    /// The gain envelope's rate for the next division (attack: a fraction
    /// of the way per frame; release: a per-frame multiplier > 1).
    fn envelope(&mut self, attack_frames: f32, release_frames: f32) {
        self.detector_average = finite_or(self.detector_average, 1.0);
        let desired = self.detector_average;
        // Pre-warp so the sin() warp below lands on `desired`.
        let scaled = desired.asin() / FRAC_PI_2;
        self.scaled_desired_gain = scaled;
        let releasing = scaled > self.compressor_gain;
        let diff_db = if scaled == 0.0 {
            if releasing {
                -1.0
            } else {
                1.0
            }
        } else {
            lin_to_db(self.compressor_gain / scaled)
        };
        self.envelope_rate = if releasing {
            self.max_attack_diff_db = -1.0;
            let x = 0.25 * (finite_or(diff_db, -1.0).clamp(-12.0, 0.0) + 12.0);
            let [a, b, c, d, e] = self.release_poly.map(|k| k * release_frames);
            let (x2, x3, x4) = (x * x, x * x * x, x * x * x * x);
            let frames = a + b * x + c * x2 + d * x3 + e * x4;
            db_to_lin(5.0 / frames)
        } else {
            let diff_db = finite_or(diff_db, 1.0);
            if self.max_attack_diff_db == -1.0 || self.max_attack_diff_db < diff_db {
                self.max_attack_diff_db = diff_db;
            }
            let eff = self.max_attack_diff_db.max(0.5);
            1.0 - (0.25 / eff).powf(1.0 / attack_frames)
        };
    }

    pub fn process(&mut self, p: &Params, input: &Bus, out: &mut Bus) {
        let settings = [p.get(0), p.get(1), p.get(2)];
        if settings != self.settings {
            self.settings = settings;
            self.curve = Curve::new(settings[0], settings[2], settings[1].max(1.0));
        }
        let sr = self.sample_rate;
        let attack_frames = p.get(3).max(0.001) * sr;
        let release_frames = sr * p.get(4);
        let sat_release_frames = SAT_RELEASE_SECONDS * sr;
        self.set_pre_delay();
        out.channels = input.channels;
        for i in 0..QUANTUM {
            if self.division == 0 {
                self.envelope(attack_frames, release_frames);
            }
            self.division = (self.division + 1) % DIVISION_FRAMES;

            let mut level = 0f32;
            for c in 0..2 {
                let x = input.chan(c)[i];
                self.look_ahead[c][self.write] = x;
                level = level.max(x.abs());
            }
            let attenuation = if level <= 0.0001 {
                1.0
            } else {
                self.curve.saturate(level) / level
            };
            let db_attenuation = (-lin_to_db(attenuation)).max(2.0);
            let sat_release_rate = db_to_lin(db_attenuation / sat_release_frames) - 1.0;
            let rate = if attenuation > self.detector_average {
                sat_release_rate
            } else {
                1.0
            };
            self.detector_average += (attenuation - self.detector_average) * rate;
            self.detector_average = finite_or(self.detector_average.min(1.0), 1.0);

            if self.envelope_rate < 1.0 {
                self.compressor_gain +=
                    (self.scaled_desired_gain - self.compressor_gain) * self.envelope_rate;
            } else {
                self.compressor_gain = (self.compressor_gain * self.envelope_rate).min(1.0);
            }
            let warped = (f64::from(FRAC_PI_2 * self.compressor_gain)).sin() as f32;
            let g = self.curve.makeup * warped;
            for c in 0..input.channels {
                out.ch[c][i] = self.look_ahead[c][self.read] * g;
            }
            self.read = (self.read + 1) & (MAX_PRE_DELAY - 1);
            self.write = (self.write + 1) & (MAX_PRE_DELAY - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_curve_compresses_above_the_knee() {
        let c = Curve::new(-24.0, 30.0, 12.0);
        // Below the threshold: unity.
        assert!((c.saturate(0.01) - 0.01).abs() < 1e-6);
        // Above the knee (+6 dB) the slope is 1/ratio in dB.
        let (a, b) = (lin_to_db(c.saturate(4.0)), lin_to_db(c.saturate(8.0)));
        let slope = (b - a) / lin_to_db(2.0);
        assert!((slope - 1.0 / 12.0).abs() < 0.02, "slope {slope}");
        assert!(c.makeup > 1.0);
    }
}
