//! Reference cases for every ported `draw/*`, `style/*` and `transform/*`
//! node.
//!
//! `ts_cases.json` was recorded by calling each TS node directly (via
//! `nodeRegistry.get(type)`: `computeRaw` with the listed spreads, or
//! `compute` with their slot-0 values) — including edge cases: NaN,
//! undefined / null, strings on number pins, unknown anchors / modes,
//! degenerate polylines, legacy Fill params. Values use the fixture
//! encoding (`{"$u":1}`, `{"$num":"NaN"}`, styles as `{"$style": …}` applied
//! to NEUTRAL). Two input-only tags build wire values: `{"$fill": color}` is
//! a `style/fill` style and `{"$point": vec2}` an unstyled `draw/point`.
//!
//! Known divergence (cases left out): TS stores a non-number `w` / `h`
//! verbatim in the shape (`w: undefined`), while `SceneNode` fields are
//! numbers, so Rust holds the coerced value (NaN). Positions agree.

use crate::node::{EvalCtx, FrameCtx, NodeInit, ParamStore};
use crate::scene::{neutral_style, style_value, SceneNode, Style, StyleOp};
use crate::spec::{Catalogue, Params};
use crate::value::{Spread, Value};
use serde_json::{json, Value as J};
use std::sync::Arc;

/// A spread element: the fixture encoding plus the two input tags.
fn decode(j: &J) -> Value {
    if let Some(c) = j.get("$fill") {
        let paint = super::super::style::paint_of(&Value::from_json(c)).unwrap();
        return Value::Style(Style::chain(None, [StyleOp::Fill(Some(paint))]));
    }
    if let Some(p) = j.get("$point") {
        let p = super::super::shared::to_vec2(&Value::from_json(p));
        return Value::Scene(Arc::new(SceneNode::Point {
            x: p.x,
            y: p.y,
            style: crate::scene::line_default_style().clone(),
        }));
    }
    Value::from_json(j)
}

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
        Value::Style(st) => json!({"$style": encode(&style_value(&st.apply(neutral_style())))}),
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

fn run(ty: &str, params: &Params, inputs: &[Spread]) -> Vec<Spread> {
    try_run(ty, params, inputs).unwrap_or_else(|e| panic!("{ty}: {e}"))
}

fn try_run(ty: &str, params: &Params, inputs: &[Spread]) -> Result<Vec<Spread>, String> {
    let cat = Catalogue::builtin();
    let spec = cat.get(ty).unwrap_or_else(|| panic!("{ty} in catalogue"));
    let resolved = cat.resolved_inputs(spec, params);
    let init = NodeInit {
        id: "n",
        spec,
        params,
        inputs: &resolved,
    };
    let factory = crate::nodes::factory(ty).unwrap_or_else(|| panic!("{ty} registered"));
    let mut node = factory(&init);
    let frame = FrameCtx::default();
    let mut store = ParamStore::default();
    let mut cx = EvalCtx {
        node_id: "n",
        frame: &frame,
        tick_frame: &frame,
        params: &mut store,
        saved_params: params,
    };
    node.eval(inputs, &mut cx)
}

#[test]
fn matches_typescript_cases() {
    let cases: Vec<J> = serde_json::from_str(include_str!("ts_cases.json")).expect("ts_cases.json");
    assert!(cases.len() > 100, "parsed {} cases", cases.len());
    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let ty = case["type"].as_str().unwrap();
        let params: Params = case["params"].as_object().cloned().unwrap_or_default();
        let inputs: Vec<Spread> = case["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| Arc::from(s.as_array().unwrap().iter().map(decode).collect::<Vec<_>>()))
            .collect();
        let got = J::Array(
            run(ty, &params, &inputs)
                .iter()
                .map(|s| J::Array(s.iter().map(encode).collect()))
                .collect(),
        );
        if !same(&got, &case["outputs"]) {
            let show = |j: &J| {
                let s = j.to_string();
                s.chars().take(600).collect::<String>()
            };
            failures.push(format!(
                "#{i} {ty} {}\n  rust {}\n  ts   {}",
                case["inputs"],
                show(&got),
                show(&case["outputs"])
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Where TS throws (and so emits nothing), the Rust node errors. Expected
/// values recorded from the TS nodes.
#[test]
fn colour_and_font_edge_cases_match_typescript() {
    let eval = |ty: &str, inputs: Vec<Vec<J>>| {
        let inputs: Vec<Spread> = inputs
            .iter()
            .map(|s| Arc::from(s.iter().map(decode).collect::<Vec<_>>()))
            .collect();
        try_run(ty, &Params::default(), &inputs)
    };
    let pts = vec![json!({"x": 0, "y": 0}), json!({"x": 1, "y": 1})];
    let path_stroke = |color: J| -> Result<J, String> {
        let out = eval(
            "draw/path",
            vec![
                pts.clone(),
                vec![color],
                vec![json!(2)],
                vec![json!(0)],
                vec![],
            ],
        )?;
        Ok(encode(&out[0][0])["style"]["stroke"].clone())
    };
    // `a` missing / not a number → `a.toFixed` throws.
    assert!(path_stroke(json!({"r": 1, "g": 0, "b": 0})).is_err());
    assert!(path_stroke(json!(5)).is_err());
    // `a >= 1` coerces; `clamp01` returns 0 for anything < 0.
    for (a, want) in [
        (json!("1"), "rgb(255,0,0)"),
        (json!(true), "rgb(255,0,0)"),
        (json!("-2"), "rgba(255,0,0,0.000)"),
    ] {
        let c = json!({"r": 1, "g": 0, "b": 0, "a": a});
        assert_eq!(path_stroke(c), Ok(json!(want)), "a = {a}");
    }
    assert!(eval("style/fill", vec![vec![json!(5)], vec![]]).is_err());
    let stroke = |c: J| {
        eval(
            "style/stroke",
            vec![
                vec![c],
                vec![json!(2)],
                vec![json!("center")],
                vec![J::Null],
            ],
        )
    };
    let out = stroke(json!({"r": "0.5", "g": null, "b": 0, "a": "5"})).unwrap();
    assert_eq!(
        encode(&out[0][0])["$style"]["stroke"],
        json!("rgb(128,0,0)")
    );
    assert!(stroke(json!("red")).is_err());
    let text_params = |font: J| {
        let mut inputs = vec![vec![font]];
        for v in [
            json!(false),
            json!(false),
            json!("left"),
            json!("alphabetic"),
            json!(0),
            json!(1.2),
            J::Null,
        ] {
            inputs.push(vec![v]);
        }
        eval("style/text-params", inputs)
    };
    assert!(text_params(json!(5)).is_err());
    // JS `trim` strips the BOM but not U+0085.
    let out = text_params(json!("\u{feff} Inter \u{85}")).unwrap();
    assert_eq!(
        encode(&out[0][0])["$style"]["fontFamily"],
        json!("Inter \u{85}")
    );
}
