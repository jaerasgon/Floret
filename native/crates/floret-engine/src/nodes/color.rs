//! `color/*` nodes. Ported from `src/nodes/color/color.ts`,
//! `src/nodes/color/palette.ts` and `src/nodes/_shared/color-conv.ts`.
//!
//! Channel conventions on node boundaries: r, g, b, a in 0..255, h in
//! 0..360, s / l in 0..100. Internally `Color` stores 0..1 floats.

use super::shared::{first, to_color, WHITE};
use super::Registrar;
use crate::js;
use crate::node::{raw, scalar, NodeImpl, NodeInit};
use crate::spec::{Params, PortSpec, PortType};
use crate::value::{one, Color, Value};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("color/color", color);
    r.dynamic("color/color", color_inputs);
    r.node("color/split", split);
    r.node("color/set-alpha", set_alpha);
    r.node("color/lerp", lerp);
    r.node("color/gradient", gradient);
    r.node("color/rgb", rgb);
    r.node("color/hsl", hsl);
    r.node("color/hex", hex);
    r.node("color/split-rgb", split_rgb);
    r.node("color/split-hsl", split_hsl);
    r.node("color/palette", palette);
    r.dynamic("color/palette", build_stop_ports);
}

// ─── color-conv.ts ───────────────────────────────────────────────────────────

/// `clamp01`: NaN passes through (both JS comparisons are false), as it
/// does with `f64::clamp`.
pub(crate) fn clamp01(n: f64) -> f64 {
    n.clamp(0.0, 1.0)
}

