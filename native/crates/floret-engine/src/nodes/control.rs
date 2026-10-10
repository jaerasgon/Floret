//! Flow-control building blocks (native-only): `logic/switch` and
//! `io/latch`.
//!
//! **Switch** picks one of N values — by an `index`, or the first case
//! whose condition holds (`first-true`, a chain of `a ? x : b ? y : z`).
//! Any value type passes: numbers, vectors, colours, strings, layers. When
//! the selector is one value the chosen input comes through whole (a
//! spread of layers stays a spread of layers); when it is a spread, the
//! choice is made per element and every input cycles to the longest.
//!
//! **Latch** is a set / reset / toggle flip-flop per element: a bool that
//! remembers which input last fired.

use super::geometry::Pins;
use super::Registrar;
use crate::node::{CarriedState, EvalCtx, NodeImpl, NodeInit, NodeResult};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Spread, Value};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("logic/switch", switch);
    r.dynamic("logic/switch", switch_inputs);
    r.node("io/latch", latch);
}

// ─── Switch ──────────────────────────────────────────────────────────────────

/// Most cases (the editor's `− n +` stepper goes to 32).
const MAX_CASES: usize = 32;

/// `slotCount` as a case count: 1..=32, 2 when unset or not a number.
fn case_count(params: &Params) -> usize {
    let raw = params
        .get("slotCount")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(2.0);
    if raw.is_finite() && raw >= 1.0 {
        (raw.floor() as usize).min(MAX_CASES)
    } else {
        2
    }
}

fn first_true(params: &Params) -> bool {
    params.get("mode").and_then(serde_json::Value::as_str) == Some("first-true")
}

fn any_pin(name: String, label: String) -> PortSpec {
    PortSpec::new(name, PortType::Any)
        .kind(PortKind::Spread)
        .label(label)
}

/// `index` mode: `index`, the values `v0 … vN-1` (labelled 0 … N-1) and
/// the `range` dropdown. `first-true` mode: `if0, v0, if1, v1 …` (labelled
/// "if 0", "then 0" …) and `else`. The values keep their names across
/// modes, so switching mode keeps their wires.
fn switch_inputs(params: &Params) -> Vec<PortSpec> {
    let n = case_count(params);
    let mut pins = Vec::new();
    if first_true(params) {
        for i in 0..n {
            pins.push(
                PortSpec::new(format!("if{i}"), PortType::Boolean)
                    .kind(PortKind::Spread)
                    .default(Value::Bool(false))
                    .label(format!("if {i}")),
            );
            pins.push(any_pin(format!("v{i}"), format!("then {i}")));
        }
        pins.push(any_pin("else".into(), "else".into()));
    } else {
        let mut index = PortSpec::new("index", PortType::Number)
            .kind(PortKind::Spread)
            .default(Value::Num(0.0));
        index.integer = true;
        pins.push(index);
        for i in 0..n {
            pins.push(any_pin(format!("v{i}"), i.to_string()));
        }
        let mut range = PortSpec::new("range", PortType::String).default(Value::str("clamp"));
        range.param = Some("range".into());
        pins.push(range);
    }
    pins
}

/// Element `i` of a value input, cycling; `Null` when it is empty.
fn elem(s: &[Value], i: usize) -> Value {
    if s.is_empty() {
        Value::Null
    } else {
        s[i % s.len()].clone()
    }
}

/// The case an index picks among `n`: NaN reads as 0, fractions floor,
/// out-of-range indices clamp to the ends or wrap round.
fn case_of(v: &Value, n: usize, wrap: bool) -> usize {
    let x = v.to_number();
    let x = if x.is_nan() { 0.0 } else { x.floor() };
    let n = n as f64;
    if wrap {
        if x.is_finite() {
            x.rem_euclid(n) as usize
        } else {
            0
        }
    } else {
        x.clamp(0.0, n - 1.0) as usize
    }
}

struct Switch {
    pins: Pins,
    first_true: bool,
    conds: Vec<String>,
    values: Vec<String>,
}

fn switch(init: &NodeInit) -> Box<dyn NodeImpl> {
    let n = case_count(init.params);
    Box::new(Switch {
        pins: Pins::new(init),
        first_true: first_true(init.params),
        conds: (0..n).map(|i| format!("if{i}")).collect(),
        values: (0..n).map(|i| format!("v{i}")).collect(),
    })
}

