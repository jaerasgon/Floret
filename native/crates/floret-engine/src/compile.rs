//! Patch → runtime graph. A step-for-step port of `src/runtime/compile.ts`.
//!
//! Passes:
//!  -1. flatten subpatches (editor-only scopes)
//!   0. expand region frame ports into synthetic splicer nodes
//!   1. index nodes, collect each region's splicers and accumulator pairs
//!   2. build runtime nodes
//!   3. resolve edges, rewriting cross-scope ones onto the region
//!
//! then topo-sorts each scope and decides whether the patch always animates.

use crate::graph::{build_graph, InputSource, NodeIdx, RuntimeEdge, RuntimeGraph, RuntimeNode};
use crate::node::{NodeInit, ParamStore};
use crate::nodes;
use crate::patch::{parse_handle, Handle, PatchFile, PortDirection, RegionPort};
use crate::spec::{
    Catalogue, EvalKind, NodeSpec, Params, PortKind, PortSpec, PortType, RegionKind, RegionRole,
};
use crate::value::{empty, one, Color, Spread, Value, Vec2};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct CompileWarning {
    pub message: String,
    pub node_ids: Vec<String>,
}

pub struct CompileResult {
    pub graph: RuntimeGraph,
    pub warnings: Vec<CompileWarning>,
    /// Node types in the patch that have no Rust implementation yet.
    pub unimplemented: Vec<String>,
}

/// An editor node as compile sees it.
#[derive(Clone, Debug)]
struct NodeRef {
    id: String,
    def_type: String,
    params: Params,
    parent: Option<String>,
    ports: Vec<RegionPort>,
    port_values: Option<Params>,
    bypassed: bool,
}

#[derive(Clone, Debug)]
struct EdgeRef {
    from: String,
    from_port: String,
    to: String,
    to_port: String,
}

pub const LOOP_VAR_NAMES: [&str; 3] = ["index", "count", "fraction"];

pub fn synthetic_splicer_id(region: &str, port: &str, dir: PortDirection) -> String {
    let tag = match dir {
        PortDirection::Input => "sin",
        PortDirection::Output => "sout",
    };
    format!("{region}__{tag}_{port}")
}

pub fn loop_var_id(region: &str, name: &str) -> String {
    format!("{region}__loopvar_{name}")
}

fn parse_loop_var_handle(h: &str) -> Option<&'static str> {
    let name = h.strip_prefix("__loopvar_")?;
    LOOP_VAR_NAMES.iter().copied().find(|n| *n == name)
}

/// Compile a loaded patch. `params` is the live param store (seeded from the
/// patch's node params by the caller, like the editor's `paramStore`).
pub fn compile(
    patch: &PatchFile,
    catalogue: &Catalogue,
    params: &ParamStore,
) -> Result<CompileResult, String> {
    let nodes: Vec<NodeRef> = patch
        .nodes
        .iter()
        .map(|n| NodeRef {
            id: n.id.clone(),
            def_type: n.def_type.clone(),
            params: n.params.clone(),
            parent: n.parent_id.clone(),
            ports: n.ports.clone().unwrap_or_default(),
            port_values: n.port_values.clone(),
            bypassed: n.bypassed,
        })
        .collect();
    let mut edges = Vec::new();
    for e in &patch.edges {
        let (Some(Handle::Out(fp)), Some(Handle::In(tp))) = (
            parse_handle(e.source_handle.as_deref()),
            parse_handle(e.target_handle.as_deref()),
        ) else {
            continue;
        };
        edges.push(EdgeRef {
            from: e.source.clone(),
            from_port: fp.into(),
            to: e.target.clone(),
            to_port: tp.into(),
        });
    }
    compile_refs(nodes, edges, catalogue, params, !patch.legacy_slot0)
}

