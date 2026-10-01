//! JS operator semantics on raw wire values.
//!
//! The TS math nodes cast their inputs with `x as number`, which does not
//! coerce: the operators then see whatever the wire carried. `-`, `*`, `/`,
//! `%` and `Math.*` apply `ToNumber`, but `+` concatenates when either side
//! is a string, `<` compares strings lexically, and `===` never coerces.
//! These helpers reproduce that for the runtime's value shapes.

use crate::js;
use crate::value::Value;

/// `ToNumber(v)` — what `Math.sin(v)`, `v - 1`, `v * k` … see.
pub fn num(v: &Value) -> f64 {
    v.to_number()
}

/// `ToPrimitive(v)`: plain objects become `"[object Object]"`, arrays their
/// `join(',')`; primitives pass through.
pub fn to_primitive(v: &Value) -> Value {
    match v {
        Value::Vec2(_) | Value::Color(_) | Value::Object(_) => Value::str("[object Object]"),
        Value::List(items) => Value::str(&join(items)),
        other => other.clone(),
    }
}

/// `Array.prototype.join(',')`: `undefined` / `null` elements are empty.
fn join(items: &[Value]) -> String {
    items
        .iter()
        .map(|v| match v {
            Value::Undefined | Value::Null => String::new(),
            other => to_js_string(other),
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// `String(v)`.
pub fn to_js_string(v: &Value) -> String {
    match v {
        Value::Undefined => "undefined".into(),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => js::num_to_string(*n),
        Value::Str(s) => s.to_string(),
        Value::List(items) => join(items),
        Value::Vec2(_) | Value::Color(_) | Value::Object(_) => "[object Object]".into(),
        Value::Scene(_) => "[object Object]".into(),
        // JS prints a function's source; there's no equivalent for a style chain.
        Value::Style(_) => "function".into(),
    }
}

/// `a + b`.
pub fn add(a: &Value, b: &Value) -> Value {
    let (pa, pb) = (to_primitive(a), to_primitive(b));
    if matches!(pa, Value::Str(_)) || matches!(pb, Value::Str(_)) {
        let mut s = to_js_string(&pa);
        s.push_str(&to_js_string(&pb));
        Value::str(&s)
    } else {
        Value::Num(pa.to_number() + pb.to_number())
    }
}

/// `a < b` (false whenever a NaN is involved).
pub fn lt(a: &Value, b: &Value) -> bool {
    let (pa, pb) = (to_primitive(a), to_primitive(b));
    if let (Value::Str(x), Value::Str(y)) = (&pa, &pb) {
        // JS compares strings by UTF-16 code units.
        return x.encode_utf16().lt(y.encode_utf16());
    }
    pa.to_number() < pb.to_number()
}

/// `a === b`. Objects are compared by value (the runtime can't observe JS
/// reference identity for its unboxed vec2 / colour values).
pub fn strict_eq(a: &Value, b: &Value) -> bool {
    a.same(b)
}

/// `Math.pow(x, y)`: like IEEE `pow` except a NaN exponent is always NaN
/// and `(±1) ** ±Infinity` is NaN.
pub fn pow(x: f64, y: f64) -> f64 {
    if y.is_nan() {
        return f64::NAN;
    }
    if y == 0.0 {
        return 1.0;
    }
    if x.is_nan() || (x.abs() == 1.0 && y.is_infinite()) {
        return f64::NAN;
    }
    x.powf(y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Vec2;

    #[test]
    fn plus() {
        assert!(matches!(add(&Value::Num(1.0), &Value::Num(2.0)), Value::Num(n) if n == 3.0));
        assert!(matches!(add(&Value::str("1"), &Value::Num(2.0)), Value::Str(s) if &*s == "12"));
        assert!(add(&Value::Undefined, &Value::Num(2.0))
            .as_f64()
            .unwrap()
            .is_nan());
        assert!(matches!(add(&Value::Null, &Value::Num(2.0)), Value::Num(n) if n == 2.0));
        assert!(matches!(add(&Value::Bool(true), &Value::Num(2.0)), Value::Num(n) if n == 3.0));
        assert!(matches!(
            add(&Value::Vec2(Vec2 { x: 1.0, y: 2.0 }), &Value::Num(0.5)),
            Value::Str(s) if &*s == "[object Object]0.5"
        ));
        let l = Value::list(vec![Value::Num(1.0), Value::Null, Value::Num(2.0)]);
        assert!(matches!(add(&l, &Value::Num(3.0)), Value::Str(s) if &*s == "1,,23"));
    }

    #[test]
    fn less_than() {
        assert!(lt(&Value::str("10"), &Value::str("9")));
        assert!(!lt(&Value::str("10"), &Value::Num(9.0)));
        assert!(!lt(&Value::Undefined, &Value::Num(0.0)));
        assert!(lt(&Value::Null, &Value::Num(1.0)));
    }

    #[test]
    fn js_pow() {
        assert!(pow(1.0, f64::NAN).is_nan());
        assert!(pow(-1.0, f64::INFINITY).is_nan());
        assert_eq!(pow(f64::NAN, 0.0), 1.0);
        assert_eq!(pow(2.0, 10.0), 1024.0);
        assert_eq!(pow(0.0, -1.0), f64::INFINITY);
    }
}
