//! Per-element evaluation over spreads (patches from v4 on): every lifting
//! node type runs once per element by Expression's rule, and unlifted
//! (older files) keeps reading slot 0.

use super::value::harness::{lifted, n, node, num, Harness};
use crate::spec::Catalogue;
use crate::value::{Color, Value, Vec2};
use serde_json::{json, Value as Json};

fn v2(x: f64, y: f64) -> Value {
    Value::Vec2(Vec2 { x, y })
}

fn rgba(r: f64, g: f64, b: f64, a: f64) -> Value {
    Value::Color(Color { r, g, b, a })
}

fn nums(xs: &[f64]) -> Vec<Value> {
    xs.iter().map(|x| n(*x)).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || (a - b).abs() <= 1e-9 * a.abs().max(1.0)
}

/// Same value, as far as a formula can tell: numbers (booleans as 0 / 1),
/// vec2s and colours per component.
fn same(a: &Value, b: &Value) -> bool {
    let as_num = |v: &Value| match v {
        Value::Bool(x) => Some(f64::from(u8::from(*x))),
        Value::Num(x) => Some(*x),
        _ => None,
    };
    match (a, b) {
        (Value::Vec2(p), Value::Vec2(q)) => close(p.x, q.x) && close(p.y, q.y),
        (Value::Color(p), Value::Color(q)) => {
            close(p.r, q.r) && close(p.g, q.g) && close(p.b, q.b) && close(p.a, q.a)
        }
        _ => match (as_num(a), as_num(b)) {
            (Some(x), Some(y)) => close(x, y),
            _ => false,
        },
    }
}

/// Every value [`same`], in order.
#[track_caller]
fn same_all(got: &[Value], want: &[Value]) {
    assert!(
        got.len() == want.len() && got.iter().zip(want).all(|(g, w)| same(g, w)),
        "got {got:?}, want {want:?}"
    );
}

/// A lifted `ty` and the formula `expr` (its variables in pin order) give
/// the same spread for `inputs`.
#[track_caller]
fn matches_expression(ty: &str, params: Json, expr: &str, inputs: Vec<Vec<Value>>) {
    let got = lifted(ty, params).eval(inputs.clone()).unwrap();
    let want = node("math/expression", json!({ "expr": expr }))
        .eval(inputs)
        .unwrap();
    assert_eq!(got[0].len(), want[0].len(), "{ty} vs {expr}: {got:?}");
    for (g, w) in got[0].iter().zip(&want[0]) {
        assert!(same(g, w), "{ty} vs {expr}: {g:?} vs {w:?}");
    }
}

#[test]
fn numbers_match_expression() {
    let a = nums(&[1.0, 2.5, -3.0, 4.0]);
    let b = nums(&[10.0, 0.5]);
    for (op, expr) in [
        ("add", "a + b"),
        ("subtract", "a - b"),
        ("multiply", "a * b"),
        ("divide", "a / b"),
        ("pow", "pow(a, b)"),
        ("min", "min(a, b)"),
        ("max", "max(a, b)"),
        ("atan2", "atan2(a, b)"),
    ] {
        matches_expression(
            "math/math",
            json!({ "op": op }),
            expr,
            vec![a.clone(), b.clone()],
        );
    }
    for (op, expr) in [
        ("sin", "sin(x)"),
        ("abs", "abs(x)"),
        ("floor", "floor(x)"),
        ("fract", "fract(x)"),
        ("square", "x * x"),
    ] {
        matches_expression("math/math", json!({ "op": op }), expr, vec![a.clone()]);
    }
    let x = nums(&[-0.5, 0.25, 0.5, 2.0, 0.9]);
    matches_expression(
        "math/clamp",
        json!({}),
        "clamp(x, lo, hi)",
        vec![x.clone(), nums(&[0.0]), nums(&[1.0, 0.6])],
    );
    matches_expression(
        "math/map",
        json!({}),
        "map(x, a, b, c, d)",
        vec![
            x.clone(),
            nums(&[0.0]),
            nums(&[1.0]),
            nums(&[10.0, -10.0]),
            nums(&[20.0]),
        ],
    );
    matches_expression(
        "math/smoothstep",
        json!({}),
        "smoothstep(e0, e1, x)",
        vec![nums(&[0.0]), nums(&[1.0]), x.clone()],
    );
    matches_expression(
        "math/lerp",
        json!({}),
        "mix(a, b, u)",
        vec![nums(&[0.0, 5.0]), nums(&[10.0]), x.clone()],
    );
    matches_expression(
        "logic/compare",
        json!({ "op": "less" }),
        "a < b",
        vec![x.clone(), nums(&[0.5])],
    );
    matches_expression(
        "logic/select",
        json!({}),
        "c ? a : b",
        vec![nums(&[1.0, 0.0, 1.0]), nums(&[7.0]), nums(&[-1.0, -2.0])],
    );
}