/// JS `String(v)` / ToString for the runtime's value shapes.
fn js_to_string(v: &Value) -> String {
    match v {
        Value::Undefined => "undefined".into(),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => js::num_to_string(*n),
        Value::Str(s) => s.to_string(),
        Value::Vec2(_) | Value::Color(_) | Value::Object(_) => "[object Object]".into(),
        Value::Scene(_) => "[object Object]".into(),
        // JS prints a function's source; there's no equivalent for a style chain.
        Value::Style(_) => "function".into(),
        Value::Handle(h) => h.js_string(),
        Value::Image(_) => "[object OffscreenCanvas]".into(),
        // Array.prototype.join: nullish elements become "".
        Value::List(items) => items
            .iter()
            .map(|x| match x {
                Value::Undefined | Value::Null => String::new(),
                other => js_to_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// `/^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i` → three bytes.
fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let body = s.strip_prefix('#').unwrap_or(s);
    let b = body.as_bytes();
    if b.len() != 6 || !b.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&body[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// `hexToColor01(hex)`. `RegExp.exec` stringifies its argument, so non-
/// string values are matched against their JS string form.
pub(crate) fn hex_to_color01(v: &Value) -> Color {
    let s = match v {
        Value::Str(s) => s.to_string(),
        other => js_to_string(other),
    };
    match parse_hex(&s) {
        Some([r, g, b]) => Color {
            r: f64::from(r) / 255.0,
            g: f64::from(g) / 255.0,
            b: f64::from(b) / 255.0,
            a: 1.0,
        },
        None => WHITE,
    }
}

/// `hslToColor01(h, s, l, a)`.
pub(crate) fn hsl_to_color01(h: f64, s: f64, l: f64, a: f64) -> Color {
    let s_n = s / 100.0;
    let l_n = l / 100.0;
    let c = (1.0 - (2.0 * l_n - 1.0).abs()) * s_n;
    let hh = (((h % 360.0) + 360.0) % 360.0) / 60.0;
    let x = c * (1.0 - ((hh % 2.0) - 1.0).abs());
    let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
    if hh < 1.0 {
        r = c;
        g = x;
    } else if hh < 2.0 {
        r = x;
        g = c;
    } else if hh < 3.0 {
        g = c;
        b = x;
    } else if hh < 4.0 {
        g = x;
        b = c;
    } else if hh < 5.0 {
        r = x;
        b = c;
    } else {
        r = c;
        b = x;
    }
    let m = l_n - c / 2.0;
    Color {
        r: r + m,
        g: g + m,
        b: b + m,
        a: clamp01(a),
    }
}

/// `color01ToHsl(col)` → (h 0..360, s 0..100, l 0..100, a 0..1).
pub(crate) fn color01_to_hsl(col: Color) -> [f64; 4] {
    let Color { r, g, b, a } = col;
    let mx = js::max_of([r, g, b]);
    let mn = js::min_of([r, g, b]);
    let l = (mx + mn) / 2.0;
    if mx == mn {
        return [0.0, 0.0, l * 100.0, a];
    }
    let d = mx - mn;
    let s = if l > 0.5 {
        d / (2.0 - mx - mn)
    } else {
        d / (mx + mn)
    };
    let h = if mx == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h * 60.0, s * 100.0, l * 100.0, a]
}

fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// `inputs[i]?.[0] ?? fallback`.
fn first_or(inputs: &[crate::value::Spread], i: usize, fallback: Value) -> Value {
    let v = first(inputs, i);
    if v.is_nullish() {
        fallback
    } else {
        v
    }
}

// ─── color/color ─────────────────────────────────────────────────────────────

fn num_port(name: &str, default: f64) -> PortSpec {
    let mut p = PortSpec::new(name, PortType::Number).default(Value::Num(default));
    p.param = Some(name.to_owned());
    p
}

/// `dynamicInputs` for `color/color`: pins follow `mode` (rgb / hsl / hex).
fn color_inputs(params: &Params) -> Vec<PortSpec> {
    let mode = match params.get("mode") {
        None | Some(serde_json::Value::Null) => "rgb",
        Some(serde_json::Value::String(s)) => s.as_str(),
        // Any other mode makes the TS switch return undefined (which the
        // editor can't resolve); expose no pins.
        Some(_) => "",
    };
    match mode {
        "rgb" => vec![
            num_port("r", 255.0),
            num_port("g", 255.0),
            num_port("b", 255.0),
            num_port("a", 255.0),
        ],
        "hsl" => vec![
            num_port("h", 0.0),
            num_port("s", 100.0),
            num_port("l", 50.0),
            num_port("a", 255.0),
        ],
        "hex" => {
            let mut p = PortSpec::new("hex", PortType::String).default(Value::str("#ffffff"));
            p.param = Some("hex".into());
            vec![p]
        }
        _ => Vec::new(),
    }
}

/// `params.param<string>(key, fallback)` compared against string literals.
fn param_str(cx: &crate::node::EvalCtx, key: &str, fallback: &str) -> Option<String> {
    match cx.param(key) {
        None => Some(fallback.to_owned()),
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(_) => None,
    }
}

fn color(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let mode = param_str(cx, "mode", "rgb");
        let n = |i: usize, d: f64| first_or(inputs, i, Value::Num(d)).to_number();
        let out = match mode.as_deref() {
            Some("rgb") => Color {
                r: clamp01(n(0, 255.0) / 255.0),
                g: clamp01(n(1, 255.0) / 255.0),
                b: clamp01(n(2, 255.0) / 255.0),
                a: clamp01(n(3, 255.0) / 255.0),
            },
            Some("hsl") => {
                let a = clamp01(n(3, 255.0) / 255.0);
                hsl_to_color01(n(0, 0.0), n(1, 100.0), n(2, 50.0), a)
            }
            Some("hex") => {
                let v = first_or(inputs, 0, Value::str("#ffffff"));
                hex_to_color01(&Value::Str(Arc::from(js_to_string(&v))))
            }
            _ => WHITE,
        };
        Ok(vec![one(Value::Color(out))])
    })
}

// ─── color/split ─────────────────────────────────────────────────────────────

fn split(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let c = to_color(&first(inputs, 0));
        let mode = param_str(cx, "mode", "rgb");
        let vals = if mode.as_deref() == Some("hsl") {
            let [h, s, l, a] = color01_to_hsl(c);
            [h, s, l, a * 255.0]
        } else {
            [c.r * 255.0, c.g * 255.0, c.b * 255.0, c.a * 255.0]
        };
        Ok(vals.iter().map(|v| one(Value::Num(*v))).collect())
    })
}

