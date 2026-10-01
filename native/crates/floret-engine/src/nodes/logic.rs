//! `logic/*` nodes. Ported from `src/nodes/logic/` (bool.ts, compare.ts,
//! select.ts).
//!
//! The comparisons are JS operators on whatever arrives (`a as number` is
//! only a type cast), so they follow JS semantics: two strings compare by
//! UTF-16 code units, anything else numerically after `ToPrimitive`, and a
//! NaN operand makes every relational operator false. `eq` / `neq` are `===`.

use super::convert::js_less_than;
use super::Registrar;
use crate::node::{scalar, NodeImpl, NodeInit};
use crate::value::Value;

pub fn register(r: &mut Registrar) {
    r.node("logic/and", and);
    r.node("logic/or", or);
    r.node("logic/not", not);
    r.node("logic/gt", gt);
    r.node("logic/lt", lt);
    r.node("logic/gte", gte);
    r.node("logic/lte", lte);
    r.node("logic/eq", eq);
    r.node("logic/neq", neq);
    r.node("logic/select", select);
}

fn bool_node(init: &NodeInit, f: fn(&Value, &Value) -> bool) -> Box<dyn NodeImpl> {
    scalar(init, move |a| Ok(vec![Value::Bool(f(&a[0], &a[1]))]))
}

fn and(init: &NodeInit) -> Box<dyn NodeImpl> {
    bool_node(init, |a, b| a.truthy() && b.truthy())
}

fn or(init: &NodeInit) -> Box<dyn NodeImpl> {
    bool_node(init, |a, b| a.truthy() || b.truthy())
}

fn not(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| Ok(vec![Value::Bool(!a[0].truthy())]))
}

/// `a > b`.
fn gt(init: &NodeInit) -> Box<dyn NodeImpl> {
    bool_node(init, |a, b| js_less_than(b, a) == Some(true))
}

/// `a < b`.
fn lt(init: &NodeInit) -> Box<dyn NodeImpl> {
    bool_node(init, |a, b| js_less_than(a, b) == Some(true))
}

/// `a >= b` is `!(a < b)`, false when undefined (NaN).
fn gte(init: &NodeInit) -> Box<dyn NodeImpl> {
    bool_node(init, |a, b| js_less_than(a, b) == Some(false))
}

/// `a <= b` is `!(b < a)`, false when undefined (NaN).
fn lte(init: &NodeInit) -> Box<dyn NodeImpl> {
    bool_node(init, |a, b| js_less_than(b, a) == Some(false))
}

/// `a === b` (reference identity for lists / objects, see `Value::same`).
fn eq(init: &NodeInit) -> Box<dyn NodeImpl> {
    bool_node(init, |a, b| a.same(b))
}

fn neq(init: &NodeInit) -> Box<dyn NodeImpl> {
    bool_node(init, |a, b| !a.same(b))
}

fn select(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        Ok(vec![if a[0].truthy() {
            a[1].clone()
        } else {
            a[2].clone()
        }])
    })
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{list, n, node, obj, s};
    use crate::value::Value;

    /// Expected results from the TS nodes, one char (T/F) per pair below.
    const EXPECTED: [(&str, &str); 8] = [
        ("logic/gt", "FTFFFTFFFTTFFFFFFFTF"),
        ("logic/lt", "TFFFTFTFFFFFFFTFFFFT"),
        ("logic/gte", "FTTFFTFFTTTTFTFFFTTF"),
        ("logic/lte", "TFTFTFTFTFFTFTTFFTFT"),
        ("logic/eq", "FFTFFFFFFFFFFFFFFTFF"),
        ("logic/neq", "TTFTTTTTTTTTTTTTTFTT"),
        ("logic/and", "TTTFTTTFFFTTFFTFFFTT"),
        ("logic/or", "TTTTTTTFFTTTTFTTFFTT"),
    ];

    fn pairs() -> Vec<(Value, Value)> {
        vec![
            (n(1.0), n(2.0)),
            (n(2.0), n(1.0)),
            (n(1.0), n(1.0)),
            (n(f64::NAN), n(1.0)),
            (s("10"), s("9")),
            (s("10"), n(9.0)),
            (s("a"), s("b")),
            (Value::Undefined, n(0.0)),
            (Value::Null, n(0.0)),
            (Value::Bool(true), n(0.0)),
            (list(&[n(2.0)]), n(1.0)),
            (list(&[n(1.0), n(2.0)]), s("1,2")),
            (obj(&[("x", n(1.0)), ("y", n(2.0))]), n(0.0)),
            (s(""), n(0.0)),
            (s("B"), s("a")),
            (s("x"), n(f64::NAN)),
            (Value::Null, Value::Undefined),
            (n(-0.0), n(0.0)),
            (s("\u{e9}"), s("z")),
            (s("\u{1F600}"), s("\u{FFFF}")),
        ]
    }

    #[test]
    fn binary_ops_match_ts() {
        for (ty, want) in EXPECTED {
            let mut node = node(ty, serde_json::json!({}));
            for (i, ((a, b), w)) in pairs().into_iter().zip(want.chars()).enumerate() {
                let got = node.eval1(&[a.clone(), b.clone()]);
                assert!(
                    matches!(got[0], Value::Bool(g) if g == (w == 'T')),
                    "{ty} pair {i} ({a:?}, {b:?}): got {:?}",
                    got[0]
                );
            }
        }
    }

    #[test]
    fn not_and_select_match_ts() {
        let ins = [
            Value::Bool(true),
            Value::Bool(false),
            n(0.0),
            n(1.0),
            s(""),
            s("a"),
            Value::Null,
            Value::Undefined,
            n(f64::NAN),
            list(&[]),
            obj(&[("x", n(0.0)), ("y", n(0.0))]),
        ];
        let mut not = node("logic/not", serde_json::json!({}));
        for (v, w) in ins.iter().zip("FTTFTFTTTFF".chars()) {
            assert!(
                matches!(not.eval1(std::slice::from_ref(v))[0], Value::Bool(g) if g == (w == 'T'))
            );
        }
        let mut sel = node("logic/select", serde_json::json!({}));
        assert!(matches!(&sel.eval1(&[n(1.0), s("a"), s("b")])[0], Value::Str(x) if &**x == "a"));
        assert!(matches!(&sel.eval1(&[s(""), s("a"), s("b")])[0], Value::Str(x) if &**x == "b"));
    }
}
