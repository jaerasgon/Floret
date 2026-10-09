//! `string/format-number` (Format Number): numbers as text for labels,
//! counters and data viz, element by element over a spread.
//!
//! Modes: `number` (`decimals`, a thousands `separator`, zero `pad`ding to
//! a number of integer digits, `sign` to always show + / −), `percent`
//! (0.25 → 25%), `compact` (1.2k, 3.4M, 5.6B, 7.8T) and `time` (seconds as
//! m:ss, or h:mm:ss from an hour, with `decimals` of a second). Every mode
//! adds `prefix` and `suffix` (units, currency). NaN reads "NaN" and
//! infinities "∞" / "-∞".

use super::convert::js_string_or_empty;
use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Spread, Value};

pub fn register(r: &mut Registrar) {
    r.node("string/format-number", format_number);
    r.dynamic("string/format-number", inputs);
}

pub const MODES: [&str; 4] = ["number", "percent", "compact", "time"];
/// Most decimals shown.
const MAX_DECIMALS: f64 = 12.0;

fn int(name: &str, default: f64) -> PortSpec {
    let mut p = PortSpec::new(name, PortType::Number).default(Value::Num(default));
    p.integer = true;
    p
}

fn text(name: &str, default: &str) -> PortSpec {
    PortSpec::new(name, PortType::String).default(Value::str(default))
}

fn flag(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Boolean).default(Value::Bool(false))
}

fn mode_of(params: &Params) -> &str {
    let m = params.get("mode").and_then(|v| v.as_str()).unwrap_or("");
    MODES.iter().find(|x| **x == m).copied().unwrap_or("number")
}

fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = vec![PortSpec::new("value", PortType::Number).kind(PortKind::Spread)];
    match mode_of(params) {
        "percent" => {
            v.push(int("decimals", 0.0));
            v.push(flag("sign"));
        }
        "compact" => v.push(int("decimals", 1.0)),
        "time" => v.push(int("decimals", 0.0)),
        _ => {
            v.push(int("decimals", 0.0));
            v.push(text("separator", ","));
            v.push(int("pad", 0.0));
            v.push(flag("sign"));
        }
    }
    v.push(text("prefix", ""));
    v.push(text("suffix", ""));
    v
}

/// How a number is written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Format {
    pub decimals: usize,
    pub separator: String,
    pub pad: usize,
    pub sign: bool,
}

fn special(x: f64) -> Option<String> {
    if x.is_nan() {
        Some("NaN".into())
    } else if x.is_infinite() {
        Some(if x > 0.0 { "∞" } else { "-∞" }.into())
    } else {
        None
    }
}

/// `x` with `f.decimals` decimals, integer digits grouped in threes by
/// `f.separator` and zero-padded to `f.pad`. No "-0".
pub fn number(x: f64, f: &Format) -> String {
    if let Some(s) = special(x) {
        return s;
    }
    // Halves round away from zero (2.25 → "2.3"), like `toFixed` mostly.
    let k = 10f64.powi(f.decimals as i32);
    let rounded = (x.abs() * k).round() / k;
    let fixed = format!(
        "{:.*}",
        f.decimals,
        if rounded.is_finite() {
            rounded
        } else {
            x.abs()
        }
    );
    let (int, frac) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let zero = fixed.bytes().all(|b| b == b'0' || b == b'.');
    let mut digits = int.to_owned();
    while digits.len() < f.pad {
        digits.insert(0, '0');
    }
    let mut grouped = String::with_capacity(digits.len() * 2);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push_str(&f.separator);
        }
        grouped.push(ch);
    }
    let sign = if x < 0.0 && !zero {
        "-"
    } else if f.sign && !zero {
        "+"
    } else {
        ""
    };
    if frac.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{frac}")
    }
}

/// 1 234 → "1.2k" (with 1 decimal); under a thousand as is.
pub fn compact(x: f64, decimals: usize) -> String {
    if let Some(s) = special(x) {
        return s;
    }
    const UNITS: [(f64, &str); 4] = [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "k")];
    let f = Format {
        decimals,
        ..Format::default()
    };
    let round = |v: f64| {
        let k = 10f64.powi(decimals as i32);
        (v * k).round() / k
    };
    for (i, (unit, name)) in UNITS.iter().enumerate() {
        if x.abs() >= *unit {
            let scaled = round(x.abs() / unit);
            // 999.96k rounds to 1000.0k: that's 1.0M.
            if scaled >= 1000.0 && i > 0 {
                let (up, up_name) = UNITS[i - 1];
                return number(x.signum() * round(x.abs() / up), &f) + up_name;
            }
            return number(x.signum() * scaled, &f) + name;
        }
    }
    let small = number(x, &f);
    // 999.96 with one decimal rounds up to a thousand.
    if round(x.abs()) >= 1000.0 {
        return number(x.signum(), &f) + "k";
    }
    small
}

