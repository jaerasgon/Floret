//! `io/save-svg`, which is dropped (no vector output): it stays as an inert
//! pass-through so old patches still draw.

use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::value::empty;

pub fn register(r: &mut Registrar) {
    r.node("io/save-svg", save_svg);
}

/// Inert: passes `layers` through and never exports.
fn save_svg(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| Ok(vec![inputs.first().cloned().unwrap_or_else(empty)]))
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{n, node};
    use crate::value::Value;
    use serde_json::json;

    #[test]
    fn save_svg_is_an_inert_pass_through() {
        let mut s = node("io/save-svg", json!({}));
        let out = s
            .eval(vec![vec![n(1.0), n(2.0)], vec![Value::Bool(true)], vec![]])
            .unwrap();
        assert_eq!(out[0].len(), 2);
    }
}
