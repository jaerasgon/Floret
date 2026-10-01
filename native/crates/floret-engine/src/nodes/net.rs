//! `io/send` / `io/receive` over the channel bus. Ported from
//! `src/nodes/io/net.ts`; the transport is [`crate::bus`].
//!
//! Timing: TS measures `interval` with `performance.now()`; here it is the
//! tick's `t` (seconds), so runs are deterministic. TS's network calls
//! resolve after the tick that made them, which this keeps: Send's `ok`
//! and Receive's value show up one tick after the send / poll.
//!
//! Also `io/save-svg`, which is dropped (no vector output): it stays as an
//! inert pass-through so old patches still draw.

use super::convert::{js_string, json_stringify};
use super::shared::first;
use super::value::param_or;
use super::Registrar;
use crate::bus::{self, BusPayload};
use crate::node::{raw, EvalCtx, NodeImpl, NodeInit};
use crate::value::{empty, one, Color, Spread, Value};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("io/send", send);
    r.node("io/receive", receive);
    r.node("io/save-svg", save_svg);
}

/// JS `String.prototype.trim`.
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}

/// `String(param(key, 'main') ?? 'main').trim() || 'main'`.
fn channel(cx: &EvalCtx) -> String {
    let v = param_or(cx, "channel", Value::str("main"));
    let s = if v.is_nullish() {
        "main".to_owned()
    } else {
        js_string(&v)
    };
    match js_trim(&s) {
        "" => "main".to_owned(),
        t => t.to_owned(),
    }
}

/// `Number(v) || fallback`.
fn number_or(v: &Value, fallback: f64) -> f64 {
    let n = v.to_number();
    if n == 0.0 || n.is_nan() {
        fallback
    } else {
        n
    }
}

/// Send — on a rising `send` edge (and every `interval` s), publish `data`:
/// layers as a PNG (`width × height` over `background`), anything else as
/// JSON. Outputs whether the last publish was accepted.
fn send(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut prev_send = false;
    let mut last_sent: Option<f64> = None;
    let mut ok = false;
    raw(move |inputs, cx| {
        let channel = channel(cx);
        let interval = number_or(&param_or(cx, "interval", Value::Num(0.0)), 0.0).max(0.0);
        let dim = |key: &str| {
            number_or(&param_or(cx, key, Value::Num(540.0)), 540.0)
                .floor()
                .max(1.0)
                .min(f64::from(u32::MAX)) as u32
        };
        let (w, h) = (dim("width"), dim("height"));
        let send = first(inputs, 2).truthy();
        let background = match first(inputs, 6) {
            Value::Color(c) => c,
            _ => Color::WHITE,
        };
        let data = inputs.first().cloned().unwrap_or_else(empty);

        let now = cx.tick_frame.t;
        let rising = send && !prev_send;
        prev_send = send;
        let due = interval > 0.0 && last_sent.is_none_or(|t| now - t >= interval);
        let out = vec![one(Value::Bool(ok))];
        let Some(bus) = bus::installed().filter(|_| rising || due) else {
            return Ok(out);
        };
        last_sent = Some(now);

        let mut layers = Vec::new();
        for item in data.iter() {
            match item {
                Value::List(items) => layers.extend(items.iter().filter_map(scene)),
                v => layers.extend(scene(v)),
            }
        }
        let payload = if layers.is_empty() {
            let json = json_stringify(&Value::List(data)).unwrap_or_else(|| "[]".into());
            BusPayload::Json(json)
        } else {
            BusPayload::Layers {
                layers,
                width: w,
                height: h,
                background,
            }
        };
        ok = bus.send(&channel, payload);
        Ok(out)
    })
}

fn scene(v: &Value) -> Option<Arc<crate::scene::SceneNode>> {
    match v {
        Value::Scene(n) => Some(n.clone()),
        _ => None,
    }
}

/// Receive — poll the channel every `interval` s; output the latest value
/// (JSON → its value / spread, text → a string).
fn receive(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut value: Spread = empty();
    let mut version: Option<u64> = None;
    let mut last_poll: Option<f64> = None;
    raw(move |_, cx| {
        let out = vec![value.clone()];
        let Some(bus) = bus::installed() else {
            return Ok(out);
        };
        let channel = channel(cx);
        let interval = number_or(&param_or(cx, "interval", Value::Num(0.5)), 0.5).max(0.05);
        let now = cx.tick_frame.t;
        if last_poll.is_some_and(|t| now - t < interval) {
            return Ok(out);
        }
        last_poll = Some(now);
        let Some(msg) = bus.latest(&channel) else {
            return Ok(out);
        };
        if version == Some(msg.version) {
            return Ok(out);
        }
        version = Some(msg.version);
        if msg.content_type.contains("json") {
            // A body that isn't JSON is skipped (TS: `r.json()` rejects).
            if let Ok(j) = serde_json::from_slice::<serde_json::Value>(&msg.body) {
                value = match Value::from_json(&j) {
                    Value::List(items) => items,
                    v => one(v),
                };
            }
        } else if msg.content_type.contains("image") {
            // TS decodes PNGs to an image; no image values on the native
            // wire yet, so the value is left as it was.
        } else {
            value = one(Value::str(&String::from_utf8_lossy(&msg.body)));
        }
        Ok(out)
    })
}

