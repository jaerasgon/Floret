//! `vec2/*` nodes. Ported from `src/nodes/vec2/vec2.ts`.
//!
//! Vec2 inputs go through `toVec2` (non-vectors read as the origin); number
//! inputs are cast with `x as number` in TS, i.e. `ToNumber` once they hit
//! arithmetic.

use super::shared::{to_vec2, ZERO_VEC2};
use super::Registrar;
use crate::node::{scalar, NodeImpl, NodeInit};
use crate::value::{Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("vec2/make", |i| one(i, make));
    r.node("vec2/split", |i| {
        scalar(i, |a| {
            let p = to_vec2(&a[0]);
            Ok(vec![Value::Num(p.x), Value::Num(p.y)])
        })
    });
    r.node("vec2/from-angle", |i| {
        one(i, |a| {
            let (ang, m) = (num(&a[0]), num(&a[1]));
            v(ang.cos() * m, ang.sin() * m)
        })
    });
    r.node("vec2/to-polar", |i| {
        scalar(i, |a| {
            let p = to_vec2(&a[0]);
            Ok(vec![Value::Num(p.y.atan2(p.x)), Value::Num(p.x.hypot(p.y))])
        })
    });

    r.node("vec2/add", |i| pair(i, |p, q| v(p.x + q.x, p.y + q.y)));
    r.node("vec2/sub", |i| pair(i, |p, q| v(p.x - q.x, p.y - q.y)));
    r.node("vec2/mul", |i| pair(i, |p, q| v(p.x * q.x, p.y * q.y)));
    r.node("vec2/scale", |i| {
        one(i, |a| {
            let (p, k) = (to_vec2(&a[0]), num(&a[1]));
            v(p.x * k, p.y * k)
        })
    });
    r.node("vec2/lerp", |i| {
        one(i, |a| {
            let (p, q, t) = (to_vec2(&a[0]), to_vec2(&a[1]), num(&a[2]));
            v(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t)
        })
    });

    r.node("vec2/mag", |i| {
        one(i, |a| {
            let p = to_vec2(&a[0]);
            Value::Num(p.x.hypot(p.y))
        })
    });
    r.node("vec2/dist", |i| {
        pair(i, |p, q| Value::Num((q.x - p.x).hypot(q.y - p.y)))
    });
    r.node("vec2/dot", |i| {
        pair(i, |p, q| Value::Num(p.x * q.x + p.y * q.y))
    });
    r.node("vec2/heading", |i| {
        one(i, |a| {
            let p = to_vec2(&a[0]);
            Value::Num(p.y.atan2(p.x))
        })
    });
    r.node("vec2/angle-between", |i| {
        pair(i, |p, q| Value::Num(q.y.atan2(q.x) - p.y.atan2(p.x)))
    });

    r.node("vec2/normalize", |i| {
        one(i, |a| {
            let p = to_vec2(&a[0]);
            let m = p.x.hypot(p.y);
            if m == 0.0 {
                return Value::Vec2(ZERO_VEC2);
            }
            v(p.x / m, p.y / m)
        })
    });
    r.node("vec2/set-mag", |i| {
        one(i, |a| {
            let (p, target) = (to_vec2(&a[0]), num(&a[1]));
            let m = p.x.hypot(p.y);
            if m == 0.0 {
                return Value::Vec2(ZERO_VEC2);
            }
            let k = target / m;
            v(p.x * k, p.y * k)
        })
    });
    r.node("vec2/limit", |i| {
        one(i, |a| {
            let (p, cap) = (to_vec2(&a[0]), num(&a[1]));
            let m = p.x.hypot(p.y);
            if m == 0.0 || m <= cap {
                return Value::Vec2(p);
            }
            let k = cap / m;
            v(p.x * k, p.y * k)
        })
    });
    r.node("vec2/rotate", |i| {
        one(i, |a| {
            let (p, ang) = (to_vec2(&a[0]), num(&a[1]));
            let (c, s) = (ang.cos(), ang.sin());
            v(p.x * c - p.y * s, p.x * s + p.y * c)
        })
    });
    r.node("vec2/reflect", |i| {
        pair(i, |i, n0| {
            let len = n0.x.hypot(n0.y);
            if len == 0.0 {
                return Value::Vec2(i);
            }
            let n = Vec2 {
                x: n0.x / len,
                y: n0.y / len,
            };
            let d = i.x * n.x + i.y * n.y;
            v(i.x - 2.0 * d * n.x, i.y - 2.0 * d * n.y)
        })
    });
    r.node("vec2/project", |i| {
        pair(i, |p, q| {
            let denom = q.x * q.x + q.y * q.y;
            if denom == 0.0 {
                return Value::Vec2(ZERO_VEC2);
            }
            let k = (p.x * q.x + p.y * q.y) / denom;
            v(q.x * k, q.y * k)
        })
    });
}

fn v(x: f64, y: f64) -> Value {
    Value::Vec2(Vec2 { x, y })
}

/// `x as number` once it reaches arithmetic: `ToNumber`.
fn num(v: &Value) -> f64 {
    v.to_number()
}

/// A scalar node with one output. The engine checks the arity, so `f` may
/// index its arguments directly.
fn one(init: &NodeInit, f: fn(&[Value]) -> Value) -> Box<dyn NodeImpl> {
    scalar(init, move |args| Ok(vec![f(args)]))
}

/// A node over two vec2 inputs (`toVec2(a)`, `toVec2(b)`).
fn pair(init: &NodeInit, f: fn(Vec2, Vec2) -> Value) -> Box<dyn NodeImpl> {
    scalar(init, move |args| {
        Ok(vec![f(to_vec2(&args[0]), to_vec2(&args[1]))])
    })
}

/// `{ x: x as number, y: y as number }` — no coercion, so a non-number
/// component makes a plain object rather than a vec2.
fn make(a: &[Value]) -> Value {
    match (&a[0], &a[1]) {
        (Value::Num(x), Value::Num(y)) => v(*x, *y),
        (x, y) => Value::Object(Arc::new(
            [("x".to_owned(), x.clone()), ("y".to_owned(), y.clone())]
                .into_iter()
                .collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use crate::nodes::math::tests::run_cases;
    use crate::spec::Catalogue;

    /// Cases recorded from `src/nodes/vec2/vec2.ts` (see `math/tests.rs`).
    #[test]
    fn matches_typescript_reference() {
        let n = run_cases(include_str!("ts_cases.json"));
        assert!(n > 1000, "only {n} cases");
    }

    #[test]
    fn every_vec2_type_is_registered() {
        let missing: Vec<_> = Catalogue::builtin()
            .iter()
            .filter(|s| s.ty.starts_with("vec2/") && crate::nodes::factory(&s.ty).is_none())
            .map(|s| s.ty.clone())
            .collect();
        assert!(missing.is_empty(), "unimplemented: {missing:?}");
    }
}
