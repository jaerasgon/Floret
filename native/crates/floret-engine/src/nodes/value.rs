//! `value/*` nodes. Ported from `src/nodes/value/number.ts` and `string.ts`.
//!
//! Also hosts `param_or` (the TS `params.param(key, fallback)`) used by every
//! param-backed control in this crate, and the unit-test harness the node
//! modules share.

use super::Registrar;
use crate::node::{raw, EvalCtx, NodeImpl, NodeInit};
use crate::value::{one, Value};

pub fn register(r: &mut Registrar) {
    r.node("value/number", number);
    r.node("value/string", string);
}

/// `params.param(key, fallback)`: the live value, then the saved params,
/// then `fallback`. Like the TS version, an explicit `null` is a value (only
/// a missing key falls back), and the value is returned as-is — no coercion.
pub(crate) fn param_or(cx: &EvalCtx, key: &str, fallback: Value) -> Value {
    match cx.param(key) {
        Some(j) => Value::from_json(j),
        None => fallback,
    }
}

/// A param-backed source node with one output: `[[param(key, fallback)]]`.
pub(crate) fn param_source(key: &'static str, fallback: Value) -> Box<dyn NodeImpl> {
    raw(move |_, cx| Ok(vec![one(param_or(cx, key, fallback.clone()))]))
}

fn number(_: &NodeInit) -> Box<dyn NodeImpl> {
    param_source("value", Value::Num(0.0))
}

fn string(_: &NodeInit) -> Box<dyn NodeImpl> {
    param_source("value", Value::str(""))
}

/// Instantiate and drive single nodes in unit tests.
#[cfg(test)]
pub(crate) mod harness {
    use crate::node::{EvalCtx, FrameCtx, NodeImpl, NodeInit, ParamStore};
    use crate::spec::{Catalogue, Params};
    use crate::value::{Spread, Value};
    use std::sync::Arc;

    pub struct Harness {
        imp: Box<dyn NodeImpl>,
        id: String,
        pub params: ParamStore,
        saved: Params,
        pub frame: FrameCtx,
        /// Unshifted frame; defaults to `frame` when `None`.
        pub tick_frame: Option<FrameCtx>,
    }

    /// Build a node of type `ty` with saved params `params` (a JSON object).
    pub fn node(ty: &str, params: serde_json::Value) -> Harness {
        let cat = Catalogue::builtin();
        let spec = cat.get(ty).unwrap_or_else(|| panic!("no spec for {ty}"));
        let saved: Params = match params {
            serde_json::Value::Object(m) => m,
            _ => Params::new(),
        };
        let resolved = cat.resolved_inputs(spec, &saved);
        let f = crate::nodes::factory(ty).unwrap_or_else(|| panic!("no factory for {ty}"));
        let imp = f(&NodeInit {
            id: "n1",
            spec,
            params: &saved,
            inputs: &resolved,
        });
        Harness {
            imp,
            id: "n1".into(),
            params: ParamStore::default(),
            saved,
            frame: FrameCtx::default(),
            tick_frame: None,
        }
    }

    impl Harness {
        /// Evaluate with whole spreads per input port.
        pub fn eval(&mut self, inputs: Vec<Vec<Value>>) -> Result<Vec<Vec<Value>>, String> {
            let spreads: Vec<Spread> = inputs.into_iter().map(Arc::from).collect();
            let tick = self
                .tick_frame
                .clone()
                .unwrap_or_else(|| self.frame.clone());
            let mut cx = EvalCtx {
                node_id: &self.id,
                frame: &self.frame,
                tick_frame: &tick,
                params: &mut self.params,
                saved_params: &self.saved,
            };
            self.imp
                .eval(&spreads, &mut cx)
                .map(|out| out.into_iter().map(|s| s.to_vec()).collect())
        }

        /// The node's state for a rebuilt node (see `NodeImpl::carry`).
        pub fn carry(&mut self) -> Option<crate::node::CarriedState> {
            self.imp.carry()
        }

        /// Take over a previous node's state (see `NodeImpl::adopt`).
        pub fn adopt(&mut self, state: crate::node::CarriedState) {
            self.imp.adopt(state);
        }

        /// Evaluate with one value per input port; returns slot 0 of each
        /// output (`Undefined` when empty).
        pub fn eval1(&mut self, args: &[Value]) -> Vec<Value> {
            let out = self
                .eval(args.iter().map(|v| vec![v.clone()]).collect())
                .expect("eval");
            out.into_iter()
                .map(|s| s.first().cloned().unwrap_or(Value::Undefined))
                .collect()
        }
    }

