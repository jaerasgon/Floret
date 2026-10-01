//! `sound/reverb`: a `ConvolverNode` with the TS node's synthetic impulse
//! response (stereo noise under a `(1 − t)^decay` envelope), normalised the
//! way Web Audio normalises convolver buffers.
//!
//! Convolution is uniformly partitioned in the frequency domain, in two
//! stages: the first 1024 IR samples in render-quantum blocks, the rest in
//! 1024-sample blocks whose output is scheduled ahead. Overlap-save on
//! whole quanta adds no latency.

use crate::dsp::{Bus, Ramp, QUANTUM, RAMP_SECONDS};
use crate::graph::Params;
use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

const HEAD: usize = QUANTUM;
const TAIL: usize = 1024;
/// Ring for scheduled tail output: covers `HEAD + TAIL` ahead of now.
const RING: usize = 2048;

/// The TS `buildIR`, seeded: channel 0 then channel 1 from one stream.
pub fn impulse_response(
    sample_rate: f32,
    seconds: f64,
    decay: f64,
    reverse: bool,
    seed: u32,
) -> [Vec<f32>; 2] {
    let len = ((f64::from(sample_rate) * seconds).floor() as usize).max(1);
    let mut rng = floret_engine::js::Mulberry32::new(seed);
    let mut chan = || {
        (0..len)
            .map(|i| {
                let k = if reverse { len - 1 - i } else { i };
                let env = (1.0 - k as f64 / len as f64).powf(decay);
                ((rng.next_f64() * 2.0 - 1.0) * env) as f32
            })
            .collect::<Vec<f32>>()
    };
    let l = chan();
    let r = chan();
    [l, r]
}

/// Web Audio's `ConvolverNode` normalisation scale for a buffer.
pub fn normalization_scale(ir: &[Vec<f32>; 2], sample_rate: f32) -> f32 {
    let len = ir[0].len();
    let power: f64 = ir
        .iter()
        .flat_map(|c| c.iter())
        .map(|&x| f64::from(x) * f64::from(x))
        .sum();
    let mut power = (power / (2.0 * len as f64)).sqrt() as f32;
    if !power.is_finite() || power < 0.000125 {
        power = 0.000125;
    }
    (1.0 / power) * 0.00125 * (44100.0 / sample_rate)
}

/// Uniformly partitioned overlap-save convolution of two channels with
/// two IR channels (input channel `c` → IR channel `c`).
struct Partitioned {
    block: usize,
    parts: usize,
    ir: [Vec<Complex<f32>>; 2],
    fdl: [Vec<Complex<f32>>; 2],
    newest: usize,
    window: [Vec<f32>; 2],
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    time: Vec<f32>,
    spec: Vec<Complex<f32>>,
    acc: Vec<Complex<f32>>,
    scratch_fwd: Vec<Complex<f32>>,
    scratch_inv: Vec<Complex<f32>>,
}

impl Partitioned {
    fn new(ir: [&[f32]; 2], block: usize, planner: &mut RealFftPlanner<f32>) -> Partitioned {
        let fft = planner.plan_fft_forward(2 * block);
        let ifft = planner.plan_fft_inverse(2 * block);
        let bins = block + 1;
        let parts = ir[0].len().div_ceil(block).max(1);
        let mut time = vec![0.0; 2 * block];
        let mut spec = vec![Complex::default(); bins];
        let mut scratch_fwd = fft.make_scratch_vec();
        let ir = ir.map(|h| {
            let mut out = Vec::with_capacity(parts * bins);
            for p in 0..parts {
                time.fill(0.0);
                let seg = &h[(p * block).min(h.len())..((p + 1) * block).min(h.len())];
                time[..seg.len()].copy_from_slice(seg);
                fft.process_with_scratch(&mut time, &mut spec, &mut scratch_fwd)
                    .expect("fft sizes");
                out.extend_from_slice(&spec);
            }
            out
        });
        Partitioned {
            block,
            parts,
            ir,
            fdl: [
                vec![Complex::default(); parts * bins],
                vec![Complex::default(); parts * bins],
            ],
            newest: 0,
            window: [vec![0.0; 2 * block], vec![0.0; 2 * block]],
            scratch_inv: ifft.make_scratch_vec(),
            fft,
            ifft,
            time,
            spec,
            acc: vec![Complex::default(); bins],
            scratch_fwd,
        }
    }

    /// Feed `block` samples per channel; adds `block` output samples per
    /// channel (the convolution up to the end of the input) into `out`.
    fn process(&mut self, input: [&[f32]; 2], out: [&mut [f32]; 2]) {
        let (b, bins) = (self.block, self.block + 1);
        self.newest = (self.newest + 1) % self.parts;
        for (c, x) in input.into_iter().enumerate() {
            let w = &mut self.window[c];
            w.copy_within(b.., 0);
            w[b..].copy_from_slice(x);
            self.time.copy_from_slice(w);
            let _ = self.fft.process_with_scratch(
                &mut self.time,
                &mut self.spec,
                &mut self.scratch_fwd,
            );
            self.fdl[c][self.newest * bins..(self.newest + 1) * bins].copy_from_slice(&self.spec);
        }
        let norm = 1.0 / (2 * b) as f32;
        for (c, y) in out.into_iter().enumerate() {
            self.acc.fill(Complex::default());
            for p in 0..self.parts {
                let slot = (self.newest + self.parts - p) % self.parts;
                let xs = &self.fdl[c][slot * bins..(slot + 1) * bins];
                let hs = &self.ir[c][p * bins..(p + 1) * bins];
                for ((a, x), h) in self.acc.iter_mut().zip(xs).zip(hs) {
                    *a += x * h;
                }
            }
            self.acc[0].im = 0.0;
            self.acc[b].im = 0.0;
            let _ = self.ifft.process_with_scratch(
                &mut self.acc,
                &mut self.time,
                &mut self.scratch_inv,
            );
            for (o, t) in y.iter_mut().zip(&self.time[b..]) {
                *o += t * norm;
            }
        }
    }
}

