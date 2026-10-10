//! Library samples flow left to right (CLAUDE.md, golden rule 1): in every
//! scope of every sample, a wire only runs to a node to its right
//! (`floret_engine::flow`). Memory across frames goes through Write → Read
//! pairs matched by slot, never through a single-node State / State Spread
//! loop, and every Write has its Read (and every Read its Write).

use floret_engine::compile::FEEDBACK_FAMILIES;
use floret_engine::flow::backward_edges;
use floret_engine::patch::PatchFile;
use std::collections::BTreeSet;
use std::path::Path;

fn samples_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples")
}

/// The legacy single-node loops.
const LOOP_TYPES: [&str; 2] = ["state/value", "state/spread"];

/// A pair's slot as the engine matches it: numbers by value, text as is,
/// unset → the default slot 1.
fn slot(params: &floret_engine::spec::Params) -> String {
    match params.get("slot") {
        None => "1".into(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(v) => v.as_f64().map_or_else(|| v.to_string(), |n| n.to_string()),
    }
}

/// Everything in a patch that breaks the flow rule, one line per kind.
fn violations(text: &str) -> Vec<String> {
    let patch = match PatchFile::load(text) {
        Ok(p) => p,
        Err(e) => return vec![format!("doesn't load: {e}")],
    };
    let mut out = Vec::new();
    let backward = backward_edges(&patch);
    if !backward.is_empty() {
        out.push(format!("backward wires {}", backward.join(", ")));
    }
    let loops: Vec<&str> = patch
        .nodes
        .iter()
        .filter(|n| LOOP_TYPES.contains(&n.def_type.as_str()))
        .map(|n| n.id.as_str())
        .collect();
    if !loops.is_empty() {
        out.push(format!("State / State Spread nodes {}", loops.join(", ")));
    }
    for (read, write) in FEEDBACK_FAMILIES {
        let slots = |ty: &str| -> BTreeSet<String> {
            patch
                .nodes
                .iter()
                .filter(|n| n.def_type == ty)
                .map(|n| slot(&n.params))
                .collect()
        };
        let (reads, writes) = (slots(read), slots(write));
        let unpaired = |ty: &str, other: &BTreeSet<String>| -> Vec<&str> {
            patch
                .nodes
                .iter()
                .filter(|n| n.def_type == ty && !other.contains(&slot(&n.params)))
                .map(|n| n.id.as_str())
                .collect()
        };
        for (ty, other) in [(write, &reads), (read, &writes)] {
            let ids = unpaired(ty, other);
            if !ids.is_empty() {
                out.push(format!("{ty} without its pair {}", ids.join(", ")));
            }
        }
    }
    out
}

#[test]
fn samples_flow_left_to_right() {
    let catalogue: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(samples_dir().join("catalogue.json")).expect("catalogue.json"),
    )
    .expect("catalogue is JSON");
    let samples = catalogue["samples"].as_array().expect("samples");
    assert!(samples.len() > 100, "found {} samples", samples.len());
    // `FLORET_FLOW_ONLY=a,b` checks just those sample ids.
    let only = std::env::var("FLORET_FLOW_ONLY").ok();
    let mut found = Vec::new();
    for s in samples {
        let id = s["id"].as_str().unwrap_or_default();
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|x| x == id))
        {
            continue;
        }
        let file = s["file"].as_str().expect("file");
        let text = std::fs::read_to_string(samples_dir().join(file))
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        for v in violations(&text) {
            found.push(format!("{file}: {v}"));
        }
    }
    assert!(
        found.is_empty(),
        "Library samples must flow left to right, with feedback only through \
         Write → Read pairs:\n{}",
        found.join("\n")
    );
}

/// The checks catch a backward wire, a State loop and a lone Write.
#[test]
fn the_check_finds_each_kind() {
    let patch = |nodes: &str, edges: &str| {
        format!(
            r#"{{"kind": "floret.patch", "version": 4, "nodes": [{nodes}], "edges": [{edges}]}}"#
        )
    };
    let node = |id: &str, ty: &str, x: i32, params: &str| {
        format!(
            r#"{{"id": "{id}", "defType": "{ty}", "position": {{"x": {x}, "y": 0}}, "params": {{{params}}}}}"#
        )
    };
    let edge = |id: &str, s: &str, sh: &str, t: &str, th: &str| {
        format!(
            r#"{{"id": "{id}", "source": "{s}", "target": "{t}", "sourceHandle": "out-{sh}", "targetHandle": "in-{th}"}}"#
        )
    };
    let a = node("a", "math/add", 0, "");
    let b = node("b", "math/add", 300, "");
    // Forward is fine, backward is caught.
    let fwd = edge("e1", "a", "result", "b", "a");
    assert!(violations(&patch(&format!("{a},{b}"), &fwd)).is_empty());
    let back = edge("e2", "b", "result", "a", "a");
    assert_eq!(
        violations(&patch(&format!("{a},{b}"), &back)),
        ["backward wires e2"]
    );
    // A State loop.
    let st = node("s", "state/value", 0, "");
    assert_eq!(
        violations(&patch(&st, "")),
        ["State / State Spread nodes s"]
    );
    // A Write → Read pair on one slot is fine; a lone Write isn't.
    let r = node("r", "state/read", 0, r#""slot": 2"#);
    let w = node("w", "state/write", 300, r#""slot": 2"#);
    assert!(violations(&patch(&format!("{r},{w}"), "")).is_empty());
    let w3 = node("w", "state/write", 300, r#""slot": 3"#);
    assert_eq!(
        violations(&patch(&format!("{r},{w3}"), "")),
        [
            "state/write without its pair w",
            "state/read without its pair r"
        ]
    );
}
