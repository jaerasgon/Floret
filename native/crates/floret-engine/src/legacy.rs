//! Load-time fix-ups for patches saved by older versions of the web app.
//! A port of `migrateLegacyEdges`, `migrateLegacyDrawCoords` and
//! `pruneDeadEdges` from `src/persistence/serialize.ts`, which the web app
//! runs on every load (in that order). Idempotent: already-current patches
//! (like the built-in samples) come through unchanged.

use crate::patch::{parse_handle, Handle, PatchEdge, PatchFile, PatchNode, Position};
use crate::spec::{Catalogue, Params};
use std::collections::HashMap;

/// Apply all fix-ups in the web app's order, then drop node types Floret
/// no longer has. Returns one line per change the user should hear about.
pub fn normalize(patch: &mut PatchFile, cat: &Catalogue) -> Vec<String> {
    migrate_legacy_edges(patch, cat);
    migrate_legacy_draw_coords(patch);
    prune_dead_edges(patch, cat);
    drop_removed_nodes(patch)
}

/// Node types that are gone, by type: (type, label, why). Files that still
/// have one lose it and its wires on load.
const REMOVED_NODES: &[(&str, &str, &str)] = &[(
    "io/save-svg",
    "Save SVG",
    "Floret no longer exports SVG (File → Export PNG… or Export Video / GIF… save the picture)",
)];

/// Remove [`REMOVED_NODES`] and every wire to or from them. One notice per
/// type found ("Removed 2 Save SVG nodes: …").
pub fn drop_removed_nodes(patch: &mut PatchFile) -> Vec<String> {
    let mut notes = Vec::new();
    for (ty, label, why) in REMOVED_NODES {
        let gone: std::collections::HashSet<String> = patch
            .nodes
            .iter()
            .filter(|n| n.def_type == *ty)
            .map(|n| n.id.clone())
            .collect();
        if gone.is_empty() {
            continue;
        }
        patch.nodes.retain(|n| !gone.contains(&n.id));
        patch
            .edges
            .retain(|e| !gone.contains(&e.source) && !gone.contains(&e.target));
        for p in &mut patch.presets {
            p.values.retain(|id, _| !gone.contains(id));
        }
        let n = gone.len();
        let s = if n == 1 { "" } else { "s" };
        notes.push(format!("Removed {n} {label} node{s}: {why}."));
    }
    notes
}

/// Explicit input renames, by node type: old name → new name.
const INPUT_RENAMES: &[(&str, &str, &str)] = &[
    ("spread/randomize", "value", "source"),
    ("spread/repeat", "value", "source"),
];

/// Nodes that had a single output in the web app and gained native-only
/// outputs after it: old output names still mean the first one.
const ONCE_SINGLE_OUTPUT: &[&str] = &["time/seconds"];

/// Old output names on single-output nodes → that output; renamed inputs.
pub fn migrate_legacy_edges(patch: &mut PatchFile, cat: &Catalogue) {
    let def_type: HashMap<&str, &str> = patch
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.def_type.as_str()))
        .collect();
    let mut updates: Vec<(usize, Option<String>, Option<String>)> = Vec::new();
    for (i, e) in patch.edges.iter().enumerate() {
        let mut source = None;
        if let Some(Handle::Out(name)) = parse_handle(e.source_handle.as_deref()) {
            let ty = def_type.get(e.source.as_str()).copied();
            if let Some(spec) = ty.and_then(|t| cat.get(t)) {
                let outs = &spec.outputs;
                let single = outs.len() == 1 || ty.is_some_and(|t| ONCE_SINGLE_OUTPUT.contains(&t));
                if spec.region_kind.is_none() && single && !outs.iter().any(|p| p.name == name) {
                    source = Some(format!("out-{}", outs[0].name));
                }
            }
        }
        let mut target = None;
        if let Some(Handle::In(name)) = parse_handle(e.target_handle.as_deref()) {
            if let Some(t) = def_type.get(e.target.as_str()) {
                if let Some((_, _, new)) = INPUT_RENAMES
                    .iter()
                    .find(|(ty, old, _)| ty == t && *old == name)
                {
                    target = Some(format!("in-{new}"));
                }
            }
        }
        if source.is_some() || target.is_some() {
            updates.push((i, source, target));
        }
    }
    for (i, s, t) in updates {
        let e = &mut patch.edges[i];
        if let Some(s) = s {
            e.source_handle = Some(s);
        }
        if let Some(t) = t {
            e.target_handle = Some(t);
        }
    }
}