#[test]
fn vec2s_match_expression() {
    let p = vec![v2(1.0, 2.0), v2(-3.0, 0.5), v2(4.0, -4.0)];
    let q = vec![v2(10.0, 20.0), v2(0.0, 1.0)];
    matches_expression(
        "math/math",
        json!({ "op": "add", "valueType": "vec2" }),
        "p + q",
        vec![p.clone(), q.clone()],
    );
    matches_expression(
        "vec2/make",
        json!({}),
        "vec2(x, y)",
        vec![nums(&[1.0, 2.0, 3.0]), nums(&[5.0])],
    );
    matches_expression(
        "vec2/math",
        json!({ "op": "length" }),
        "length(v)",
        vec![p.clone()],
    );
    matches_expression(
        "vec2/math",
        json!({ "op": "distance" }),
        "distance(p, q)",
        vec![p.clone(), q.clone()],
    );
    matches_expression(
        "math/clamp",
        json!({ "valueType": "vec2" }),
        "clamp(p, lo, hi)",
        vec![p.clone(), vec![v2(-1.0, -1.0)], vec![v2(2.0, 3.0)]],
    );
    matches_expression(
        "math/map",
        json!({ "valueType": "vec2" }),
        "map(p, a, b, c, d)",
        vec![
            p.clone(),
            vec![v2(0.0, 0.0)],
            vec![v2(1.0, 2.0)],
            vec![v2(0.0, 10.0)],
            vec![v2(100.0, 20.0)],
        ],
    );
}

#[test]
fn colours_match_expression() {
    let c = vec![rgba(0.1, 0.2, 0.3, 1.0), rgba(0.5, 0.25, 0.0, 0.5)];
    let d = vec![rgba(0.2, 0.2, 0.2, 0.0)];
    matches_expression(
        "math/math",
        json!({ "op": "add", "valueType": "color" }),
        "c + d",
        vec![c.clone(), d],
    );
    // Colour from channels (0..255) per element.
    matches_expression(
        "color/color",
        json!({ "mode": "rgb" }),
        "rgb(r / 255, g / 255, b / 255, a / 255)",
        vec![
            nums(&[0.0, 51.0, 255.0]),
            nums(&[102.0]),
            nums(&[204.0, 0.0]),
            nums(&[255.0]),
        ],
    );
    // Split per element: one value per colour on every output.
    let out = lifted("color/split", json!({})).eval(vec![c]).unwrap();
    assert_eq!(out.len(), 4);
    assert!(out.iter().all(|o| o.len() == 2));
    assert!(close(num(&out[0][1]), 0.5 * 255.0));
    assert!(close(num(&out[3][1]), 0.5 * 255.0));
}

