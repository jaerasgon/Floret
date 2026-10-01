//! `input/*` nodes. Ported from `src/nodes/input/`.
//!
//! `input/mouse` and `input/keyboard` read the evaluation frame. The MIDI
//! nodes have no device layer yet, so they emit exactly what the TS nodes
//! emit when no MIDI message has arrived (no device attached). Camera and
//! landmarks need media capture and are not ported here.

use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::value::{one, Value};

pub fn register(r: &mut Registrar) {
    r.node("input/mouse", mouse);
    r.node("input/keyboard", keyboard);
    r.node("input/midi-cc", midi_cc);
    r.node("input/midi-note", midi_note);
}

fn mouse(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let f = cx.frame;
        Ok(vec![
            one(Value::Num(f.mouse_x)),
            one(Value::Num(f.mouse_y)),
            one(Value::Bool(f.mouse_is_pressed)),
        ])
    })
}

fn keyboard(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let f = cx.frame;
        Ok(vec![
            one(Value::Bool(f.key_is_pressed)),
            one(Value::str(&f.key)),
        ])
    })
}

/// `getCC` returns -1 until a CC arrives → `[[0], [0]]`.
fn midi_cc(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, _| Ok(vec![one(Value::Num(0.0)), one(Value::Num(0.0))]))
}

/// No held note → `[[0], [0], [pressed = false], [0]]`.
fn midi_note(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, _| {
        Ok(vec![
            one(Value::Num(0.0)),
            one(Value::Num(0.0)),
            one(Value::Bool(false)),
            one(Value::Num(0.0)),
        ])
    })
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{node, num};
    use crate::value::Value;

    #[test]
    fn mouse_and_keyboard() {
        let mut m = node("input/mouse", serde_json::json!({}));
        m.frame.mouse_x = -12.0;
        m.frame.mouse_y = 30.0;
        m.frame.mouse_is_pressed = true;
        let out = m.eval1(&[]);
        assert_eq!(num(&out[0]), -12.0);
        assert_eq!(num(&out[1]), 30.0);
        assert!(matches!(out[2], Value::Bool(true)));

        let mut k = node("input/keyboard", serde_json::json!({}));
        k.frame.key = "a".into();
        let out = k.eval1(&[]);
        assert!(matches!(out[0], Value::Bool(false)));
        assert!(matches!(&out[1], Value::Str(s) if &**s == "a"));
    }

    #[test]
    fn midi_defaults() {
        let mut n = node("input/midi-note", serde_json::json!({}));
        let out = n.eval1(&[]);
        assert_eq!(out.len(), 4);
        assert!(matches!(out[2], Value::Bool(false)));
        let mut c = node("input/midi-cc", serde_json::json!({ "cc": 7 }));
        let out = c.eval1(&[]);
        assert_eq!((num(&out[0]), num(&out[1])), (0.0, 0.0));
    }
}
