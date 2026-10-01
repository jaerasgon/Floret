//! `io/*` nodes (inline controls, metro, gates). Ported from `src/nodes/io/`
//! (control.ts, slider.ts, toggle.ts, xy-pad.ts, color-picker.ts,
//! bang-bus.ts).
//!
//! Per-node state that TS keeps in module-level maps keyed by node id
//! (metro / counter / edge / gate) lives in each node's closure here, so it
//! resets on recompile instead of surviving it.
//!
//! Send / receive / save-svg live in `net.rs`; file, image-url and svg
//! (media loading) are not ported here.

use super::convert::js_string;
use super::shared::first;
use super::value::{param_or, param_source};
use super::Registrar;
use crate::js;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::value::{empty, one, Color, Spread, Value};

pub fn register(r: &mut Registrar) {
    r.node("io/bang", bang);
    r.node("io/button", button);
    r.node("io/stepper", stepper);
    r.node("io/select", select);
    r.node("io/slider", slider);
    r.node("io/toggle", toggle);
    r.node("io/xy-pad", xy_pad);
    r.node("io/color-picker", color_picker);
    r.node("io/metro", metro);
    r.node("io/counter", counter);
    r.node("io/edge", edge);
    r.node("io/gate", gate);
}

// ─── Interactive widgets ─────────────────────────────────────────────────────

/// Live-param key the host sets to `true` to fire a bang (`fireBang`).
pub const BANG_PARAM: &str = "bang";

/// Bang — `true` for exactly one tick after a fire, then `false`.
///
/// TS keeps pending fires in a module-level map (`bang-bus.ts`). Here the
/// host fires by setting the live param `bang` to `true`; the node consumes
/// it (`consumeBang`) by writing it back to `false`. Only the live store is
/// read, so a saved `bang` param never fires. Waking the follow-up tick
/// that emits the trailing `false` is the driver's job, as in TS.
fn bang(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let pending = cx.params.get(cx.node_id, BANG_PARAM) == Some(&serde_json::Value::Bool(true));
        if pending {
            cx.params
                .set(cx.node_id, BANG_PARAM, serde_json::Value::Bool(false));
        }
        Ok(vec![one(Value::Bool(pending))])
    })
}

fn button(_: &NodeInit) -> Box<dyn NodeImpl> {
    param_source("value", Value::Bool(false))
}

fn stepper(_: &NodeInit) -> Box<dyn NodeImpl> {
    param_source("value", Value::Num(0.0))
}

fn select(_: &NodeInit) -> Box<dyn NodeImpl> {
    param_source("value", Value::str(""))
}

fn slider(_: &NodeInit) -> Box<dyn NodeImpl> {
    param_source("value", Value::Num(0.0))
}

fn toggle(_: &NodeInit) -> Box<dyn NodeImpl> {
    param_source("value", Value::Bool(false))
}

fn xy_pad(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        Ok(vec![
            one(param_or(cx, "x", Value::Num(0.5))),
            one(param_or(cx, "y", Value::Num(0.5))),
        ])
    })
}

/// `hexToRgb`: `/^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i`, else white.
fn hex_to_rgb(hex: &str) -> [u8; 3] {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    // `$` also matches before a final "\n" only with the `m` flag, so the
    // match is exact: six hex digits.
    if h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()) {
        let ch = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(255);
        [ch(0), ch(2), ch(4)]
    } else {
        [255, 255, 255]
    }
}

fn color_picker(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        // `RegExp.exec` stringifies its argument.
        let hex = js_string(&param_or(cx, "hex", Value::str("#ffffff")));
        let [r, g, b] = hex_to_rgb(&hex).map(f64::from);
        let color = Color {
            r: r / 255.0,
            g: g / 255.0,
            b: b / 255.0,
            a: 1.0,
        };
        Ok(vec![
            one(Value::Color(color)),
            one(Value::Num(r)),
            one(Value::Num(g)),
            one(Value::Num(b)),
        ])
    })
}

