//! `motion/lfo`: a low-frequency oscillator for animating things.
//!
//! A native-only node (no TypeScript original). The phase accumulates
//! `dt · rate` from the tick's unshifted frame, so changing `rate` live
//! doesn't jump; on its first frame (and after a recompile) it starts from
//! `t · rate`, so a running patch stays where it was. A rising edge on
//! `reset` restarts the cycle. With `count` > 1 the output carries that
//! many copies, each `spread / count` of a cycle later than the one before.
//!
//! `sync: loop` ties it to the patch loop instead: `rate` whole cycles per
//! loop (at least one), the phase a function of the clock, and the random
//! shapes keyed on the cycle within the loop — so a recording of whole
//! loops joins seamlessly.

use super::shared::first;
use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::value::{one, Spread, Value};
use std::f64::consts::TAU;
use std::sync::Arc;

/// More copies than this is a mistake (or a slider pushed too far).
const MAX_COUNT: usize = 4096;

pub fn register(r: &mut Registrar) {
    r.node("motion/lfo", lfo);
}

/// Input order, as in `nodes.json`.
const SHAPE: usize = 0;
const RATE: usize = 1;
const OFFSET: usize = 2;
const MIN: usize = 3;
const MAX: usize = 4;
const WIDTH: usize = 5;
const RESET: usize = 6;
const COUNT: usize = 7;
const SPREAD: usize = 8;
const BPM: usize = 9;
const SEED: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    Sine,
    Triangle,
    Saw,
    RampDown,
    Square,
    Random,
    SmoothRandom,
}

impl Shape {
    fn parse(s: &str) -> Shape {
        match s {
            "triangle" => Shape::Triangle,
            "saw" => Shape::Saw,
            "ramp-down" => Shape::RampDown,
            "square" => Shape::Square,
            "random" => Shape::Random,
            "smooth-random" => Shape::SmoothRandom,
            _ => Shape::Sine,
        }
    }
}

/// The waveform in 0..1 at `cycles` (whole cycles + phase) for copy `copy`.
/// Every shape but `square` starts at 0 on phase 0.
fn wave(shape: Shape, cycles: f64, width: f64, seed: u64, copy: u64) -> f64 {
    let n = cycles.floor();
    let p = cycles - n;
    match shape {
        Shape::Sine => 0.5 - 0.5 * (TAU * p).cos(),
        Shape::Triangle => 1.0 - (1.0 - 2.0 * p).abs(),
        Shape::Saw => p,
        Shape::RampDown => 1.0 - p,
        Shape::Square => f64::from(u8::from(p < width)),
        Shape::Random => hash01(seed, copy, n),
        Shape::SmoothRandom => {
            let (a, b) = (hash01(seed, copy, n), hash01(seed, copy, n + 1.0));
            a + (b - a) * p * p * (3.0 - 2.0 * p)
        }
    }
}