/// Inert: passes `layers` through and never exports.
fn save_svg(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| Ok(vec![inputs.first().cloned().unwrap_or_else(empty)]))
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{n, node, Harness};
    use crate::bus::{Bus, MemoryBus};
    use crate::scene::{default_style, SceneNode};
    use crate::value::Value;
    use serde_json::json;
    use std::sync::{Arc, OnceLock};

    /// One bus for the whole test binary; tests use their own channels.
    fn bus() -> Arc<MemoryBus> {
        static BUS: OnceLock<Arc<MemoryBus>> = OnceLock::new();
        BUS.get_or_init(|| {
            let b = Arc::new(MemoryBus::new());
            crate::bus::install(Some(b.clone()));
            b
        })
        .clone()
    }

    fn tick(h: &mut Harness, t: f64, inputs: Vec<Vec<Value>>) -> Vec<Value> {
        h.frame.t = t;
        h.eval(inputs).unwrap().remove(0)
    }

    fn send_inputs(data: Vec<Value>, send: bool) -> Vec<Vec<Value>> {
        vec![
            data,
            vec![],
            vec![Value::Bool(send)],
            vec![],
            vec![],
            vec![],
            vec![],
        ]
    }

    #[test]
    fn send_publishes_json_on_rising_edges_then_reports_ok() {
        let bus = bus();
        let mut s = node("io/send", json!({ "channel": "  t-json ", "interval": 0 }));
        let out = tick(&mut s, 0.0, send_inputs(vec![n(1.0), n(2.5)], true));
        // `ok` lags one tick, like the TS fetch.
        assert!(matches!(out[0], Value::Bool(false)));
        let m = bus.latest("t-json").unwrap();
        assert_eq!(
            (&*m.body, m.content_type.as_str()),
            (&b"[1,2.5]"[..], "application/json")
        );
        let out = tick(&mut s, 0.1, send_inputs(vec![n(3.0)], true));
        assert!(matches!(out[0], Value::Bool(true)));
        // Held high: no new publish.
        assert_eq!(bus.latest("t-json").unwrap().version, m.version);
        tick(&mut s, 0.2, send_inputs(vec![n(3.0)], false));
        tick(&mut s, 0.3, send_inputs(vec![n(4.0)], true));
        assert_eq!(&*bus.latest("t-json").unwrap().body, b"[4]");
    }

    #[test]
    fn send_interval_and_layers() {
        let bus = bus();
        let mut s = node(
            "io/send",
            json!({ "channel": "t-layers", "interval": 2, "width": 250.7, "height": 0 }),
        );
        let dot = Value::Scene(Arc::new(SceneNode::Point {
            x: 0.0,
            y: 0.0,
            style: default_style().clone(),
        }));
        let data = vec![dot.clone(), Value::list(vec![dot, n(1.0)])];
        for (i, t) in [0.0, 1.0, 1.9, 2.0, 3.0, 4.1].into_iter().enumerate() {
            tick(&mut s, t, send_inputs(data.clone(), false));
            let jobs = bus.take_layer_jobs();
            let sent = jobs.iter().any(|j| j.channel == "t-layers");
            assert_eq!(sent, matches!(i, 0 | 3 | 5), "t = {t}");
            if let Some(j) = jobs.iter().find(|j| j.channel == "t-layers") {
                assert_eq!((j.layers.len(), j.width, j.height), (2, 250, 540));
            }
        }
    }

    #[test]
    fn receive_polls_and_lags_a_tick() {
        let bus = bus();
        let mut r = node(
            "io/receive",
            json!({ "channel": "t-recv", "interval": 0.5 }),
        );
        assert!(tick(&mut r, 0.0, vec![]).is_empty());
        bus.publish(
            "t-recv",
            "application/json",
            b"[1,{\"x\":2,\"y\":3}]".to_vec(),
        );
        // Next poll is due at 0.5; its result shows on the tick after.
        assert!(tick(&mut r, 0.25, vec![]).is_empty());
        assert!(tick(&mut r, 0.5, vec![]).is_empty());
        let v = tick(&mut r, 0.6, vec![]);
        assert_eq!(format!("{v:?}"), "[1, {x:2, y:3}]");
        bus.publish("t-recv", "text/plain", b"hello".to_vec());
        tick(&mut r, 1.0, vec![]);
        assert_eq!(format!("{:?}", tick(&mut r, 1.1, vec![])), "[\"hello\"]");
        bus.publish("t-recv", "application/json", b"42".to_vec());
        tick(&mut r, 1.5, vec![]);
        assert_eq!(format!("{:?}", tick(&mut r, 1.6, vec![])), "[42]");
    }

    #[test]
    fn save_svg_is_an_inert_pass_through() {
        let mut s = node("io/save-svg", json!({}));
        let out = s
            .eval(vec![vec![n(1.0), n(2.0)], vec![Value::Bool(true)], vec![]])
            .unwrap();
        assert_eq!(out[0].len(), 2);
    }
}
