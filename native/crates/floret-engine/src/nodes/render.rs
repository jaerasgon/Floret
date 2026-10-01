//! `render/*` nodes. Ported from `src/nodes/render/renderer.ts`.

use super::Registrar;
use crate::node::{raw, NodeInit};
use crate::value::{empty, Value};

pub fn register(r: &mut Registrar) {
    r.node("render/renderer", renderer);
}

/// The sink: passes layers and background through. Writes the resolved
/// (possibly wired) width/height back to the param store, where the render
/// driver reads canvas size.
fn renderer(_: &NodeInit) -> Box<dyn crate::node::NodeImpl> {
    raw(|inputs, cx| {
        for (port, key) in [(2, "width"), (3, "height")] {
            if let Some(Value::Num(n)) = inputs.get(port).and_then(|s| s.first()) {
                if n.is_finite() {
                    let v = serde_json::Number::from_f64(*n).map(serde_json::Value::Number);
                    if let Some(v) = v {
                        if cx.params.get(cx.node_id, key) != Some(&v) {
                            cx.params.set(cx.node_id, key, v);
                        }
                    }
                }
            }
        }
        Ok(vec![
            inputs.first().cloned().unwrap_or_else(empty),
            inputs.get(1).cloned().unwrap_or_else(empty),
        ])
    })
}
