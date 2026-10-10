//! Library samples are built from visual nodes: no `math/expression`
//! (formula) node in any of them. The Expression node itself stays
//! available for users' own patches.
//!
//! The samples are being rebuilt in batches; each batch lists its ids here.
//! Once every batch is in, the final merge switches this check to every
//! sample in `catalogue.json` (and drops the lists).

use std::path::Path;

/// The Systems samples (rebuilt by agent B2).
const SYSTEMS: &[&str] = &[
    "sys-epidemic",
    "sys-ecosystem",
    "sys-weather",
    "sys-orbits",
    "sys-landscape",
    "sys-tile-world",
    "sys-city",
    "sys-music-viz",
    "sys-data-story",
    "sys-garden",
    "sys-clock",
    "sys-glitch-tracker",
    "sys-poster",
    "sys-plotter",
    "sys-rd-topo",
];

fn expression_nodes(id: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../samples")
        .join(format!("{id}.flora"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{id}: {e}"));
    let patch = floret_engine::load_patch(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
    patch
        .nodes
        .iter()
        .filter(|n| n.def_type == "math/expression")
        .map(|n| n.id.clone())
        .collect()
}

#[test]
fn rebuilt_samples_have_no_expression_nodes() {
    let mut found = Vec::new();
    for id in SYSTEMS {
        let nodes = expression_nodes(id);
        if !nodes.is_empty() {
            found.push(format!("{id}: {}", nodes.join(", ")));
        }
    }
    assert!(
        found.is_empty(),
        "Expression nodes in Library samples:\n{}",
        found.join("\n")
    );
}
