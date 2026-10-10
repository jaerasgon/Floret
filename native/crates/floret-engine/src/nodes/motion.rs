//! `motion/*` nodes. Ported from `src/nodes/motion/motion.ts` and
//! `src/nodes/motion/_shared.ts`, plus the few adaptive-value helpers they
//! use from `src/nodes/math/_adaptive.ts` (kept private here).
//!
//! The stateful nodes read `dt` / `t` from the tick's unshifted frame
//! (`frameCtxStore` in TS) and keep their state in the node instance, so it
//! clears on recompile like the TS param-bag state.

use super::shared::first;
use super::Registrar;
use crate::js;
use crate::node::{per_element, per_element_state, raw, NodeImpl, NodeInit};
use crate::spec::{Params, PortSpec, PortType};
use crate::value::{one, Color, Spread, Value, Vec2};
use std::collections::VecDeque;
use std::f64::consts::PI;

pub fn register(r: &mut Registrar) {
    // Smooth and Spring follow each element of a spread with its own state
    // (v4 patches on); Delay Line and Trigger read slot 0.
    r.node("motion/smooth", |i| per_element_state(i, smooth));
    r.dynamic("motion/smooth", smooth_inputs);
    r.node("motion/spring", |i| per_element_state(i, spring));
    r.dynamic("motion/spring", spring_inputs);
    r.node("motion/delay-line", delay_line);
    r.dynamic("motion/delay-line", delay_line_inputs);
    r.node("motion/ease", |i| per_element(i, ease(i)));
    r.node("motion/trigger", trigger);
    r.lifted(&["motion/smooth", "motion/spring", "motion/ease"]);
}

// ─── Adaptive helpers (from math/_adaptive.ts) ───────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Number,
    Vec2,
    Color,
}

fn kind_for_value_type(vt: Option<&serde_json::Value>) -> Kind {
    match vt.and_then(serde_json::Value::as_str) {
        Some("vec2") => Kind::Vec2,
        Some("color") => Kind::Color,
        _ => Kind::Number,
    }
}

fn port_type_for_value_type(vt: Option<&serde_json::Value>) -> PortType {
    match vt.and_then(serde_json::Value::as_str) {
        Some("vec2") => PortType::Vec2,
        Some("color") => PortType::Color,
        Some("number") => PortType::Number,
        _ => PortType::Any,
    }
}

/// `kindOf(v)`: the value kind from its runtime shape.
fn kind_of(v: &Value) -> Option<Kind> {
    if matches!(v, Value::Num(_)) {
        return Some(Kind::Number);
    }
    if v.is_object() {
        let is_num = |k: &str| matches!(v.get(k), Value::Num(_));
        if is_num("r") && is_num("g") && is_num("b") {
            return Some(Kind::Color);
        }
        if is_num("x") && is_num("y") {
            return Some(Kind::Vec2);
        }
    }
    None
}

/// `componentsOf(v, kind)`.
fn components_of(v: &Value, kind: Kind) -> Vec<f64> {
    let num_or = |k: &str, d: f64| v.get(k).as_f64().unwrap_or(d);
    match kind {
        // `typeof v === 'number' ? v : Number(v) || 0`
        Kind::Number => vec![match v {
            Value::Num(n) => *n,
            other => {
                let n = other.to_number();
                if n.is_nan() || n == 0.0 {
                    0.0
                } else {
                    n
                }
            }
        }],
        Kind::Vec2 => vec![num_or("x", 0.0), num_or("y", 0.0)],
        Kind::Color => vec![
            num_or("r", 0.0),
            num_or("g", 0.0),
            num_or("b", 0.0),
            num_or("a", 1.0),
        ],
    }
}

/// `fromComponents(c, kind)`.
fn from_components(c: &[f64], kind: Kind) -> Value {
    let at = |i: usize, d: f64| c.get(i).copied().unwrap_or(d);
    match kind {
        Kind::Number => Value::Num(at(0, 0.0)),
        Kind::Vec2 => Value::Vec2(Vec2 {
            x: at(0, 0.0),
            y: at(1, 0.0),
        }),
        Kind::Color => Value::Color(Color {
            r: at(0, 0.0),
            g: at(1, 0.0),
            b: at(2, 0.0),
            a: at(3, 1.0),
        }),
    }
}