// ─── Ops ─────────────────────────────────────────────────────────────────────

fn set_alpha(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let col = to_color(&args[0]);
        Ok(vec![Value::Color(Color {
            a: clamp01(args[1].to_number() / 255.0),
            ..col
        })])
    })
}

fn lerp(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let pa = to_color(&args[0]);
        let pb = to_color(&args[1]);
        Ok(vec![Value::Color(lerp_color(pa, pb, args[2].to_number()))])
    })
}

fn gradient(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let stops: &[Value] = inputs.first().map(|s| &s[..]).unwrap_or(&[]);
        let t = first_or(inputs, 1, Value::Num(0.0)).to_number();
        let out = match stops.len() {
            0 => WHITE,
            1 => to_color(&stops[0]),
            len => {
                let tc = clamp01(t);
                let last = (len - 1) as f64;
                let scaled = tc * last;
                let lo = scaled.floor();
                let hi = js::min(last, lo + 1.0);
                let f = scaled - lo;
                // `stops[NaN]` is undefined → white.
                let at = |i: f64| {
                    if i.is_finite() && i >= 0.0 && (i as usize) < len {
                        to_color(&stops[i as usize])
                    } else {
                        WHITE
                    }
                };
                lerp_color(at(lo), at(hi), f)
            }
        };
        Ok(vec![one(Value::Color(out))])
    })
}

// ─── Legacy single-mode nodes ────────────────────────────────────────────────

fn rgb(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let ch = |i: usize| clamp01(args[i].to_number() / 255.0);
        Ok(vec![Value::Color(Color {
            r: ch(0),
            g: ch(1),
            b: ch(2),
            a: ch(3),
        })])
    })
}

fn hsl(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        Ok(vec![Value::Color(hsl_to_color01(
            args[0].to_number(),
            args[1].to_number(),
            args[2].to_number(),
            args[3].to_number() / 255.0,
        ))])
    })
}

fn hex(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        Ok(vec![Value::Color(hex_to_color01(&args[0]))])
    })
}

fn split_rgb(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let c = to_color(&args[0]);
        Ok([c.r, c.g, c.b, c.a]
            .iter()
            .map(|v| Value::Num(v * 255.0))
            .collect())
    })
}

fn split_hsl(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let [h, s, l, a] = color01_to_hsl(to_color(&args[0]));
        Ok(vec![
            Value::Num(h),
            Value::Num(s),
            Value::Num(l),
            Value::Num(a * 255.0),
        ])
    })
}

// ─── color/palette ───────────────────────────────────────────────────────────

const MIN_STOPS: f64 = 1.0;
const MAX_STOPS: f64 = 64.0;
const DEFAULT_STOP_COUNT: f64 = 4.0;
const DEFAULT_STOPS: [&str; 8] = [
    "#fed000", "#fc8405", "#ed361a", "#4464a1", "#cf3895", "#6d358a", "#06b4b0", "#4b8a5f",
];

/// `clampStops(n)`: `Number.isFinite` does not coerce, so anything but a
/// finite number gives the default count.
fn clamp_stops(n: &Value) -> usize {
    match n {
        Value::Num(n) if n.is_finite() => {
            js::max(MIN_STOPS, js::min(MAX_STOPS, n.floor())) as usize
        }
        _ => DEFAULT_STOP_COUNT as usize,
    }
}

/// `buildStopPorts`: one colour pin per stop (`c0`, `c1`, …).
fn build_stop_ports(params: &Params) -> Vec<PortSpec> {
    let raw = params
        .get("stopCount")
        .map(Value::from_json)
        .unwrap_or(Value::Undefined)
        .to_number();
    let count = if raw.is_nan() || raw == 0.0 {
        DEFAULT_STOP_COUNT
    } else {
        raw
    };
    (0..clamp_stops(&Value::Num(count)))
        .map(|i| {
            PortSpec::new(format!("c{i}"), PortType::Color)
                .label(format!("{}", i + 1))
                .default(Value::Null)
        })
        .collect()
}

