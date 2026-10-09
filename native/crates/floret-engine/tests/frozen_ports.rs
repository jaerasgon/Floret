//! Port-name freeze (#78): saved `.flora` files refer to node types, pin
//! names (edges' `in-<name>` / `out-<name>` handles, `portValues` keys)
//! and param names. `frozen_ports.txt` records every one the built-in
//! catalogue offers — hidden legacy nodes and every mode's dynamic pins
//! included — and this test fails when one disappears, because files that
//! use it would silently lose wires and values.
//!
//! - Added a node, pin or param? Record it:
//!   `FLORET_UPDATE_PORTS=1 cargo test -p floret-engine --test frozen_ports`
//! - Renamed or removed one? Add a load-time migration first
//!   (`legacy.rs` / the version ladder in `patch.rs`) so old files keep
//!   their wires, then record with `FLORET_ALLOW_PORT_REMOVAL=1` as well.

use floret_engine::spec::{Catalogue, NodeSpec, Params};
use serde_json::Value as Json;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/frozen_ports.txt")
}

/// The param sets worth resolving pins for: the defaults, each dropdown
/// option on its own, and each pair of options (modes with sub-choices,
/// e.g. Effect `blur` × `kind`).
fn param_sets(spec: &NodeSpec) -> Vec<Params> {
    let base = spec.default_params.clone();
    let mut keys: Vec<&String> = spec.param_options.keys().collect();
    keys.sort();
    let mut out = vec![base.clone()];
    for (i, k) in keys.iter().enumerate() {
        for v in &spec.param_options[*k] {
            let mut p = base.clone();
            p.insert((*k).clone(), Json::String(v.clone()));
            out.push(p.clone());
            for k2 in &keys[i + 1..] {
                for v2 in &spec.param_options[*k2] {
                    let mut q = p.clone();
                    q.insert((*k2).clone(), Json::String(v2.clone()));
                    out.push(q);
                }
            }
        }
    }
    out
}

fn current() -> BTreeSet<String> {
    let cat = Catalogue::builtin();
    let mut lines = BTreeSet::new();
    for spec in cat.iter() {
        let ty = &spec.ty;
        lines.insert(format!("{ty} node"));
        for k in spec.default_params.keys().chain(spec.param_options.keys()) {
            lines.insert(format!("{ty} param {k}"));
        }
        for p in &spec.inputs {
            lines.insert(format!("{ty} in {}", p.name));
        }
        for p in &spec.outputs {
            lines.insert(format!("{ty} out {}", p.name));
        }
        if spec.has_dynamic_inputs || spec.has_dynamic_outputs {
            for params in param_sets(spec) {
                for p in cat.resolved_inputs(spec, &params) {
                    lines.insert(format!("{ty} in {}", p.name));
                }
                for p in cat.resolved_outputs(spec, &params) {
                    lines.insert(format!("{ty} out {}", p.name));
                }
            }
        }
    }
    lines
}

#[test]
fn saved_patch_names_stay_valid() {
    let now = current();
    let path = snapshot_path();
    let frozen: BTreeSet<String> = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect();
    let removed: Vec<&String> = frozen.difference(&now).collect();
    let added: Vec<&String> = now.difference(&frozen).collect();
    let update = std::env::var("FLORET_UPDATE_PORTS").is_ok();
    let allow_removal = std::env::var("FLORET_ALLOW_PORT_REMOVAL").is_ok();
    if update && (removed.is_empty() || allow_removal) {
        let mut text = String::from(
            "# Every node type, pin and param name saved patches may use (see frozen_ports.rs).\n",
        );
        for l in &now {
            text.push_str(l);
            text.push('\n');
        }
        std::fs::write(&path, text).expect("write frozen_ports.txt");
        return;
    }
    assert!(
        removed.is_empty(),
        "{} saved-patch name(s) no longer exist — files using them would lose wires or values. \
         Add a load-time migration (legacy.rs / patch.rs), then record with \
         FLORET_UPDATE_PORTS=1 FLORET_ALLOW_PORT_REMOVAL=1:\n  {}",
        removed.len(),
        removed
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    assert!(
        added.is_empty(),
        "{} new node / pin / param name(s) aren't recorded yet; run \
         FLORET_UPDATE_PORTS=1 cargo test -p floret-engine --test frozen_ports:\n  {}",
        added.len(),
        added
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}
