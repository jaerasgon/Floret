//! `math/*` nodes. Ported from `src/nodes/math/` (`binops.ts`, `unary.ts`,
//! `map.ts`, `aggregate.ts`, `_adaptive.ts`).
//!
//! The TS computes cast inputs with `x as number`, which doesn't coerce, so
//! the arithmetic below goes through [`js_ops`] wherever JS would behave
//! differently from plain `f64` maths on a non-number input (`+` on a
//! string, `===`, raw pass-through returns).

pub mod adaptive;
pub mod aggregate;
pub mod js_ops;

#[cfg(test)]
pub(crate) mod tests;

use super::Registrar;
use crate::js;
use crate::node::{scalar, NodeImpl, NodeInit};
use crate::spec::{Params, PortSpec, PortType};
use crate::value::Value;
use adaptive::{from_components, identity_value, kind_for_value_type, kind_of, Kind};
use js_ops::{add, lt, num, strict_eq};

pub fn register(r: &mut Registrar) {
    // Adaptive (number / vec2 / color) ops.
    r.node("math/add", |i| adaptive::reducer(i, adaptive::reduce_add));
    r.node("math/sub", |i| adaptive::reducer(i, adaptive::reduce_sub));
    r.node("math/mul", |i| adaptive::reducer(i, adaptive::reduce_mul));
    r.node("math/div", |i| adaptive::reducer(i, adaptive::reduce_div));
    r.node("math/min", |i| adaptive::reducer(i, adaptive::reduce_min));
    r.node("math/max", |i| adaptive::reducer(i, adaptive::reduce_max));
    r.node("math/lerp", adaptive::lerp_node);
    r.node("math/neg", |i| adaptive::mapper(i, |v| -v));
    r.node("math/abs", |i| adaptive::mapper(i, f64::abs));
    r.dynamic("math/add", |p| adaptive::adaptive_slots(p, 0.0));
    r.dynamic("math/sub", |p| adaptive::adaptive_slots(p, 0.0));
    r.dynamic("math/mul", |p| adaptive::adaptive_slots(p, 1.0));
    r.dynamic("math/div", |p| adaptive::adaptive_slots(p, 1.0));
    r.dynamic("math/min", |p| adaptive::adaptive_slots(p, f64::INFINITY));
    r.dynamic("math/max", |p| {
        adaptive::adaptive_slots(p, f64::NEG_INFINITY)
    });
    r.dynamic("math/lerp", adaptive::adaptive_lerp_inputs);
    r.dynamic("math/neg", |p| adaptive::adaptive_unary_inputs(p, "x", 0.0));
    r.dynamic("math/abs", |p| adaptive::adaptive_unary_inputs(p, "x", 0.0));

    // Binary number ops.
    r.node("math/mod", |i| fixed(i, modulo));
    r.node("math/pow", |i| {
        fixed(i, |a| Value::Num(js_ops::pow(num(&a[0]), num(&a[1]))))
    });
    r.node("math/atan2", |i| {
        fixed(i, |a| Value::Num(num(&a[0]).atan2(num(&a[1]))))
    });

    // Unary number ops.
    r.node("math/sin", |i| unary(i, f64::sin));
    r.node("math/cos", |i| unary(i, f64::cos));
    r.node("math/tan", |i| unary(i, f64::tan));
    r.node("math/sign", |i| unary(i, js::sign));
    r.node("math/floor", |i| unary(i, f64::floor));
    r.node("math/round", |i| unary(i, js::round));
    r.node("math/ceil", |i| unary(i, f64::ceil));
    r.node("math/exp", |i| unary(i, f64::exp));
    r.node("math/atan", |i| unary(i, f64::atan));
    r.node("math/sqrt", |i| {
        unary(i, |n| if n < 0.0 { 0.0 } else { n.sqrt() })
    });
    r.node("math/sq", |i| unary(i, |n| n * n));
    r.node("math/fract", |i| unary(i, |n| n - n.floor()));
    r.node("math/log", |i| {
        unary(i, |n| if n <= 0.0 { 0.0 } else { n.ln() })
    });
    r.node("math/asin", |i| unary(i, |n| clamp_unit(n).asin()));
    r.node("math/acos", |i| unary(i, |n| clamp_unit(n).acos()));

    // Mapping / shaping. Map, Clamp and Wrap adapt to vec2 / colour like
    // Math (`valueType`), computing per component.
    r.node("math/map", |i| adapting(i, map, |a| per_component(a, map)));
    r.dynamic("math/map", |p| {
        adaptive_pins(
            p,
            &[
                ("x", 0.0),
                ("inMin", 0.0),
                ("inMax", 1.0),
                ("outMin", 0.0),
                ("outMax", 1.0),
            ],
        )
    });
    r.node("math/clamp", |i| {
        adapting(i, clamp, |a| per_component(a, clamp))
    });
    r.dynamic("math/clamp", |p| {
        adaptive_pins(p, &[("x", 0.0), ("min", 0.0), ("max", 1.0)])
    });
    r.node("math/norm", |i| fixed(i, norm));
    r.node("math/mag", |i| {
        fixed(i, |a| Value::Num(js::hypot(num(&a[0]), num(&a[1]))))
    });
    r.node("math/dist", |i| {
        fixed(i, |a| {
            Value::Num(js::hypot(num(&a[2]) - num(&a[0]), num(&a[3]) - num(&a[1])))
        })
    });
    r.node("math/wrap", |i| {
        adapting(i, wrap, |a| per_component(a, wrap))
    });
    r.dynamic("math/wrap", |p| {
        adaptive_pins(p, &[("x", 0.0), ("min", 0.0), ("max", 1.0)])
    });
    r.node("math/smoothstep", |i| match SmoothMode::of(i.params) {
        SmoothMode::Step => fixed(i, smoothstep),
        SmoothMode::Window => fixed(i, window),
    });
    r.dynamic("math/smoothstep", smoothstep_inputs);
    r.node("math/step", |i| {
        fixed(i, |a| Value::Num(if lt(&a[1], &a[0]) { 0.0 } else { 1.0 }))
    });

    // Spread aggregations.
    r.node("math/sum", aggregate::sum);
    r.node("math/mean", aggregate::mean);
    r.node("math/min-spread", aggregate::min_spread);
    r.node("math/max-spread", aggregate::max_spread);
    r.node("math/median", aggregate::median);
    r.node("math/stddev", aggregate::stddev);
    r.node("math/cumulative", aggregate::cumulative);
    r.node("math/delta", aggregate::delta);
    r.node("math/histogram", aggregate::histogram);

    // Per element over spreads (v4 patches on).
    r.lifted(&[
        "math/add",
        "math/sub",
        "math/mul",
        "math/div",
        "math/min",
        "math/max",
        "math/lerp",
        "math/neg",
        "math/abs",
        "math/mod",
        "math/pow",
        "math/atan2",
        "math/sin",
        "math/cos",
        "math/tan",
        "math/sign",
        "math/floor",
        "math/round",
        "math/ceil",
        "math/exp",
        "math/atan",
        "math/sqrt",
        "math/sq",
        "math/fract",
        "math/log",
        "math/asin",
        "math/acos",
        "math/map",
        "math/clamp",
        "math/norm",
        "math/mag",
        "math/dist",
        "math/wrap",
        "math/smoothstep",
        "math/step",
    ]);
}