fn palette(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let count = clamp_stops(
            &cx.param("stopCount")
                .map(Value::from_json)
                .unwrap_or(Value::Num(DEFAULT_STOP_COUNT)),
        );
        let stops: Vec<Value> = (0..count)
            .map(|i| {
                let wired = first(inputs, i);
                // `wired && typeof wired === 'object' && 'r' in wired`.
                let has_r = match &wired {
                    Value::Color(_) => true,
                    Value::Object(m) => m.contains_key("r"),
                    _ => false,
                };
                if has_r {
                    return Value::Color(to_color(&wired));
                }
                let hex = cx
                    .param(&format!("c{i}"))
                    .map(Value::from_json)
                    .unwrap_or_else(|| Value::str(DEFAULT_STOPS.get(i).unwrap_or(&"#ffffff")));
                Value::Color(hex_to_color01(&hex))
            })
            .collect();
        Ok(vec![Arc::from(stops)])
    })
}

#[cfg(test)]
mod tests {
    //! Expected values recorded from the TypeScript implementation.
    use super::*;
    use crate::node::{EvalCtx, FrameCtx, ParamStore};
    use crate::spec::Catalogue;
    use crate::value::{empty, Spread, Vec2};
    use serde_json::json;

    const NAN: f64 = f64::NAN;
    const W: [f64; 4] = [1.0; 4];

    fn run(ty: &str, params: serde_json::Value, inputs: Vec<Spread>) -> Vec<Spread> {
        let cat = Catalogue::builtin();
        let spec = cat.get(ty).unwrap();
        let params: Params = params.as_object().cloned().unwrap_or_default();
        let resolved = cat.resolved_inputs(spec, &params);
        let init = NodeInit {
            id: "n",
            spec,
            params: &params,
            inputs: &resolved,
        };
        let mut node = crate::nodes::factory(ty).unwrap()(&init);
        let frame = FrameCtx::default();
        let mut store = ParamStore::default();
        let mut cx = EvalCtx {
            node_id: "n",
            frame: &frame,
            tick_frame: &frame,
            params: &mut store,
            saved_params: &params,
        };
        node.eval(&inputs, &mut cx).unwrap()
    }

    /// Scalar-node call with single values (`None` = empty spread → undefined).
    fn call(ty: &str, args: Vec<Option<Value>>) -> Vec<Spread> {
        run(
            ty,
            json!({}),
            args.into_iter()
                .map(|a| a.map_or_else(empty, one))
                .collect(),
        )
    }

    fn c(r: f64, g: f64, b: f64, a: f64) -> Value {
        Value::Color(Color { r, g, b, a })
    }

    fn n(x: f64) -> Value {
        Value::Num(x)
    }

    fn s(x: &str) -> Value {
        Value::str(x)
    }

    fn obj(j: serde_json::Value) -> Value {
        Value::from_json(&j)
    }

    fn close(a: f64, b: f64) -> bool {
        (a.is_nan() && b.is_nan()) || a == b || (a - b).abs() <= 1e-12 * (1.0 + a.abs())
    }

    fn assert_col(v: &Value, want: [f64; 4]) {
        let Value::Color(col) = v else {
            panic!("not a colour: {v:?}")
        };
        let got = [col.r, col.g, col.b, col.a];
        assert!(
            got.iter().zip(&want).all(|(a, b)| close(*a, *b)),
            "{got:?} vs {want:?}"
        );
    }

    fn assert_nums(out: &[Spread], want: &[f64]) {
        let got: Vec<f64> = out.iter().map(|s| s[0].as_f64().unwrap()).collect();
        assert!(
            got.len() == want.len() && got.iter().zip(want).all(|(a, b)| close(*a, *b)),
            "{got:?} vs {want:?}"
        );
    }