// ─── Timing / flow ───────────────────────────────────────────────────────────

/// `Number(v) || fallback`.
fn number_or(v: &Value, fallback: f64) -> f64 {
    let n = v.to_number();
    if n == 0.0 || n.is_nan() {
        fallback
    } else {
        n
    }
}

/// `(slot0 ?? true) !== false`.
fn not_false(inputs: &[Spread], port: usize) -> bool {
    !matches!(first(inputs, port), Value::Bool(false))
}

/// Metro — a one-tick bang every `interval` seconds while `run` is on.
///
/// TS reads `t` from `frameCtxStore` (the tick's unshifted frame), so this
/// uses `cx.tick_frame.t`. Like TS, `interval` comes from the param only:
/// a wire into the `interval` pin is ignored.
fn metro(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut last_fire: Option<f64> = None;
    raw(move |inputs, cx| {
        let t = cx.tick_frame.t;
        let interval = js::max(
            0.01,
            number_or(&param_or(cx, "interval", Value::Num(1.0)), 1.0),
        );
        let run = not_false(inputs, 1);
        let Some(last) = last_fire.as_mut() else {
            last_fire = Some(t);
            return Ok(vec![one(Value::Bool(false))]);
        };
        let fire = run && t - *last >= interval;
        if !run || fire {
            *last = t;
        }
        Ok(vec![one(Value::Bool(fire))])
    })
}

/// Counter — `+step` on each rising edge of `inc`, back to `start` on a
/// rising edge of `reset`. `step` / `start` come from the params only
/// (wires into those pins are ignored, as in TS), and `step` 0 reads as 1.
fn counter(_: &NodeInit) -> Box<dyn NodeImpl> {
    struct State {
        count: f64,
        prev_inc: bool,
        prev_reset: bool,
    }
    let mut state: Option<State> = None;
    raw(move |inputs, cx| {
        let inc = first(inputs, 0).truthy();
        let reset = first(inputs, 1).truthy();
        let step = number_or(&param_or(cx, "step", Value::Num(1.0)), 1.0);
        let start = number_or(&param_or(cx, "start", Value::Num(0.0)), 0.0);
        let st = state.get_or_insert(State {
            count: start,
            prev_inc: false,
            prev_reset: false,
        });
        if reset && !st.prev_reset {
            st.count = start;
        }
        if inc && !st.prev_inc {
            st.count += step;
        }
        st.prev_inc = inc;
        st.prev_reset = reset;
        Ok(vec![one(Value::Num(st.count))])
    })
}

/// Edge — bang when `in` crosses `threshold` (rising / falling) or changes.
/// `mode` / `threshold` come from the params only, as in TS.
fn edge(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut prev: Option<f64> = None;
    raw(move |inputs, cx| {
        let v = match first(inputs, 0) {
            x if x.is_nullish() => 0.0,
            x => number_or(&x, 0.0),
        };
        let mode = param_or(cx, "mode", Value::str("rising"));
        let th = param_or(cx, "threshold", Value::Num(0.5)).to_number();
        let Some(p) = prev.as_mut() else {
            prev = Some(v);
            return Ok(vec![one(Value::Bool(false))]);
        };
        let fire = match &mode {
            Value::Str(m) if &**m == "rising" => *p <= th && v > th,
            Value::Str(m) if &**m == "falling" => *p > th && v <= th,
            _ => v != *p,
        };
        *p = v;
        Ok(vec![one(Value::Bool(fire))])
    })
}