/// A scalar node with a fixed input list and one output. The engine checks
/// the arity, so `f` may index its arguments directly.
fn fixed(init: &NodeInit, f: fn(&[Value]) -> Value) -> Box<dyn NodeImpl> {
    scalar(init, move |args| Ok(vec![f(args)]))
}

/// `Math.f(x as number)`-style unary node.
fn unary(init: &NodeInit, f: fn(f64) -> f64) -> Box<dyn NodeImpl> {
    scalar(init, move |args| {
        let x = args.first().map_or(f64::NAN, num);
        Ok(vec![Value::Num(f(x))])
    })
}

/// `numbers` while the node's `valueType` is unset / `any` / `number` (old
/// files: a vec2 reaching it computes NaN, as it always did), `components`
/// once the editor latched it to vec2 or colour.
fn adapting(
    init: &NodeInit,
    numbers: fn(&[Value]) -> Value,
    components: fn(&[Value]) -> Value,
) -> Box<dyn NodeImpl> {
    match kind_for_value_type(init.params) {
        Kind::Number => fixed(init, numbers),
        Kind::Vec2 | Kind::Color => fixed(init, components),
    }
}

/// Pins named `pins` (with their number defaults) that follow `valueType`
/// like Math's: number, vec2 or colour (`any` until a wire latches it).
fn adaptive_pins(params: &Params, pins: &[(&str, f64)]) -> Vec<PortSpec> {
    let kind = kind_for_value_type(params);
    let ty = adaptive::port_type_for_value_type(params);
    pins.iter()
        .map(|(name, d)| PortSpec::new(*name, ty).default(identity_value(kind, *d)))
        .collect()
}

/// `f` per component when a vec2 or colour arrives (numbers broadcast over
/// the components), like Math; `f` as is over numbers.
fn per_component(a: &[Value], f: fn(&[Value]) -> Value) -> Value {
    let Some(kind) = a.iter().filter_map(kind_of).find(|k| *k != Kind::Number) else {
        return f(a);
    };
    let n = match kind {
        Kind::Vec2 => 2,
        _ => 4,
    };
    let parts: Vec<Option<Vec<f64>>> = a
        .iter()
        .map(|v| match kind_of(v) {
            Some(Kind::Vec2 | Kind::Color) => Some(adaptive::components_of(v, kind)),
            _ => None,
        })
        .collect();
    let mut args: Vec<Value> = a.to_vec();
    let mut out = [0.0; 4];
    for (c, o) in out.iter_mut().enumerate().take(n) {
        for ((slot, v), part) in args.iter_mut().zip(a).zip(&parts) {
            *slot = Value::Num(match part {
                Some(p) => p[c],
                None => num(v),
            });
        }
        *o = num(&f(&args));
    }
    from_components(&out[..n], kind)
}