    #[test]
    fn hex() {
        let cases: Vec<(Option<Value>, [f64; 4])> = vec![
            (Some(s("#ff8000")), [1.0, 0.5019607843137255, 0.0, 1.0]),
            (
                Some(s("0A0b0C")),
                [
                    0.0392156862745098,
                    0.043137254901960784,
                    0.047058823529411764,
                    1.0,
                ],
            ),
            (Some(s("#fff")), W),
            (Some(s("#ff8000 ")), W),
            (Some(s("##ff8000")), W),
            (
                Some(n(123456.0)),
                [
                    0.07058823529411765,
                    0.20392156862745098,
                    0.33725490196078434,
                    1.0,
                ],
            ),
            (None, W),
            (
                Some(Value::list(vec![s("102030")])),
                [
                    0.06274509803921569,
                    0.12549019607843137,
                    0.18823529411764706,
                    1.0,
                ],
            ),
            (Some(s("#gg0000")), W),
            (Some(Value::Null), W),
            (Some(obj(json!({"r": 1}))), W),
        ];
        for (input, want) in cases {
            assert_col(&call("color/hex", vec![input])[0][0], want);
        }
    }

    #[test]
    fn legacy_hsl_and_rgb() {
        let out = call(
            "color/hsl",
            vec![Some(n(-30.0)), Some(n(80.0)), Some(n(40.0)), Some(n(128.0))],
        );
        assert_col(
            &out[0][0],
            [
                0.7200000000000001,
                0.07999999999999996,
                0.4,
                0.5019607843137255,
            ],
        );
        let out = call(
            "color/hsl",
            vec![Some(n(NAN)), Some(n(50.0)), Some(n(50.0)), Some(n(255.0))],
        );
        assert_col(&out[0][0], [0.75, 0.25, NAN, 1.0]);
        let out = call(
            "color/hsl",
            vec![Some(n(720.5)), Some(s("50")), Some(Value::Bool(true)), None],
        );
        assert_col(
            &out[0][0],
            [
                0.015000000000000005,
                0.005083333333333329,
                0.004999999999999996,
                NAN,
            ],
        );
        let out = call(
            "color/rgb",
            vec![Some(n(300.0)), Some(s("51")), Some(n(NAN)), None],
        );
        assert_col(&out[0][0], [1.0, 0.2, NAN, NAN]);
        let out = call(
            "color/rgb",
            vec![
                Some(Value::Null),
                Some(Value::Bool(true)),
                Some(Value::list(vec![n(7.0)])),
                Some(obj(json!({"r": 1}))),
            ],
        );
        assert_col(
            &out[0][0],
            [0.0, 0.00392156862745098, 0.027450980392156862, NAN],
        );
    }

    #[test]
    fn unified_color() {
        let out = run(
            "color/color",
            json!({"mode": "rgb"}),
            vec![one(n(51.0)), one(Value::Null), one(n(-4.0)), empty()],
        );
        assert_col(&out[0][0], [0.2, 1.0, 0.0, 1.0]);
        let out = run(
            "color/color",
            json!({"mode": "hsl"}),
            vec![one(n(200.0)), one(Value::Null), one(n(25.0)), one(n(510.0))],
        );
        assert_col(&out[0][0], [0.0, 0.33333333333333326, 0.5, 1.0]);
        let out = run(
            "color/color",
            json!({"mode": "hex"}),
            vec![one(s("336699"))],
        );
        assert_col(&out[0][0], [0.2, 0.4, 0.6, 1.0]);
        let out = run("color/color", json!({"mode": "hex"}), vec![empty()]);
        assert_col(&out[0][0], W);
        let out = run("color/color", json!({"mode": "bogus"}), vec![one(n(1.0))]);
        assert_col(&out[0][0], W);
        let out = run("color/color", json!({"mode": null}), vec![one(n(1.0))]);
        assert_col(&out[0][0], W);

        let ports = color_inputs(json!({"mode": "hsl"}).as_object().unwrap());
        let got: Vec<_> = ports
            .iter()
            .map(|p| {
                (
                    p.name.as_str(),
                    p.ty,
                    p.default.as_ref().and_then(Value::as_f64),
                    p.param.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("h", PortType::Number, Some(0.0), Some("h")),
                ("s", PortType::Number, Some(100.0), Some("s")),
                ("l", PortType::Number, Some(50.0), Some("l")),
                ("a", PortType::Number, Some(255.0), Some("a")),
            ]
        );
        let rgb = color_inputs(&Params::new());
        let names: Vec<_> = rgb.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["r", "g", "b", "a"]);
        assert_eq!(rgb[3].default.as_ref().and_then(Value::as_f64), Some(255.0));
        let hex = color_inputs(json!({"mode": "hex"}).as_object().unwrap());
        assert_eq!(hex.len(), 1);
        assert_eq!(hex[0].ty, PortType::String);
        assert_eq!(hex[0].param.as_deref(), Some("hex"));
    }

