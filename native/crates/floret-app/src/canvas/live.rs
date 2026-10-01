//! What the running engine says about the patch: pin values for tooltips
//! and output readouts, region iteration badges, and warning chips.

use super::geom::{Decor, PinRef};
use crate::editor::ports::{self, find_node, Side};
use crate::editor::{regions, subpatch, values, widgets};
use floret_engine::compile::{loop_var_id, synthetic_splicer_id};
use floret_engine::patch::{PatchFile, PortDirection};
use floret_engine::spec::{Catalogue, RegionKind};
use floret_engine::tick::{Outputs, RegionStats};
use floret_engine::{CompileWarning, Engine, Spread, Value};
use std::collections::{HashMap, HashSet};

/// The engine state the canvas reads each frame.
pub struct Live<'a> {
    pub engine: &'a Engine,
    pub outputs: &'a Outputs,
    pub region_stats: &'a HashMap<usize, RegionStats>,
    pub warnings: &'a [CompileWarning],
    /// Node compute errors from the last tick.
    pub errors: &'a [(usize, String)],
}

/// A pin's value: live from the engine, or the static value an unwired
/// input falls back to.
pub enum PinValue {
    Live(Spread),
    Static(serde_json::Value),
}

impl PinValue {
    pub fn text(&self) -> String {
        match self {
            PinValue::Live(s) => values::tooltip(s),
            PinValue::Static(j) => values::tooltip(&floret_engine::value::one(Value::from_json(j))),
        }
    }
}

impl Live<'_> {
    fn by_id(&self, id: &str, port: usize) -> Option<Spread> {
        let idx = self.engine.graph.idx(id)?;
        self.outputs.get(idx)?.as_ref()?.get(port).cloned()
    }

    /// The spread at a node's output pin. Region frame pins read their
    /// synthetic splicers; subpatches are flattened at compile, so their
    /// pins pass through whatever feeds them.
    pub fn output(
        &self,
        patch: &PatchFile,
        cat: &Catalogue,
        node: &str,
        port: &str,
    ) -> Option<Spread> {
        self.output_depth(patch, cat, node, port, 0)
    }

    fn output_depth(
        &self,
        patch: &PatchFile,
        cat: &Catalogue,
        node: &str,
        port: &str,
        depth: usize,
    ) -> Option<Spread> {
        if depth > 32 {
            return None;
        }
        let n = find_node(patch, node)?;
        let spec = cat.get(&n.def_type)?;
        match spec.region_kind {
            Some(RegionKind::Subpatch) => self.feed(patch, cat, node, port, depth + 1),
            Some(_) => {
                if let Some(v) = port.strip_prefix("__loopvar_") {
                    return self.by_id(&loop_var_id(node, v), 0);
                }
                let p = n.ports.iter().flatten().find(|p| p.id == port)?;
                self.by_id(&synthetic_splicer_id(node, port, p.direction), 0)
            }
            None => {
                let k = ports::resolved_outputs(spec, &n.params)
                    .iter()
                    .position(|p| p.name == port)?;
                self.by_id(node, k)
            }
        }
    }

    /// What reaches input `port` of `node` over a wire.
    fn feed(
        &self,
        patch: &PatchFile,
        cat: &Catalogue,
        node: &str,
        port: &str,
        depth: usize,
    ) -> Option<Spread> {
        let handle = Side::In.handle(port);
        let e = patch
            .edges
            .iter()
            .find(|e| e.target == node && e.target_handle.as_deref() == Some(handle.as_str()))?;
        let (_, sname) = ports::split_handle(e.source_handle.as_deref())?;
        self.output_depth(patch, cat, &e.source, sname, depth)
    }

    pub fn pin_value(&self, patch: &PatchFile, cat: &Catalogue, pin: &PinRef) -> Option<PinValue> {
        if pin.side == Side::Out {
            return self
                .output(patch, cat, &pin.node, &pin.port)
                .map(PinValue::Live);
        }
        if let Some(v) = self.feed(patch, cat, &pin.node, &pin.port, 0) {
            return Some(PinValue::Live(v));
        }
        let n = find_node(patch, &pin.node)?;
        let spec = cat.get(&n.def_type)?;
        if spec.region_kind.is_some() {
            return None;
        }
        let port = cat
            .resolved_inputs(spec, &n.params)
            .into_iter()
            .find(|p| p.name == pin.port)?;
        let key = match &port.param {
            Some(k) => values::EditKey::Param(k.clone()),
            None => values::EditKey::Port(port.name.clone()),
        };
        // A portValue overrides the param at runtime.
        let v = n
            .port_values
            .as_ref()
            .and_then(|pv| pv.get(&port.name))
            .cloned()
            .unwrap_or_else(|| values::current(n, spec, &key, Some(&port)));
        Some(PinValue::Static(v))
    }

    /// Warning chips (compile warnings + tick errors, on the visible node
    /// standing in for each) and region iteration badges.
    pub fn decor(&self, patch: &PatchFile, cat: &Catalogue, visible: &HashSet<String>) -> Decor {
        let mut warn: HashMap<String, Vec<String>> = HashMap::new();
        let mut add = |id: &str, msg: &str| {
            if let Some(at) = subpatch::visible_stand_in(patch, id, visible) {
                let list = warn.entry(at).or_default();
                if !list.iter().any(|m| m == msg) {
                    list.push(msg.to_owned());
                }
            }
        };
        for w in self.warnings {
            for id in &w.node_ids {
                add(id, &w.message);
            }
        }
        for (idx, msg) in self.errors {
            if let Some(n) = self.engine.graph.nodes.get(*idx) {
                add(&n.id, msg);
            }
        }
        let mut badges = HashMap::new();
        for id in visible {
            if !regions::is_drawable_node(patch, cat, id) {
                continue;
            }
            let Some(stats) = self
                .engine
                .graph
                .idx(id)
                .and_then(|i| self.region_stats.get(&i))
            else {
                continue;
            };
            let names: Vec<String> = find_node(patch, id)
                .into_iter()
                .flat_map(|n| n.ports.iter().flatten())
                .filter(|p| p.direction == PortDirection::Input)
                .map(|p| p.name.clone())
                .collect();
            let lens = stats
                .input_lengths
                .iter()
                .enumerate()
                .map(|(i, l)| format!("{}={l}", names.get(i).map_or("?", String::as_str)))
                .collect::<Vec<_>>()
                .join(", ");
            let (hint, capped) = match stats.bottleneck {
                Some(b) => (
                    format!(
                        "capped by {} ({lens})",
                        names.get(b).map_or("?", String::as_str)
                    ),
                    true,
                ),
                None => (lens, false),
            };
            badges.insert(id.clone(), (format!("×{}", stats.iterations), hint, capped));
        }
        let probes = patch
            .nodes
            .iter()
            .filter(|n| n.def_type == "meta/probe" && visible.contains(&n.id))
            .map(|n| {
                let port = self.by_id(&n.id, 0);
                (n.id.clone(), widgets::probe_view(port.as_ref()))
            })
            .collect();
        Decor {
            warnings: warn.into_iter().map(|(k, v)| (k, v.join("\n"))).collect(),
            badges,
            probes,
        }
    }
}