/// Gate — pass `value` while `open`; when closed, hold the last value that
/// passed (empty if none has).
fn gate(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut held: Option<Spread> = None;
    raw(move |inputs, _| {
        let value = inputs.first().cloned().unwrap_or_else(empty);
        if not_false(inputs, 1) {
            held = Some(value.clone());
            return Ok(vec![value]);
        }
        Ok(vec![held.clone().unwrap_or_else(empty)])
    })
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{expect, expect_spreads, n, node, s};
    use super::BANG_PARAM;
    use crate::value::Value;
    use serde_json::json;

    fn bools(v: &[Value]) -> String {
        let b: Vec<&str> = v
            .iter()
            .map(|x| match x {
                Value::Bool(true) => "T",
                Value::Bool(false) => "F",
                _ => "?",
            })
            .collect();
        b.concat()
    }

    // Expected sequences recorded from the TS nodes.
    #[test]
    fn metro_matches_ts() {
        let mut m = node("io/metro", json!({ "interval": 0.6 }));
        let mut out = Vec::new();
        for i in 0..14 {
            m.frame.t = f64::from(i) * 0.25;
            let run = match i {
                6 | 7 => vec![Value::Bool(false)],
                9 => vec![n(0.0)],
                10 => vec![],
                _ => vec![Value::Bool(true)],
            };
            out.push(m.eval(vec![vec![], run]).unwrap()[0][0].clone());
        }
        assert_eq!(bools(&out), "FFFTFFFFFFTFFT");

        // `interval: 0` reads as 1 (`Number(v) || 1`).
        let mut m = node("io/metro", json!({ "interval": 0 }));
        let mut out = Vec::new();
        for i in 0..6 {
            m.frame.t = f64::from(i) * 0.004;
            out.push(m.eval(vec![vec![], vec![Value::Bool(true)]]).unwrap()[0][0].clone());
        }
        assert_eq!(bools(&out), "FFFFFF");
    }

    #[test]
    fn metro_uses_unshifted_tick_time() {
        let mut m = node("io/metro", json!({ "interval": 1 }));
        m.frame.t = 0.0;
        m.eval(vec![vec![], vec![]]).unwrap();
        // A ForEach delay shifts `frame.t`; metro must ignore it.
        m.frame.t = -5.0;
        let mut tick = m.frame.clone();
        tick.t = 1.0;
        m.tick_frame = Some(tick);
        assert!(matches!(
            m.eval(vec![vec![], vec![]]).unwrap()[0][0],
            Value::Bool(true)
        ));
    }

    #[test]
    fn counter_matches_ts() {
        let mut c = node("io/counter", json!({ "step": 2.5, "start": 10 }));
        let incs = [0.0, 1.0, 1.0, 0.0, 1.0, f64::NAN, 0.0, 1.0, 1.0, 0.0, 1.0];
        let resets = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0];
        let mut out = Vec::new();
        for (i, (inc, reset)) in incs.iter().zip(resets).enumerate() {
            // The TS run used the string 'x' (truthy) at index 5.
            let inc = if i == 5 { s("x") } else { n(*inc) };
            out.extend(c.eval1(&[inc, n(reset), n(0.0), n(0.0)]));
        }
        expect(
            &out,
            r##"["#10","#12.5","#12.5","#12.5","#15","#15","#10","#12.5","#12.5","#12.5","#15"]"##,
        );

        // step 0 → 1, a non-numeric start → 0.
        let mut c = node("io/counter", json!({ "step": 0, "start": "abc" }));
        let mut out = Vec::new();
        for i in 0..4 {
            out.extend(c.eval1(&[Value::Bool(i % 2 == 1), Value::Bool(false), n(0.0), n(0.0)]));
        }
        expect(&out, r##"["#0","#1","#1","#2"]"##);
    }

    #[test]
    fn edge_matches_ts() {
        let ins: Vec<Vec<Value>> = vec![
            vec![n(0.0)],
            vec![n(0.7)],
            vec![n(0.7)],
            vec![n(0.2)],
            vec![n(0.5)],
            vec![n(0.51)],
            vec![],
            vec![s("0.9")],
            vec![n(f64::NAN)],
            vec![n(0.9)],
            vec![n(0.1)],
        ];
        for (mode, want) in [
            ("rising", "FTFFFTFTFTF"),
            ("falling", "FFFTFFTFTFT"),
            ("change", "FTFTTTTTTTT"),
            ("bogus", "FTFTTTTTTTT"),
        ] {
            let mut e = node("io/edge", json!({ "mode": mode, "threshold": 0.5 }));
            let out: Vec<Value> = ins
                .iter()
                .map(|v| e.eval(vec![v.clone(), vec![], vec![]]).unwrap()[0][0].clone())
                .collect();
            assert_eq!(bools(&out), want, "{mode}");
        }
    }

    #[test]
    fn gate_matches_ts() {
        let f = || vec![Value::Bool(false)];
        let t = || vec![Value::Bool(true)];
        let steps: Vec<(Vec<Value>, Vec<Value>)> = vec![
            (vec![n(1.0)], f()),
            (vec![n(2.0), n(3.0)], t()),
            (vec![n(4.0)], f()),
            (vec![n(5.0)], vec![]),
            (vec![n(6.0)], vec![n(0.0)]),
            (vec![n(7.0)], f()),
            (vec![], t()),
            (vec![n(8.0)], f()),
        ];
        let mut g = node("io/gate", json!({}));
        let out: Vec<Vec<Value>> = steps
            .into_iter()
            .map(|(v, o)| g.eval(vec![v, o]).unwrap().remove(0))
            .collect();
        expect_spreads(
            &out,
            r##"[[],["#2","#3"],["#2","#3"],["#5"],["#6"],["#6"],[],[]]"##,
        );
    }

    #[test]
    fn bang_fires_once_per_trigger() {
        let mut b = node("io/bang", json!({}));
        let mut out = Vec::new();
        for i in 0..5 {
            if i == 1 || i == 3 {
                b.params.set("n1", BANG_PARAM, json!(true));
            }
            out.push(b.eval(vec![]).unwrap()[0][0].clone());
        }
        assert_eq!(bools(&out), "FTFTF");
        // A saved (non-live) param never fires.
        let mut b = node("io/bang", json!({ "bang": true }));
        assert!(matches!(b.eval(vec![]).unwrap()[0][0], Value::Bool(false)));
    }

    #[test]
    fn color_picker_matches_ts() {
        let white = r##"[[{"r":"#1","g":"#1","b":"#1","a":"#1"}],["#255"],["#255"],["#255"]]"##;
        for (hex, want) in [
            (
                json!("#ff8000"),
                r##"[[{"r":"#1","g":"#0.5019607843137255","b":"#0","a":"#1"}],["#255"],["#128"],["#0"]]"##,
            ),
            (
                json!("00FF7f"),
                r##"[[{"r":"#0","g":"#1","b":"#0.4980392156862745","a":"#1"}],["#0"],["#255"],["#127"]]"##,
            ),
            (json!("#abc"), white),
            (
                json!(123456),
                r##"[[{"r":"#0.07058823529411765","g":"#0.20392156862745098","b":"#0.33725490196078434","a":"#1"}],["#18"],["#52"],["#86"]]"##,
            ),
            (json!(null), white),
            (json!("##ff0000"), white),
            (json!("#ff8000\n"), white),
        ] {
            let mut p = node("io/color-picker", json!({ "hex": hex }));
            expect_spreads(&p.eval(vec![]).unwrap(), want);
        }
    }

    #[test]
    fn param_controls() {
        let mut x = node("io/xy-pad", json!({ "x": 0.25 }));
        expect(&x.eval1(&[]), r##"["#0.25","#0.5"]"##);
        let mut t = node("io/toggle", json!({}));
        expect(&t.eval1(&[]), "[false]");
        t.params.set("n1", "value", json!(true));
        expect(&t.eval1(&[]), "[true]");
        let mut sel = node("io/select", json!({ "options": "a,b", "value": "b" }));
        expect(&sel.eval1(&[]), r#"["b"]"#);
        let mut st = node("io/stepper", json!({ "value": 3 }));
        expect(&st.eval1(&[]), r##"["#3"]"##);
        let mut sl = node("io/slider", json!({}));
        expect(&sl.eval1(&[]), r##"["#0"]"##);
        let mut b = node("io/button", json!({ "value": true }));
        expect(&b.eval1(&[]), "[true]");
    }
}