    #[test]
    fn splits() {
        let col = c(0.2, 0.4, 0.6, 0.5);
        let hsl = |v: Value| call("color/split-hsl", vec![Some(v)]);
        assert_nums(&hsl(col.clone()), &[210.0, 49.99999999999999, 40.0, 127.5]);
        assert_nums(&hsl(c(0.9, 0.1, 0.3, 1.0)), &[345.0, 80.0, 50.0, 255.0]);
        assert_nums(
            &hsl(obj(json!({"r": 0.1, "g": 0.9, "b": 0.3}))),
            &[135.0, 80.0, 50.0, 255.0],
        );
        assert_nums(&hsl(c(0.5, 0.5, 0.5, 1.0)), &[0.0, 0.0, 50.0, 255.0]);
        assert_nums(&hsl(c(NAN, 0.5, 0.5, 1.0)), &[NAN, NAN, NAN, 255.0]);
        let rgb = |v: Value| call("color/split-rgb", vec![Some(v)]);
        assert_nums(&rgb(n(3.0)), &[255.0; 4]);
        assert_nums(&rgb(col.clone()), &[51.0, 102.0, 153.0, 127.5]);
        let split = |mode: serde_json::Value, v: Value| run("color/split", mode, vec![one(v)]);
        assert_nums(
            &split(json!({"mode": "hsl"}), col.clone()),
            &[210.0, 49.99999999999999, 40.0, 127.5],
        );
        assert_nums(
            &split(json!({"mode": "rgb"}), col),
            &[51.0, 102.0, 153.0, 127.5],
        );
        assert_nums(&split(json!({}), Value::Null), &[255.0; 4]);
    }

    #[test]
    fn set_alpha_and_lerp() {
        let col = c(0.2, 0.4, 0.6, 0.5);
        let out = call("color/set-alpha", vec![Some(Value::Null), Some(n(51.0))]);
        assert_col(&out[0][0], [1.0, 1.0, 1.0, 0.2]);
        let out = call("color/set-alpha", vec![Some(col.clone()), Some(s("300"))]);
        assert_col(&out[0][0], [0.2, 0.4, 0.6, 1.0]);
        let out = call("color/set-alpha", vec![Some(col.clone()), None]);
        assert_col(&out[0][0], [0.2, 0.4, 0.6, NAN]);
        let red = c(1.0, 0.0, 0.0, 1.0);
        let out = call(
            "color/lerp",
            vec![Some(col.clone()), Some(red.clone()), Some(s("0.5"))],
        );
        assert_col(&out[0][0], [0.6000000000000001, 0.2, 0.3, 0.75]);
        let out = call("color/lerp", vec![Some(col.clone()), Some(red), None]);
        assert_col(&out[0][0], [NAN; 4]);
        let out = call("color/lerp", vec![Some(col), Some(n(5.0)), Some(n(2.0))]);
        assert_col(&out[0][0], [1.8, 1.6, 1.4, 1.5]);
    }

