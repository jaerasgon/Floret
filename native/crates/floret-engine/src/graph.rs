//! The compiled, runnable graph.

use crate::node::NodeImpl;
use crate::spec::{Params, RegionKind, RegionRole};
use crate::value::{Spread, Value};
use indexmap::IndexMap;
use std::collections::HashMap;

pub type NodeIdx = usize;

/// How an unwired input finds its value each tick (`defaultOverride`).
#[derive(Clone, Debug, Default)]
pub struct InputSource {
    /// Per-instance constant from the patch (`portValues`).
    pub port_value: Option<Value>,
    /// Inline-editor param read live from the param store.
    pub param: Option<String>,
}

pub struct RuntimeNode {
    pub id: String,
    pub ty: String,
    pub in_arity: usize,
    pub out_arity: usize,
    pub defaults: Vec<Spread>,
    /// Present for non-region nodes (TS: `defaultOverride`).
    pub sources: Option<Vec<InputSource>>,
    pub parent: Option<NodeIdx>,
    pub region_kind: Option<RegionKind>,
    /// Static inputs declared by the region def; splicer-ins come after.
    pub region_static_in: usize,
    pub role: Option<RegionRole>,
    /// Accumulator pairing slot.
    pub acc_slot: f64,
    pub stateful: bool,
    pub stateful_next_port: usize,
    pub multi: Vec<bool>,
    pub constant: Vec<bool>,
    /// Compile-time values for constant ports (wired ones fill on first tick).
    pub constants: Vec<Option<Spread>>,
    /// Compute / source implementation. `None` = no compute.
    pub imp: Option<Box<dyn NodeImpl>>,
    /// Source node: called with no inputs, never gathered.
    pub is_context: bool,
    /// True when the node type exists but has no Rust implementation yet.
    pub unimplemented: bool,
    pub saved_params: Params,
    /// Cross-frame state (stateful nodes).
    pub state_value: Option<Spread>,
    /// Cache region: hash of last inputs + outputs.
    pub cache: Option<(String, Vec<Spread>)>,
}

