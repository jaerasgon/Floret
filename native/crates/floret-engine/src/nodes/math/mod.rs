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
use crate::value::Value;
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

    // Mapping / shaping.
    r.node("math/map", |i| fixed(i, map));
    r.node("math/clamp", |i| {
        fixed(i, |a| {
            Value::Num(js::max(num(&a[1]), js::min(num(&a[2]), num(&a[0]))))
        })
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
    r.node("math/wrap", |i| fixed(i, wrap));
    r.node("math/smoothstep", |i| fixed(i, smoothstep));
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