/// `identityValue(kind, scalar)`: every component set to `scalar`.
fn identity_value(kind: Kind, scalar: f64) -> Value {
    let n = match kind {
        Kind::Number => 1,
        Kind::Vec2 => 2,
        Kind::Color => 4,
    };
    from_components(&vec![scalar; n], kind)
}

/// `motionInputs(controls)(params)`: one adaptive `value` pin, then fixed
/// number controls.
fn motion_inputs(params: &Params, controls: &[(&str, f64)]) -> Vec<PortSpec> {
    let vt = params.get("valueType");
    let kind = kind_for_value_type(vt);
    std::iter::once(
        PortSpec::new("value", port_type_for_value_type(vt)).default(identity_value(kind, 0.0)),
    )
    .chain(
        controls
            .iter()
            .map(|(name, d)| PortSpec::new(*name, PortType::Number).default(Value::Num(*d))),
    )
    .collect()
}

fn smooth_inputs(params: &Params) -> Vec<PortSpec> {
    motion_inputs(params, &[("rate", 8.0)])
}

fn spring_inputs(params: &Params) -> Vec<PortSpec> {
    motion_inputs(params, &[("stiffness", 120.0), ("damping", 14.0)])
}

fn delay_line_inputs(params: &Params) -> Vec<PortSpec> {
    motion_inputs(params, &[("delay", 0.5)])
}

/// `Number(inputs[i]?.[0] ?? fallback)`.
fn num_input(inputs: &[Spread], i: usize, fallback: f64) -> f64 {
    let v = first(inputs, i);
    if v.is_nullish() {
        fallback
    } else {
        v.to_number()
    }
}

// ─── _shared.ts ──────────────────────────────────────────────────────────────

/// `clamp01`: NaN passes through, as in JS.
fn clamp01(t: f64) -> f64 {
    t.clamp(0.0, 1.0)
}

/// JS `x > 0` (false for NaN).
fn positive(x: f64) -> bool {
    x > 0.0
}

const C4: f64 = (2.0 * PI) / 3.0;
const C5: f64 = (2.0 * PI) / 4.5;

/// The `EASING` table; unknown names fall back to linear.
pub(crate) fn ease_fn(name: &str, t: f64) -> f64 {
    match name {
        "inQuad" => t * t,
        "outQuad" => 1.0 - (1.0 - t) * (1.0 - t),
        "inOutQuad" => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powf(2.0) / 2.0
            }
        }
        "inCubic" => t * t * t,
        "outCubic" => 1.0 - (1.0 - t).powf(3.0),
        "inOutCubic" => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powf(3.0) / 2.0
            }
        }
        "inElastic" => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                -(2f64.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * C4).sin()
            }
        }
        "outElastic" => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                2f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * C4).sin() + 1.0
            }
        }
        "inOutElastic" => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else if t < 0.5 {
                -(2f64.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * C5).sin()) / 2.0
            } else {
                (2f64.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * C5).sin()) / 2.0 + 1.0
            }
        }
        _ => t, // linear (and the fallback)
    }
}

/// `expSmooth(current, target, dt, rate)`.
fn exp_smooth(current: f64, target: f64, dt: f64, rate: f64) -> f64 {
    if !positive(dt) || !positive(rate) {
        return current;
    }
    let k = 1.0 - (-rate * dt).exp();
    let next = current + (target - current) * k;
    if next.is_finite() {
        next
    } else {
        target
    }
}

/// `integrateSpring(value, target, vel, dt, k, damp)` → (value, vel).
fn integrate_spring(value: f64, target: f64, vel: f64, dt: f64, k: f64, damp: f64) -> (f64, f64) {
    if !positive(dt) {
        return (value, vel);
    }
    const MAX_STEP: f64 = 1.0 / 120.0;
    let mut remaining = js::min(dt, 0.1);
    let mut v = value;
    let mut vv = vel;
    while remaining > 0.0 {
        let h = js::min(MAX_STEP, remaining);
        let accel = k * (target - v) - damp * vv;
        vv += accel * h;
        v += vv * h;
        remaining -= h;
    }
    if !v.is_finite() || !vv.is_finite() {
        return (target, 0.0);
    }
    (v, vv)
}

