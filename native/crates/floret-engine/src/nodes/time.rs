//! `time/*` nodes. Ported from `src/nodes/time/time.ts`.
//!
//! Context nodes: they read the evaluation frame (`cx.frame`), which inside
//! a ForEach with `delay` is shifted per iteration — same as the frame TS
//! passes to `readFromContext`.

use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::value::{one, Value};

pub fn register(r: &mut Registrar) {
    r.node("time/seconds", seconds);
    r.node("time/frame", frame);
    r.node("time/delta", delta);
}

fn seconds(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| Ok(vec![one(Value::Num(cx.frame.t))]))
}

fn frame(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| Ok(vec![one(Value::Num(cx.frame.frame_count))]))
}

fn delta(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| Ok(vec![one(Value::Num(cx.frame.dt))]))
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{node, num};

    #[test]
    fn reads_frame() {
        for (ty, want) in [
            ("time/seconds", 1.5),
            ("time/frame", 90.0),
            ("time/delta", 0.25),
        ] {
            let mut n = node(ty, serde_json::json!({}));
            n.frame.t = 1.5;
            n.frame.frame_count = 90.0;
            n.frame.dt = 0.25;
            assert_eq!(num(&n.eval1(&[])[0]), want, "{ty}");
        }
    }
}
