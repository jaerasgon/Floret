//! `convert/*` nodes. Ported from `src/nodes/convert/convert.ts`.
//!
//! Also hosts the JS coercions the logic / string / io nodes share:
//! `String(v)`, `ToPrimitive`, and `JSON.stringify`.

use super::Registrar;
use crate::js;
use crate::node::{scalar, NodeImpl, NodeInit};
use crate::value::{js_string_to_number, Value};

pub fn register(r: &mut Registrar) {
    r.node("convert/to-string", to_string);
    r.node("convert/to-number", to_number);
    r.node("convert/to-int", to_int);
    r.node("convert/to-bool", to_bool);
    r.node("convert/to-degrees", to_degrees);
    r.node("convert/to-radians", to_radians);
    // Per element over spreads (v4 patches on).
    r.lifted(&[
        "convert/to-string",
        "convert/to-number",
        "convert/to-int",
        "convert/to-bool",
        "convert/to-degrees",
        "convert/to-radians",
    ]);
}

// ─── JS coercions ───────────────────────────────────────────────────────────

/// JS `String(v)`.
pub(crate) fn js_string(v: &Value) -> String {
    match v {
        Value::Undefined => "undefined".into(),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => js::num_to_string(*n),
        Value::Str(s) => s.to_string(),
        Value::List(items) => js_join(items, ","),
        Value::Vec2(_) | Value::Color(_) | Value::Object(_) => "[object Object]".into(),
        Value::Scene(_) => "[object Object]".into(),
        // JS prints a function's source; there's no equivalent for a style chain.
        Value::Style(_) => "function".into(),
        Value::Handle(h) => h.js_string(),
        Value::Image(_) => "[object OffscreenCanvas]".into(),
    }
}

/// `Array.prototype.join`: `undefined` / `null` elements become "".
pub(crate) fn js_join(items: &[Value], sep: &str) -> String {
    let mut out = String::new();
    for (i, v) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(sep);
        }
        if !v.is_nullish() {
            out.push_str(&js_string(v));
        }
    }
    out
}

/// `String(v ?? '')`.
pub(crate) fn js_string_or_empty(v: &Value) -> String {
    if v.is_nullish() {
        String::new()
    } else {
        js_string(v)
    }
}

/// A JS primitive, the result of `ToPrimitive(v, hint number)`.
pub(crate) enum Prim {
    Num(f64),
    Str(String),
}

/// `ToPrimitive` then the numeric/string split the relational operators
/// branch on. Arrays become their `join(",")`, plain objects
/// `"[object Object]"`; everything else is already numeric.
pub(crate) fn to_primitive(v: &Value) -> Prim {
    match v {
        Value::Str(s) => Prim::Str(s.to_string()),
        Value::List(_) | Value::Vec2(_) | Value::Color(_) | Value::Object(_) => {
            Prim::Str(js_string(v))
        }
        other => Prim::Num(other.to_number()),
    }
}

/// JS `a < b` (`IsLessThan` with left-first evaluation). `None` is the
/// spec's `undefined` result (a NaN operand), which every operator maps to
/// `false`.
pub(crate) fn js_less_than(a: &Value, b: &Value) -> Option<bool> {
    match (to_primitive(a), to_primitive(b)) {
        (Prim::Str(x), Prim::Str(y)) => Some(x.encode_utf16().lt(y.encode_utf16())),
        (pa, pb) => {
            let num = |p: Prim| match p {
                Prim::Num(n) => n,
                Prim::Str(s) => js_string_to_number(&s),
            };
            let (x, y) = (num(pa), num(pb));
            if x.is_nan() || y.is_nan() {
                None
            } else {
                Some(x < y)
            }
        }
    }
}

/// `JSON.stringify(v)` for the runtime's value shapes (`None` = the JS
/// `undefined` result, e.g. for a top-level `undefined`).
pub(crate) fn json_stringify(v: &Value) -> Option<String> {
    let mut s = String::new();
    write_json(v, &mut s).then_some(s)
}

