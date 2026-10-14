//! Library samples are built from visual nodes: no `math/expression`
//! (formula) node in any of them, subpatches and regions included. The
//! Expression node itself stays available for users' own patches.

use std::path::Path;

fn samples_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples")
}

/// The ids of a patch's Expression nodes.
fn expression_nodes(text: &str) -> Vec<String> {
    let patch: serde_json::Value = serde_json::from_str(text).expect("sample is JSON");
    patch["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .filter(|n| n["defType"] == "math/expression")
        .map(|n| n["id"].as_str().unwrap_or("?").to_owned())
        .collect()
}

#[test]
fn no_sample_uses_the_expression_node() {
    let catalogue: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(samples_dir().join("catalogue.json")).expect("catalogue.json"),
    )
    .expect("catalogue is JSON");
    let samples = catalogue["samples"].as_array().expect("samples");
    assert!(samples.len() > 100, "found {} samples", samples.len());
    let mut found = Vec::new();
    for s in samples {
        let file = s["file"].as_str().expect("file");
        let text = std::fs::read_to_string(samples_dir().join(file))
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        let nodes = expression_nodes(&text);
        if !nodes.is_empty() {
            found.push(format!("{file}: {}", nodes.join(", ")));
        }
    }
    assert!(
        found.is_empty(),
        "Library samples must be built from visual nodes; Expression nodes in:\n{}",
        found.join("\n")
    );
    // The check finds one.
    let one = r#"{"nodes": [{"id": "e", "defType": "math/expression"}], "edges": []}"#;
    assert_eq!(expression_nodes(one), ["e"]);
}