impl NodeImpl for Switch {
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        let values: Vec<&[Value]> = self
            .values
            .iter()
            .map(|v| self.pins.spread(inputs, v))
            .collect();
        let longest = |extra: usize| values.iter().map(|v| v.len()).fold(extra, usize::max);
        if self.first_true {
            let conds: Vec<&[Value]> = self
                .conds
                .iter()
                .map(|c| self.pins.spread(inputs, c))
                .collect();
            let other = self.pins.spread(inputs, "else");
            // One condition each: the chosen input comes through whole.
            if conds.iter().all(|c| c.len() <= 1) {
                let k = conds
                    .iter()
                    .position(|c| c.first().is_some_and(Value::truthy));
                let out = k.map_or(other, |k| values[k]);
                return Ok(vec![
                    Arc::from(out),
                    one(Value::Num(k.map_or(-1.0, |k| k as f64))),
                ]);
            }
            let len = longest(conds.iter().map(|c| c.len()).fold(other.len(), usize::max));
            let mut out = Vec::with_capacity(len);
            let mut which = Vec::with_capacity(len);
            for i in 0..len {
                let k = conds
                    .iter()
                    .position(|c| !c.is_empty() && c[i % c.len()].truthy());
                out.push(elem(k.map_or(other, |k| values[k]), i));
                which.push(Value::Num(k.map_or(-1.0, |k| k as f64)));
            }
            return Ok(vec![Arc::from(out), Arc::from(which)]);
        }
        let index = self.pins.spread(inputs, "index");
        let wrap = match self.pins.value(inputs, "range") {
            Value::Str(s) => &*s == "wrap",
            _ => cx.param("range").and_then(|v| v.as_str()) == Some("wrap"),
        };
        let n = values.len();
        if index.is_empty() || n == 0 {
            return Ok(vec![Spread::from(Vec::new()), Spread::from(Vec::new())]);
        }
        if index.len() == 1 {
            let k = case_of(&index[0], n, wrap);
            return Ok(vec![Arc::from(values[k]), one(Value::Num(k as f64))]);
        }
        let len = longest(index.len());
        let mut out = Vec::with_capacity(len);
        let mut which = Vec::with_capacity(len);
        for i in 0..len {
            let k = case_of(&index[i % index.len()], n, wrap);
            out.push(elem(values[k], i));
            which.push(Value::Num(k as f64));
        }
        Ok(vec![Arc::from(out), Arc::from(which)])
    }
}

// ─── Latch ───────────────────────────────────────────────────────────────────

/// Per-element flip-flop state.
#[derive(Default)]
struct LatchState {
    on: Vec<bool>,
    /// Last tick's (set, reset, toggle), for edge detection.
    prev: Vec<[bool; 3]>,
}

struct Latch {
    pins: Pins,
    state: LatchState,
}

fn latch(init: &NodeInit) -> Box<dyn NodeImpl> {
    Box::new(Latch {
        pins: Pins::new(init),
        state: LatchState::default(),
    })
}

/// Element `i` of a bool spread, cycling (an empty spread reads false).
fn bool_at(s: &[Value], i: usize) -> bool {
    !s.is_empty() && s[i % s.len()].truthy()
}

impl NodeImpl for Latch {
    fn eval(&mut self, inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
        let set = self.pins.spread(inputs, "set");
        let reset = self.pins.spread(inputs, "reset");
        let toggle = self.pins.spread(inputs, "toggle");
        let initial = self.pins.bool(inputs, "initial", false);
        let n = if set.is_empty() || reset.is_empty() || toggle.is_empty() {
            0
        } else {
            set.len().max(reset.len()).max(toggle.len())
        };
        let st = &mut self.state;
        st.on.resize(n, initial);
        st.prev.resize(n, [false; 3]);
        let mut changed = Vec::with_capacity(n);
        for i in 0..n {
            let now = [bool_at(set, i), bool_at(reset, i), bool_at(toggle, i)];
            let rose = |k: usize| now[k] && !st.prev[i][k];
            let before = st.on[i];
            if rose(2) {
                st.on[i] = !st.on[i];
            }
            if rose(0) {
                st.on[i] = true;
            }
            // Reset wins over set in the same tick.
            if rose(1) {
                st.on[i] = false;
            }
            st.prev[i] = now;
            changed.push(Value::Bool(st.on[i] != before));
        }
        let state: Vec<Value> = st.on.iter().map(|b| Value::Bool(*b)).collect();
        Ok(vec![Arc::from(state), Arc::from(changed)])
    }

    fn carry(&mut self) -> Option<CarriedState> {
        Some(Box::new(std::mem::take(&mut self.state)))
    }

