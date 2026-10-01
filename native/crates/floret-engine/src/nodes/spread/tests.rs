//! Reference cases for every `spread/*` node.
//!
//! `ts_cases.json` was recorded by calling each TS node's `computeRaw`
//! directly (via `nodeRegistry.get(type)`) on the listed inputs — including
//! edge cases: empty spreads, NaN / Infinity / undefined / null, negative
//! and fractional counts, long NaN-laden sorts that exercise TimSort's
//! galloping merges — plus `spread/merge`'s `dynamicInputs` for a range of
//! `slotCount` params. Values use the fixture encoding (`{"$u":1}` for
//! undefined, `{"$num":"NaN"}` for non-finite numbers); a node that threw
//! is recorded as `{"$err": message}`.

use crate::node::{EvalCtx, FrameCtx, NodeInit, ParamStore};
use crate::spec::{Catalogue, Params, PortKind, PortType};
use crate::value::{Spread, Value};
use serde_json::{json, Value as J};
use std::sync::Arc;

fn decode_spread(j: &J) -> Spread {
    let items = j.as_array().expect("spread is an array");
    Arc::from(items.iter().map(Value::from_json).collect::<Vec<_>>())
}

/// Fixture encoding of a runtime value.
fn encode(v: &Value) -> J {
    let num = |n: f64| {
        if n.is_finite() {
            json!(n)
        } else if n.is_nan() {
            json!({"$num": "NaN"})
        } else if n > 0.0 {
            json!({"$num": "Infinity"})
        } else {
            json!({"$num": "-Infinity"})
        }
    };
    match v {
        Value::Undefined => json!({"$u": 1}),
        Value::Null => J::Null,
        Value::Bool(b) => json!(b),
        Value::Num(n) => num(*n),
        Value::Str(s) => json!(&**s),
        Value::Vec2(p) => json!({"x": num(p.x), "y": num(p.y)}),
        Value::Color(c) => json!({"r": num(c.r), "g": num(c.g), "b": num(c.b), "a": num(c.a)}),
        Value::List(items) => J::Array(items.iter().map(encode).collect()),
        Value::Object(m) => J::Object(m.iter().map(|(k, v)| (k.clone(), encode(v))).collect()),
        Value::Scene(n) => encode(&n.to_value()),
        Value::Style(_) => json!({"$fn": "style"}),
        Value::Handle(h) => match h.to_value() {
            Some(obj) => encode(&obj),
            None => json!({"$fn": h.fn_name()}),
        },
        Value::Image(_) => json!({"$opaque": "OffscreenCanvas"}),
    }
}

fn same(a: &J, b: &J) -> bool {
    match (a, b) {
        (J::Number(x), J::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            let d = (x - y).abs();
            d <= 1e-9 || d <= 1e-9 * x.abs().max(y.abs())
        }
        (J::Array(x), J::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (J::Object(x), J::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, p)| y.get(k).is_some_and(|q| same(p, q)))
        }
        _ => a == b,
    }
}

fn run(ty: &str, params: &Params, inputs: &[Spread]) -> Result<Vec<Spread>, String> {
    let cat = Catalogue::builtin();
    let spec = cat.get(ty).unwrap_or_else(|| panic!("{ty} in catalogue"));
    let resolved = cat.resolved_inputs(spec, params);
    let init = NodeInit {
        id: "t",
        spec,
        params,
        inputs: &resolved,
    };
    let factory = crate::nodes::factory(ty).unwrap_or_else(|| panic!("{ty} registered"));
    let mut node = factory(&init);
    let frame = FrameCtx::default();
    let mut store = ParamStore::default();
    let mut cx = EvalCtx {
        node_id: "t",
        frame: &frame,
        tick_frame: &frame,
        params: &mut store,
        saved_params: params,
    };
    node.eval(inputs, &mut cx)
}

fn fixture() -> J {
    serde_json::from_str(include_str!("ts_cases.json")).expect("ts_cases.json")
}

#[test]
fn matches_typescript_reference_cases() {
    let fx = fixture();
    let mut failures = Vec::new();
    let cases = fx["cases"].as_array().unwrap();
    for (i, case) in cases.iter().enumerate() {
        let ty = case["type"].as_str().unwrap();
        let params = case["params"].as_object().cloned().unwrap_or_default();
        let inputs: Vec<Spread> = case["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(decode_spread)
            .collect();
        let got = run(ty, &params, &inputs);
        let want = &case["out"];
        let ok = match (&got, want.get("$err")) {
            (Err(_), Some(_)) => true,
            (Ok(outs), None) => {
                let enc = J::Array(
                    outs.iter()
                        .map(|s| J::Array(s.iter().map(encode).collect()))
                        .collect(),
                );
                same(&enc, want)
            }
            _ => false,
        };
        if !ok {
            let got = got.map(|o| {
                J::Array(
                    o.iter()
                        .map(|s| J::Array(s.iter().map(encode).collect()))
                        .collect(),
                )
            });
            failures.push(format!(
                "#{i} {ty} {params:?} inputs {}\n   want {want}\n   got  {got:?}",
                case["inputs"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} / {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

#[test]
fn every_spread_type_is_covered() {
    let fx = fixture();
    let covered: std::collections::HashSet<&str> = fx["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["type"].as_str())
        .collect();
    let cat = Catalogue::builtin();
    let spread_types: Vec<&str> = cat
        .iter()
        .filter(|s| s.ty.starts_with("spread/"))
        .map(|s| s.ty.as_str())
        .collect();
    assert_eq!(spread_types.len(), 56);
    for ty in spread_types {
        assert!(crate::nodes::factory(ty).is_some(), "{ty} not registered");
        assert!(covered.contains(ty), "{ty} has no reference case");
    }
}

#[test]
fn merge_slots_match_typescript() {
    let fx = fixture();
    let cat = Catalogue::builtin();
    let spec = cat.get("spread/merge").unwrap();
    for case in fx["dyn"].as_array().unwrap() {
        let params = case["params"].as_object().cloned().unwrap_or_default();
        let got = cat.resolved_inputs(spec, &params);
        let want = case["ports"].as_array().unwrap();
        assert_eq!(got.len(), want.len(), "{params:?}");
        for (p, w) in got.iter().zip(want) {
            assert_eq!(p.name, w["name"].as_str().unwrap());
            assert_eq!(p.label.as_deref(), w["label"].as_str());
            assert_eq!(p.ty, PortType::Any);
            assert_eq!(p.kind, PortKind::Spread);
            assert!(p.default.is_none());
        }
    }
}

#[test]
fn ungroup_reads_live_count_param() {
    // The live param store wins over the saved params.
    let cat = Catalogue::builtin();
    let spec = cat.get("spread/ungroup").unwrap();
    let saved: Params = json!({"count": 2}).as_object().cloned().unwrap();
    let init = NodeInit {
        id: "u",
        spec,
        params: &saved,
        inputs: &spec.inputs,
    };
    let mut node = crate::nodes::factory("spread/ungroup").unwrap()(&init);
    let frame = FrameCtx::default();
    let mut store = ParamStore::default();
    store.set("u", "count", json!(5));
    let mut cx = EvalCtx {
        node_id: "u",
        frame: &frame,
        tick_frame: &frame,
        params: &mut store,
        saved_params: &saved,
    };
    let groups: Spread = Arc::from(vec![Value::Num(1.0)]);
    let out = node.eval(&[groups], &mut cx).unwrap();
    assert_eq!(out.len(), 5);
}
