//! Chromium's band-limited oscillator (`PeriodicWaveHandler` +
//! `OscillatorHandler`): each basic waveform is a Fourier series rendered
//! into tables with fewer and fewer partials (three ranges per octave),
//! normalised so the fullest table peaks at 1. Playback picks the two
//! tables around the frequency and crossfades them. This is why Chrome's
//! square and saw sit ~1.4 dB below ±1 (Gibbs overshoot is what reaches 1)
//! and why its triangle is band-limited too.

use realfft::num_complex::Complex;
use realfft::RealFftPlanner;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Square,
    Sawtooth,
    Triangle,
}

pub struct PeriodicWave {
    size: usize,
    lowest_fundamental: f32,
    rate_scale: f32,
    tables: Vec<Box<[f32]>>,
}

const OCTAVE_BANDS: f32 = 3.0;
const CENTS_PER_RANGE: f32 = 1200.0 / OCTAVE_BANDS;
const INTERPOLATE_2POINT: f32 = 0.3;
const INTERPOLATE_3POINT: f32 = 0.16;

/// `PeriodicWaveSize()`: the table length for a sample rate.
fn wave_size(sample_rate: f32) -> usize {
    if sample_rate <= 24000.0 {
        2048
    } else if sample_rate <= 88200.0 {
        4096
    } else {
        16384
    }
}

/// `GenerateBasicWaveform`: the sine coefficient of partial `n`.
fn coefficient(shape: Shape, n: usize) -> f32 {
    let pi_factor = 2.0 / (n as f32 * std::f32::consts::PI);
    let odd = n & 1 == 1;
    match shape {
        Shape::Square => {
            if odd {
                2.0 * pi_factor
            } else {
                0.0
            }
        }
        Shape::Sawtooth => pi_factor * if odd { 1.0 } else { -1.0 },
        Shape::Triangle => {
            if odd {
                let sign = if ((n - 1) >> 1) & 1 == 1 { -1.0 } else { 1.0 };
                2.0 * pi_factor * pi_factor * sign
            } else {
                0.0
            }
        }
    }
}

impl PeriodicWave {
    /// The shared tables for `shape` at `sample_rate` (built once; call
    /// off the audio thread).
    pub fn basic(shape: Shape, sample_rate: f32) -> Arc<PeriodicWave> {
        type Cache = Vec<(Shape, u32, Arc<PeriodicWave>)>;
        static CACHE: Mutex<Cache> = Mutex::new(Vec::new());
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((.., w)) = cache
            .iter()
            .find(|(s, sr, _)| *s == shape && *sr == sample_rate.to_bits())
        {
            return w.clone();
        }
        let w = Arc::new(PeriodicWave::build(shape, sample_rate));
        cache.push((shape, sample_rate.to_bits(), w.clone()));
        w
    }

    fn build(shape: Shape, sample_rate: f32) -> PeriodicWave {
        let size = wave_size(sample_rate);
        let half = size / 2;
        let ranges = (0.5 + OCTAVE_BANDS * (size as f32).log2()) as usize;
        let ifft = RealFftPlanner::<f32>::new().plan_fft_inverse(size);
        let mut spectrum = vec![Complex::default(); half + 1];
        let mut scratch = ifft.make_scratch_vec();
        let mut normalization = 1.0;
        let mut tables = Vec::with_capacity(ranges);
        for range in 0..ranges {
            let culling = 2f32.powf(-(range as f32) * CENTS_PER_RANGE / 1200.0);
            let partials = (culling * half as f32) as usize;
            for (n, s) in spectrum.iter_mut().enumerate() {
                // A sine partial b·sin(nθ) is −i·b/2 in the half spectrum.
                *s = if n > 0 && n < half && n <= partials {
                    Complex::new(0.0, -coefficient(shape, n) / 2.0)
                } else {
                    Complex::default()
                };
            }
            let mut table = vec![0f32; size];
            let _ = ifft.process_with_scratch(&mut spectrum, &mut table, &mut scratch);
            if range == 0 {
                let peak = table.iter().fold(0f32, |m, v| m.max(v.abs()));
                if peak > 0.0 {
                    normalization = 1.0 / peak;
                }
            }
            for v in &mut table {
                *v *= normalization;
            }
            tables.push(table.into_boxed_slice());
        }
        PeriodicWave {
            size,
            lowest_fundamental: 0.5 * sample_rate / half as f32,
            rate_scale: size as f32 / sample_rate,
            tables,
        }
    }