/// Seconds as "m:ss" (or "h:mm:ss" from an hour), `decimals` of a second.
pub fn time(secs: f64, decimals: usize) -> String {
    if let Some(s) = special(secs) {
        return s;
    }
    let k = 10f64.powi(decimals as i32);
    // Round once, in the smallest unit shown, then split.
    let total = (secs.abs() * k).round();
    let whole = (total / k).floor();
    let frac = total - whole * k;
    let (h, m, s) = (
        (whole / 3600.0).floor(),
        ((whole % 3600.0) / 60.0).floor(),
        whole % 60.0,
    );
    let sign = if secs < 0.0 && total > 0.0 { "-" } else { "" };
    let mut out = if h > 0.0 {
        format!("{sign}{h}:{m:02}:{s:02}")
    } else {
        format!("{sign}{m}:{s:02}")
    };
    if decimals > 0 {
        out.push_str(&format!(".{:0width$}", frac as u64, width = decimals));
    }
    out
}

fn format_number(init: &NodeInit) -> Box<dyn NodeImpl> {
    let names: Vec<String> = init.inputs.iter().map(|p| p.name.clone()).collect();
    raw(move |inputs, cx| {
        let pin = |name: &str| -> Option<Value> {
            names
                .iter()
                .position(|n| n == name)
                .and_then(|i| inputs.get(i))
                .and_then(|s| s.first().cloned())
                .filter(|v| !v.is_nullish())
        };
        let n = |name: &str, fallback: f64| {
            pin(name)
                .map(|v| v.to_number())
                .filter(|v| v.is_finite())
                .unwrap_or(fallback)
        };
        let s = |name: &str, fallback: &str| {
            pin(name).map_or_else(|| fallback.to_owned(), |v| js_string_or_empty(&v))
        };
        let mode = match cx.param("mode") {
            Some(serde_json::Value::String(m)) if MODES.contains(&m.as_str()) => m.clone(),
            _ => "number".to_owned(),
        };
        let decimals = n("decimals", if mode == "compact" { 1.0 } else { 0.0 })
            .clamp(0.0, MAX_DECIMALS) as usize;
        let f = Format {
            decimals,
            separator: if mode == "number" {
                s("separator", ",")
            } else {
                String::new()
            },
            pad: n("pad", 0.0).clamp(0.0, 64.0) as usize,
            sign: pin("sign").is_some_and(|v| v.truthy()),
        };
        let (prefix, suffix) = (s("prefix", ""), s("suffix", ""));
        let values = names
            .iter()
            .position(|n| n == "value")
            .and_then(|i| inputs.get(i))
            .map_or(&[][..], |s| &s[..]);
        let out: Vec<Value> = values
            .iter()
            .map(|v| {
                let x = v.to_number();
                let body = match mode.as_str() {
                    "percent" => {
                        let p = number(x * 100.0, &f);
                        if x.is_finite() {
                            p + "%"
                        } else {
                            p
                        }
                    }
                    "compact" => compact(x, f.decimals),
                    "time" => time(x, f.decimals),
                    _ => number(x, &f),
                };
                Value::str(&format!("{prefix}{body}{suffix}"))
            })
            .collect();
        Ok(vec![Spread::from(out)])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::value::harness::node;
    use serde_json::json;

    fn fmt(decimals: usize, separator: &str, pad: usize, sign: bool) -> Format {
        Format {
            decimals,
            separator: separator.into(),
            pad,
            sign,
        }
    }

    #[test]
    fn numbers_group_pad_and_sign() {
        assert_eq!(number(1234567.891, &fmt(2, ",", 0, false)), "1,234,567.89");
        assert_eq!(number(1234567.0, &fmt(0, " ", 0, false)), "1 234 567");
        assert_eq!(number(999.0, &fmt(0, ",", 0, false)), "999");
        assert_eq!(number(-1234.5, &fmt(1, ",", 0, false)), "-1,234.5");
        assert_eq!(number(7.0, &fmt(0, "", 3, false)), "007");
        assert_eq!(number(42.0, &fmt(0, ",", 6, false)), "000,042");
        assert_eq!(number(3.0, &fmt(0, "", 0, true)), "+3");
        assert_eq!(number(0.0, &fmt(2, "", 0, true)), "0.00");
        // No negative zero after rounding.
        assert_eq!(number(-0.004, &fmt(2, "", 0, false)), "0.00");
        assert_eq!(number(-0.0, &fmt(0, "", 0, false)), "0");
        assert_eq!(number(f64::NAN, &fmt(2, ",", 0, false)), "NaN");
        assert_eq!(number(f64::NEG_INFINITY, &fmt(2, ",", 0, false)), "-∞");
        assert_eq!(number(0.5, &fmt(0, "", 0, false)), "1");
        assert_eq!(number(2.25, &fmt(1, "", 0, false)), "2.3");
        assert_eq!(number(2.675, &fmt(1, "", 0, false)), "2.7");
    }

    #[test]
    fn compact_units_and_carry() {
        assert_eq!(compact(950.0, 1), "950.0");
        assert_eq!(compact(1234.0, 1), "1.2k");
        assert_eq!(compact(-5_600_000.0, 1), "-5.6M");
        assert_eq!(compact(7.8e12, 1), "7.8T");
        assert_eq!(compact(999_960.0, 1), "1.0M");
        assert_eq!(compact(999.96, 1), "1.0k");
        assert_eq!(compact(2e15, 0), "2000T");
        assert_eq!(compact(0.0, 0), "0");
        assert_eq!(compact(f64::INFINITY, 1), "∞");
    }

    #[test]
    fn times_round_once() {
        assert_eq!(time(0.0, 0), "0:00");
        assert_eq!(time(75.5, 1), "1:15.5");
        assert_eq!(time(59.96, 1), "1:00.0");
        assert_eq!(time(59.6, 0), "1:00");
        assert_eq!(time(3725.0, 0), "1:02:05");
        assert_eq!(time(-90.0, 0), "-1:30");
        assert_eq!(time(-0.01, 0), "0:00");
        assert_eq!(time(1.234, 2), "0:01.23");
        assert_eq!(time(f64::NAN, 0), "NaN");
    }

    fn texts(out: &[Value]) -> Vec<String> {
        out.iter().map(js_string_or_empty).collect()
    }

    #[test]
    fn node_formats_every_element() {
        // value, decimals, separator, pad, sign, prefix, suffix
        let mut n = node("string/format-number", json!({ "mode": "number" }));
        let out = n
            .eval(vec![
                vec![Value::Num(1500.0), Value::Num(-2.25), Value::str("x")],
                vec![Value::Num(1.0)],
                vec![],
                vec![],
                vec![],
                vec![Value::str("$")],
                vec![Value::str(" USD")],
            ])
            .unwrap();
        assert_eq!(texts(&out[0]), ["$1,500.0 USD", "$-2.3 USD", "$NaN USD"]);
        // percent: value, decimals, sign, prefix, suffix
        let mut n = node("string/format-number", json!({ "mode": "percent" }));
        let out = n
            .eval(vec![
                vec![Value::Num(0.256)],
                vec![Value::Num(1.0)],
                vec![Value::Bool(true)],
                vec![],
                vec![],
            ])
            .unwrap();
        assert_eq!(texts(&out[0]), ["+25.6%"]);
        // Empty in, empty out.
        let mut n = node("string/format-number", json!({ "mode": "time" }));
        let out = n.eval(vec![vec![], vec![], vec![], vec![]]).unwrap();
        assert!(out[0].is_empty());
        // Pins follow the mode.
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("string/format-number").unwrap();
        let names = |mode: &str| -> Vec<String> {
            let p: Params = serde_json::from_value(json!({ "mode": mode })).unwrap();
            cat.resolved_inputs(spec, &p)
                .into_iter()
                .map(|p| p.name)
                .collect()
        };
        assert_eq!(
            names("number"),
            [
                "value",
                "decimals",
                "separator",
                "pad",
                "sign",
                "prefix",
                "suffix"
            ]
        );
        assert_eq!(names("compact"), ["value", "decimals", "prefix", "suffix"]);
        let statics: Vec<&str> = spec.inputs.iter().map(|p| &*p.name).collect();
        assert_eq!(statics, names("number"));
        assert_eq!(spec.param_options["mode"], MODES);
    }
}