impl RuntimeNode {
    pub fn is_state_read(&self) -> bool {
        self.ty == "state/read" || self.ty == "state/read-spread"
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeEdge {
    pub from: NodeIdx,
    pub from_port: usize,
    pub to: NodeIdx,
    pub to_port: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Scope {
    pub parent: Option<NodeIdx>,
    pub order: Vec<NodeIdx>,
    pub splicer_in: Vec<NodeIdx>,
    pub splicer_out: Vec<NodeIdx>,
    /// Accumulator pairs: anchor (Read) indices, in declaration order.
    pub accumulators: Vec<NodeIdx>,
    /// Anchor → (read, write).
    pub acc_pairs: HashMap<NodeIdx, (NodeIdx, NodeIdx)>,
}

pub struct RuntimeGraph {
    pub nodes: Vec<RuntimeNode>,
    pub index: HashMap<String, NodeIdx>,
    /// Keyed by region (None = root), in first-seen order.
    pub scopes: IndexMap<Option<NodeIdx>, Scope>,
    /// All edges, grouped by target node, in insertion order.
    pub edges_by_target: Vec<Vec<RuntimeEdge>>,
    pub renderer_ids: Vec<NodeIdx>,
    pub always_animate: bool,
}

impl RuntimeGraph {
    pub fn idx(&self, id: &str) -> Option<NodeIdx> {
        self.index.get(id).copied()
    }

    pub fn renderer(&self) -> Option<NodeIdx> {
        self.renderer_ids.first().copied()
    }
}

/// `buildRuntimeGraph`: group by scope, topo-sort each scope.
pub fn build_graph(
    nodes: Vec<RuntimeNode>,
    index: HashMap<String, NodeIdx>,
    edges: Vec<RuntimeEdge>,
    renderer_ids: Vec<NodeIdx>,
) -> Result<RuntimeGraph, String> {
    let n = nodes.len();

    let mut by_scope: IndexMap<Option<NodeIdx>, Vec<NodeIdx>> = IndexMap::new();
    for (i, node) in nodes.iter().enumerate() {
        by_scope.entry(node.parent).or_default().push(i);
    }
    by_scope.entry(None).or_default();

    let mut edges_by_target: Vec<Vec<RuntimeEdge>> = vec![Vec::new(); n];
    for e in &edges {
        edges_by_target[e.to].push(*e);
    }

    let mut adj: Vec<Vec<NodeIdx>> = vec![Vec::new(); n];
    let mut in_deg: Vec<usize> = vec![0; n];

    let ancestor_in_scope = |start: NodeIdx, target: Option<NodeIdx>| -> Option<NodeIdx> {
        let mut cur = Some(start);
        while let Some(c) = cur {
            if nodes[c].parent == target {
                return Some(c);
            }
            cur = nodes[c].parent;
        }
        None
    };

    for e in &edges {
        let src = &nodes[e.from];
        let dst = &nodes[e.to];
        if src.parent == dst.parent {
            if dst.role == Some(RegionRole::AccumulatorWrite) {
                continue;
            }
            if dst.stateful && e.to_port == dst.stateful_next_port {
                continue;
            }
            adj[e.from].push(e.to);
            in_deg[e.to] += 1;
            continue;
        }
        if let Some(dst_anc) = ancestor_in_scope(e.to, src.parent) {
            if dst_anc != e.from && !adj[e.from].contains(&dst_anc) {
                adj[e.from].push(dst_anc);
                in_deg[dst_anc] += 1;
            }
        } else if let Some(src_anc) = ancestor_in_scope(e.from, dst.parent) {
            if !adj[src_anc].contains(&e.to) {
                adj[src_anc].push(e.to);
                in_deg[e.to] += 1;
            }
        }
    }

    let mut scopes = IndexMap::new();
    for (key, ids) in &by_scope {
        let mut order = Vec::with_capacity(ids.len());
        let mut queue: std::collections::VecDeque<NodeIdx> =
            ids.iter().copied().filter(|&i| in_deg[i] == 0).collect();
        while let Some(i) = queue.pop_front() {
            order.push(i);
            for &next in &adj[i] {
                in_deg[next] -= 1;
                if in_deg[next] == 0 {
                    queue.push_back(next);
                }
            }
        }
        if order.len() != ids.len() {
            let remaining: Vec<&str> = ids
                .iter()
                .filter(|i| !order.contains(i))
                .map(|&i| nodes[i].id.as_str())
                .collect();
            let key_name = key.map_or("null".to_owned(), |k| nodes[k].id.clone());
            return Err(format!(
                "cycle in scope {key_name}: {}",
                remaining.join(", ")
            ));
        }

        let mut scope = Scope {
            parent: *key,
            order,
            ..Default::default()
        };
        if key.is_some() {
            let mut reads: IndexMap<u64, NodeIdx> = IndexMap::new();
            let mut writes: HashMap<u64, NodeIdx> = HashMap::new();
            for &i in ids {
                match nodes[i].role {
                    Some(RegionRole::SplicerIn) => scope.splicer_in.push(i),
                    Some(RegionRole::SplicerOut) => scope.splicer_out.push(i),
                    Some(RegionRole::AccumulatorRead) => {
                        reads.insert(nodes[i].acc_slot.to_bits(), i);
                    }
                    Some(RegionRole::AccumulatorWrite) => {
                        writes.insert(nodes[i].acc_slot.to_bits(), i);
                    }
                    _ => {}
                }
            }
            for (slot, read) in reads {
                if let Some(&write) = writes.get(&slot) {
                    scope.accumulators.push(read);
                    scope.acc_pairs.insert(read, (read, write));
                }
            }
        }
        scopes.insert(*key, scope);
    }

    Ok(RuntimeGraph {
        nodes,
        index,
        scopes,
        edges_by_target,
        renderer_ids,
        always_animate: false,
    })
}