/// Appends `v`; returns false (writing nothing) for values JSON omits.
fn write_json(v: &Value, out: &mut String) -> bool {
    let num = |n: f64, out: &mut String| {
        if n.is_finite() {
            out.push_str(&js::num_to_string(n));
        } else {
            out.push_str("null");
        }
    };
    let string = |s: &str, out: &mut String| {
        out.push_str(&serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into()));
    };
    let fields = |pairs: &[(&str, &Value)], out: &mut String| {
        out.push('{');
        let mut first = true;
        for (k, x) in pairs {
            let mark = out.len();
            if !first {
                out.push(',');
            }
            string(k, out);
            out.push(':');
            if write_json(x, out) {
                first = false;
            } else {
                out.truncate(mark);
            }
        }
        out.push('}');
    };
    match v {
        // JSON.stringify drops functions like undefined.
        Value::Undefined | Value::Style(_) => return false,
        Value::Scene(n) => return write_json(&n.to_value(), out),
        Value::Image(_) => out.push_str("{}"),
        Value::Handle(h) => {
            return match h.to_value() {
                Some(obj) => write_json(&obj, out),
                None => false,
            }
        }
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Num(n) => num(*n, out),
        Value::Str(s) => string(s, out),
        Value::Vec2(p) => {
            let (x, y) = (Value::Num(p.x), Value::Num(p.y));
            fields(&[("x", &x), ("y", &y)], out);
        }
        Value::Color(c) => {
            let ch = [c.r, c.g, c.b, c.a].map(Value::Num);
            fields(
                &[("r", &ch[0]), ("g", &ch[1]), ("b", &ch[2]), ("a", &ch[3])],
                out,
            );
        }
        Value::List(items) => {
            out.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if !write_json(x, out) {
                    out.push_str("null");
                }
            }
            out.push(']');
        }
        Value::Object(map) => {
            let pairs: Vec<(&str, &Value)> = map.iter().map(|(k, x)| (k.as_str(), x)).collect();
            fields(&pairs, out);
        }
    }
    true
}

/// Number of own enumerable keys (`Object.keys(v).length`).
fn key_count(v: &Value) -> usize {
    match v {
        Value::Vec2(_) => 2,
        Value::Color(_) => 4,
        Value::Object(m) => m.len(),
        _ => 0,
    }
}

// ─── Nodes ───────────────────────────────────────────────────────────────────

/// `valueToString` from convert.ts.
pub(crate) fn value_to_string(v: &Value) -> String {
    match v {
        Value::Undefined | Value::Null => String::new(),
        Value::Scene(n) => value_to_string(&n.to_value()),
        Value::Image(_) => value_to_string(&Value::Object(Default::default())),
        Value::Style(_) => "function".into(),
        Value::Handle(h) => match h.to_value() {
            Some(obj) => value_to_string(&obj),
            None => "function".into(),
        },
        Value::Str(s) => s.to_string(),
        Value::Num(n) => {
            let n = *n;
            if n.is_finite() && n.trunc() == n {
                js::num_to_string(n)
            } else {
                // `toFixed(3).replace(/\.?0+$/, '')`: toFixed always has a
                // '.' here, so this strips trailing zeros then a bare '.'.
                let s = js::to_fixed(n, 3);
                if s.contains('.') {
                    let t = s.trim_end_matches('0');
                    t.strip_suffix('.').unwrap_or(t).to_owned()
                } else {
                    s
                }
            }
        }
        Value::Bool(b) => b.to_string(),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(value_to_string).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Vec2(_) | Value::Color(_) | Value::Object(_) => {
            let (x, y) = (v.get("x"), v.get("y"));
            if let (Value::Num(_), Value::Num(_)) = (&x, &y) {
                if key_count(v) == 2 {
                    return format!("{}, {}", value_to_string(&x), value_to_string(&y));
                }
            }
            if let (Value::Num(r), Value::Num(g), Value::Num(b)) =
                (v.get("r"), v.get("g"), v.get("b"))
            {
                let hx = |n: f64| {
                    let c = js::max(0.0, js::min(255.0, js::round(n * 255.0)));
                    if c.is_nan() {
                        "NaN".to_owned()
                    } else {
                        format!("{:02x}", c as u32)
                    }
                };
                return format!("#{}{}{}", hx(r), hx(g), hx(b));
            }
            json_stringify(v).unwrap_or_default()
        }
    }
}

