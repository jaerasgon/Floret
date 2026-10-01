//! `DynamicsCompressorNode`, after Chromium's `DynamicsCompressorKernel`:
//! the same static curve (linear below the threshold, an exponential knee,
//! then a constant ratio; `(1 / curve(1))^0.6` make-up gain) and a 6 ms
//! look-ahead. The gain envelope is a plain attack / release follower in
//! dB rather than Chromium's adaptive multi-zone release.

use crate::dsp::{Bus, QUANTUM};
use crate::graph::Params;

const PRE_DELAY_SECONDS: f32 = 0.006;

fn db_to_lin(db: f32) -> f32 {
    10f32.powf(0.05 * db)
}

fn lin_to_db(x: f32) -> f32 {
    20.0 * x.log10()
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

    fn slope_at(&self, x: f32, k: f32) -> f32 {
        if x < self.linear_threshold {
            return 1.0;
        }
        let x2 = x * 1.001;
        let (x_db, x2_db) = (lin_to_db(x), lin_to_db(x2));
        let (y_db, y2_db) = (
            lin_to_db(self.knee_curve(x, k)),
            lin_to_db(self.knee_curve(x2, k)),
        );
        (y2_db - y_db) / (x2_db - x_db)
    }

    fn k_at_slope(&self, desired: f32, knee_threshold_db: f32) -> f32 {
        let x = db_to_lin(knee_threshold_db);
        let (mut min_k, mut max_k, mut k) = (0.1f32, 10000f32, 5f32);
        for _ in 0..15 {
            if self.slope_at(x, k) < desired {
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
    look_ahead: [Vec<f32>; 2],
    pos: usize,
    delay: usize,
    /// Current gain reduction (dB, ≤ 0).
    gain_db: f32,
}

impl Compressor {
    pub fn new(sample_rate: f32) -> Compressor {
        let delay = ((PRE_DELAY_SECONDS * sample_rate).round() as usize).max(1);
        Compressor {
            sample_rate,
            settings: [f32::NAN; 3],
            curve: Curve::new(-24.0, 30.0, 12.0),
            look_ahead: [vec![0.0; delay], vec![0.0; delay]],
            pos: 0,
            delay,
            gain_db: 0.0,
        }
    }

    pub fn process(&mut self, p: &Params, input: &Bus, out: &mut Bus) {
        let settings = [p.get(0), p.get(1), p.get(2)];
        if settings != self.settings {
            self.settings = settings;
            self.curve = Curve::new(settings[0], settings[2], settings[1].max(1.0));
        }
        let coef = |t: f32| (-1.0 / (t.max(0.001) * self.sample_rate)).exp();
        let (attack, release) = (coef(p.get(3)), coef(p.get(4)));
        out.channels = input.channels;
        for i in 0..QUANTUM {
            let mut level = 0f32;
            for c in 0..input.channels {
                level = level.max(input.ch[c][i].abs());
            }
            let target_db = if level <= 0.0001 {
                0.0
            } else {
                lin_to_db(self.curve.saturate(level) / level).min(0.0)
            };
            let k = if target_db < self.gain_db {
                attack
            } else {
                release
            };
            self.gain_db = target_db + (self.gain_db - target_db) * k;
            let g = db_to_lin(self.gain_db) * self.curve.makeup;
            for c in 0..input.channels {
                let delayed = self.look_ahead[c][self.pos];
                self.look_ahead[c][self.pos] = input.ch[c][i];
                out.ch[c][i] = delayed * g;
            }
            self.pos = (self.pos + 1) % self.delay;
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