    pub fn num(v: &Value) -> f64 {
        v.as_f64().unwrap_or_else(|| panic!("not a number: {v:?}"))
    }

    pub fn close(a: f64, b: f64) -> bool {
        (a.is_nan() && b.is_nan()) || a == b || (a - b).abs() <= 1e-12 * a.abs().max(1.0)
    }

    // ─── Value builders ──────────────────────────────────────────────────

    pub fn n(x: f64) -> Value {
        Value::Num(x)
    }

    pub fn s(x: &str) -> Value {
        Value::str(x)
    }

    pub fn list(items: &[Value]) -> Value {
        Value::list(items.to_vec())
    }

    pub fn obj(pairs: &[(&str, Value)]) -> Value {
        Value::Object(Arc::new(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        ))
    }

    // ─── Expectations recorded from the TS implementation ────────────────
    //
    // Reference values were printed by running the real TS nodes under
    // vitest and encoding with `JSON.stringify` plus tags: numbers as
    // `"#<String(n)>"`, non-finite numbers as `"$NaN"` / `"$Infinity"` /
    // `"$-Infinity"`, `undefined` as `"$undef"`.

    /// Does `got` match the encoded TS value `exp`?
    pub fn matches(exp: &serde_json::Value, got: &Value) -> bool {
        use serde_json::Value as J;
        match exp {
            J::String(t) if t.starts_with('#') => {
                let want: f64 = t[1..].parse().expect("number tag");
                got.as_f64().is_some_and(|g| close(g, want))
            }
            J::String(t) if t == "$undef" => got.is_undefined(),
            J::String(t) if t == "$NaN" => got.as_f64().is_some_and(f64::is_nan),
            J::String(t) if t == "$Infinity" => got.as_f64() == Some(f64::INFINITY),
            J::String(t) if t == "$-Infinity" => got.as_f64() == Some(f64::NEG_INFINITY),
            J::String(t) => matches!(got, Value::Str(g) if **g == **t),
            J::Null => matches!(got, Value::Null),
            J::Bool(b) => matches!(got, Value::Bool(g) if g == b),
            J::Array(items) => match got {
                Value::List(g) => {
                    g.len() == items.len() && items.iter().zip(g.iter()).all(|(e, v)| matches(e, v))
                }
                _ => false,
            },
            J::Object(map) => {
                let keys = match got {
                    Value::Vec2(_) => 2,
                    Value::Color(_) => 4,
                    Value::Object(m) => m.len(),
                    _ => return false,
                };
                keys == map.len() && map.iter().all(|(k, e)| matches(e, &got.get(k)))
            }
            J::Number(_) => panic!("untagged number in expectation"),
        }
    }

    /// `compute`-style: one value per output port.
    #[track_caller]
    pub fn expect(got: &[Value], exp: &str) {
        let e: serde_json::Value = serde_json::from_str(exp).expect("expectation json");
        assert!(
            matches(&e, &Value::list(got.to_vec())),
            "got {got:?}, want {exp}"
        );
    }

    /// `computeRaw` / `readFromContext`-style: one spread per output port.
    #[track_caller]
    pub fn expect_spreads(got: &[Vec<Value>], exp: &str) {
        let as_list: Vec<Value> = got.iter().map(|s| Value::list(s.clone())).collect();
        expect(&as_list, exp);
    }
}

#[cfg(test)]
mod tests {
    use super::harness::node;
    use crate::value::Value;

    #[test]
    fn number_and_string_read_params() {
        let mut n = node("value/number", serde_json::json!({ "value": 3.5 }));
        assert!(matches!(n.eval1(&[])[0], Value::Num(x) if x == 3.5));
        n.params.set("n1", "value", serde_json::json!(7));
        assert!(matches!(n.eval1(&[])[0], Value::Num(x) if x == 7.0));
        let mut n = node("value/number", serde_json::json!({}));
        assert!(matches!(n.eval1(&[])[0], Value::Num(x) if x == 0.0));
        // No coercion: a string value passes straight through.
        let mut n = node("value/number", serde_json::json!({ "value": "4" }));
        assert!(matches!(&n.eval1(&[])[0], Value::Str(s) if &**s == "4"));
        let mut s = node("value/string", serde_json::json!({}));
        assert!(matches!(&s.eval1(&[])[0], Value::Str(s) if s.is_empty()));
        let mut s = node("value/string", serde_json::json!({ "value": null }));
        assert!(matches!(&s.eval1(&[])[0], Value::Null));
    }
}
