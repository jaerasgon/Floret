//! A family node set to an op must be indistinguishable from the
//! single-op node it supersedes: same pins, same output type, same results
//! for every input (NaN, ±0, ±∞, strings, null / undefined, vec2s,
//! colours, lists, empty and multi-element spreads).

use super::*;
use crate::nodes::value::harness::{list, n, node, obj, s};
use crate::spec::PortKind;
use crate::value::{Color, Vec2};
use serde_json::{json, Value as Json};

/// Exact rendering: `f64`'s `Display` round-trips, prints `-0` and `NaN`.
fn canon(v: &[Vec<Value>]) -> String {
    format!("{v:?}")
}

fn port_sig(p: &PortSpec) -> String {
    format!(
        "{} {:?} {:?} {:?} {:?} {} {}",
        p.name, p.ty, p.kind, p.default, p.param, p.integer, p.constant
    )
}

fn params_with(base: &Json, key: &str, op: &str) -> Json {
    let mut p = base.clone();
    p.as_object_mut()
        .unwrap()
        .insert(key.into(), Json::String(op.into()));
    p
}

fn as_params(j: &Json) -> Params {
    j.as_object().cloned().unwrap_or_default()
}

/// A wide spread of single values, including the awkward ones.
fn singles() -> Vec<Value> {
    vec![
        n(0.0),
        n(1.0),
        n(-2.5),
        n(3.0),
        n(0.5),
        n(-0.0),
        n(f64::NAN),
        n(f64::INFINITY),
        n(f64::NEG_INFINITY),
        n(1e-300),
        n(-7.0),
        s("4"),
        s("abc"),
        s(""),
        Value::Undefined,
        Value::Null,
        Value::Bool(true),
        Value::Bool(false),
        Value::Vec2(Vec2 { x: 3.0, y: -4.0 }),
        Value::Vec2(Vec2 { x: 0.0, y: 0.0 }),
        Value::Color(Color {
            r: 0.2,
            g: 0.4,
            b: 0.6,
            a: 0.8,
        }),
        list(&[n(2.0)]),
        obj(&[("x", n(1.0)), ("y", n(2.0))]),
    ]
}

/// Input spreads for one pin: each single value alone, plus empty and
/// multi-element spreads (scalar nodes read slot 0).
fn spreads() -> Vec<Vec<Value>> {
    let mut out: Vec<Vec<Value>> = singles().into_iter().map(|v| vec![v]).collect();
    out.push(vec![]);
    out.push(vec![n(2.0), n(5.0), n(-1.0)]);
    out.push(vec![n(f64::NAN), n(1.0)]);
    out.push(vec![s("3"), Value::Null, Value::Bool(true), n(-0.0)]);
    out.push(vec![n(f64::INFINITY), n(f64::NEG_INFINITY), n(4.0)]);
    out.push((0..37).map(|i| n(f64::from(i * 7 % 11) - 5.0)).collect());
    out
}

/// Argument tuples for `arity` pins: the full product up to two pins,
/// a deterministic sample beyond.
fn arg_sets(arity: usize) -> Vec<Vec<Vec<Value>>> {
    let pool = spreads();
    let m = pool.len();
    match arity {
        0 => vec![vec![]],
        1 => pool.iter().map(|a| vec![a.clone()]).collect(),
        2 => {
            let mut out = Vec::new();
            for a in &pool {
                for b in &pool {
                    out.push(vec![a.clone(), b.clone()]);
                }
            }
            out
        }
        k => (0..1500)
            .map(|i| {
                (0..k)
                    .map(|j| pool[(i * (7 + 6 * j) + j * j + i / m) % m].clone())
                    .collect()
            })
            .collect(),
    }
}

/// Param variants worth checking per family (the adaptive ops' latch and
/// slot count reshape their pins).
fn variants(f: &Family) -> Vec<Json> {
    match f.ty {
        "math/math" => {
            let mut v = Vec::new();
            for vt in ["any", "number", "vec2", "color"] {
                for slots in [1, 2, 3] {
                    v.push(json!({"slotCount": slots, "valueType": vt}));
                }
            }
            v.push(json!({}));
            v
        }
        _ => vec![json!({})],
    }
}