/// Draw nodes' old number `x`/`y` (and line `x1,y1`/`x2,y2`) inputs → one
/// vec2 input, fed by a synthesised `vec2/make`.
/// (vec2 input, old x input, old y input).
type CoordGroup = (&'static str, &'static str, &'static str);

const LEGACY_COORD_GROUPS: &[(&str, &[CoordGroup])] = &[
    ("draw/rect", &[("pos", "x", "y")]),
    ("draw/ellipse", &[("pos", "x", "y")]),
    ("draw/image", &[("pos", "x", "y")]),
    ("draw/text", &[("pos", "x", "y")]),
    ("draw/walker-cluster", &[("pos", "x", "y")]),
    ("draw/line", &[("a", "x1", "y1"), ("b", "x2", "y2")]),
];

fn const_coord(n: &PatchNode, key: &str) -> Option<f64> {
    let num = |p: &Params| p.get(key).and_then(serde_json::Value::as_f64);
    n.port_values
        .as_ref()
        .and_then(num)
        .or_else(|| num(&n.params))
}

pub fn migrate_legacy_draw_coords(patch: &mut PatchFile) {
    let mut seq = 0usize;
    let originals: Vec<PatchNode> = patch.nodes.clone();
    for n in &originals {
        let Some((_, groups)) = LEGACY_COORD_GROUPS.iter().find(|(t, _)| *t == n.def_type) else {
            continue;
        };
        for (out, xk, yk) in groups.iter() {
            let handle = |k: &str| Some(format!("in-{k}"));
            let targets = |e: &PatchEdge, k: &str| e.target == n.id && e.target_handle == handle(k);
            let pos_wired = patch.edges.iter().any(|e| targets(e, out));
            let xi = patch.edges.iter().position(|e| targets(e, xk));
            let yi = patch.edges.iter().position(|e| targets(e, yk));
            let (cx, cy) = (const_coord(n, xk), const_coord(n, yk));
            if pos_wired || (xi.is_none() && yi.is_none() && cx.is_none() && cy.is_none()) {
                continue;
            }
            let make_id = format!("{}-xy{seq}", n.id);
            seq += 1;
            let (cxv, cyv) = (cx.unwrap_or(0.0), cy.unwrap_or(0.0));
            let mut params = Params::new();
            params.insert("x".into(), cxv.into());
            params.insert("y".into(), cyv.into());
            patch.nodes.push(PatchNode {
                id: make_id.clone(),
                def_type: "vec2/make".into(),
                position: Position {
                    x: n.position.x - 60.0,
                    y: n.position.y,
                },
                params: params.clone(),
                parent_id: n.parent_id.clone(),
                port_values: Some(params),
                ports: None,
                width: None,
                height: None,
                bypassed: false,
                extra: Default::default(),
            });
            if let Some(i) = xi {
                patch.edges[i].target = make_id.clone();
                patch.edges[i].target_handle = Some("in-x".into());
            }
            if let Some(i) = yi {
                patch.edges[i].target = make_id.clone();
                patch.edges[i].target_handle = Some("in-y".into());
            }
            patch.edges.push(PatchEdge {
                id: format!("e_xy_{make_id}"),
                source: make_id,
                target: n.id.clone(),
                source_handle: Some("out-vector".into()),
                target_handle: Some(format!("in-{out}")),
            });
        }
    }
}

/// Drop edges whose handle certainly no longer exists (invisible dead wires).
pub fn prune_dead_edges(patch: &mut PatchFile, cat: &Catalogue) {
    let by_id: HashMap<&str, &PatchNode> = patch.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let exists = |node: &str, handle: Option<&str>| -> bool {
        let Some(n) = by_id.get(node) else {
            return true;
        };
        let Some(spec) = cat.get(&n.def_type) else {
            return true;
        };
        if spec.region_kind.is_some() {
            return true;
        }
        match parse_handle(handle) {
            None => true,
            Some(Handle::Out(name)) => {
                spec.has_dynamic_outputs || spec.outputs.iter().any(|p| p.name == name)
            }
            // Pins we can't resolve yet (dynamic inputs not ported) are kept.
            Some(Handle::In(_)) if spec.has_dynamic_inputs && !cat.has_dynamic_impl(&spec.ty) => {
                true
            }
            Some(Handle::In(name)) => cat
                .resolved_inputs(spec, &n.params)
                .iter()
                .any(|p| p.name == name),
        }
    };
    let keep: Vec<bool> = patch
        .edges
        .iter()
        .map(|e| {
            exists(&e.source, e.source_handle.as_deref())
                && exists(&e.target, e.target_handle.as_deref())
        })
        .collect();
    let mut it = keep.into_iter();
    patch.edges.retain(|_| it.next().unwrap_or(true));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(json: &str) -> PatchFile {
        let mut p = PatchFile::load(json).unwrap();
        normalize(&mut p, Catalogue::builtin());
        p
    }

    #[test]
    fn remaps_old_single_output_names_and_input_renames() {
        let p = load(
            r#"{"kind":"floret.patch","version":2,"nodes":[
              {"id":"t","defType":"time/seconds","position":{"x":0,"y":0},"params":{}},
              {"id":"r","defType":"spread/repeat","position":{"x":0,"y":0},"params":{}}],
             "edges":[{"id":"e","source":"t","target":"r","sourceHandle":"out-out","targetHandle":"in-value"}]}"#,
        );
        let e = &p.edges[0];
        assert_ne!(e.source_handle.as_deref(), Some("out-out"));
        // The native-only `loop` output doesn't change what `out-out` meant.
        assert_eq!(e.source_handle.as_deref(), Some("out-t"));
        assert_eq!(e.target_handle.as_deref(), Some("in-source"));
    }

    #[test]
    fn draw_xy_becomes_vec2_make() {
        let p = load(
            r#"{"kind":"floret.patch","version":2,"nodes":[
              {"id":"n","defType":"value/number","position":{"x":0,"y":0},"params":{"value":5}},
              {"id":"r","defType":"draw/rect","position":{"x":100,"y":0},"params":{},"portValues":{"y":7}}],
             "edges":[{"id":"e","source":"n","target":"r","sourceHandle":"out-value","targetHandle":"in-x"}]}"#,
        );
        let make = p
            .nodes
            .iter()
            .find(|n| n.id == "r-xy0")
            .expect("synthesised vec2/make");
        assert_eq!(make.def_type, "vec2/make");
        assert_eq!(make.params["y"], 7.0);
        assert!(p
            .edges
            .iter()
            .any(|e| e.target == "r-xy0" && e.target_handle.as_deref() == Some("in-x")));
        assert!(p
            .edges
            .iter()
            .any(|e| e.source == "r-xy0" && e.target_handle.as_deref() == Some("in-pos")));
    }

    #[test]
    fn prunes_edges_to_missing_ports_and_is_idempotent_on_samples() {
        let p = load(
            r#"{"kind":"floret.patch","version":2,"nodes":[
              {"id":"t","defType":"time/seconds","position":{"x":0,"y":0},"params":{}},
              {"id":"s","defType":"math/sin","position":{"x":0,"y":0},"params":{}}],
             "edges":[{"id":"e","source":"t","target":"s","sourceHandle":"out-seconds","targetHandle":"in-nope"}]}"#,
        );
        assert!(p.edges.is_empty());

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|x| x == "flora") {
                let text = std::fs::read_to_string(&path).unwrap();
                let before = PatchFile::load(&text).unwrap();
                let mut after = before.clone();
                let notes = normalize(&mut after, Catalogue::builtin());
                assert_eq!(
                    after.to_json(),
                    before.to_json(),
                    "{} changed",
                    path.display()
                );
                assert!(notes.is_empty(), "{}: {notes:?}", path.display());
            }
        }
    }

    #[test]
    fn save_svg_nodes_and_their_wires_are_dropped_with_a_notice() {
        let text = r#"{"kind":"floret.patch","version":2,"nodes":[
              {"id":"r","defType":"draw/rect","position":{"x":0,"y":0},"params":{}},
              {"id":"b","defType":"io/bang","position":{"x":0,"y":0},"params":{}},
              {"id":"svg","defType":"io/save-svg","position":{"x":0,"y":0},"params":{}},
              {"id":"svg2","defType":"io/save-svg","position":{"x":0,"y":0},"params":{}},
              {"id":"out","defType":"render/renderer","position":{"x":0,"y":0},"params":{}}],
             "edges":[
              {"id":"e1","source":"r","target":"svg","sourceHandle":"out-layer","targetHandle":"in-layers"},
              {"id":"e2","source":"b","target":"svg","sourceHandle":"out-out","targetHandle":"in-save"},
              {"id":"e3","source":"svg","target":"out","sourceHandle":"out-layers","targetHandle":"in-layers"},
              {"id":"e4","source":"r","target":"out","sourceHandle":"out-layer","targetHandle":"in-layers"}],
             "presets":[{"name":"p","values":{"svg":{"params":{"x":1}},"r":{"params":{"x":2}}}}]}"#;
        let mut p = PatchFile::load(text).unwrap();
        let notes = normalize(&mut p, Catalogue::builtin());
        assert_eq!(
            p.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            ["r", "b", "out"]
        );
        assert_eq!(
            p.edges.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["e4"]
        );
        assert_eq!(
            p.presets[0]
                .values
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["r"]
        );
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].starts_with("Removed 2 Save SVG nodes: "),
            "{notes:?}"
        );
        assert!(!notes[0].contains('\n'));
        // Loading again finds nothing more to do; the patch still compiles.
        assert!(normalize(&mut p, Catalogue::builtin()).is_empty());
        assert!(crate::Engine::from_patch(&p).is_ok());
        // The app's loader reports it.
        let (q, notes) = crate::load_patch_noting(text).unwrap();
        assert_eq!(q.nodes.len(), 3);
        assert_eq!(notes.len(), 1);
        assert!(Catalogue::builtin().get("io/save-svg").is_none());
    }
}
