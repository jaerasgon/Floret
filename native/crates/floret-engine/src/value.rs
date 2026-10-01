//! Values that flow on wires.
//!
//! The TypeScript engine is dynamically typed: a wire carries a JS array of
//! arbitrary values. `Value` mirrors the JS value lattice closely enough to
//! reproduce its behaviour (including the `undefined` / `null` distinction
//! that several nodes branch on), while giving the common shapes — vectors
//! and colours — their own unboxed variants.

use indexmap::IndexMap;
use std::fmt;
use std::sync::Arc;

/// Every wire carries a spread: a 1-D list. A "single" value is a spread of
/// length 1. Cheap to clone (reference counted).
pub type Spread = Arc<[Value]>;

/// An empty spread.
pub fn empty() -> Spread {
    Arc::from([])
}

/// A length-1 spread.
pub fn one(v: Value) -> Spread {
    Arc::from([v])
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

/// RGBA, channels in 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
}

#[derive(Clone)]
pub enum Value {
    /// JS `undefined` — "nothing here", e.g. an empty spread's slot 0.
    Undefined,
    /// JS `null` — an explicit "no value" (unwired layer / style / image).
    Null,
    Bool(bool),
    Num(f64),
    Str(Arc<str>),
    Vec2(Vec2),
    Color(Color),
    /// A JS array that is itself a value (mat3, a list inside a spread, …).
    List(Spread),
    /// A JS object with no dedicated variant. Key order is preserved.
    Object(Arc<IndexMap<String, Value>>),
    /// A drawable layer (`SceneNode`).
    Scene(Arc<crate::scene::SceneNode>),
    /// A style chain (a JS closure in TS).
    Style(crate::scene::Style),
    /// An opaque JS object: a physics world, force, collider, particle
    /// buffer or typed array (see `crate::particles`).
    Handle(crate::particles::Handle),
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(Arc::from(s))
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Arc::from(items))
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self, Value::Undefined)
    }

    pub fn is_nullish(&self) -> bool {
        matches!(self, Value::Undefined | Value::Null)
    }

    /// JS property read `v[key]` on the runtime's object shapes
    /// (`Undefined` when absent).
    pub fn get(&self, key: &str) -> Value {
        match (self, key) {
            (Value::Vec2(p), "x") => Value::Num(p.x),
            (Value::Vec2(p), "y") => Value::Num(p.y),
            (Value::Color(c), "r") => Value::Num(c.r),
            (Value::Color(c), "g") => Value::Num(c.g),
            (Value::Color(c), "b") => Value::Num(c.b),
            (Value::Color(c), "a") => Value::Num(c.a),
            (Value::Object(m), k) => m.get(k).cloned().unwrap_or(Value::Undefined),
            (Value::List(items), "length") => Value::Num(items.len() as f64),
            (Value::Str(s), "length") => Value::Num(s.encode_utf16().count() as f64),
            (Value::Handle(h), k) => h.to_value().map_or(Value::Undefined, |v| v.get(k)),
            _ => Value::Undefined,
        }
    }

    /// JS `v && typeof v === 'object'` (arrays included).
    pub fn is_object(&self) -> bool {
        match self {
            Value::Vec2(_)
            | Value::Color(_)
            | Value::List(_)
            | Value::Object(_)
            | Value::Scene(_) => true,
            Value::Handle(h) => !h.is_function(),
            _ => false,
        }
    }

    /// JS `typeof v === 'number'`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// JS truthiness (`Boolean(v)`).
    pub fn truthy(&self) -> bool {
        match self {
            Value::Undefined | Value::Null => false,
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0 && !n.is_nan(),
            Value::Str(s) => !s.is_empty(),
            _ => true,
        }
    }

    /// JS `Number(v)` (ToNumber), for the value shapes Floret uses.
    pub fn to_number(&self) -> f64 {
        match self {
            Value::Undefined => f64::NAN,
            Value::Null => 0.0,
            Value::Bool(b) => f64::from(u8::from(*b)),
            Value::Num(n) => *n,
            Value::Str(s) => js_string_to_number(s),
            Value::List(items) => match items.len() {
                0 => 0.0,
                1 => match &items[0] {
                    Value::Undefined | Value::Null => 0.0,
                    Value::Str(s) => js_string_to_number(s),
                    v @ Value::Num(_) => v.to_number(),
                    v @ Value::List(_) => v.to_number(),
                    Value::Bool(b) => js_string_to_number(if *b { "true" } else { "false" }),
                    _ => f64::NAN,
                },
                _ => f64::NAN,
            },
            _ => f64::NAN,
        }
    }

    /// The runtime's `asNumber`: numbers pass, booleans → 0/1, else
    /// `Number(v) || 0`.
    pub fn as_number(&self) -> f64 {
        match self {
            Value::Num(n) => *n,
            Value::Bool(b) => f64::from(u8::from(*b)),
            other => {
                let n = other.to_number();
                if n.is_nan() || n == 0.0 {
                    0.0
                } else {
                    n
                }
            }
        }
    }

    /// Identity-style equality, mirroring JS `===` on the runtime's values:
    /// primitives by value (NaN never equal), reference-counted values by
    /// pointer, small structs by value (they are fresh objects in JS, but
    /// value equality is the stricter-safe choice for memoisation).
    pub fn same(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Vec2(a), Value::Vec2(b)) => a == b,
            (Value::Color(a), Value::Color(b)) => a == b,
            (Value::List(a), Value::List(b)) => Arc::ptr_eq(a, b),
            (Value::Object(a), Value::Object(b)) => Arc::ptr_eq(a, b),
            (Value::Scene(a), Value::Scene(b)) => Arc::ptr_eq(a, b),
            (Value::Style(a), Value::Style(b)) => Arc::ptr_eq(&a.0, &b.0),
            (Value::Handle(a), Value::Handle(b)) => a.ptr_eq(b),
            _ => false,
        }
    }

    /// Convert a JSON value (from a patch file's params / portValues) into
    /// a runtime value. Objects shaped exactly `{x,y}` become `Vec2` and
    /// `{r,g,b,a}` become `Color`, matching how the TS nodes read them.
    pub fn from_json(j: &serde_json::Value) -> Value {
        use serde_json::Value as J;
        match j {
            J::Null => Value::Null,
            J::Bool(b) => Value::Bool(*b),
            J::Number(n) => Value::Num(n.as_f64().unwrap_or(f64::NAN)),
            J::String(s) => Value::str(s),
            J::Array(items) => Value::list(items.iter().map(Value::from_json).collect()),
            J::Object(map) => {
                if let Some(tag) = map.get("$num").and_then(J::as_str) {
                    return Value::Num(match tag {
                        "Infinity" => f64::INFINITY,
                        "-Infinity" => f64::NEG_INFINITY,
                        _ => f64::NAN,
                    });
                }
                if map.len() == 1 && map.contains_key("$u") {
                    return Value::Undefined;
                }
                // Decode fields first so tagged numbers ({"$num":"NaN"}) count.
                let num = |k: &str| map.get(k).map(Value::from_json).and_then(|v| v.as_f64());
                if map.len() == 2 {
                    if let (Some(x), Some(y)) = (num("x"), num("y")) {
                        return Value::Vec2(Vec2 { x, y });
                    }
                }
                if map.len() == 4 {
                    if let (Some(r), Some(g), Some(b), Some(a)) =
                        (num("r"), num("g"), num("b"), num("a"))
                    {
                        return Value::Color(Color { r, g, b, a });
                    }
                }
                Value::Object(Arc::new(
                    map.iter()
                        .map(|(k, v)| (k.clone(), Value::from_json(v)))
                        .collect(),
                ))
            }
        }
    }
}