    fn adopt(&mut self, state: CarriedState) {
        if let Ok(s) = state.downcast::<LatchState>() {
            self.state = *s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::value::harness::{n, node, s, Harness};
    use serde_json::json;

    fn b(x: bool) -> Value {
        Value::Bool(x)
    }

    fn sw(params: serde_json::Value) -> Harness {
        node("logic/switch", params)
    }

    /// Inputs by name, in the node's resolved pin order (unnamed pins
    /// take their defaults).
    fn ins(params: &serde_json::Value, given: &[(&str, Vec<Value>)]) -> Vec<Vec<Value>> {
        let p: Params = serde_json::from_value(params.clone()).unwrap();
        switch_inputs(&p)
            .iter()
            .map(|pin| {
                given
                    .iter()
                    .find(|(k, _)| *k == pin.name)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| pin.default.clone().into_iter().collect())
            })
            .collect()
    }

    fn run(params: serde_json::Value, given: &[(&str, Vec<Value>)]) -> (Vec<Value>, Vec<Value>) {
        let mut h = sw(params.clone());
        let mut out = h.eval(ins(&params, given)).unwrap();
        let which = out.pop().unwrap();
        (out.pop().unwrap(), which)
    }

    fn nums(v: &[Value]) -> Vec<f64> {
        v.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()
    }

    #[test]
    fn pins_follow_mode_and_slots() {
        let names = |p: serde_json::Value| -> Vec<String> {
            switch_inputs(&serde_json::from_value(p).unwrap())
                .into_iter()
                .map(|p| p.name)
                .collect()
        };
        assert_eq!(names(json!({})), ["index", "v0", "v1", "range"]);
        assert_eq!(
            names(json!({"mode": "first-true", "slotCount": 3})),
            ["if0", "v0", "if1", "v1", "if2", "v2", "else"]
        );
        assert_eq!(names(json!({"slotCount": 0})).len(), 4);
        assert_eq!(names(json!({"slotCount": 99})).len(), MAX_CASES + 2);
        // The static spec matches the default mode's pins.
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("logic/switch").unwrap();
        let st: Vec<&str> = spec.inputs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(st, ["index", "v0", "v1", "range"]);
    }

    #[test]
    fn index_passes_whole_spreads() {
        let p = json!({"slotCount": 3});
        let v0 = vec![n(1.0), n(2.0)];
        let v1 = vec![s("a"), s("b"), s("c")];
        let v2 = vec![n(9.0)];
        let given = |i: f64| {
            vec![
                ("index", vec![n(i)]),
                ("v0", v0.clone()),
                ("v1", v1.clone()),
                ("v2", v2.clone()),
            ]
        };
        let (out, which) = run(p.clone(), &given(1.0));
        assert_eq!(out.len(), 3);
        assert!(matches!(&out[2], Value::Str(x) if &**x == "c"));
        assert_eq!(nums(&which), [1.0]);
        // Clamp by default; fractions floor; NaN is 0.
        assert_eq!(nums(&run(p.clone(), &given(7.0)).1), [2.0]);
        assert_eq!(nums(&run(p.clone(), &given(-3.0)).1), [0.0]);
        assert_eq!(nums(&run(p.clone(), &given(1.9)).1), [1.0]);
        assert_eq!(nums(&run(p.clone(), &given(f64::NAN)).1), [0.0]);
        // Wrap.
        let w = json!({"slotCount": 3, "range": "wrap"});
        let wrapped = |i: f64| {
            let mut g = given(i);
            g.push(("range", vec![s("wrap")]));
            nums(&run(w.clone(), &g).1)
        };
        assert_eq!(wrapped(4.0), [1.0]);
        assert_eq!(wrapped(-1.0), [2.0]);
        assert_eq!(wrapped(f64::INFINITY), [0.0]);
    }

    #[test]
    fn index_spread_picks_per_element() {
        let p = json!({});
        let (out, which) = run(
            p,
            &[
                ("index", vec![n(0.0), n(1.0), n(1.0), n(0.0)]),
                ("v0", vec![n(10.0)]),
                ("v1", vec![n(20.0), n(21.0), n(22.0)]),
            ],
        );
        assert_eq!(nums(&out), [10.0, 21.0, 22.0, 10.0]);
        assert_eq!(nums(&which), [0.0, 1.0, 1.0, 0.0]);
        // An empty case gives Null for its elements.
        let (out, _) = run(
            json!({}),
            &[("index", vec![n(1.0), n(0.0)]), ("v0", vec![n(5.0)])],
        );
        assert!(matches!(out[0], Value::Null));
        assert_eq!(out[1].as_f64(), Some(5.0));
        // An empty index: nothing out.
        let (out, which) = run(json!({}), &[("index", vec![]), ("v0", vec![n(5.0)])]);
        assert!(out.is_empty() && which.is_empty());
    }

    #[test]
    fn first_true_is_a_ternary_chain() {
        let p = json!({"mode": "first-true", "slotCount": 2});
        let case = |a: bool, c: bool| {
            run(
                p.clone(),
                &[
                    ("if0", vec![b(a)]),
                    ("v0", vec![s("x")]),
                    ("if1", vec![b(c)]),
                    ("v1", vec![s("y"), s("y2")]),
                    ("else", vec![s("z")]),
                ],
            )
        };
        let name = |o: &[Value]| match &o[0] {
            Value::Str(s) => s.to_string(),
            _ => "?".into(),
        };
        assert_eq!(name(&case(true, true).0), "x");
        assert_eq!(name(&case(false, true).0), "y");
        assert_eq!(case(false, true).0.len(), 2);
        assert_eq!(name(&case(false, false).0), "z");
        assert_eq!(nums(&case(false, false).1), [-1.0]);
        // Per element.
        let (out, which) = run(
            p.clone(),
            &[
                ("if0", vec![b(true), b(false), b(false)]),
                ("v0", vec![n(1.0)]),
                ("if1", vec![b(false), b(true), b(false)]),
                ("v1", vec![n(2.0)]),
                ("else", vec![n(3.0)]),
            ],
        );
        assert_eq!(nums(&out), [1.0, 2.0, 3.0]);
        assert_eq!(nums(&which), [0.0, 1.0, -1.0]);
        // Nothing true and no else: empty.
        let (out, _) = run(p, &[("v0", vec![n(1.0)])]);
        assert!(out.is_empty());
    }

    #[test]
    fn layers_pass_through() {
        let layer = Value::Scene(Arc::new(crate::scene::SceneNode::group(vec![])));
        let (out, _) = run(
            json!({}),
            &[
                ("index", vec![n(1.0)]),
                ("v1", vec![layer.clone(), layer.clone()]),
            ],
        );
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0], Value::Scene(_)));
    }

    fn latch_step(h: &mut Harness, set: &[bool], reset: &[bool], toggle: &[bool]) -> Vec<bool> {
        let bs = |x: &[bool]| x.iter().map(|v| b(*v)).collect::<Vec<_>>();
        let out = h
            .eval(vec![bs(set), bs(reset), bs(toggle), vec![b(false)]])
            .unwrap();
        out[0].iter().map(Value::truthy).collect()
    }

    #[test]
    fn latch_sets_resets_and_toggles_on_edges() {
        let mut h = node("io/latch", json!({}));
        let f = [false];
        let t = [true];
        assert_eq!(latch_step(&mut h, &f, &f, &f), [false]);
        assert_eq!(latch_step(&mut h, &t, &f, &f), [true]);
        // Held set doesn't re-fire after a toggle.
        assert_eq!(latch_step(&mut h, &t, &f, &t), [false]);
        assert_eq!(latch_step(&mut h, &t, &f, &f), [false]);
        assert_eq!(latch_step(&mut h, &f, &f, &t), [true]);
        assert_eq!(latch_step(&mut h, &f, &f, &f), [true]);
        assert_eq!(latch_step(&mut h, &f, &f, &t), [false]);
        // Set and reset rising together: reset wins.
        assert_eq!(latch_step(&mut h, &t, &t, &f), [false]);
        // Reset alone, from on.
        assert_eq!(latch_step(&mut h, &f, &f, &f), [false]);
        assert_eq!(latch_step(&mut h, &f, &f, &t), [true]);
        assert_eq!(latch_step(&mut h, &f, &t, &f), [false]);
    }

    #[test]
    fn latch_per_element_initial_and_changed() {
        let mut h = node("io/latch", json!({}));
        let out = h
            .eval(vec![
                vec![b(false), b(true), b(false)],
                vec![b(false)],
                vec![b(false)],
                vec![b(true)],
            ])
            .unwrap();
        let st: Vec<bool> = out[0].iter().map(Value::truthy).collect();
        let ch: Vec<bool> = out[1].iter().map(Value::truthy).collect();
        // Initial true; element 1's set keeps it true (no change).
        assert_eq!(st, [true, true, true]);
        assert_eq!(ch, [false, false, false]);
        let out = h
            .eval(vec![
                vec![b(false)],
                vec![b(false), b(true), b(false)],
                vec![b(false)],
                vec![b(true)],
            ])
            .unwrap();
        let st: Vec<bool> = out[0].iter().map(Value::truthy).collect();
        let ch: Vec<bool> = out[1].iter().map(Value::truthy).collect();
        assert_eq!(st, [true, false, true]);
        assert_eq!(ch, [false, true, false]);
        // An empty spread empties the output.
        let out = h
            .eval(vec![vec![], vec![b(false)], vec![b(false)], vec![b(false)]])
            .unwrap();
        assert!(out[0].is_empty());
    }

    #[test]
    fn latch_state_survives_an_edit() {
        let mut h = node("io/latch", json!({}));
        latch_step(&mut h, &[true], &[false], &[false]);
        let carried = h.carry().unwrap();
        let mut h2 = node("io/latch", json!({}));
        h2.adopt(carried);
        // Still on, and the held `set` isn't a new edge.
        assert_eq!(latch_step(&mut h2, &[true], &[false], &[false]), [true]);
    }
}