// ─── Nodes ───────────────────────────────────────────────────────────────────

fn smooth() -> Box<dyn NodeImpl> {
    let mut current: Option<Vec<f64>> = None;
    raw(move |inputs, cx| {
        let target = first(inputs, 0);
        let rate = num_input(inputs, 1, 8.0);
        let dt = cx.tick_frame.dt;
        let kind = kind_of(&target).unwrap_or(Kind::Number);
        let tc = components_of(&target, kind);
        let out = match &mut current {
            Some(cur) if cur.len() == tc.len() => {
                for (c, t) in cur.iter_mut().zip(&tc) {
                    *c = exp_smooth(*c, *t, dt, rate);
                }
                from_components(cur, kind)
            }
            _ => {
                current = Some(tc.clone());
                from_components(&tc, kind)
            }
        };
        Ok(vec![one(out)])
    })
}

fn spring() -> Box<dyn NodeImpl> {
    // (value, vel) per component.
    let mut state: Option<Vec<(f64, f64)>> = None;
    raw(move |inputs, cx| {
        let target = first(inputs, 0);
        let k = num_input(inputs, 1, 120.0);
        let damp = num_input(inputs, 2, 14.0);
        let dt = cx.tick_frame.dt;
        let kind = kind_of(&target).unwrap_or(Kind::Number);
        let tc = components_of(&target, kind);
        let out = match &mut state {
            Some(st) if st.len() == tc.len() => {
                for (s, t) in st.iter_mut().zip(&tc) {
                    *s = integrate_spring(s.0, *t, s.1, dt, k, damp);
                }
                let values: Vec<f64> = st.iter().map(|s| s.0).collect();
                from_components(&values, kind)
            }
            _ => {
                state = Some(tc.iter().map(|v| (*v, 0.0)).collect());
                from_components(&tc, kind)
            }
        };
        Ok(vec![one(out)])
    })
}

struct Sample {
    t: f64,
    v: Vec<f64>,
}

fn delay_line(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut hist: Option<VecDeque<Sample>> = None;
    raw(move |inputs, cx| {
        let target0 = first(inputs, 0);
        let delay = js::max(0.0, num_input(inputs, 1, 0.5));
        let kind = kind_of(&target0).unwrap_or(Kind::Number);
        let v = components_of(&target0, kind);
        let t = cx.tick_frame.t;
        let reset = match &hist {
            None => true,
            Some(h) => h.front().is_some_and(|s| s.v.len() != v.len()),
        };
        if reset {
            hist = Some(VecDeque::new());
        }
        let hist = hist.as_mut().expect("history initialised");
        hist.push_back(Sample { t, v });
        // Keep the bracketing sample just before the cutoff; hard cap 4096.
        let cutoff = t - delay - 0.001;
        while hist.len() > 2 && hist[1].t < cutoff {
            hist.pop_front();
        }
        if hist.len() > 4096 {
            hist.pop_front();
        }

        let target = t - delay;
        if target <= hist[0].t {
            return Ok(vec![one(from_components(&hist[0].v, kind))]);
        }
        for i in (0..hist.len()).rev() {
            if hist[i].t <= target {
                let a = &hist[i];
                let b = hist.get(i + 1).unwrap_or(a);
                let span = b.t - a.t;
                let f = if span > 0.0 {
                    (target - a.t) / span
                } else {
                    0.0
                };
                let c: Vec<f64> =
                    a.v.iter()
                        .enumerate()
                        .map(|(j, c)| c + (b.v.get(j).copied().unwrap_or(0.0) - c) * f)
                        .collect();
                return Ok(vec![one(from_components(&c, kind))]);
            }
        }
        Ok(vec![one(from_components(&hist[0].v, kind))])
    })
}

fn ease(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let t = num_input(inputs, 0, 0.0);
        // `EASING[name] ?? EASING.linear`: a missing param is 'inOutQuad';
        // any non-string (or unknown) name is linear.
        let name = match cx.param("curve") {
            None => "inOutQuad",
            Some(serde_json::Value::String(s)) => s.as_str(),
            Some(_) => "linear",
        };
        Ok(vec![one(Value::Num(ease_fn(name, clamp01(t))))])
    })
}