/// `max(min, min(max, x))`.
fn clamp(a: &[Value]) -> Value {
    Value::Num(js::max(num(&a[1]), js::min(num(&a[2]), num(&a[0]))))
}

/// Smoothstep's `mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmoothMode {
    /// 0 → 1 across `[edge0, edge1]` (the classic; old files).
    Step,
    /// Up across `[edge0, edge1]`, back down across `[edge2, edge3]`.
    Window,
}

impl SmoothMode {
    fn of(params: &Params) -> SmoothMode {
        match params.get("mode").and_then(serde_json::Value::as_str) {
            Some("window") => SmoothMode::Window,
            _ => SmoothMode::Step,
        }
    }
}

fn smoothstep_inputs(params: &Params) -> Vec<PortSpec> {
    let pin = |name: &str, d: f64| PortSpec::new(name, PortType::Number).default(Value::Num(d));
    match SmoothMode::of(params) {
        SmoothMode::Step => vec![pin("edge0", 0.0), pin("edge1", 1.0), pin("x", 0.0)],
        SmoothMode::Window => vec![
            pin("edge0", 0.0),
            pin("edge1", 0.25),
            pin("edge2", 0.75),
            pin("edge3", 1.0),
            pin("x", 0.0),
        ],
    }
}

/// A smooth bump: rises 0 → 1 across `[edge0, edge1]` and falls back to 0
/// across `[edge2, edge3]` (`smoothstep(e0, e1, x) · (1 − smoothstep(e2,
/// e3, x))`).
fn window(a: &[Value]) -> Value {
    let x = &a[4];
    let rise = num(&smoothstep(&[a[0].clone(), a[1].clone(), x.clone()]));
    let fall = num(&smoothstep(&[a[2].clone(), a[3].clone(), x.clone()]));
    Value::Num(rise * (1.0 - fall))
}

/// `n < -1 ? -1 : n > 1 ? 1 : n` (NaN passes through).
fn clamp_unit(n: f64) -> f64 {
    n.clamp(-1.0, 1.0)
}

/// Euclidean modulo: `bv === 0 ? 0 : ((av % bv) + bv) % bv`.
fn modulo(a: &[Value]) -> Value {
    let (av, bv) = (&a[0], &a[1]);
    if matches!(bv, Value::Num(n) if *n == 0.0) {
        return Value::Num(0.0);
    }
    let r = Value::Num(num(av) % num(bv));
    Value::Num(num(&add(&r, bv)) % num(bv))
}

/// `iMv === imv ? omv : omv + ((x - im) / (iM - im)) * (oM - om)`.
fn map(a: &[Value]) -> Value {
    let (x, im, i_m, om, o_m) = (&a[0], &a[1], &a[2], &a[3], &a[4]);
    if strict_eq(i_m, im) {
        return om.clone();
    }
    let k = ((num(x) - num(im)) / (num(i_m) - num(im))) * (num(o_m) - num(om));
    add(om, &Value::Num(k))
}

/// `start === stop ? 0 : (x - start) / (stop - start)`.
fn norm(a: &[Value]) -> Value {
    if strict_eq(&a[1], &a[2]) {
        return Value::Num(0.0);
    }
    let (x, s, e) = (num(&a[0]), num(&a[1]), num(&a[2]));
    Value::Num((x - s) / (e - s))
}

/// Wrap into `[min, max)`. Returns the raw `min` when the range is empty.
fn wrap(a: &[Value]) -> Value {
    let lo = &a[1];
    let range = num(&a[2]) - num(lo);
    if range <= 0.0 {
        return lo.clone();
    }
    let mut r = (num(&a[0]) - num(lo)) % range;
    if r < 0.0 {
        r += range;
    }
    add(lo, &Value::Num(r))
}

fn smoothstep(a: &[Value]) -> Value {
    let (e0, e1, x) = (&a[0], &a[1], &a[2]);
    if strict_eq(e0, e1) {
        return Value::Num(if lt(x, e0) { 0.0 } else { 1.0 });
    }
    let t = (num(x) - num(e0)) / (num(e1) - num(e0));
    let t = t.clamp(0.0, 1.0); // `t < 0 ? 0 : t > 1 ? 1 : t`
    Value::Num(t * t * (3.0 - 2.0 * t))
}