    #[test]
    fn gradient_stops() {
        let red = c(1.0, 0.0, 0.0, 1.0);
        let blue = c(0.0, 0.0, 1.0, 0.0);
        let stops: Spread = Arc::from(vec![red.clone(), blue, red.clone()]);
        let grad = |stops: Spread, t: Value| run("color/gradient", json!({}), vec![stops, one(t)]);
        let cases = [
            (n(0.25), [0.5, 0.0, 0.5, 0.5]),
            (n(2.0), [1.0, 0.0, 0.0, 1.0]),
            (n(-1.0), [1.0, 0.0, 0.0, 1.0]),
            (n(NAN), [NAN; 4]),
            (n(0.999), [0.998, 0.0, 0.0020000000000000018, 0.998]),
            (Value::Null, [1.0, 0.0, 0.0, 1.0]),
            (s("0.5"), [0.0, 0.0, 1.0, 0.0]),
        ];
        for (t, want) in cases {
            assert_col(&grad(stops.clone(), t)[0][0], want);
        }
        assert_col(&grad(empty(), n(0.5))[0][0], W);
        assert_col(&grad(one(n(5.0)), n(0.5))[0][0], W);
        assert_col(
            &grad(Arc::from(vec![red, n(7.0)]), n(0.75))[0][0],
            [1.0, 0.75, 0.75, 1.0],
        );
    }

    #[test]
    fn palette_stops() {
        let mut ins = vec![one(Value::Null); 10];
        ins[1] = one(c(0.2, 0.4, 0.6, 0.5));
        ins[3] = one(Value::Vec2(Vec2 { x: 1.0, y: 2.0 }));
        ins[4] = one(obj(json!({"r": "x"})));
        let out = run(
            "color/palette",
            json!({"stopCount": 10, "c0": "#000000", "c2": null}),
            ins,
        );
        let want = [
            [0.0, 0.0, 0.0, 1.0],
            [0.2, 0.4, 0.6, 0.5],
            W,
            [
                0.26666666666666666,
                0.39215686274509803,
                0.6313725490196078,
                1.0,
            ],
            W,
            [
                0.42745098039215684,
                0.20784313725490197,
                0.5411764705882353,
                1.0,
            ],
            [
                0.023529411764705882,
                0.7058823529411765,
                0.6901960784313725,
                1.0,
            ],
            [
                0.29411764705882354,
                0.5411764705882353,
                0.37254901960784315,
                1.0,
            ],
            W,
            W,
        ];
        assert_eq!(out[0].len(), want.len());
        for (v, w) in out[0].iter().zip(want) {
            assert_col(v, w);
        }
        let first4 = [
            [0.996078431372549, 0.8156862745098039, 0.0, 1.0],
            [
                0.9882352941176471,
                0.5176470588235295,
                0.0196078431372549,
                1.0,
            ],
            [
                0.9294117647058824,
                0.21176470588235294,
                0.10196078431372549,
                1.0,
            ],
            [
                0.26666666666666666,
                0.39215686274509803,
                0.6313725490196078,
                1.0,
            ],
        ];
        // A string stopCount isn't a finite number at compute time → 4.
        let out = run(
            "color/palette",
            json!({"stopCount": "5"}),
            vec![one(Value::Null); 5],
        );
        assert_eq!(out[0].len(), 4);
        for (v, w) in out[0].iter().zip(first4) {
            assert_col(v, w);
        }
        let out = run(
            "color/palette",
            json!({"stopCount": 2.7}),
            vec![one(Value::Null); 2],
        );
        assert_eq!(out[0].len(), 2);
        let out = run("color/palette", json!({"stopCount": 0}), vec![]);
        assert_eq!(out[0].len(), 1);
        assert_col(&out[0][0], first4[0]);

        let count = |p: serde_json::Value| build_stop_ports(p.as_object().unwrap()).len();
        assert_eq!(count(json!({"stopCount": 10})), 10);
        assert_eq!(count(json!({"stopCount": "0"})), 4);
        assert_eq!(count(json!({"stopCount": 500})), 64);
        assert_eq!(count(json!({})), 4);
        assert_eq!(count(json!({"stopCount": -3})), 1);
        assert_eq!(count(json!({"stopCount": "7"})), 7);
        assert_eq!(count(json!({"stopCount": 2.9})), 2);
        let ports = build_stop_ports(json!({"stopCount": 10}).as_object().unwrap());
        assert_eq!(ports[9].name, "c9");
        assert_eq!(ports[9].label.as_deref(), Some("10"));
        assert_eq!(ports[9].ty, PortType::Color);
        assert!(matches!(ports[9].default, Some(Value::Null)));
    }
}