/// `lift`: scalar nodes of the types `nodes::lifts` lists run per element
/// over spreads (patches from v4 on; see `PatchFile::legacy_slot0`).
fn compile_refs(
    nodes_in: Vec<NodeRef>,
    edges_in: Vec<EdgeRef>,
    cat: &Catalogue,
    params: &ParamStore,
    lift: bool,
) -> Result<CompileResult, String> {
    let mut warnings: Vec<CompileWarning> = Vec::new();
    let mut warn = |message: String, ids: &[&str]| {
        warnings.push(CompileWarning {
            message,
            node_ids: ids.iter().map(|s| s.to_string()).collect(),
        });
    };

    let (nodes_in, edges_in) = flatten_sections(nodes_in, edges_in, cat);
    let (flat_nodes, flat_edges) = flatten_subpatches(nodes_in, edges_in, cat);
    let (nodes, edges) = expand_region_ports(flat_nodes, flat_edges, cat);

    // ─── Pass 1 ──────────────────────────────────────────────────────────
    let node_by_id: HashMap<&str, &NodeRef> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut spec_by_id: HashMap<&str, &NodeSpec> = HashMap::new();
    for n in &nodes {
        match cat.get(&n.def_type) {
            Some(s) => {
                spec_by_id.insert(n.id.as_str(), s);
                // A family node with an op it doesn't have runs its default
                // op (so a newer file still loads): say so, or a typo'd op
                // silently computes something else.
                if let Some(f) = crate::nodes::family::family(&n.def_type) {
                    let op = n.params.get(f.param).and_then(|v| v.as_str());
                    if let Some(op) = op.filter(|op| f.op_by_id(op).is_none()) {
                        warn(
                            format!(
                                "{} has no {} \"{op}\" — it runs {} instead.",
                                s.label, f.param, f.ops[0].id
                            ),
                            &[&n.id],
                        );
                    }
                }
            }
            None => warn(
                format!("Unknown node type \"{}\" — node skipped.", n.def_type),
                &[&n.id],
            ),
        }
    }

    #[derive(Default)]
    struct RegionMeta {
        splicer_in: Vec<String>,
        splicer_out: Vec<String>,
        accumulators: Vec<String>,
        splicer_in_index: HashMap<String, usize>,
        splicer_out_index: HashMap<String, usize>,
        acc_index: HashMap<String, usize>,
    }
    let mut region_meta: HashMap<String, RegionMeta> = HashMap::new();
    for n in &nodes {
        if spec_by_id
            .get(n.id.as_str())
            .is_some_and(|s| s.region_kind.is_some())
        {
            region_meta.insert(n.id.clone(), RegionMeta::default());
        }
    }
    for n in &nodes {
        let (Some(parent), Some(spec)) = (&n.parent, spec_by_id.get(n.id.as_str())) else {
            continue;
        };
        let Some(meta) = region_meta.get_mut(parent) else {
            continue;
        };
        match spec.region_role {
            Some(RegionRole::SplicerIn) => {
                meta.splicer_in_index
                    .insert(n.id.clone(), meta.splicer_in.len());
                meta.splicer_in.push(n.id.clone());
            }
            Some(RegionRole::SplicerOut) => {
                meta.splicer_out_index
                    .insert(n.id.clone(), meta.splicer_out.len());
                meta.splicer_out.push(n.id.clone());
            }
            _ => {}
        }
    }
    let region_ids: Vec<String> = nodes
        .iter()
        .filter(|n| region_meta.contains_key(&n.id))
        .map(|n| n.id.clone())
        .collect();
    for region_id in &region_ids {
        let mut reads: IndexMap<u64, (f64, String)> = IndexMap::new();
        let mut writes: IndexMap<u64, (f64, String)> = IndexMap::new();
        for n in &nodes {
            if n.parent.as_deref() != Some(region_id.as_str()) {
                continue;
            }
            let Some(spec) = spec_by_id.get(n.id.as_str()) else {
                continue;
            };
            let slot = slot_of(&n.params);
            match spec.region_role {
                Some(RegionRole::AccumulatorRead) => {
                    reads.insert(slot.to_bits(), (slot, n.id.clone()));
                }
                Some(RegionRole::AccumulatorWrite) => {
                    writes.insert(slot.to_bits(), (slot, n.id.clone()));
                }
                _ => {}
            }
        }
        let meta = region_meta.get_mut(region_id).expect("region meta");
        for (key, (slot, read_id)) in &reads {
            let Some((_, write_id)) = writes.get(key) else {
                warn(
                    format!(
                        "Accumulator Read (slot {}) in region \"{region_id}\" has no matching Write — pair dropped.",
                        js_num(*slot)
                    ),
                    &[read_id, region_id],
                );
                continue;
            };
            let idx = meta.accumulators.len();
            meta.acc_index.insert(read_id.clone(), idx);
            meta.acc_index.insert(write_id.clone(), idx);
            meta.accumulators.push(read_id.clone());
        }
        for (key, (slot, write_id)) in &writes {
            if !reads.contains_key(key) {
                warn(
                    format!(
                        "Accumulator Write (slot {}) in region \"{region_id}\" has no matching Read — pair dropped.",
                        js_num(*slot)
                    ),
                    &[write_id, region_id],
                );
            }
        }
    }

    // ─── Pass 2: runtime nodes ───────────────────────────────────────────
    let mut runtime_nodes: Vec<RuntimeNode> = Vec::new();
    let mut index: HashMap<String, NodeIdx> = HashMap::new();
    let mut unimplemented: Vec<String> = Vec::new();
    let mut parents: Vec<Option<String>> = Vec::new();
    // Per runtime node: runs per element over spreads (so a spread wire
    // into one of its single-value pins is kept).
    let mut lifting: Vec<bool> = Vec::new();
    for n in &nodes {
        let Some(spec) = spec_by_id.get(n.id.as_str()) else {
            continue;
        };
        let meta = region_meta.get(&n.id);
        let mut rn = to_runtime_node(
            cat,
            spec,
            n,
            params,
            meta.map(|m| {
                (
                    m.splicer_in.len(),
                    m.splicer_out.len(),
                    m.accumulators.len(),
                )
            }),
        )?;
        if rn.unimplemented && !unimplemented.contains(&rn.ty) {
            unimplemented.push(rn.ty.clone());
        }
        let lifts = lift && nodes::lifts(&spec.ty);
        if lifts {
            // A bypassed node's pass-through takes the spread as it is.
            if let Some(imp) = rn.imp.as_mut() {
                imp.lift_spreads();
            }
        }
        lifting.push(lifts);
        index.insert(n.id.clone(), runtime_nodes.len());
        parents.push(n.parent.clone());
        runtime_nodes.push(rn);
    }
    for (i, p) in parents.iter().enumerate() {
        runtime_nodes[i].parent = p.as_ref().and_then(|p| index.get(p).copied());
    }

    // ─── Pass 3: edges ───────────────────────────────────────────────────
    let mut inputs_cache: HashMap<String, Vec<PortSpec>> = HashMap::new();
    let mut resolved_inputs = |id: &str| -> Vec<PortSpec> {
        if let Some(c) = inputs_cache.get(id) {
            return c.clone();
        }
        let v = match (node_by_id.get(id), spec_by_id.get(id)) {
            (Some(n), Some(s)) => cat.resolved_inputs(s, &n.params),
            _ => Vec::new(),
        };
        inputs_cache.insert(id.to_owned(), v.clone());
        v
    };
    let mut runtime_edges: Vec<RuntimeEdge> = Vec::new();
    for e in &edges {
        let Some(src) = node_by_id.get(e.from.as_str()) else {
            warn(
                format!("Edge from missing node \"{}\" — wire dropped.", e.from),
                &[],
            );
            continue;
        };
        let Some(dst) = node_by_id.get(e.to.as_str()) else {
            warn(
                format!("Edge to missing node \"{}\" — wire dropped.", e.to),
                &[],
            );
            continue;
        };
        let (Some(src_spec), Some(dst_spec)) = (
            spec_by_id.get(src.id.as_str()),
            spec_by_id.get(dst.id.as_str()),
        ) else {
            warn(
                "Edge between nodes of unknown type — wire dropped.".into(),
                &[&src.id, &dst.id],
            );
            continue;
        };
        let dst_inputs = resolved_inputs(&dst.id);
        let idx = |id: &str| index[id];
        let out_idx = |s: &NodeSpec, name: &str| s.outputs.iter().position(|p| p.name == name);
        let in_idx = |ins: &[PortSpec], name: &str| ins.iter().position(|p| p.name == name);
        let src_scope = src.parent.as_deref();
        let dst_scope = dst.parent.as_deref();
        let parent_of = |id: &str| node_by_id.get(id).and_then(|n| n.parent.as_deref());

        if src_scope == dst_scope {
            let Some(from) = out_idx(src_spec, &e.from_port) else {
                warn(
                    format!(
                        "Unknown output port \"{}\" on \"{}\" — wire dropped.",
                        e.from_port, e.from
                    ),
                    &[&src.id],
                );
                continue;
            };
            let Some(to) = in_idx(&dst_inputs, &e.to_port) else {
                warn(
                    format!(
                        "Unknown input port \"{}\" on \"{}\" — wire dropped.",
                        e.to_port, e.to
                    ),
                    &[&dst.id],
                );
                continue;
            };
            let (sp, dp) = (&src_spec.outputs[from], &dst_inputs[to]);
            if sp.kind == PortKind::Spread && dp.kind == PortKind::Single && !lifting[idx(&e.to)] {
                let hint = if sp.ty == PortType::Layer {
                    "Wrap the consumer in a ForEach, or feed the spread through a Group first."
                } else {
                    "Wrap the consumer in a ForEach region to iterate."
                };
                warn(
                    format!(
                        "Spread wired into single-value input \"{}.{}\" — only slot 0 reaches it. {hint}",
                        e.to, e.to_port
                    ),
                    &[&src.id, &dst.id],
                );
                continue;
            }
            runtime_edges.push(RuntimeEdge {
                from: idx(&e.from),
                from_port: from,
                to: idx(&e.to),
                to_port: to,
            });
            continue;
        }

        // A) outer → splicer-in
        if dst_spec.region_role == Some(RegionRole::SplicerIn) {
            if let Some(region) = dst.parent.as_deref().filter(|r| parent_of(r) == src_scope) {
                let Some(meta) = region_meta.get(region) else {
                    warn(
                        format!(
                            "Splicer-in \"{}\" has no region meta — internal compile bug.",
                            dst.id
                        ),
                        &[&dst.id, region],
                    );
                    continue;
                };
                let Some(from) = out_idx(src_spec, &e.from_port) else {
                    warn(
                        format!(
                            "Unknown output port \"{}\" on \"{}\" — wire dropped.",
                            e.from_port, e.from
                        ),
                        &[&src.id],
                    );
                    continue;
                };
                let Some(&s) = meta.splicer_in_index.get(&dst.id) else {
                    warn(
                        format!(
                            "Splicer-in \"{}\" not indexed — internal compile bug.",
                            dst.id
                        ),
                        &[&dst.id, region],
                    );
                    continue;
                };
                let to = resolved_inputs(region).len() + s;
                runtime_edges.push(RuntimeEdge {
                    from: idx(&e.from),
                    from_port: from,
                    to: idx(region),
                    to_port: to,
                });
                continue;
            }
        }
        // B) splicer-out → outer
        if src_spec.region_role == Some(RegionRole::SplicerOut) {
            if let Some(region) = src.parent.as_deref().filter(|r| parent_of(r) == dst_scope) {
                let Some(meta) = region_meta.get(region) else {
                    warn(
                        format!(
                            "Splicer-out \"{}\" has no region meta — internal compile bug.",
                            src.id
                        ),
                        &[&src.id, region],
                    );
                    continue;
                };
                let Some(&from) = meta.splicer_out_index.get(&src.id) else {
                    warn(
                        format!(
                            "Splicer-out \"{}\" not indexed — internal compile bug.",
                            src.id
                        ),
                        &[&src.id, region],
                    );
                    continue;
                };
                let Some(to) = in_idx(&dst_inputs, &e.to_port) else {
                    warn(
                        format!(
                            "Unknown input port \"{}\" on \"{}\" — wire dropped.",
                            e.to_port, e.to
                        ),
                        &[&dst.id],
                    );
                    continue;
                };
                runtime_edges.push(RuntimeEdge {
                    from: idx(region),
                    from_port: from,
                    to: idx(&e.to),
                    to_port: to,
                });
                continue;
            }
        }
        // Accumulator Read init: outer → region input
        if dst_spec.region_role == Some(RegionRole::AccumulatorRead) && e.to_port == "init" {
            if let Some(region) = dst.parent.as_deref().filter(|r| parent_of(r) == src_scope) {
                let Some(meta) = region_meta.get(region) else {
                    warn(
                        format!(
                            "AccRead \"{}\" has no region meta — internal compile bug.",
                            dst.id
                        ),
                        &[&dst.id, region],
                    );
                    continue;
                };
                let Some(from) = out_idx(src_spec, &e.from_port) else {
                    warn(
                        format!(
                            "Unknown output port \"{}\" on \"{}\" — wire dropped.",
                            e.from_port, e.from
                        ),
                        &[&src.id],
                    );
                    continue;
                };
                let Some(&acc) = meta.acc_index.get(&dst.id) else {
                    warn(
                        format!(
                            "AccRead \"{}\" not paired with a Write — wire dropped.",
                            dst.id
                        ),
                        &[&dst.id, region],
                    );
                    continue;
                };
                let to = resolved_inputs(region).len() + meta.splicer_in.len() + acc;
                runtime_edges.push(RuntimeEdge {
                    from: idx(&e.from),
                    from_port: from,
                    to: idx(region),
                    to_port: to,
                });
                continue;
            }
        }
        // Accumulator Write final: region output → outer
        if src_spec.region_role == Some(RegionRole::AccumulatorWrite) && e.from_port == "final" {
            if let Some(region) = src.parent.as_deref().filter(|r| parent_of(r) == dst_scope) {
                let Some(meta) = region_meta.get(region) else {
                    warn(
                        format!(
                            "AccWrite \"{}\" has no region meta — internal compile bug.",
                            src.id
                        ),
                        &[&src.id, region],
                    );
                    continue;
                };
                let Some(&acc) = meta.acc_index.get(&src.id) else {
                    warn(
                        format!(
                            "AccWrite \"{}\" not paired with a Read — wire dropped.",
                            src.id
                        ),
                        &[&src.id, region],
                    );
                    continue;
                };
                let from = meta.splicer_out.len() + acc;
                let Some(to) = in_idx(&dst_inputs, &e.to_port) else {
                    warn(
                        format!(
                            "Unknown input port \"{}\" on \"{}\" — wire dropped.",
                            e.to_port, e.to
                        ),
                        &[&dst.id],
                    );
                    continue;
                };
                runtime_edges.push(RuntimeEdge {
                    from: idx(region),
                    from_port: from,
                    to: idx(&e.to),
                    to_port: to,
                });
                continue;
            }
        }

        let outer_to_inner = src_scope.is_none() && dst.parent.is_some();
        let inner_to_outer = src.parent.is_some() && dst_scope.is_none();
        if outer_to_inner {
            let Some(from) = out_idx(src_spec, &e.from_port) else {
                warn(
                    format!(
                        "Unknown output port \"{}\" on \"{}\" — wire dropped.",
                        e.from_port, e.from
                    ),
                    &[&src.id],
                );
                continue;
            };
            let Some(to) = in_idx(&dst_inputs, &e.to_port) else {
                warn(
                    format!(
                        "Unknown input port \"{}\" on \"{}\" — wire dropped.",
                        e.to_port, e.to
                    ),
                    &[&dst.id],
                );
                continue;
            };
            if src_spec.outputs[from].kind == PortKind::Spread
                && dst_inputs[to].kind == PortKind::Single
            {
                warn(
                    format!(
                        "Spread crossing into \"{}.{}\" needs a splicer-in inside the region.",
                        e.to, e.to_port
                    ),
                    &[&src.id, &dst.id],
                );
                continue;
            }
            runtime_edges.push(RuntimeEdge {
                from: idx(&e.from),
                from_port: from,
                to: idx(&e.to),
                to_port: to,
            });
            continue;
        }
        if inner_to_outer {
            warn(
                format!(
                    "Wire from inside the region to \"{}\" needs a splicer-out.",
                    e.to
                ),
                &[&src.id, &dst.id],
            );
            continue;
        }
        warn(
            format!(
                "Cross-region wire from \"{}\" to \"{}\" is not supported.",
                e.from, e.to
            ),
            &[&src.id, &dst.id],
        );
    }

    let renderer_ids: Vec<NodeIdx> = nodes
        .iter()
        .filter(|n| n.def_type == "render/renderer" && n.parent.is_none())
        .filter_map(|n| index.get(&n.id).copied())
        .collect();

    let mut graph = build_graph(runtime_nodes, index, runtime_edges, renderer_ids)?;

    graph.always_animate = nodes.iter().any(|n| {
        if ALWAYS_LIVE_TYPES.contains(&n.def_type.as_str()) {
            return true;
        }
        cat.get(&n.def_type).is_some_and(|s| {
            s.inputs.iter().any(|p| p.ty == PortType::Audio)
                || s.outputs.iter().any(|p| p.ty == PortType::Audio)
        })
    });

    Ok(CompileResult {
        graph,
        warnings,
        unimplemented,
    })
}