/// `valueToNumber` from convert.ts.
fn value_to_number(v: &Value) -> f64 {
    let finite_or_zero = |n: f64| if n.is_finite() { n } else { 0.0 };
    match v {
        Value::Num(n) => finite_or_zero(*n),
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Str(s) => finite_or_zero(js_string_to_number(s)),
        _ => 0.0,
    }
}

fn to_string(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| Ok(vec![Value::str(&value_to_string(&a[0]))]))
}

fn to_number(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| Ok(vec![Value::Num(value_to_number(&a[0]))]))
}

fn to_int(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        Ok(vec![Value::Num(js::round(value_to_number(&a[0])))])
    })
}

fn to_bool(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| Ok(vec![Value::Bool(a[0].truthy())]))
}

fn to_degrees(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| Ok(vec![Value::Num(a[0].to_number() * 360.0)]))
}

fn to_radians(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        Ok(vec![Value::Num(
            a[0].to_number() * (std::f64::consts::PI / 180.0),
        )])
    })
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{expect, list, n, node, obj, s};
    use crate::value::{Color, Value};

    /// The inputs the TS reference run used, in order.
    fn inputs() -> Vec<Value> {
        let u = Value::Undefined;
        vec![
            n(1.0),
            n(1.5),
            n(1.23456),
            n(-0.0001),
            n(100.0001),
            n(f64::NAN),
            n(f64::INFINITY),
            n(f64::NEG_INFINITY),
            n(1e21),
            n(1.5e300),
            n(0.1 + 0.2),
            n(2.0005),
            n(1.0005),
            n(-1.5),
            n(0.0005),
            n(0.00049),
            n(123456.789),
            Value::Bool(true),
            Value::Bool(false),
            Value::Null,
            u.clone(),
            s("s"),
            s("  12 "),
            s("abc"),
            s("0x1f"),
            s(""),
            s("1e3"),
            list(&[n(1.0), n(2.5), Value::Null, s("a"), u.clone()]),
            list(&[]),
            list(&[n(5.0)]),
            list(&[s("7")]),
            Value::Vec2(crate::value::Vec2 { x: 1.0, y: 2.25 }),
            Value::Color(Color {
                r: 1.0,
                g: 0.5,
                b: 0.0,
                a: 1.0,
            }),
            Value::Color(Color {
                r: f64::NAN,
                g: 2.0,
                b: -1.0,
                a: 1.0,
            }),
            obj(&[("r", n(0.1)), ("g", n(0.2)), ("b", n(0.3))]),
            obj(&[
                ("a", n(1.0)),
                ("b", s("x\n\"q\"")),
                ("c", u.clone()),
                (
                    "d",
                    list(&[n(1.0), u.clone(), n(f64::NAN), obj(&[("e", Value::Null)])]),
                ),
            ]),
            obj(&[("x", n(1.0)), ("y", n(2.0)), ("z", n(3.0))]),
            list(&[
                list(&[n(1.0), n(2.0)]),
                obj(&[("x", n(1.0)), ("y", n(2.0))]),
            ]),
            obj(&[("x", s("a")), ("y", n(2.0))]),
            n(-2.5),
            n(2.5),
            n(-0.4),
            s("3.7"),
            list(&[n(1.0), n(2.0)]),
        ]
    }

    const EXPECTED: [(&str, &str); 6] = [
        (
            "convert/to-string",
            r##"["1", "1.5", "1.235", "-0", "100", "NaN", "Infinity", "-Infinity", "1e+21", "1.5e+300", "0.3", "2.001", "1", "-1.5", "0.001", "0", "123456.789", "true", "false", "", "", "s", "  12 ", "abc", "0x1f", "", "1e3", "[1, 2.5, , a, ]", "[]", "[5]", "[7]", "1, 2.25", "#ff8000", "#NaNff00", "#1a334d", "{\"a\":1,\"b\":\"x\\n\\\"q\\\"\",\"d\":[1,null,null,{\"e\":null}]}", "{\"x\":1,\"y\":2,\"z\":3}", "[[1, 2], 1, 2]", "{\"x\":\"a\",\"y\":2}", "-2.5", "2.5", "-0.4", "3.7", "[1, 2]"]"##,
        ),
        (
            "convert/to-number",
            r##"["#1", "#1.5", "#1.23456", "#-0.0001", "#100.0001", "#0", "#0", "#0", "#1e+21", "#1.5e+300", "#0.30000000000000004", "#2.0005", "#1.0005", "#-1.5", "#0.0005", "#0.00049", "#123456.789", "#1", "#0", "#0", "#0", "#0", "#12", "#0", "#31", "#0", "#1000", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#-2.5", "#2.5", "#-0.4", "#3.7", "#0"]"##,
        ),
        (
            "convert/to-int",
            r##"["#1", "#2", "#1", "#0", "#100", "#0", "#0", "#0", "#1e+21", "#1.5e+300", "#0", "#2", "#1", "#-1", "#0", "#0", "#123457", "#1", "#0", "#0", "#0", "#0", "#12", "#0", "#31", "#0", "#1000", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#0", "#-2", "#3", "#0", "#4", "#0"]"##,
        ),
        (
            "convert/to-bool",
            r##"[true, true, true, true, true, false, true, true, true, true, true, true, true, true, true, true, true, true, false, false, false, true, true, true, true, false, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true]"##,
        ),
        (
            "convert/to-degrees",
            r##"["#360", "#540", "#444.44160000000005", "#-0.036000000000000004", "#36000.036", "$NaN", "$Infinity", "$-Infinity", "#3.6e+23", "#5.400000000000001e+302", "#108.00000000000001", "#720.1800000000001", "#360.18", "#-540", "#0.18", "#0.1764", "#44444444.04", "#360", "#0", "#0", "$NaN", "$NaN", "#4320", "$NaN", "#11160", "#0", "#360000", "$NaN", "#0", "#1800", "#2520", "$NaN", "$NaN", "$NaN", "$NaN", "$NaN", "$NaN", "$NaN", "$NaN", "#-900", "#900", "#-144", "#1332", "$NaN"]"##,
        ),
        (
            "convert/to-radians",
            r##"["#0.017453292519943295", "#0.026179938779914945", "#0.021547136813421196", "#-0.0000017453292519943296", "#1.7453309973235815", "$NaN", "$Infinity", "$-Infinity", "#17453292519943295000", "#2.6179938779914944e+298", "#0.005235987755982989", "#0.03491531168614657", "#0.017462019166203265", "#-0.026179938779914945", "#0.000008726646259971648", "#0.000008552113334772215", "#2154.727451989918", "#0.017453292519943295", "#0", "#0", "$NaN", "$NaN", "#0.20943951023931956", "$NaN", "#0.5410520681182421", "#0", "#17.453292519943297", "$NaN", "#0", "#0.08726646259971647", "#0.12217304763960307", "$NaN", "$NaN", "$NaN", "$NaN", "$NaN", "$NaN", "$NaN", "$NaN", "#-0.04363323129985824", "#0.04363323129985824", "#-0.006981317007977318", "#0.0645771823237902", "$NaN"]"##,
        ),
    ];
    #[test]
    fn matches_ts_reference() {
        for (ty, want) in EXPECTED {
            let want: Vec<serde_json::Value> = serde_json::from_str(want).unwrap();
            let mut node = node(ty, serde_json::json!({}));
            for (i, (v, w)) in inputs().iter().zip(&want).enumerate() {
                let got = node.eval1(std::slice::from_ref(v));
                if ty == "convert/to-string" {
                    // Plain strings (a leading '#' is not a number tag here).
                    let w = w.as_str().unwrap();
                    assert!(
                        matches!(&got[0], Value::Str(g) if &**g == w),
                        "{ty} #{i} {v:?}: got {:?}, want {w:?}",
                        got[0]
                    );
                } else {
                    expect(&got, &format!("[{w}]"));
                }
            }
        }
    }
}
