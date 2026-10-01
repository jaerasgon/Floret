//! The adaptive-math `valueType` latch (`reconcileAdaptiveTypes` in
//! `compile-pipeline.ts`): the first wire with a concrete peer type latches
//! number / vec2 / color; removing the last wire resets to `any`.

use super::ports::{find_node, is_loop_var, resolved_outputs, split_handle, Side};
use floret_engine::patch::PatchFile;
use floret_engine::spec::{Catalogue, PortType};
use std::collections::HashSet;

/// Concrete value type at a node's handle, or `None` for `any` / unknown.
/// Adaptive peers report their latched type; boolean counts as number.
fn port_concrete_type(
    patch: &PatchFile,
    cat: &Catalogue,
    node: &str,
    handle: Option<&str>,
) -> Option<&'static str> {
    let (side, name) = split_handle(handle)?;
    let n = find_node(patch, node)?;
    let spec = cat.get(&n.def_type)?;
    let t = if spec.adaptive_type {
        match n.params.get("valueType").and_then(|v| v.as_str()) {
            Some("vec2") => PortType::Vec2,
            Some("color") => PortType::Color,
            Some("number") => PortType::Number,
            _ => return None,
        }
    } else if spec.region_kind.is_some() {
        if is_loop_var(name) {
            PortType::Number
        } else if let Some(p) = n.ports.as_ref().and_then(|ps| ps.iter().find(|p| p.id == name)) {
            PortType::parse(&p.ty)?
        } else {
            spec.inputs.iter().find(|p| p.name == name)?.ty
        }
    } else if side == Side::Out {
        resolved_outputs(spec, &n.params)
            .iter()
            .find(|p| p.name == name)?
            .ty
    } else {
        cat.resolved_inputs(spec, &n.params)
            .iter()
            .find(|p| p.name == name)?
            .ty
    };
    match t {
        PortType::Number | PortType::Boolean => Some("number"),
        PortType::Vec2 => Some("vec2"),
        PortType::Color => Some("color"),
        _ => None,
    }
}

/// Maintain every adaptive node's latch. Idempotent; returns whether any
/// `valueType` changed (dynamic inputs then change shape too).
pub fn reconcile_adaptive_types(patch: &mut PatchFile, cat: &Catalogue) -> bool {
    let mut updates: Vec<(usize, &'static str)> = Vec::new();
    for (i, n) in patch.nodes.iter().enumerate() {
        let Some(spec) = cat.get(&n.def_type).filter(|s| s.adaptive_type) else {
            continue;
        };
        let cur = n
            .params
            .get("valueType")
            .and_then(|v| v.as_str())
            .unwrap_or("any");
        // Only ADAPTIVE inputs drive the latch: those whose type changes
        // between valueType number and vec2 (lerp's `t`, spring's
        // `stiffness` … stay fixed numbers and don't count).
        let with = |vt: &str| {
            let mut p = n.params.clone();
            p.insert("valueType".into(), vt.into());
            cat.resolved_inputs(spec, &p)
        };
        let (a, b) = (with("number"), with("vec2"));
        let adaptive_ins: HashSet<&str> = a
            .iter()
            .zip(b.iter())
            .filter(|(x, y)| x.ty != y.ty)
            .map(|(x, _)| x.name.as_str())
            .collect();
        let touching: Vec<_> = patch
            .edges
            .iter()
            .filter(|e| {
                e.source == n.id
                    || (e.target == n.id
                        && matches!(split_handle(e.target_handle.as_deref()),
                            Some((Side::In, name)) if adaptive_ins.contains(name)))
            })
            .collect();
        let next: &'static str = if touching.is_empty() {
            "any"
        } else if cur == "any" {
            touching
                .iter()
                .find_map(|e| {
                    if e.source == n.id {
                        port_concrete_type(patch, cat, &e.target, e.target_handle.as_deref())
                    } else {
                        port_concrete_type(patch, cat, &e.source, e.source_handle.as_deref())
                    }
                })
                .unwrap_or("any")
        } else {
            continue;
        };
        if next != cur {
            updates.push((i, next));
        }
    }
    let changed = !updates.is_empty();
    for (i, vt) in updates {
        patch.nodes[i]
            .params
            .insert("valueType".into(), serde_json::Value::from(vt));
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::connect::{connect, Conn};
    use crate::editor::test_util::{node, patch};

    fn vt(p: &PatchFile, id: &str) -> String {
        find_node(p, id).unwrap().params["valueType"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn first_wire_latches_last_wire_resets() {
        let cat = Catalogue::builtin();
        let mut add = node("add", "math/add");
        add.params = cat.get("math/add").unwrap().default_params.clone();
        let mut p = patch(
            vec![
                add,
                node("mk", "vec2/make"),
                node("t", "time/seconds"),
                node("mk2", "vec2/make"),
            ],
            vec![],
        );
        assert!(!reconcile_adaptive_types(&mut p, cat));
        assert_eq!(vt(&p, "add"), "any");

        // vec2 output → add.a latches vec2.
        let e1 = connect(&mut p, cat, &Conn::new("mk", "out-vector", "add", "in-a")).unwrap();
        assert!(reconcile_adaptive_types(&mut p, cat));
        assert_eq!(vt(&p, "add"), "vec2");
        // Now a number source no longer fits `b` (vec2) …
        assert!(connect(&mut p, cat, &Conn::new("t", "out-t", "add", "in-b")).is_none());
        // … but another vec2 does, and the latch holds.
        let e2 = connect(&mut p, cat, &Conn::new("mk2", "out-vector", "add", "in-b")).unwrap();
        assert!(!reconcile_adaptive_types(&mut p, cat));
        assert_eq!(vt(&p, "add"), "vec2");

        // Removing one wire keeps the latch; removing the last resets.
        p.edges.retain(|e| e.id != e1);
        assert!(!reconcile_adaptive_types(&mut p, cat));
        assert_eq!(vt(&p, "add"), "vec2");
        p.edges.retain(|e| e.id != e2);
        assert!(reconcile_adaptive_types(&mut p, cat));
        assert_eq!(vt(&p, "add"), "any");

        // A number wire latches number.
        connect(&mut p, cat, &Conn::new("t", "out-t", "add", "in-a")).unwrap();
        reconcile_adaptive_types(&mut p, cat);
        assert_eq!(vt(&p, "add"), "number");
    }

    #[test]
    fn output_wire_latches_from_downstream_and_fixed_inputs_do_not() {
        let cat = Catalogue::builtin();
        let mut lerp = node("lerp", "math/lerp");
        lerp.params = cat.get("math/lerp").unwrap().default_params.clone();
        let mut p = patch(
            vec![lerp, node("t", "time/seconds"), node("mk", "vec2/make")],
            vec![],
        );
        // `t` is a fixed number control: wiring it doesn't latch.
        connect(&mut p, cat, &Conn::new("t", "out-t", "lerp", "in-t")).unwrap();
        reconcile_adaptive_types(&mut p, cat);
        assert_eq!(vt(&p, "lerp"), "any");
        // result → vec2/make.x (number) latches number from downstream.
        connect(&mut p, cat, &Conn::new("lerp", "out-result", "mk", "in-x")).unwrap();
        reconcile_adaptive_types(&mut p, cat);
        assert_eq!(vt(&p, "lerp"), "number");
    }
}