const ALWAYS_LIVE_TYPES: &[&str] = &[
    "time/seconds",
    "time/frame",
    "time/delta",
    "motion/smooth",
    "motion/spring",
    "motion/trigger",
    "motion/delay-line",
    "state/value",
    "state/spread",
    "state/read",
    "state/write",
    "state/read-spread",
    "state/write-spread",
    "input/mouse",
    "input/keyboard",
    "input/midi-cc",
    "input/midi-note",
    "input/camera",
    "input/landmarks",
    "io/metro",
    "io/receive",
];

/// `Number(params.slot ?? 1)`, falling back to 1 when not finite.
fn slot_of(params: &Params) -> f64 {
    let v = params
        .get("slot")
        .map(Value::from_json)
        .unwrap_or(Value::Num(1.0));
    match v {
        Value::Undefined | Value::Null => 1.0,
        other => other.to_number(),
    }
}

/// Format a number like JS `String(n)` for messages (integers without `.0`).
fn js_num(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e21 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn to_runtime_node(
    cat: &Catalogue,
    spec: &NodeSpec,
    n: &NodeRef,
    store: &ParamStore,
    region: Option<(usize, usize, usize)>,
) -> Result<RuntimeNode, String> {
    let resolved = cat.resolved_inputs(spec, &n.params);
    let mut node = RuntimeNode {
        id: n.id.clone(),
        ty: spec.ty.clone(),
        in_arity: resolved.len(),
        out_arity: spec.outputs.len(),
        defaults: resolved.iter().map(default_for_port).collect(),
        sources: None,
        parent: None,
        region_kind: None,
        region_static_in: 0,
        role: None,
        acc_slot: 1.0,
        stateful: false,
        stateful_next_port: 1,
        multi: vec![false; resolved.len()],
        constant: vec![false; resolved.len()],
        constants: vec![None; resolved.len()],
        imp: None,
        is_context: false,
        memoise: false,
        unimplemented: false,
        saved_params: n.params.clone(),
        state_value: None,
        cache: None,
    };

    if let Some(kind) = spec.region_kind {
        if kind == RegionKind::Subpatch {
            return Err(format!(
                "subpatch {} should have been flattened pre-compile",
                n.id
            ));
        }
        let (n_in, n_out, n_acc) = region.unwrap_or((0, 0, 0));
        let static_in = resolved.len();
        node.in_arity = static_in + n_in + n_acc;
        node.out_arity = n_out + n_acc;
        node.defaults.resize(node.in_arity, empty());
        node.multi = vec![false; node.in_arity];
        node.constant = vec![false; node.in_arity];
        node.constants = vec![None; node.in_arity];
        node.region_kind = Some(kind);
        node.region_static_in = static_in;
        return Ok(node);
    }

    let is_role = spec.region_role.is_some();
    let is_state_read = spec.ty == "state/read" || spec.ty == "state/read-spread";
    if !is_role && !spec.stateful && !is_state_read {
        match spec.eval {
            EvalKind::Context | EvalKind::Scalar | EvalKind::Raw => {
                let init = NodeInit {
                    id: &n.id,
                    spec,
                    params: &n.params,
                    inputs: &resolved,
                };
                match nodes::factory(&spec.ty) {
                    Some(f) => node.imp = Some(f(&init)),
                    None => node.unimplemented = true,
                }
                node.is_context = spec.eval == EvalKind::Context;
                // TS `memoisable`: a pure compute with no audio port (Web
                // Audio handles mutate every tick) and no image port (a
                // canvas's pixels change under the same reference).
                let media = resolved
                    .iter()
                    .chain(&spec.outputs)
                    .any(|p| matches!(p.ty, PortType::Audio | PortType::Image));
                node.memoise = !node.is_context
                    && !spec.effectful
                    && !media
                    && node.imp.as_ref().is_some_and(|imp| imp.memoise());
            }
            EvalKind::None => {
                if !(spec.inputs.is_empty() && spec.outputs.is_empty()) {
                    return Err(format!(
                        "NodeDef {} has no compute / computeRaw / readFromContext / regionKind / regionRole",
                        spec.ty
                    ));
                }
                // Editor-only annotation: a no-op compute.
                node.imp = Some(crate::node::raw(|_, _| Ok(Vec::new())));
            }
        }
    }

    if n.bypassed && crate::bypass::can_bypass(spec) {
        let outputs = cat.resolved_outputs(spec, &n.params);
        node.imp = Some(Box::new(crate::bypass::Bypass::new(&resolved, &outputs)));
        node.is_context = false;
        node.memoise = false;
        node.unimplemented = false;
    }

    node.role = spec.region_role;
    if matches!(
        spec.region_role,
        Some(RegionRole::AccumulatorRead | RegionRole::AccumulatorWrite)
    ) {
        let slot = slot_of(&n.params);
        node.acc_slot = if slot.is_finite() { slot } else { 1.0 };
    }
    if spec.stateful {
        node.stateful = true;
        node.stateful_next_port = spec.stateful_next_port.unwrap_or(1);
    }
    for (i, p) in resolved.iter().enumerate() {
        node.multi[i] = p.kind == PortKind::Multi;
        node.constant[i] = p.constant;
    }

    // defaultOverride: the app always supplies a portValue reader, so every
    // non-region node has one.
    let sources: Vec<InputSource> = resolved
        .iter()
        .map(|p| InputSource {
            port_value: n
                .port_values
                .as_ref()
                .and_then(|pv| pv.get(&p.name))
                .map(Value::from_json),
            param: p.param.clone(),
        })
        .collect();
    for (i, src) in sources.iter().enumerate() {
        if node.constant[i] {
            node.constants[i] =
                Some(source_value(src, &n.id, store).unwrap_or_else(|| node.defaults[i].clone()));
        }
    }
    node.sources = Some(sources);
    Ok(node)
}

/// portValue override → live param → none (caller falls back to the default).
pub fn source_value(src: &InputSource, node_id: &str, store: &ParamStore) -> Option<Spread> {
    if let Some(v) = &src.port_value {
        return Some(one(v.clone()));
    }
    let key = src.param.as_ref()?;
    store.get(node_id, key).map(|j| one(Value::from_json(j)))
}

/// Default for an unwired input (`defaultForPort`).
pub fn default_for_port(p: &PortSpec) -> Spread {
    match p.kind {
        PortKind::Spread | PortKind::Multi => match &p.default {
            Some(d) => one(d.clone()),
            None => empty(),
        },
        PortKind::Single => one(match &p.default {
            Some(d) => d.clone(),
            None => default_by_type(p.ty),
        }),
    }
}

fn default_by_type(t: PortType) -> Value {
    match t {
        PortType::Number => Value::Num(0.0),
        PortType::Boolean => Value::Bool(false),
        PortType::String => Value::str(""),
        PortType::Vec2 => Value::Vec2(Vec2::default()),
        PortType::Color => Value::Color(Color::WHITE),
        PortType::Mat3 => Value::List(Arc::from(
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
                .map(Value::Num)
                .to_vec(),
        )),
        PortType::Layer | PortType::Style | PortType::Audio | PortType::Image | PortType::Any => {
            Value::Null
        }
        PortType::World | PortType::Force | PortType::Collider | PortType::Particle => {
            Value::Undefined
        }
    }
}

// ─── Pass 0: region frame ports ──────────────────────────────────────────────

fn expand_region_ports(
    nodes: Vec<NodeRef>,
    edges: Vec<EdgeRef>,
    cat: &Catalogue,
) -> (Vec<NodeRef>, Vec<EdgeRef>) {
    let mut out_nodes = nodes.clone();
    let mut ports_by_region: HashMap<String, HashMap<String, RegionPort>> = HashMap::new();
    let mut loop_var_regions: std::collections::HashSet<String> = Default::default();
    for n in &nodes {
        let Some(kind) = cat.get(&n.def_type).and_then(|s| s.region_kind) else {
            continue;
        };
        if matches!(kind, RegionKind::Foreach | RegionKind::Cache) {
            loop_var_regions.insert(n.id.clone());
        }
        if n.ports.is_empty() {
            continue;
        }
        let mut map = HashMap::new();
        for p in &n.ports {
            map.insert(p.id.clone(), p.clone());
            let def_type = match p.direction {
                PortDirection::Input => format!("region/splicer-in/{}", p.ty),
                PortDirection::Output => format!("region/splicer-out/{}", p.ty),
            };
            out_nodes.push(NodeRef {
                id: synthetic_splicer_id(&n.id, &p.id, p.direction),
                def_type,
                params: Params::new(),
                parent: Some(n.id.clone()),
                ports: Vec::new(),
                port_values: None,
                bypassed: false,
            });
        }
        ports_by_region.insert(n.id.clone(), map);
    }

    let mut loop_var_refs: IndexMap<String, indexmap::IndexSet<&'static str>> = IndexMap::new();
    let mut out_edges = Vec::with_capacity(edges.len());
    for e in edges {
        let mut from = e.from.clone();
        let mut from_port = e.from_port.clone();
        let mut to = e.to.clone();
        let mut to_port = e.to_port.clone();

        if loop_var_regions.contains(&from) {
            if let Some(lv) = parse_loop_var_handle(&from_port) {
                loop_var_refs.entry(from.clone()).or_default().insert(lv);
                from = loop_var_id(&e.from, lv);
                from_port = "inner".into();
            }
        }
        if let Some(port) = ports_by_region.get(&from).and_then(|m| m.get(&from_port)) {
            from = synthetic_splicer_id(&e.from, &port.id, port.direction);
            from_port = match port.direction {
                PortDirection::Input => "inner",
                PortDirection::Output => "outer",
            }
            .into();
        }
        if let Some(port) = ports_by_region.get(&to).and_then(|m| m.get(&to_port)) {
            to = synthetic_splicer_id(&e.to, &port.id, port.direction);
            to_port = match port.direction {
                PortDirection::Input => "outer",
                PortDirection::Output => "inner",
            }
            .into();
        }
        out_edges.push(EdgeRef {
            from,
            from_port,
            to,
            to_port,
        });
    }
    for (region, names) in loop_var_refs {
        for name in names {
            out_nodes.push(NodeRef {
                id: loop_var_id(&region, name),
                def_type: "region/loop-var".into(),
                params: Params::new(),
                parent: Some(region.clone()),
                ports: Vec::new(),
                port_values: None,
                bypassed: false,
            });
        }
    }
    (out_nodes, out_edges)
}

// ─── Pass -2: sections ───────────────────────────────────────────────────────

/// Sections are canvas backdrops: drop them and hand their children to the
/// enclosing scope, so they compute exactly as if no section were there.
fn flatten_sections(
    nodes: Vec<NodeRef>,
    edges: Vec<EdgeRef>,
    cat: &Catalogue,
) -> (Vec<NodeRef>, Vec<EdgeRef>) {
    let parent_of: HashMap<String, Option<String>> = nodes
        .iter()
        .filter(|n| cat.get(&n.def_type).and_then(|s| s.region_kind) == Some(RegionKind::Section))
        .map(|n| (n.id.clone(), n.parent.clone()))
        .collect();
    if parent_of.is_empty() {
        return (nodes, edges);
    }
    // The nearest ancestor that isn't a section (sections may nest).
    let lift = |mut p: Option<String>| {
        for _ in 0..64 {
            match p.as_ref().and_then(|id| parent_of.get(id)) {
                Some(up) => p = up.clone(),
                None => break,
            }
        }
        p
    };
    let nodes = nodes
        .into_iter()
        .filter(|n| !parent_of.contains_key(&n.id))
        .map(|mut n| {
            n.parent = lift(n.parent.take());
            n
        })
        .collect();
    let edges = edges
        .into_iter()
        .filter(|e| !parent_of.contains_key(&e.from) && !parent_of.contains_key(&e.to))
        .collect();
    (nodes, edges)
}

// ─── Pass -1: subpatches ─────────────────────────────────────────────────────

fn flatten_subpatches(
    mut nodes: Vec<NodeRef>,
    mut edges: Vec<EdgeRef>,
    cat: &Catalogue,
) -> (Vec<NodeRef>, Vec<EdgeRef>) {
    for _ in 0..10_000 {
        let Some(si) = nodes.iter().position(|n| {
            cat.get(&n.def_type).and_then(|s| s.region_kind) == Some(RegionKind::Subpatch)
        }) else {
            break;
        };
        let sub = nodes[si].clone();
        let port_ids: indexmap::IndexSet<&str> = sub.ports.iter().map(|p| p.id.as_str()).collect();
        let mut supplies: IndexMap<&str, Vec<EdgeRef>> =
            port_ids.iter().map(|&p| (p, Vec::new())).collect();
        let mut consumers: IndexMap<&str, Vec<EdgeRef>> =
            port_ids.iter().map(|&p| (p, Vec::new())).collect();
        let mut other = Vec::new();
        for e in edges {
            if e.to == sub.id && port_ids.contains(e.to_port.as_str()) {
                supplies.get_mut(e.to_port.as_str()).expect("port").push(e);
            } else if e.from == sub.id && port_ids.contains(e.from_port.as_str()) {
                consumers
                    .get_mut(e.from_port.as_str())
                    .expect("port")
                    .push(e);
            } else if e.from == sub.id || e.to == sub.id {
                // dangling: drop
            } else {
                other.push(e);
            }
        }
        for pid in &port_ids {
            for s in &supplies[pid] {
                for c in &consumers[pid] {
                    other.push(EdgeRef {
                        from: s.from.clone(),
                        from_port: s.from_port.clone(),
                        to: c.to.clone(),
                        to_port: c.to_port.clone(),
                    });
                }
            }
        }
        edges = other;
        nodes = nodes
            .into_iter()
            .filter(|n| n.id != sub.id)
            .map(|mut n| {
                if n.parent.as_deref() == Some(sub.id.as_str()) {
                    n.parent = sub.parent.clone();
                }
                n
            })
            .collect();
    }
    (nodes, edges)
}
