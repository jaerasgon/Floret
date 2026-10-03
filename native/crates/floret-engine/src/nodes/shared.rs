//! Helpers shared across node categories. Port of `src/nodes/_shared/coerce.ts`.

use crate::value::{Color, Value, Vec2};

pub const ZERO_VEC2: Vec2 = Vec2 { x: 0.0, y: 0.0 };
pub const WHITE: Color = Color::WHITE;

/// `toVec2`: `{x,y}` numbers, else (0, 0).
pub fn to_vec2(v: &Value) -> Vec2 {
    match v {
        Value::Vec2(p) => *p,
        Value::Object(_) => match (v.get("x"), v.get("y")) {
            (Value::Num(x), Value::Num(y)) => Vec2 { x, y },
            _ => ZERO_VEC2,
        },
        _ => ZERO_VEC2,
    }
}

/// `toColor`: `{r,g,b}` numbers (alpha defaults to 1), else white.
pub fn to_color(v: &Value) -> Color {
    match v {
        Value::Color(c) => *c,
        Value::Object(_) => match (v.get("r"), v.get("g"), v.get("b")) {
            (Value::Num(r), Value::Num(g), Value::Num(b)) => {
                let a = v.get("a").as_f64().unwrap_or(1.0);
                Color { r, g, b, a }
            }
            _ => WHITE,
        },
        _ => WHITE,
    }
}

/// `typeof v === 'number' ? v : fallback`.
pub fn num_or(v: &Value, fallback: f64) -> f64 {
    v.as_f64().unwrap_or(fallback)
}

/// Slot 0 of a spread (`inputs[i]?.[0]`), `Undefined` when absent.
pub fn first(inputs: &[crate::value::Spread], i: usize) -> Value {
    inputs
        .get(i)
        .and_then(|s| s.first())
        .cloned()
        .unwrap_or(Value::Undefined)
}

/// Element count for a per-element evaluation over several spreads: the
/// longest one (shorter ones cycle), or 0 when any of them is empty.
pub fn cyclic_len<'a>(spreads: impl IntoIterator<Item = &'a crate::value::Spread>) -> usize {
    let mut n = 0;
    for s in spreads {
        if s.is_empty() {
            return 0;
        }
        n = n.max(s.len());
    }
    n
}

/// Element `i` of a spread, cycling (`s[i % len]`); `Undefined` when empty.
pub fn cyclic(s: &crate::value::Spread, i: usize) -> Value {
    if s.is_empty() {
        Value::Undefined
    } else {
        s[i % s.len()].clone()
    }
}
