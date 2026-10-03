//! `time/*` nodes. Ported from `src/nodes/time/time.ts`.
//!
//! Context nodes: they read the evaluation frame (`cx.frame`), which inside
//! a ForEach with `delay` is shifted per iteration — same as the frame TS
//! passes to `readFromContext`.

use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::value::{one, Value};

pub fn register(r: &mut Registrar) {
    r.node("time/seconds", seconds);
    r.node("time/frame", frame);
    r.node("time/delta", delta);
}

/// `t` and the `loop` phase: 0..1 over the patch's loop length (native
/// only; TS has just `t`, see `legacy::ONCE_SINGLE_OUTPUT`).
fn seconds(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let f = cx.frame;
        Ok(vec![
            one(Value::Num(f.t)),
            one(Value::Num(loop_phase(f.t, f.loop_length))),
        ])
    })
}

/// Where `t` is in a loop of `length` seconds, 0..1. 0 for a length that
/// isn't positive.
///
/// Times within a hair (1e-9 loops) of a loop boundary are exactly 0, so
/// the frame after a recording of whole loops (`t = N · dt`, give or take
/// rounding) is bit-for-bit frame 0 again and the recording joins
/// seamlessly.
pub fn loop_phase(t: f64, length: f64) -> f64 {
    if length.is_nan() || length <= 0.0 || !t.is_finite() {
        return 0.0;
    }
    let x = t / length;
    if (x - x.round()).abs() < 1e-9 {
        return 0.0;
    }
    x.rem_euclid(1.0)
}

fn frame(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| Ok(vec![one(Value::Num(cx.frame.frame_count))]))
}

fn delta(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| Ok(vec![one(Value::Num(cx.frame.dt))]))
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{node, num};
    use super::loop_phase;

    #[test]
    fn reads_frame() {
        for (ty, want) in [
            ("time/seconds", 1.5),
            ("time/frame", 90.0),
            ("time/delta", 0.25),
        ] {
            let mut n = node(ty, serde_json::json!({}));
            n.frame.t = 1.5;
            n.frame.frame_count = 90.0;
            n.frame.dt = 0.25;
            assert_eq!(num(&n.eval1(&[])[0]), want, "{ty}");
        }
    }

    #[test]
    fn loop_output_is_the_phase_through_the_loop_length() {
        let mut n = node("time/seconds", serde_json::json!({}));
        n.frame.loop_length = 4.0;
        for (t, want) in [
            (0.0, 0.0),
            (1.0, 0.25),
            (3.0, 0.75),
            (4.0, 0.0),
            (9.0, 0.25),
        ] {
            n.frame.t = t;
            let out = n.eval1(&[]);
            assert_eq!(num(&out[0]), t, "t at {t}");
            assert!((num(&out[1]) - want).abs() < 1e-12, "loop at {t}");
        }
    }

    #[test]
    fn loop_phase_wraps_and_guards() {
        assert_eq!(loop_phase(2.0, 2.0), 0.0);
        assert!((loop_phase(-0.5, 2.0) - 0.75).abs() < 1e-12);
        assert_eq!(loop_phase(-1e-300, 2.0), 0.0);
        assert_eq!(loop_phase(1.0, 0.0), 0.0);
        assert_eq!(loop_phase(f64::NAN, 2.0), 0.0);
        // N frames of a loop at a fixed dt: frame N is exactly frame 0
        // again, however the products round.
        for (len, n) in [(3.0, 90), (1.01, 30), (4.7, 141), (0.3, 7)] {
            let dt = len / f64::from(n);
            let at = |i: i32| f64::from(i) * dt;
            assert_eq!(loop_phase(at(n), len), 0.0, "{len} s / {n}");
            assert_eq!(loop_phase(at(2 * n), len), 0.0, "{len} s / {n}, twice");
            for i in 1..n {
                let p = loop_phase(at(i), len);
                assert!((p - f64::from(i) / f64::from(n)).abs() < 1e-9);
            }
        }
    }
}