pub struct Reverb {
    head: Partitioned,
    tail: Option<Partitioned>,
    tail_in: [Vec<f32>; 2],
    tail_fill: usize,
    tail_out: [Vec<f32>; 2],
    ring: [Vec<f32>; 2],
    now: usize,
    wet: Ramp,
    dry: Ramp,
    conv: [[f32; QUANTUM]; 2],
}

impl Reverb {
    pub fn new(sr: f32, seconds: f64, decay: f64, reverse: bool, seed: u32, wet: f64) -> Reverb {
        let mut ir = impulse_response(sr, seconds, decay, reverse, seed);
        let scale = normalization_scale(&ir, sr);
        for c in ir.iter_mut() {
            c.iter_mut().for_each(|x| *x *= scale);
        }
        Self::with_ir(sr, ir, wet)
    }

    /// A reverb on an explicit (already normalised) IR.
    pub fn with_ir(sr: f32, ir: [Vec<f32>; 2], wet: f64) -> Reverb {
        let mut planner = RealFftPlanner::new();
        let split = ir[0].len().min(TAIL);
        let head = Partitioned::new([&ir[0][..split], &ir[1][..split]], HEAD, &mut planner);
        let tail = (ir[0].len() > TAIL)
            .then(|| Partitioned::new([&ir[0][TAIL..], &ir[1][TAIL..]], TAIL, &mut planner));
        Reverb {
            head,
            tail,
            tail_in: [vec![0.0; TAIL], vec![0.0; TAIL]],
            tail_fill: 0,
            tail_out: [vec![0.0; TAIL], vec![0.0; TAIL]],
            ring: [vec![0.0; RING], vec![0.0; RING]],
            now: 0,
            wet: Ramp::new(wet as f32, RAMP_SECONDS, sr),
            dry: Ramp::new(1.0 - wet as f32, RAMP_SECONDS, sr),
            conv: [[0.0; QUANTUM]; 2],
        }
    }

    pub fn process(&mut self, p: &Params, input: &Bus, out: &mut Bus) {
        self.wet.set(p.get(0));
        self.dry.set(1.0 - p.get(0));
        let (l, r) = (input.chan(0), input.chan(1));
        self.conv = [[0.0; QUANTUM]; 2];
        let [c0, c1] = &mut self.conv;
        self.head
            .process([&l[..], &r[..]], [&mut c0[..], &mut c1[..]]);
        if let Some(tail) = &mut self.tail {
            let f = self.tail_fill;
            self.tail_in[0][f..f + QUANTUM].copy_from_slice(l);
            self.tail_in[1][f..f + QUANTUM].copy_from_slice(r);
            self.tail_fill += QUANTUM;
            if self.tail_fill == TAIL {
                self.tail_fill = 0;
                for o in self.tail_out.iter_mut() {
                    o.fill(0.0);
                }
                let [o0, o1] = &mut self.tail_out;
                tail.process(
                    [&self.tail_in[0][..], &self.tail_in[1][..]],
                    [&mut o0[..], &mut o1[..]],
                );
                // The tail starts at the next quantum.
                for c in 0..2 {
                    for (j, &y) in self.tail_out[c].iter().enumerate() {
                        self.ring[c][(self.now + HEAD + j) % RING] += y;
                    }
                }
            }
        }
        out.channels = 2;
        for i in 0..QUANTUM {
            let (w, d) = (self.wet.tick(), self.dry.tick());
            for c in 0..2 {
                let slot = &mut self.ring[c][(self.now + i) % RING];
                let wet = self.conv[c][i] + *slot;
                *slot = 0.0;
                out.ch[c][i] = input.chan(c)[i] * d + wet * w;
            }
        }
        self.now = (self.now + QUANTUM) % RING;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Params;

    #[test]
    fn convolution_matches_direct() {
        // A short random IR long enough to use both stages.
        let sr = 48000.0;
        let ir = impulse_response(sr, 3000.0 / 48000.0, 1.0, false, 7);
        let len = ir[0].len();
        let mut rv = Reverb::with_ir(sr, ir.clone(), 1.0);
        let p = Params::default();
        p.set(0, 1.0);
        let mut rng = floret_engine::js::Mulberry32::new(3);
        let input: Vec<f32> = (0..QUANTUM * 40)
            .map(|_| (rng.next_f64() * 2.0 - 1.0) as f32)
            .collect();
        let mut got = Vec::new();
        for q in input.chunks(QUANTUM) {
            let mut bus = Bus::new();
            bus.ch[0].copy_from_slice(q);
            let mut out = Bus::new();
            rv.process(&p, &bus, &mut out);
            got.extend_from_slice(&out.ch[1]);
        }
        // Wet output is exactly the convolution.
        for n in (0..input.len()).step_by(37) {
            let want: f64 = (0..len.min(n + 1))
                .map(|k| f64::from(ir[1][k]) * f64::from(input[n - k]))
                .sum();
            assert!(
                (f64::from(got[n]) - want).abs() < 2e-3,
                "n={n}: {} vs {want}",
                got[n]
            );
        }
    }
}