#[test]
fn cycling_single_values_and_empty_spreads() {
    let mut add = lifted("math/math", json!({ "op": "add" }));
    // One value everywhere: the plain path, one result.
    let out = add.eval(vec![nums(&[1.0]), nums(&[2.0])]).unwrap();
    same_all(&out[0], &nums(&[3.0]));
    // The longest spread wins; shorter ones cycle; single values repeat.
    let out = add
        .eval(vec![nums(&[1.0, 2.0, 3.0, 4.0, 5.0]), nums(&[10.0, 20.0])])
        .unwrap();
    same_all(&out[0], &nums(&[11.0, 22.0, 13.0, 24.0, 15.0]));
    // Beside a spread, an empty one empties the result, as in Expression.
    assert!(add.eval(vec![nums(&[1.0, 2.0]), vec![]]).unwrap()[0].is_empty());
    let mut expr = node("math/expression", json!({ "expr": "a + b" }));
    assert!(expr.eval(vec![nums(&[1.0, 2.0]), vec![]]).unwrap()[0].is_empty());
    // With no spread of two or more, nothing lifts: an empty pin reads as
    // "nothing", exactly as unlifted — so a Select falling back when a
    // device's output is empty still does.
    let plain = node("math/math", json!({ "op": "add" }))
        .eval(vec![vec![], nums(&[1.0])])
        .unwrap();
    let got = add.eval(vec![vec![], nums(&[1.0])]).unwrap();
    assert_eq!(format!("{got:?}"), format!("{plain:?}"));
    assert_eq!(got[0].len(), 1);
    let mut pick = lifted("logic/select", json!({}));
    let out = pick.eval(vec![vec![], nums(&[1.0]), nums(&[2.0])]).unwrap();
    same_all(&out[0], &nums(&[2.0]));
    let mut pick = node("logic/select", json!({}));
    let out = pick.eval(vec![vec![], nums(&[1.0]), nums(&[2.0])]).unwrap();
    same_all(&out[0], &nums(&[2.0]));
    // Several outputs: each one carries a value per element.
    let out = lifted("vec2/split", json!({}))
        .eval(vec![vec![v2(1.0, 2.0), v2(3.0, 4.0), v2(5.0, 6.0)]])
        .unwrap();
    same_all(&out[0], &nums(&[1.0, 3.0, 5.0]));
    same_all(&out[1], &nums(&[2.0, 4.0, 6.0]));
    // A whole-spread pin stays whole: Gradient samples one list per `t`.
    let stops = vec![rgba(0.0, 0.0, 0.0, 1.0), rgba(1.0, 1.0, 1.0, 1.0)];
    let out = lifted("color/gradient", json!({}))
        .eval(vec![stops, nums(&[0.0, 0.5, 1.0])])
        .unwrap();
    assert_eq!(out[0].len(), 3);
    assert!(same(&out[0][1], &rgba(0.5, 0.5, 0.5, 1.0)));
}

#[test]
fn unlifted_nodes_read_slot_0() {
    // Older files: exactly as before (slot 0; an empty spread is the
    // pin's "nothing", which Math reads as 0).
    let mut add = node("math/math", json!({ "op": "add" }));
    let out = add
        .eval(vec![nums(&[1.0, 2.0, 3.0]), nums(&[10.0])])
        .unwrap();
    same_all(&out[0], &nums(&[11.0]));
    let out = add.eval(vec![vec![], nums(&[1.0])]).unwrap();
    assert_eq!(out[0].len(), 1);
    let mut col = node("color/color", json!({ "mode": "rgb" }));
    let out = col
        .eval(vec![
            nums(&[0.0, 255.0]),
            nums(&[0.0]),
            nums(&[0.0]),
            nums(&[255.0]),
        ])
        .unwrap();
    assert_eq!(out[0].len(), 1);
    let mut smooth = node("motion/smooth", json!({}));
    let out = smooth.eval(vec![nums(&[1.0, 2.0]), nums(&[8.0])]).unwrap();
    same_all(&out[0], &nums(&[1.0]));
}

/// Smooth follows every element with its own state; a new element starts
/// at its target, a dropped one forgets its state.
#[test]
fn smooth_keeps_state_per_element() {
    let mut h: Harness = lifted("motion/smooth", json!({}));
    h.frame.dt = 0.1;
    // First frame: every element snaps to its target.
    let out = h.eval(vec![nums(&[0.0, 100.0]), nums(&[8.0])]).unwrap();
    same_all(&out[0], &nums(&[0.0, 100.0]));
    // Targets jump: each eases from where it was.
    let out = h.eval(vec![nums(&[10.0, 0.0]), nums(&[8.0])]).unwrap();
    let k = 1.0 - (-8.0f64 * 0.1).exp();
    assert!(close(num(&out[0][0]), 10.0 * k));
    assert!(close(num(&out[0][1]), 100.0 - 100.0 * k));
    // A third element appears: it starts at its target, the others go on.
    let out = h.eval(vec![nums(&[10.0, 0.0, 5.0]), nums(&[8.0])]).unwrap();
    assert_eq!(out[0].len(), 3);
    assert!(num(&out[0][0]) > 10.0 * k && num(&out[0][0]) < 10.0);
    assert_eq!(num(&out[0][2]), 5.0);
    // Back to one value: element 0 keeps easing as the plain node would.
    let before = num(&out[0][0]);
    let out = h.eval(vec![nums(&[10.0]), nums(&[8.0])]).unwrap();
    assert!(num(&out[0][0]) > before && num(&out[0][0]) < 10.0);
    // Vec2 targets work per element too.
    let mut h = lifted("motion/smooth", json!({ "valueType": "vec2" }));
    let out = h
        .eval(vec![vec![v2(1.0, 2.0), v2(3.0, 4.0)], nums(&[8.0])])
        .unwrap();
    same_all(&out[0], &[v2(1.0, 2.0), v2(3.0, 4.0)]);
}