/// JS `Number(string)`: trims whitespace; empty → 0; decimal, hex/octal/
/// binary literals and `Infinity`; anything else → NaN.
pub fn js_string_to_number(s: &str) -> f64 {
    // JS StrWhiteSpaceChar: Unicode White_Space minus U+0085 (NEL), plus BOM.
    let t = s.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}');
    if t.is_empty() {
        return 0.0;
    }
    let radix = |p: &str, r: u32| {
        u64::from_str_radix(p, r)
            .map(|v| v as f64)
            .unwrap_or(f64::NAN)
    };
    if let Some(p) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return radix(p, 16);
    }
    if let Some(p) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        return radix(p, 8);
    }
    if let Some(p) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return radix(p, 2);
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    // Rust's parser accepts "inf"/"nan"/"infinity" which JS does not.
    if t.chars()
        .any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E')
    {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Undefined => write!(f, "undefined"),
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Num(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Vec2(v) => write!(f, "{{x:{}, y:{}}}", v.x, v.y),
            Value::Color(c) => write!(f, "{{r:{}, g:{}, b:{}, a:{}}}", c.r, c.g, c.b, c.a),
            Value::List(items) => f.debug_list().entries(items.iter()).finish(),
            Value::Object(map) => f.debug_map().entries(map.iter()).finish(),
            Value::Scene(n) => write!(f, "<{}>", n.kind()),
            Value::Style(st) => write!(f, "<style ×{}>", st.0.len()),
            Value::Handle(h) => write!(f, "<{}>", h.type_name()),
        }
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Num(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<Vec2> for Value {
    fn from(v: Vec2) -> Self {
        Value::Vec2(v)
    }
}

impl From<Color> for Value {
    fn from(c: Color) -> Self {
        Value::Color(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_number_conversion() {
        assert_eq!(js_string_to_number(" 12.5 "), 12.5);
        assert_eq!(js_string_to_number(""), 0.0);
        assert_eq!(js_string_to_number("0x10"), 16.0);
        assert!(js_string_to_number("abc").is_nan());
        assert!(js_string_to_number("inf").is_nan());
        assert_eq!(js_string_to_number("-Infinity"), f64::NEG_INFINITY);
        assert_eq!(js_string_to_number("1e3"), 1000.0);
        assert!(js_string_to_number("\u{85}1").is_nan());
        assert_eq!(js_string_to_number("\u{feff}\u{a0}2\n"), 2.0);
    }

    #[test]
    fn as_number_matches_runtime_helper() {
        assert_eq!(Value::Bool(true).as_number(), 1.0);
        assert_eq!(Value::Undefined.as_number(), 0.0);
        assert_eq!(Value::str("7").as_number(), 7.0);
        assert_eq!(Value::str("x").as_number(), 0.0);
    }

    #[test]
    fn json_shapes() {
        let v = Value::from_json(&serde_json::json!({"x": 1, "y": 2}));
        assert!(matches!(v, Value::Vec2(Vec2 { x, y }) if x == 1.0 && y == 2.0));
        let c = Value::from_json(&serde_json::json!({"r": 1, "g": 0, "b": 0, "a": 1}));
        assert!(matches!(c, Value::Color(_)));
        let n = Value::from_json(&serde_json::json!({"x": {"$num": "NaN"}, "y": 1}));
        assert!(matches!(n, Value::Vec2(Vec2 { x, .. }) if x.is_nan()));
        let o = Value::from_json(&serde_json::json!({"x": 1}));
        assert!(matches!(o, Value::Object(_)));
    }
}