fn trigger(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut prev: Option<f64> = None;
    raw(move |inputs, cx| {
        let value = num_input(inputs, 0, 0.0);
        let threshold = num_input(inputs, 1, 0.5);
        let Some(p) = prev else {
            prev = Some(value);
            return Ok(vec![one(Value::Bool(false))]);
        };
        let fire = match cx.param("mode").and_then(serde_json::Value::as_str) {
            Some("change") => value != p,
            Some("threshold") => p < threshold && value >= threshold,
            // 'rising' (the default) and anything else: a boolean-ish edge.
            _ => p < 0.5 && value >= 0.5,
        };
        prev = Some(value);
        Ok(vec![one(Value::Bool(fire))])
    })
}

#[cfg(test)]
mod tests {
    //! Expected values recorded from the TypeScript implementation.
    use super::*;
    use crate::node::{EvalCtx, FrameCtx, ParamStore};
    use crate::spec::Catalogue;
    use crate::value::empty;
    use serde_json::json;

    const NAN: f64 = f64::NAN;
    const INF: f64 = f64::INFINITY;
    const DT: f64 = 1.0 / 60.0;

    /// One compiled node instance driven tick by tick.
    struct Driver {
        node: Box<dyn NodeImpl>,
        params: Params,
        store: ParamStore,
    }

    impl Driver {
        fn new(ty: &str, params: serde_json::Value) -> Driver {
            let cat = Catalogue::builtin();
            let spec = cat.get(ty).unwrap();
            let params: Params = params.as_object().cloned().unwrap_or_default();
            let resolved = cat.resolved_inputs(spec, &params);
            let init = NodeInit {
                id: "n",
                spec,
                params: &params,
                inputs: &resolved,
            };
            let node = crate::nodes::factory(ty).unwrap()(&init);
            Driver {
                node,
                params,
                store: ParamStore::default(),
            }
        }