/// Every type the registry says lifts builds a node that lifts, in every
/// op of the families.
#[test]
fn every_lifting_type_lifts() {
    let cat = Catalogue::builtin();
    let lifted_types = super::lifted_types();
    assert!(lifted_types.len() > 60, "{}", lifted_types.len());
    for ty in lifted_types {
        let spec = cat
            .get(ty)
            .unwrap_or_else(|| panic!("{ty} not in the catalogue"));
        let mut param_sets = vec![spec.default_params.clone()];
        if let Some(f) = super::family::family(ty) {
            for op in f.ops {
                let mut p = spec.default_params.clone();
                p.insert(f.param.into(), op.id.into());
                param_sets.push(p);
            }
        }
        for params in param_sets {
            let mut h = node(ty, Json::Object(params.clone()));
            assert!(h.lift_spreads(), "{ty} {params:?}");
        }
    }
}

/// End to end: a v4 patch keeps a spread wire into a lifting node's single
/// pin and runs it per element; the same file saved as v3 drops the wire
/// with the old warning, and a spread reaching it dynamically reads slot 0.
#[test]
fn patch_version_decides() {
    let file = |v: u32| {
        format!(
            r#"{{"kind":"floret.patch","version":{v},"nodes":[
            {{"id":"lin","defType":"spread/linear","position":{{"x":0,"y":0}},"params":{{}},
              "portValues":{{"from":0,"to":3,"count":4}}}},
            {{"id":"ex","defType":"math/expression","position":{{"x":0,"y":0}},"params":{{"expr":"x * 1"}}}},
            {{"id":"add","defType":"math/math","position":{{"x":0,"y":0}},"params":{{"op":"add"}},
              "portValues":{{"b":10}}}},
            {{"id":"dyn","defType":"math/math","position":{{"x":0,"y":0}},"params":{{"op":"multiply"}},
              "portValues":{{"b":2}}}}],
            "edges":[
            {{"id":"e1","source":"lin","target":"add","sourceHandle":"out-values","targetHandle":"in-a"}},
            {{"id":"e2","source":"lin","target":"ex","sourceHandle":"out-values","targetHandle":"in-x"}},
            {{"id":"e3","source":"ex","target":"dyn","sourceHandle":"out-value","targetHandle":"in-a"}}]}}"#
        )
    };
    let run = |v: u32| {
        let (mut engine, warnings) = crate::Engine::load_with_warnings(&file(v)).unwrap();
        let out = engine.tick(&crate::FrameCtx::default()).outputs;
        let get = |id: &str| {
            let i = engine.graph.idx(id).unwrap();
            out[i].as_ref().map(|o| o[0].to_vec()).unwrap_or_default()
        };
        (warnings, get("add"), get("dyn"))
    };
    let (warnings, add, dyn_) = run(crate::patch::CURRENT_PATCH_VERSION);
    assert!(warnings.is_empty(), "{warnings:?}");
    same_all(&add, &nums(&[10.0, 11.0, 12.0, 13.0]));
    same_all(&dyn_, &nums(&[0.0, 2.0, 4.0, 6.0]));
    let (warnings, add, dyn_) = run(3);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].message.contains("only slot 0"));
    same_all(&add, &nums(&[10.0]));
    same_all(&dyn_, &nums(&[0.0]));
}