    /// `WaveDataForFundamentalFrequency`: (fewer partials, more partials,
    /// crossfade from the second to the first).
    fn tables_for(&self, freq: f32) -> (&[f32], &[f32], f32) {
        let f = freq.abs();
        let ratio = if f > 0.0 {
            f / self.lowest_fundamental
        } else {
            0.5
        };
        let cents = ratio.log2() * 1200.0;
        let last = (self.tables.len() - 1) as f32;
        let pitch_range = (1.0 + cents / CENTS_PER_RANGE).clamp(0.0, last);
        let i1 = pitch_range as usize;
        let i2 = (i1 + 1).min(self.tables.len() - 1);
        (&self.tables[i2], &self.tables[i1], pitch_range - i1 as f32)
    }
}

/// One oscillator reading a [`PeriodicWave`].
#[derive(Clone)]
pub struct WaveOsc {
    wave: Arc<PeriodicWave>,
    /// `virtual_read_index`: position in the table, `0..size`.
    index: f64,
}

impl WaveOsc {
    pub fn new(wave: Arc<PeriodicWave>) -> WaveOsc {
        WaveOsc { wave, index: 0.0 }
    }

    /// The next sample at `freq` Hz (`DoInterpolation`).
    pub fn next(&mut self, freq: f32) -> f32 {
        let w = &*self.wave;
        let mask = w.size - 1;
        let incr = freq * w.rate_scale;
        let (lower, higher, factor) = w.tables_for(freq);
        let i0 = self.index as usize;
        let t = self.index - i0 as f64;
        let at = |k: isize| (i0 as isize + k) as usize & mask;
        let (lo, hi) = if incr.abs() >= INTERPOLATE_2POINT {
            let t = f64::from(self.index as f32 - i0 as f32);
            let lerp = |d: &[f32]| (1.0 - t) * f64::from(d[at(0)]) + t * f64::from(d[at(1)]);
            (lerp(lower), lerp(higher))
        } else if incr.abs() >= INTERPOLATE_3POINT {
            let a = [0.5 * t * (t - 1.0), 1.0 - t * t, 0.5 * t * (t + 1.0)];
            let sum = |d: &[f32]| {
                (0..3)
                    .map(|k| a[k] * f64::from(d[at(k as isize - 1)]))
                    .sum()
            };
            (sum(lower), sum(higher))
        } else {
            let t2 = t * t;
            let a = [
                t * (t2 - 1.0) * (t - 2.0) / 24.0,
                -t * (t - 1.0) * (t2 - 4.0) / 6.0,
                (t2 - 1.0) * (t2 - 4.0) / 4.0,
                -t * (t + 1.0) * (t2 - 4.0) / 6.0,
                t * (t2 - 1.0) * (t + 2.0) / 24.0,
            ];
            let sum = |d: &[f32]| {
                (0..5)
                    .map(|k| a[k] * f64::from(d[at(k as isize - 2)]))
                    .sum()
            };
            (sum(lower), sum(higher))
        };
        let size = w.size as f64;
        self.index += f64::from(incr);
        self.index -= (self.index / size).floor() * size;
        ((1.0 - factor) * hi as f32) + factor * lo as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_match_chromium() {
        let sq = PeriodicWave::basic(Shape::Square, 48000.0);
        assert_eq!((sq.size, sq.tables.len()), (4096, 36));
        // Normalised so the Gibbs overshoot of the fullest table peaks at
        // 1: the plateau sits at 1 / 1.179.
        let peak = sq.tables[0].iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((peak - 1.0).abs() < 1e-6);
        assert!(
            (sq.tables[0][1024] - 0.8482).abs() < 1e-3,
            "{}",
            sq.tables[0][1024]
        );
        let tri = PeriodicWave::basic(Shape::Triangle, 48000.0);
        assert!((tri.tables[0][1024] - 1.0).abs() < 1e-3);
        // Range 33 keeps a single partial (a sine); the top range none.
        let one = &sq.tables[33];
        let s = (2.0 * std::f32::consts::PI * 512.0 / 4096.0).sin();
        assert!((one[512] / one[1024] - s).abs() < 1e-4);
        assert!(sq.tables.last().unwrap().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn saw_rises_from_zero() {
        let mut o = WaveOsc::new(PeriodicWave::basic(Shape::Sawtooth, 48000.0));
        let a = o.next(100.0);
        let b = o.next(100.0);
        assert!(a.abs() < 1e-3 && b > a, "{a} {b}");
    }
}