/// A stable value in [0, 1) per (seed, copy, cycle).
fn hash01(seed: u64, copy: u64, cycle: f64) -> f64 {
    // splitmix64 over the three keys.
    let mut x = seed
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(copy.wrapping_mul(0xbf58_476d_1ce4_e5b9))
        .wrapping_add((cycle as i64 as u64).wrapping_mul(0x94d0_49bb_1331_11eb));
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// `sync: loop`: cycles at time `t` for `rate` whole cycles (at least one)
/// per loop of `length` seconds, wrapped to the loop, so every loop — the
/// random shapes' values included — repeats the first. The engine always
/// fills the length in; 0 (a bare frame) reads as the default loop.
fn loop_cycles(t: f64, length: f64, rate: f64) -> f64 {
    let per_loop = rate.round().max(1.0);
    let length = if length > 0.0 {
        length
    } else {
        crate::patch::DEFAULT_LOOP_LENGTH
    };
    let c = per_loop * super::time::loop_phase(t, length);
    // Snap a hair off a cycle boundary onto it (as `loop_phase` does for
    // the loop's), so every loop agrees on which cycle a frame is in.
    let whole = c.round();
    if (c - whole).abs() < 1e-9 * per_loop {
        whole
    } else {
        c
    }
}

/// A finite number from input `i`, else `fallback`.
fn num(inputs: &[Spread], i: usize, fallback: f64) -> f64 {
    let v = first(inputs, i).to_number();
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

fn lfo(_: &NodeInit) -> Box<dyn NodeImpl> {
    // Whole cycles elapsed (not wrapped: the random shapes key on the cycle).
    let mut cycles: Option<f64> = None;
    let mut was_reset = false;
    raw(move |inputs, cx| {
        let shape = match first(inputs, SHAPE) {
            Value::Str(s) => Shape::parse(&s),
            _ => Shape::parse(cx.param("shape").and_then(|v| v.as_str()).unwrap_or("")),
        };
        let bpm = num(inputs, BPM, 0.0);
        // Cycles per second; with a tempo, `rate` counts cycles per beat.
        let freq = num(inputs, RATE, 0.5) * if bpm > 0.0 { bpm / 60.0 } else { 1.0 };
        let reset = first(inputs, RESET).truthy();
        let frame = cx.tick_frame;
        let c = if cx.param("sync").and_then(|v| v.as_str()) == Some("loop") {
            loop_cycles(frame.t, frame.loop_length, num(inputs, RATE, 0.5))
        } else {
            match cycles {
                _ if reset && !was_reset => 0.0,
                Some(c) => c + frame.dt.max(0.0) * freq,
                None => frame.t * freq,
            }
        };
        cycles = Some(c);
        was_reset = reset;

        let offset = num(inputs, OFFSET, 0.0);
        let (lo, hi) = (num(inputs, MIN, 0.0), num(inputs, MAX, 1.0));
        let width = num(inputs, WIDTH, 0.5).clamp(0.0, 1.0);
        let count = num(inputs, COUNT, 1.0).round().clamp(1.0, MAX_COUNT as f64) as usize;
        let spread = num(inputs, SPREAD, 1.0);
        let seed = cx.seed(num(inputs, SEED, 0.0)) as i64 as u64;
        let values: Vec<Value> = (0..count)
            .map(|i| {
                let at = c + offset + spread * i as f64 / count as f64;
                Value::Num(lo + (hi - lo) * wave(shape, at, width, seed, i as u64))
            })
            .collect();
        let phase = (c + offset).rem_euclid(1.0);
        Ok(vec![Arc::from(values), one(Value::Num(phase))])
    })
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{n, node, num, s, Harness};
    use super::*;
    use serde_json::json;

    const EPS: f64 = 1e-9;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    /// Inputs in port order; `None` leaves a port at its default.
    #[derive(Clone)]
    struct Ins {
        shape: &'static str,
        rate: f64,
        offset: f64,
        min: f64,
        max: f64,
        width: f64,
        reset: bool,
        count: f64,
        spread: f64,
        bpm: f64,
        seed: f64,
    }

    impl Default for Ins {
        fn default() -> Self {
            Ins {
                shape: "sine",
                rate: 1.0,
                offset: 0.0,
                min: 0.0,
                max: 1.0,
                width: 0.5,
                reset: false,
                count: 1.0,
                spread: 1.0,
                bpm: 0.0,
                seed: 0.0,
            }
        }
    }

    impl Ins {
        fn values(&self) -> Vec<Value> {
            vec![
                s(self.shape),
                n(self.rate),
                n(self.offset),
                n(self.min),
                n(self.max),
                n(self.width),
                Value::Bool(self.reset),
                n(self.count),
                n(self.spread),
                n(self.bpm),
                n(self.seed),
            ]
        }
    }

    /// Evaluate at time `t` (after a step of `dt`): (values, phase).
    fn at(h: &mut Harness, t: f64, dt: f64, ins: &Ins) -> (Vec<f64>, f64) {
        h.frame.t = t;
        h.frame.dt = dt;
        let out = h
            .eval(ins.values().into_iter().map(|v| vec![v]).collect())
            .unwrap();
        assert_eq!(out.len(), 2);
        (out[0].iter().map(num).collect(), num(&out[1][0]))
    }

    /// The value of a fresh node (rate 1 Hz) at time = phase `p`.
    fn sample(shape: &'static str, p: f64) -> f64 {
        let mut h = node("motion/lfo", json!({ "shape": shape }));
        let ins = Ins {
            shape,
            ..Ins::default()
        };
        at(&mut h, p, 0.0, &ins).0[0]
    }

    #[test]
    fn waveforms_at_known_phases() {
        let table: [(&str, [f64; 5]); 5] = [
            // phase:    0     0.25   0.5   0.75   0.9
            ("sine", [0.0, 0.5, 1.0, 0.5, 0.5 - 0.5 * (TAU * 0.9).cos()]),
            ("triangle", [0.0, 0.5, 1.0, 0.5, 0.2]),
            ("saw", [0.0, 0.25, 0.5, 0.75, 0.9]),
            ("ramp-down", [1.0, 0.75, 0.5, 0.25, 0.1]),
            ("square", [1.0, 1.0, 0.0, 0.0, 0.0]),
        ];
        for (shape, want) in table {
            for (p, w) in [0.0, 0.25, 0.5, 0.75, 0.9].into_iter().zip(want) {
                let got = sample(shape, p);
                assert!(near(got, w), "{shape} at {p}: {got} != {w}");
            }
        }
    }

    #[test]
    fn square_pulse_width_and_range() {
        let mut h = node("motion/lfo", json!({}));
        let ins = Ins {
            shape: "square",
            width: 0.2,
            min: -10.0,
            max: 30.0,
            ..Ins::default()
        };
        assert_eq!(at(&mut h, 0.1, 0.0, &ins).0, [30.0]);
        assert_eq!(at(&mut h, 0.3, 0.2, &ins).0, [-10.0]);
        // Range maps sine's 0.5 to the middle.
        let mut h = node("motion/lfo", json!({}));
        let ins = Ins {
            min: 100.0,
            max: 200.0,
            ..Ins::default()
        };
        assert!(near(at(&mut h, 0.25, 0.0, &ins).0[0], 150.0));
    }

    #[test]
    fn random_steps_hold_within_a_cycle_and_smooth_random_interpolates() {
        let r = |p| sample("random", p);
        let (a, b) = (r(0.1), r(0.9));
        assert_eq!(a, b, "sample & hold keeps one value per cycle");
        assert_ne!(r(0.1), r(1.1), "a new value each cycle");
        assert!((0.0..1.0).contains(&a));
        // Smooth random passes through the step values at cycle starts.
        assert!(near(sample("smooth-random", 0.0), r(0.5)));
        assert!(near(sample("smooth-random", 1.0), r(1.5)));
        let mid = sample("smooth-random", 0.5);
        assert!(near(mid, (r(0.5) + r(1.5)) / 2.0));
        // Deterministic: the same seed gives the same sequence.
        assert_eq!(sample("random", 3.2), sample("random", 3.2));
    }

    #[test]
    fn phase_accumulates_from_dt_and_offsets() {
        let mut h = node("motion/lfo", json!({}));
        let ins = Ins {
            shape: "saw",
            rate: 2.0,
            ..Ins::default()
        };
        // Starts at t · rate, then follows dt.
        let (_, p) = at(&mut h, 0.1, 0.1, &ins);
        assert!(near(p, 0.2));
        let (v, p) = at(&mut h, 0.2, 0.1, &ins);
        assert!(near(p, 0.4) && near(v[0], 0.4));
        // A rate change continues from the current phase (no jump).
        let slow = Ins {
            rate: 1.0,
            ..ins.clone()
        };
        let (_, p) = at(&mut h, 0.3, 0.1, &slow);
        assert!(near(p, 0.5));
        // The offset shifts both outputs; the phase wraps to 0..1.
        let shifted = Ins {
            offset: 0.75,
            ..slow
        };
        let (v, p) = at(&mut h, 0.4, 0.1, &shifted);
        assert!(near(p, 0.35) && near(v[0], 0.35), "{p}");
    }

    #[test]
    fn bpm_makes_rate_cycles_per_beat() {
        let mut h = node("motion/lfo", json!({}));
        let ins = Ins {
            shape: "saw",
            rate: 0.25,
            bpm: 120.0,
            ..Ins::default()
        };
        // 120 bpm = 2 beats a second; a quarter cycle per beat → 0.5 Hz.
        assert!(near(at(&mut h, 0.5, 0.0, &ins).1, 0.25));
    }

    #[test]
    fn reset_restarts_on_the_rising_edge_only() {
        let mut h = node("motion/lfo", json!({}));
        let ins = Ins {
            shape: "saw",
            ..Ins::default()
        };
        let held = Ins {
            reset: true,
            ..ins.clone()
        };
        at(&mut h, 0.3, 0.3, &ins);
        let (v, p) = at(&mut h, 0.4, 0.1, &held);
        assert!(near(p, 0.0) && near(v[0], 0.0), "retriggered");
        // Still held: runs on.
        let (_, p) = at(&mut h, 0.5, 0.1, &held);
        assert!(near(p, 0.1));
        let (_, p) = at(&mut h, 0.6, 0.1, &ins);
        assert!(near(p, 0.2));
        // A new press retriggers again.
        let (_, p) = at(&mut h, 0.7, 0.1, &held);
        assert!(near(p, 0.0));
    }

    #[test]
    fn count_outputs_phase_shifted_copies() {
        let mut h = node("motion/lfo", json!({}));
        let ins = Ins {
            shape: "saw",
            count: 4.0,
            ..Ins::default()
        };
        let (v, p) = at(&mut h, 0.1, 0.0, &ins);
        assert!(near(p, 0.1), "phase is copy 0's");
        let want = [0.1, 0.35, 0.6, 0.85];
        assert!(v.iter().zip(want).all(|(a, b)| near(*a, b)), "{v:?}");
        // `spread` 0.5: the copies span half a cycle.
        let half = Ins {
            spread: 0.5,
            ..ins.clone()
        };
        let (v, _) = at(&mut h, 0.1, 0.0, &half);
        let want = [0.1, 0.225, 0.35, 0.475];
        assert!(v.iter().zip(want).all(|(a, b)| near(*a, b)), "{v:?}");
        // Random copies are independent of each other.
        let rnd = Ins {
            shape: "random",
            count: 3.0,
            spread: 0.0,
            ..Ins::default()
        };
        let (v, _) = at(&mut h, 0.1, 0.0, &rnd);
        assert!(v[0] != v[1] && v[1] != v[2], "{v:?}");
    }

    #[test]
    fn bad_inputs_fall_back() {
        let mut h = node("motion/lfo", json!({}));
        let out = h.eval1(&[s("nope"), n(f64::NAN)]);
        // Unknown shape → sine; NaN rate → 0.5 Hz; one copy at t = 0.
        assert_eq!(num(&out[0]), 0.0);
        assert_eq!(num(&out[1]), 0.0);
    }

    #[test]
    fn loop_sync_repeats_every_loop() {
        let ins = |shape: &'static str, rate: f64| Ins {
            shape,
            rate,
            ..Ins::default()
        };
        for shape in ["sine", "saw", "random", "smooth-random"] {
            let mut h = node("motion/lfo", json!({ "shape": shape, "sync": "loop" }));
            h.frame.loop_length = 2.0;
            let i = ins(shape, 3.0);
            // A recording at a fixed dt: frame N (one loop on) is frame 0
            // again, and so is every frame after.
            let dt = 2.0 / 48.0;
            let first: Vec<_> = (0..48).map(|k| at(&mut h, k as f64 * dt, dt, &i)).collect();
            for k in 48..96 {
                let again = at(&mut h, k as f64 * dt, dt, &i);
                let was = &first[k - 48];
                assert!(near(again.0[0], was.0[0]), "{shape} frame {k}");
                assert!(near(again.1, was.1), "{shape} phase {k}");
            }
        }
        // `rate` whole cycles a loop: a saw at rate 2.4 runs 2 per loop.
        let mut h = node("motion/lfo", json!({ "shape": "saw", "sync": "loop" }));
        h.frame.loop_length = 4.0;
        let i = ins("saw", 2.4);
        assert!(near(at(&mut h, 1.0, 0.0, &i).0[0], 0.5));
        // At least one cycle a loop.
        let i = ins("saw", 0.0);
        assert!(near(at(&mut h, 1.0, 0.0, &i).0[0], 0.25));
        // Free (the default) still accumulates rate · dt.
        let mut h = node("motion/lfo", json!({ "shape": "saw" }));
        h.frame.loop_length = 4.0;
        let i = ins("saw", 0.5);
        at(&mut h, 0.0, 0.0, &i);
        assert!(near(at(&mut h, 1.0, 1.0, &i).0[0], 0.5));
    }
}