#[test]
fn every_op_matches_the_node_it_supersedes() {
    let cat = Catalogue::builtin();
    let mut checked = 0usize;
    for f in FAMILIES {
        for o in f.ops {
            let Some(legacy) = o.legacy() else { continue };
            for base in variants(f) {
                let fp = params_with(&base, f.param, o.id);
                // Same pins.
                let fam_spec = cat.get(f.ty).unwrap();
                let old_spec = cat.get(legacy).unwrap();
                let fam_in = cat.resolved_inputs(fam_spec, &as_params(&fp));
                let old_in = cat.resolved_inputs(old_spec, &as_params(&base));
                assert_eq!(
                    fam_in.iter().map(port_sig).collect::<Vec<_>>(),
                    old_in.iter().map(port_sig).collect::<Vec<_>>(),
                    "{} {} pins",
                    f.ty,
                    o.id
                );
                // Same output type, renamed `result`.
                let fam_out = cat.resolved_outputs(fam_spec, &as_params(&fp));
                let old_out = cat.resolved_outputs(old_spec, &as_params(&base));
                assert_eq!(fam_out.len(), 1);
                assert_eq!(fam_out[0].name, "result");
                assert_eq!(
                    (fam_out[0].ty, fam_out[0].kind),
                    (old_out[0].ty, old_out[0].kind),
                    "{} {} output",
                    f.ty,
                    o.id
                );
                // Same results.
                let mut fam = node(f.ty, fp.clone());
                let mut old = node(legacy, base.clone());
                for args in arg_sets(old_in.len()) {
                    let a = fam.eval(args.clone());
                    let b = old.eval(args.clone());
                    match (a, b) {
                        (Ok(a), Ok(b)) => assert_eq!(
                            canon(&a),
                            canon(&b),
                            "{} {} vs {legacy} on {args:?}",
                            f.ty,
                            o.id
                        ),
                        (Err(a), Err(b)) => assert_eq!(a, b),
                        (a, b) => panic!("{} {}: {a:?} vs {b:?}", f.ty, o.id),
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 50_000, "only {checked} cases");
}

#[test]
fn statistics_match_over_whole_spreads() {
    // The spread pin: every pool spread plus longer and degenerate ones.
    let mut pool = spreads();
    pool.push(
        (0..1000)
            .map(|i| n(((i * 7919) % 1009) as f64 / 7.0))
            .collect(),
    );
    pool.push(vec![n(5.0); 4]);
    pool.push(vec![Value::Undefined, Value::Null]);
    for o in STATISTICS.ops {
        let legacy = o.legacy().unwrap();
        let mut fam = node(STATISTICS.ty, json!({ "op": o.id }));
        let mut old = node(legacy, json!({}));
        for sp in &pool {
            assert_eq!(
                canon(&fam.eval(vec![sp.clone()]).unwrap()),
                canon(&old.eval(vec![sp.clone()]).unwrap()),
                "{} on {sp:?}",
                o.id
            );
        }
    }
}

#[test]
fn xor_is_exactly_one_truthy() {
    let mut x = node("logic/logic", json!({"op": "xor"}));
    let mut and = node("logic/and", json!({}));
    let mut or = node("logic/or", json!({}));
    for a in singles() {
        for b in singles() {
            let got = x.eval1(&[a.clone(), b.clone()]);
            let and_ = and.eval1(&[a.clone(), b.clone()]);
            let or_ = or.eval1(&[a.clone(), b.clone()]);
            let want = matches!(or_[0], Value::Bool(true)) && !matches!(and_[0], Value::Bool(true));
            assert!(
                matches!(got[0], Value::Bool(g) if g == want),
                "{a:?} xor {b:?}"
            );
        }
    }
    let p = as_params(&json!({"op": "xor"}));
    let cat = Catalogue::builtin();
    let pins = cat.resolved_inputs(cat.get("logic/logic").unwrap(), &p);
    assert_eq!(
        pins.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
}

#[test]
fn catalogue_entries_agree_with_the_tables() {
    let cat = Catalogue::builtin();
    for f in FAMILIES {
        let spec = cat.get(f.ty).unwrap_or_else(|| panic!("{} missing", f.ty));
        assert!(!spec.hidden && spec.has_dynamic_inputs, "{}", f.ty);
        assert!(cat.has_dynamic_impl(f.ty));
        assert_eq!(
            spec.param_options[f.param],
            f.ops.iter().map(|o| o.id.to_owned()).collect::<Vec<_>>(),
            "{} dropdown",
            f.ty
        );
        assert_eq!(spec.default_params[f.param], json!(f.ops[0].id));
        // nodes.json's static pins are the default op's.
        let names = |ps: &[PortSpec]| ps.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(&spec.inputs),
            names(&cat.resolved_inputs(spec, &spec.default_params)),
            "{} static pins",
            f.ty
        );
        assert_eq!(names(&spec.outputs), ["result"]);
        let mut ids: Vec<_> = f.ops.iter().map(|o| o.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), f.ops.len(), "{} duplicate op", f.ty);
        for o in f.ops {
            let old = cat.get(o.node).unwrap();
            if let Some(legacy) = o.legacy() {
                // Superseded: hidden from the browser, still loadable.
                assert!(old.hidden, "{legacy} should be hidden");
                assert!(crate::nodes::factory(legacy).is_some());
                assert_eq!(superseded_by(legacy).map(|(_, o2)| o2.id), Some(o.id));
            }
            assert_eq!(old.outputs.len(), 1, "{}", o.node);
            // Statistics read a whole spread; the rest are per-value.
            let spread_in = old.inputs.iter().any(|p| p.kind == PortKind::Spread);
            assert_eq!(spread_in, f.ty == "math/statistics", "{}", o.node);
        }
        assert!(f.adaptive.iter().all(|a| f.op_by_id(a).is_some()));
    }
    assert_eq!(family("math/math").map(|f| f.ops.len()), Some(28));
    assert!(family("math/lerp").is_none());
}

#[test]
fn only_arithmetic_ops_adapt() {
    let cat = Catalogue::builtin();
    let spec = cat.get("math/math").unwrap();
    let with = |op: &str| as_params(&json!({"op": op, "valueType": "vec2", "slotCount": 3}));
    assert!(cat.is_adaptive(spec, &with("multiply")));
    assert!(!cat.is_adaptive(spec, &with("sin")));
    let ty = |op: &str| cat.resolved_outputs(spec, &with(op))[0].ty;
    assert_eq!(ty("multiply"), crate::spec::PortType::Vec2);
    assert_eq!(ty("sin"), crate::spec::PortType::Number);
    assert_eq!(cat.resolved_inputs(spec, &with("multiply")).len(), 3);
    // Vector Math's output type follows the op.
    let v = cat.get("vec2/math").unwrap();
    let out = |op: &str| cat.resolved_outputs(v, &as_params(&json!({ "op": op })))[0].ty;
    assert_eq!(out("dot"), crate::spec::PortType::Number);
    assert_eq!(out("rotate"), crate::spec::PortType::Vec2);
    let c = cat.get("convert/convert").unwrap();
    let out = |to: &str| cat.resolved_outputs(c, &as_params(&json!({ "to": to })))[0].ty;
    assert_eq!(out("string"), crate::spec::PortType::String);
    assert_eq!(out("boolean"), crate::spec::PortType::Boolean);
}

#[test]
fn an_unknown_op_falls_back_to_the_default() {
    let mut m = node("math/math", json!({"op": "frobnicate"}));
    assert!(matches!(m.eval1(&[n(2.0), n(3.0)])[0], Value::Num(x) if x == 5.0));
    assert_eq!(MATH.op(&Params::new()).id, "add");
}

/// Whole patches: wired and unwired (`portValues`) pins compile the same.
#[test]
fn patches_compute_the_same_through_the_compiler() {
    use crate::node::FrameCtx;
    use crate::Engine;
    let run = |ty: &str, params: Json, pv: Json| -> String {
        let text = json!({
            "kind": "floret.patch", "version": 2,
            "nodes": [
                {"id": "t", "defType": "time/seconds", "position": {"x": 0, "y": 0}, "params": {}},
                {"id": "n", "defType": ty, "position": {"x": 0, "y": 0},
                 "params": params, "portValues": pv},
            ],
            "edges": [],
        });
        let mut e = Engine::load(&text.to_string()).unwrap();
        let r = e.tick(&FrameCtx::default());
        let out = r.outputs[e.graph.idx("n").unwrap()].clone().unwrap();
        format!("{:?}", out.iter().map(|s| s.to_vec()).collect::<Vec<_>>())
    };
    let pv = json!({"a": 7, "b": -3, "x": 0.25, "y": 2, "x1": 1, "y1": 1, "x2": 4, "y2": 5,
                    "v": {"x": 3, "y": 4}, "angle": 0.5, "mag": 2, "max": 1.5, "s": 2,
                    "normal": {"x": 0, "y": 1}, "in": "12.6", "deg": 90});
    for f in FAMILIES.iter().filter(|f| f.ty != "math/statistics") {
        for o in f.ops {
            let Some(legacy) = o.legacy() else { continue };
            let fam = run(f.ty, params_with(&json!({}), f.param, o.id), pv.clone());
            let old = run(legacy, json!({}), pv.clone());
            assert_eq!(fam, old, "{} {}", f.ty, o.id);
        }
    }
}