        /// `inputs`: one entry per port, `None` = empty spread.
        fn step(&mut self, dt: f64, t: f64, inputs: &[Option<Value>]) -> Value {
            // The shifted `frame` must not be what the node reads.
            let shifted = FrameCtx {
                t: t + 100.0,
                dt: dt + 100.0,
                ..FrameCtx::default()
            };
            let tick = FrameCtx {
                t,
                dt,
                ..FrameCtx::default()
            };
            let mut cx = EvalCtx {
                node_id: "n",
                frame: &shifted,
                tick_frame: &tick,
                params: &mut self.store,
                saved_params: &self.params,
            };
            let ins: Vec<Spread> = inputs
                .iter()
                .map(|v| v.clone().map_or_else(empty, one))
                .collect();
            let out = self.node.eval(&ins, &mut cx).unwrap();
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].len(), 1);
            out[0][0].clone()
        }
    }

    fn n(x: f64) -> Value {
        Value::Num(x)
    }

    fn v2(x: f64, y: f64) -> Value {
        Value::Vec2(Vec2 { x, y })
    }

    fn c(r: f64, g: f64, b: f64, a: f64) -> Value {
        Value::Color(Color { r, g, b, a })
    }

    fn close(a: f64, b: f64) -> bool {
        (a.is_nan() && b.is_nan())
            || a == b
            || (a - b).abs() <= 1e-12 * (1.0 + a.abs().max(b.abs()))
    }

    fn same(got: &Value, want: &Value) -> bool {
        match (got, want) {
            (Value::Num(a), Value::Num(b)) => close(*a, *b),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Vec2(a), Value::Vec2(b)) => close(a.x, b.x) && close(a.y, b.y),
            (Value::Color(a), Value::Color(b)) => {
                close(a.r, b.r) && close(a.g, b.g) && close(a.b, b.b) && close(a.a, b.a)
            }
            _ => false,
        }
    }

    /// Run a sequence of (dt, t, inputs) steps and compare every output.
    fn check(ty: &str, params: serde_json::Value, steps: &[(f64, f64, Vec<Option<Value>>, Value)]) {
        let mut d = Driver::new(ty, params);
        for (i, (dt, t, ins, want)) in steps.iter().enumerate() {
            let got = d.step(*dt, *t, ins);
            assert!(
                same(&got, want),
                "{ty} step {i}: got {got:?}, want {want:?}"
            );
        }
    }

    /// Steps at dt = 1/60, t = i/60 with the given value and fixed controls.
    fn seq(
        vals: Vec<Value>,
        ctl: Vec<Option<Value>>,
        wants: Vec<Value>,
    ) -> Vec<(f64, f64, Vec<Option<Value>>, Value)> {
        vals.into_iter()
            .zip(wants)
            .enumerate()
            .map(|(i, (v, w))| {
                let mut ins = vec![Some(v)];
                ins.extend(ctl.iter().cloned());
                (DT, i as f64 / 60.0, ins, w)
            })
            .collect()
    }

    #[test]
    fn ease_curves() {
        let cases: [(&str, [f64; 2]); 10] = [
            ("linear", [0.3, 0.7]),
            ("inQuad", [0.09, 0.48999999999999994]),
            ("outQuad", [0.51, 0.9099999999999999]),
            ("inOutQuad", [0.18, 0.82]),
            ("inCubic", [0.027, 0.3429999999999999]),
            ("outCubic", [0.657, 0.973]),
            ("inOutCubic", [0.108, 0.8919999999999999]),
            ("inElastic", [-0.0039062499999999918, 0.125]),
            ("outElastic", [0.875, 1.00390625]),
            ("inOutElastic", [0.023938888847468056, 0.9760611111525319]),
        ];
        let ease = |params: serde_json::Value, t: Option<Value>| {
            Driver::new("motion/ease", params).step(DT, 0.0, &[t])
        };
        for (curve, [a, b]) in cases {
            let p = json!({ "curve": curve });
            assert!(same(&ease(p.clone(), Some(n(0.3))), &n(a)), "{curve}");
            assert!(same(&ease(p.clone(), Some(n(0.7))), &n(b)), "{curve}");
            assert!(same(&ease(p.clone(), Some(n(-1.0))), &n(0.0)), "{curve}");
            assert!(same(&ease(p.clone(), Some(n(2.0))), &n(1.0)), "{curve}");
            assert!(same(&ease(p, Some(n(NAN))), &n(NAN)), "{curve}");
        }
        for p in [json!({"curve": "nope"}), json!({"curve": 5})] {
            assert!(same(&ease(p.clone(), Some(n(0.3))), &n(0.3)));
            assert!(same(&ease(p, Some(n(2.0))), &n(1.0)));
        }
        assert!(same(&ease(json!({}), Some(n(0.3))), &n(0.18)));
        assert!(same(&ease(json!({}), None), &n(0.0)));
        assert!(same(
            &ease(json!({}), Some(Value::str("0.4"))),
            &n(0.32000000000000006)
        ));
    }

    #[test]
    fn adaptive_ports() {
        let ports = |ty: &str, vt: Option<&str>| {
            let mut p = Params::new();
            if let Some(vt) = vt {
                p.insert("valueType".into(), vt.into());
            }
            let f = match ty {
                "smooth" => smooth_inputs,
                "spring" => spring_inputs,
                _ => delay_line_inputs,
            };
            f(&p)
                .into_iter()
                .map(|p| (p.name, p.ty, p.default.unwrap()))
                .collect::<Vec<_>>()
        };
        let any = ports("smooth", Some("any"));
        assert_eq!(any.len(), 2);
        assert_eq!((any[0].0.as_str(), any[0].1), ("value", PortType::Any));
        assert!(same(&any[0].2, &n(0.0)));
        assert_eq!((any[1].0.as_str(), any[1].1), ("rate", PortType::Number));
        assert!(same(&any[1].2, &n(8.0)));
        assert_eq!(ports("smooth", None)[0].1, PortType::Any);
        let num = ports("smooth", Some("number"));
        assert_eq!(num[0].1, PortType::Number);
        let vec = ports("smooth", Some("vec2"));
        assert_eq!(vec[0].1, PortType::Vec2);
        assert!(same(&vec[0].2, &v2(0.0, 0.0)));
        let col = ports("spring", Some("color"));
        assert_eq!(col[0].1, PortType::Color);
        assert!(same(&col[0].2, &c(0.0, 0.0, 0.0, 0.0)));
        let names: Vec<_> = col.iter().map(|p| p.0.as_str()).collect();
        assert_eq!(names, ["value", "stiffness", "damping"]);
        assert!(same(&col[1].2, &n(120.0)) && same(&col[2].2, &n(14.0)));
        let dl = ports("delay", Some("vec2"));
        assert_eq!(dl[0].1, PortType::Vec2);
        assert_eq!(dl[1].0, "delay");
        assert!(same(&dl[1].2, &n(0.5)));
    }

    #[test]
    fn smooth_sequences() {
        let p = json!({});
        check(
            "motion/smooth",
            p.clone(),
            &seq(
                [0.0, 1.0, 1.0, 1.0, 1.0, 5.0, 5.0, 5.0].map(n).to_vec(),
                vec![Some(n(8.0))],
                [
                    0.0,
                    0.12482668095705252,
                    0.23407166163535126,
                    0.3296799539643606,
                    0.4133537804899681,
                    0.9858896047956178,
                    1.4869576824241832,
                    1.925479094988844,
                ]
                .map(n)
                .to_vec(),
            ),
        );
        // Varying dt, default rate.
        check(
            "motion/smooth",
            p.clone(),
            &[
                (0.0, 0.0, vec![Some(n(0.0)), None], n(0.0)),
                (0.016, 0.0, vec![Some(n(1.0)), None], n(0.12014662085535621)),
                (0.5, 0.0, vec![Some(n(1.0)), None], n(0.9838849232325542)),
                (0.1, 0.0, vec![Some(n(1.0)), None], n(0.9927590292494141)),
            ],
        );
        // Kind changes reset the state.
        check(
            "motion/smooth",
            p.clone(),
            &seq(
                vec![
                    v2(0.0, 0.0),
                    v2(1.0, 2.0),
                    v2(1.0, 2.0),
                    n(3.0),
                    n(3.0),
                    v2(4.0, 4.0),
                ],
                vec![Some(n(3.0))],
                vec![
                    v2(0.0, 0.0),
                    v2(0.048770575499285984, 0.09754115099857197),
                    v2(0.09516258196404041, 0.19032516392808083),
                    n(3.0),
                    n(3.0),
                    v2(4.0, 4.0),
                ],
            ),
        );
        check(
            "motion/smooth",
            p.clone(),
            &seq(
                vec![
                    c(0.2, 0.4, 0.6, 0.5),
                    c(1.0, 0.0, 0.0, 1.0),
                    c(1.0, 0.0, 0.0, 1.0),
                    c(0.0, 0.0, 1.0, 0.0),
                    Value::from_json(&json!({"r": 1, "g": 1, "b": 1})),
                ],
                vec![Some(n(20.0))],
                vec![
                    c(0.2, 0.4, 0.6, 0.5),
                    c(
                        0.4267749515409686,
                        0.2866125242295157,
                        0.42991878634427355,
                        0.6417343447131054,
                    ),
                    c(
                        0.5892663047739264,
                        0.20536684761303683,
                        0.3080502714195552,
                        0.743291440483704,
                    ),
                    c(
                        0.42222775763663545,
                        0.14715177646857697,
                        0.5041963541290762,
                        0.5325915899880681,
                    ),
                    c(
                        0.5860080979662214,
                        0.38890754467250144,
                        0.6447411638368441,
                        0.6650872394009394,
                    ),
                ],
            ),
        );
        // Non-finite targets.
        check(
            "motion/smooth",
            p.clone(),
            &seq(
                vec![
                    n(1.0),
                    n(2.0),
                    n(NAN),
                    n(2.0),
                    Value::str("x"),
                    n(3.0),
                    n(INF),
                    n(4.0),
                ],
                vec![Some(n(8.0))],
                [
                    1.0,
                    1.1248266809570526,
                    NAN,
                    2.0,
                    1.750346638085895,
                    1.906336719600455,
                    INF,
                    4.0,
                ]
                .map(n)
                .to_vec(),
            ),
        );
        check(
            "motion/smooth",
            p,
            &seq(
                [0.0, 10.0, 10.0].map(n).to_vec(),
                vec![Some(n(-1.0))],
                [0.0, 0.0, 0.0].map(n).to_vec(),
            ),
        );
    }

    #[test]
    fn spring_sequences() {
        let p = json!({});
        check(
            "motion/spring",
            p.clone(),
            &seq(
                [0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]
                    .map(n)
                    .to_vec(),
                vec![Some(n(120.0)), Some(n(14.0))],
                [
                    0.0,
                    0.02395833333333333,
                    0.07322157118055556,
                    0.14068676644784434,
                    0.22024157216696094,
                    0.30673801994562033,
                    0.39594185607378296,
                    0.48446439257703017,
                    0.5696828494588526,
                    0.6496542117101176,
                ]
                .map(n)
                .to_vec(),
            ),
        );
        // Large / zero dt, default stiffness + damping.
        check(
            "motion/spring",
            p.clone(),
            &[
                (DT, 0.0, vec![Some(n(0.0)), None, None], n(0.0)),
                (
                    0.25,
                    0.0,
                    vec![Some(n(1.0)), None, None],
                    n(0.39594185607378307),
                ),
                (
                    0.05,
                    0.0,
                    vec![Some(n(1.0)), None, None],
                    n(0.6496542117101178),
                ),
                (
                    0.0,
                    0.0,
                    vec![Some(n(1.0)), None, None],
                    n(0.6496542117101178),
                ),
                (
                    0.3,
                    0.0,
                    vec![Some(n(1.0)), None, None],
                    n(0.9743021673926405),
                ),
            ],
        );
        check(
            "motion/spring",
            p.clone(),
            &seq(
                vec![v2(0.0, 0.0), v2(10.0, -5.0), v2(10.0, -5.0), v2(10.0, -5.0)],
                vec![Some(n(300.0)), Some(n(5.0))],
                vec![
                    v2(0.0, 0.0),
                    v2(0.6119791666666666, -0.3059895833333333),
                    v2(1.935984293619792, -0.967992146809896),
                    v2(3.8064555547855523, -1.9032277773927762),
                ],
            ),
        );
        let red = c(1.0, 0.0, 0.0, 1.0);
        check(
            "motion/spring",
            p.clone(),
            &seq(
                vec![c(0.2, 0.4, 0.6, 0.5), red.clone(), red.clone(), red],
                vec![Some(n(50.0)), Some(n(2.0))],
                vec![
                    c(0.2, 0.4, 0.6, 0.5),
                    c(
                        0.20827739197530865,
                        0.3958613040123457,
                        0.5937919560185185,
                        0.505173369984568,
                    ),
                    c(
                        0.22717613766610195,
                        0.38641193116694905,
                        0.5796178967504235,
                        0.5169850860413138,
                    ),
                    c(
                        0.25608507494981003,
                        0.371957462525095,
                        0.5579361937876425,
                        0.5350531718436313,
                    ),
                ],
            ),
        );
        // Unstable stiffness diverges (finite, so no reset).
        check(
            "motion/spring",
            p,
            &seq(
                [0.0, 1.0, 1.0, 1.0, 1.0].map(n).to_vec(),
                vec![Some(n(1e10)), Some(n(0.0))],
                [
                    0.0,
                    -482251003086.4197,
                    -2.325656950827748e+23,
                    -1.1215487823366877e+35,
                    -5.408672464411422e+46,
                ]
                .map(n)
                .to_vec(),
            ),
        );
    }

    #[test]
    fn delay_line_sequences() {
        let dl = |vals: Vec<Value>, delay: Value, ts: &[f64], wants: Vec<Value>| {
            vals.into_iter()
                .zip(ts)
                .zip(wants)
                .map(|((v, t), w)| (DT, *t, vec![Some(v), Some(delay.clone())], w))
                .collect::<Vec<_>>()
        };
        let p = json!({});
        let ts: Vec<f64> = (0..9).map(|i| f64::from(i) * 0.02).collect();
        check(
            "motion/delay-line",
            p.clone(),
            &dl(
                (0..9).map(|i| n(f64::from(i))).collect(),
                n(0.05),
                &ts,
                [
                    0.0,
                    0.0,
                    0.0,
                    0.4999999999999997,
                    1.5,
                    2.5,
                    3.4999999999999996,
                    4.5,
                    5.5,
                ]
                .map(n)
                .to_vec(),
            ),
        );
        let ts = [0.0, 0.3, 0.6, 0.9, 1.2];
        let ramp: Vec<Value> = (0..5).map(|i| n(f64::from(i))).collect();
        // `[]` → Number([]) = 0 delay; negative clamps to 0; NaN never advances.
        for (delay, want) in [
            (Value::list(vec![]), [0.0, 1.0, 2.0, 3.0, 4.0]),
            (n(-1.0), [0.0, 1.0, 2.0, 3.0, 4.0]),
            (n(NAN), [0.0; 5]),
        ] {
            check(
                "motion/delay-line",
                p.clone(),
                &dl(ramp.clone(), delay, &ts, want.map(n).to_vec()),
            );
        }
        // Repeated timestamps (zero span).
        check(
            "motion/delay-line",
            p.clone(),
            &dl(
                [0.0, 1.0, 1.0, 2.0, 3.0].map(n).to_vec(),
                n(0.1),
                &[0.0, 0.1, 0.1, 0.15, 0.3],
                [0.0, 0.0, 0.0, 0.4999999999999999, 2.333333333333333]
                    .map(n)
                    .to_vec(),
            ),
        );
        // Kind changes clear the history.
        let col = c(0.2, 0.4, 0.6, 0.5);
        check(
            "motion/delay-line",
            p.clone(),
            &dl(
                vec![
                    v2(1.0, 2.0),
                    v2(3.0, 4.0),
                    n(5.0),
                    n(6.0),
                    v2(0.0, 0.0),
                    col.clone(),
                    col.clone(),
                ],
                n(0.03),
                &[0.0, 0.02, 0.04, 0.06, 0.08, 0.1, 0.12],
                vec![
                    v2(1.0, 2.0),
                    v2(1.0, 2.0),
                    n(5.0),
                    n(5.0),
                    v2(0.0, 0.0),
                    col.clone(),
                    col,
                ],
            ),
        );
        // Time going backwards.
        check(
            "motion/delay-line",
            p,
            &dl(
                [1.0, 2.0, 3.0, 4.0].map(n).to_vec(),
                n(0.1),
                &[1.0, 0.9, 1.5, 1.55],
                [1.0, 1.0, 2.833333333333333, 2.9166666666666665]
                    .map(n)
                    .to_vec(),
            ),
        );
    }

    #[test]
    fn trigger_modes() {
        let b = |xs: &[bool]| xs.iter().map(|x| Value::Bool(*x)).collect::<Vec<_>>();
        check(
            "motion/trigger",
            json!({}),
            &seq(
                [0.0, 1.0, 1.0, 0.0, 0.6, 0.6, 0.4, 0.7, NAN, NAN]
                    .map(n)
                    .to_vec(),
                vec![None],
                b(&[
                    false, true, false, false, true, false, false, true, false, false,
                ]),
            ),
        );
        check(
            "motion/trigger",
            json!({"mode": "change"}),
            &seq(
                vec![
                    n(0.0),
                    n(1.0),
                    n(1.0),
                    n(0.0),
                    n(NAN),
                    n(NAN),
                    Value::str("2"),
                    n(2.0),
                ],
                vec![None],
                b(&[false, true, false, true, true, true, true, false]),
            ),
        );
        check(
            "motion/trigger",
            json!({"mode": "threshold"}),
            &seq(
                [0.0, 1.0, 1.0, 0.0, 3.0, 3.0, 1.0, 3.0].map(n).to_vec(),
                vec![Some(n(2.0))],
                b(&[false, false, false, false, true, false, false, true]),
            ),
        );
        check(
            "motion/trigger",
            json!({"mode": 7}),
            &seq(
                [0.0, 1.0, 1.0, 0.0].map(n).to_vec(),
                vec![Some(n(2.0))],
                b(&[false, true, false, false]),
            ),
        );
    }
}
