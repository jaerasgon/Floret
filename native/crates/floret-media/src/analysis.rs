//! `AnalyserNode` on the control thread.
//!
//! The audio thread only copies each analyser's (mono down-mixed) input
//! into a lock-free [`Tap`]. When a tick starts, [`Analyser`] reads the
//! most recent `fftSize` samples and does what Chromium's
//! `RealtimeAnalyser` does on `getByteFrequencyData`: Blackman window,
//! FFT, `|X| / N` magnitudes, smoothing over time, then dB mapped from
//! −100…−30 to 0…255. (Older Chromium FFTs returned 2·X to match vDSP;
//! since the switch to RustFFT it is the spec's plain DFT, as here.) Like Chromium it only advances the smoothing when new audio has
//! arrived since the last call.

use crate::dsp::{Bus, QUANTUM};
use floret_engine::audio::Analysis;
use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

/// Samples kept per tap: at least the largest `fftSize` the nodes use.
pub const TAP_LEN: usize = 4096;
const MIN_DB: f64 = -100.0;
const MAX_DB: f64 = -30.0;

/// The last [`TAP_LEN`] input samples of an analyser, written by the audio
/// thread, read by the control thread.
pub struct Tap {
    buf: Box<[AtomicU32]>,
    /// Samples written so far.
    written: AtomicUsize,
}

impl Default for Tap {
    fn default() -> Tap {
        Tap {
            buf: (0..TAP_LEN).map(|_| AtomicU32::new(0)).collect(),
            written: AtomicUsize::new(0),
        }
    }
}

impl Tap {
    pub fn push(&self, input: &Bus) {
        let w = self.written.load(Ordering::Relaxed);
        for i in 0..QUANTUM {
            self.buf[(w + i) % TAP_LEN].store(input.mono(i).to_bits(), Ordering::Relaxed);
        }
        self.written.store(w + QUANTUM, Ordering::Release);
    }

    pub fn written(&self) -> usize {
        self.written.load(Ordering::Acquire)
    }

    /// The most recent `out.len()` samples (zeros before the first).
    pub fn latest(&self, out: &mut [f32]) {
        let w = self.written();
        let n = out.len();
        for (i, o) in out.iter_mut().enumerate() {
            *o = match (w + i).checked_sub(n) {
                Some(at) => f32::from_bits(self.buf[at % TAP_LEN].load(Ordering::Relaxed)),
                None => 0.0,
            };
        }
    }
}

/// One analyser node's FFT state.
pub struct Analyser {
    fft_size: usize,
    fft: Option<Arc<dyn RealToComplex<f32>>>,
    smoothed: Vec<f64>,
    seen: usize,
    last: Analysis,
}

impl Default for Analyser {
    fn default() -> Analyser {
        Analyser {
            fft_size: 0,
            fft: None,
            smoothed: Vec::new(),
            seen: usize::MAX,
            last: Analysis::default(),
        }
    }
}

impl Analyser {
    pub fn analyse(&mut self, tap: &Tap, fft_size: usize, smoothing: f64) -> Analysis {
        let fft_size = fft_size.clamp(32, TAP_LEN);
        if fft_size != self.fft_size {
            self.fft_size = fft_size;
            self.fft = Some(RealFftPlanner::new().plan_fft_forward(fft_size));
            self.smoothed = vec![0.0; fft_size / 2];
            self.seen = usize::MAX;
        }
        let written = tap.written();
        if written == self.seen {
            return self.last.clone();
        }
        self.seen = written;
        let mut time = vec![0f32; fft_size];
        tap.latest(&mut time);
        let mut windowed: Vec<f32> = time
            .iter()
            .enumerate()
            .map(|(i, &x)| {
                let t = i as f64 / fft_size as f64;
                let tau = std::f64::consts::TAU;
                let w = 0.42 - 0.5 * (tau * t).cos() + 0.08 * (tau * 2.0 * t).cos();
                x * w as f32
            })
            .collect();
        let fft = self.fft.as_ref().expect("planned");
        let mut spec = vec![Complex::<f32>::default(); fft_size / 2 + 1];
        let _ = fft.process(&mut windowed, &mut spec);
        let k = smoothing.clamp(0.0, 1.0);
        let scale = 1.0 / fft_size as f64;
        let bytes: Vec<u8> = self
            .smoothed
            .iter_mut()
            .zip(&spec)
            .enumerate()
            .map(|(i, (s, x))| {
                // The packed Nyquist term doesn't count toward bin 0.
                let im = if i == 0 { 0.0 } else { f64::from(x.im) };
                let mag = f64::from(x.re).hypot(im) * scale;
                let v = k * *s + (1.0 - k) * mag;
                *s = if v.is_finite() { v } else { 0.0 };
                let db = 20.0 * s.log10();
                let scaled = 255.0 * (db - MIN_DB) / (MAX_DB - MIN_DB);
                scaled.clamp(0.0, 255.0) as u8
            })
            .collect();
        self.last = Analysis {
            time: Arc::from(time),
            bytes: Arc::from(bytes),
        };
        self.last.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap_with(f: impl Fn(usize) -> f32, quanta: usize) -> Tap {
        let tap = Tap::default();
        for q in 0..quanta {
            let mut bus = Bus::new();
            for i in 0..QUANTUM {
                bus.ch[0][i] = f(q * QUANTUM + i);
            }
            tap.push(&bus);
        }
        tap
    }

    #[test]
    fn tone_peaks_in_its_bin() {
        // 48 kHz, fftSize 256 → 187.5 Hz bins; 1500 Hz is bin 8.
        let sr = 48000.0;
        let tap = tap_with(
            |n| (std::f64::consts::TAU * 1500.0 * n as f64 / sr).sin() as f32 * 0.01,
            40,
        );
        let a = Analyser::default().analyse(&tap, 256, 0.0);
        assert_eq!(a.time.len(), 256);
        assert_eq!(a.bytes.len(), 128);
        let peak = (0..128).max_by_key(|&i| a.bytes[i]).unwrap();
        assert_eq!(peak, 8);
        // A 0.01 sine: 0.01 / 2 × Blackman gain 0.42 → −53.6 dB.
        assert_eq!(a.bytes[8], 169);
        assert_eq!(a.bytes[60], 0);
    }

    #[test]
    fn smoothing_only_advances_with_new_audio() {
        let tap = tap_with(|n| if n % 64 < 32 { 0.1 } else { -0.1 }, 16);
        let mut an = Analyser::default();
        let first = an.analyse(&tap, 64, 0.8);
        assert_eq!(an.analyse(&tap, 64, 0.8), first);
        let mut bus = Bus::new();
        bus.ch[0] = [0.1; QUANTUM];
        tap.push(&bus);
        let second = an.analyse(&tap, 64, 0.8);
        assert_ne!(second, first);
        // Silence decays toward zero instead of dropping at once.
        let quiet = Tap::default();
        let mut an = Analyser::default();
        let loud = an.analyse(&tap, 64, 0.8);
        quiet.push(&Bus::new());
        let after = an.analyse(&quiet, 64, 0.8);
        assert!(after
            .bytes
            .iter()
            .zip(loud.bytes.iter())
            .any(|(a, l)| *a > 0 && a <= l));
    }
}
