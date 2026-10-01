//! `meta/*` nodes. Ported from `src/nodes/meta/`.
//!
//! `meta/comment` has no compute (compile gives it a no-op) and
//! `meta/subpatch` is a region flattened away by compile, so only
//! `meta/probe` needs an implementation.

use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::value::empty;

pub fn register(r: &mut Registrar) {
    r.node("meta/probe", probe);
}

/// Pass-through reroute: re-emits the gathered spread verbatim.
fn probe(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| Ok(vec![inputs.first().cloned().unwrap_or_else(empty)]))
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::node;
    use crate::value::Value;

    #[test]
    fn probe_passes_spread_through() {
        let mut p = node("meta/probe", serde_json::json!({}));
        let out = p
            .eval(vec![vec![Value::Num(1.0), Value::Num(2.0)]])
            .unwrap();
        assert_eq!(out[0].len(), 2);
    }
}
