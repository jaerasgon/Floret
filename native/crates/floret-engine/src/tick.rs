//! Per-frame evaluation. A port of `src/runtime/tick.ts`.

use crate::compile::{loop_var_id, source_value};
use crate::graph::{NodeIdx, RuntimeGraph, RuntimeNode, Scope};
use crate::node::{EvalCtx, FrameCtx, ParamStore};
use crate::spec::{RegionKind, RegionRole};
use crate::value::{empty, one, Spread, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Node outputs for one tick, indexed by node. `None` = not evaluated.
pub type Outputs = Vec<Option<Arc<Vec<Spread>>>>;

#[derive(Clone, Debug, PartialEq)]
pub struct RegionStats {
    pub iterations: usize,
    pub input_lengths: Vec<usize>,
    /// Splicer-in that limited iteration, or `None`.
    pub bottleneck: Option<usize>,
}

pub struct TickResult {
    pub outputs: Outputs,
    pub region_stats: HashMap<NodeIdx, RegionStats>,
    /// Errors raised by node computes this tick: (node index, message).
    pub errors: Vec<(NodeIdx, String)>,
}

impl TickResult {
    /// Layers reaching a renderer (its first output).
    pub fn layers(&self, renderer: NodeIdx) -> Spread {
        self.outputs[renderer]
            .as_ref()
            .and_then(|o| o.first().cloned())
            .unwrap_or_else(empty)
    }
}

/// Bindings active while evaluating one iteration of a region.
struct IterCtx {
    /// Splicer-in / loop-var node → this iteration's value (a spread).
    splicer_in: HashMap<NodeIdx, Spread>,
    /// Accumulator anchor → current value.
    acc: HashMap<NodeIdx, Value>,
}

/// The engine's mutable state across ticks: the compiled graph plus the
/// live param store.
pub struct Engine {
    pub graph: RuntimeGraph,
    pub params: ParamStore,
    /// Wired constant pins whose value has been captured (first tick).
    constants_wired: bool,
}

impl Engine {
    pub fn new(graph: RuntimeGraph, params: ParamStore) -> Engine {
        Engine {
            graph,
            params,
            constants_wired: false,
        }
    }

    pub fn tick(&mut self, frame: &FrameCtx) -> TickResult {
        let mut t = Ticker {
            graph: &mut self.graph,
            params: &mut self.params,
            tick_frame: frame,
            stats: HashMap::new(),
            errors: Vec::new(),
            slot_writers: HashMap::new(),
            capture_constants: !self.constants_wired,
        };
        t.slot_writers = t.build_slot_writers();
        let mut outputs: Outputs = vec![None; t.graph.nodes.len()];
        if let Some(root) = t.graph.scopes.get(&None).cloned() {
            t.eval_scope(&root, frame, &mut outputs, None);
            t.capture_stateful_next(&root, &outputs);
        }
        self.constants_wired = true;
        let (stats, errors) = (t.stats, t.errors);
        TickResult {
            outputs,
            region_stats: stats,
            errors,
        }
    }
}

struct Ticker<'a> {
    graph: &'a mut RuntimeGraph,
    params: &'a mut ParamStore,
    tick_frame: &'a FrameCtx,
    stats: HashMap<NodeIdx, RegionStats>,
    errors: Vec<(NodeIdx, String)>,
    /// `writeType:slot` → state/write node.
    slot_writers: HashMap<String, NodeIdx>,
    capture_constants: bool,
}

fn slot_key(v: Option<&serde_json::Value>) -> Option<String> {
    v.map(|j| match j {
        serde_json::Value::String(s) => s.clone(),
        other => crate::value::Value::from_json(other)
            .to_number()
            .to_string(),
    })
}

impl Ticker<'_> {
    fn build_slot_writers(&self) -> HashMap<String, NodeIdx> {
        let mut w = HashMap::new();
        for (i, n) in self.graph.nodes.iter().enumerate() {
            if n.ty == "state/write" || n.ty == "state/write-spread" {
                if let Some(slot) = slot_key(self.params.get(&n.id, "slot")) {
                    w.insert(format!("{}:{slot}", n.ty), i);
                }
            }
        }
        w
    }

    fn capture_stateful_next(&mut self, root: &Scope, outputs: &Outputs) {
        for &i in &root.order {
            let n = &self.graph.nodes[i];
            if !n.stateful {
                continue;
            }
            let next = n.stateful_next_port;
            if !self.graph.edges_by_target[i]
                .iter()
                .any(|e| e.to_port == next)
            {
                continue;
            }
            let inputs = self.gather(i, outputs);
            if let Some(v) = inputs.get(next) {
                self.graph.nodes[i].state_value = Some(v.clone());
            }
        }
    }

    fn eval_scope(
        &mut self,
        scope: &Scope,
        frame: &FrameCtx,
        outputs: &mut Outputs,
        iter: Option<&IterCtx>,
    ) {
        for &i in &scope.order {
            self.eval_node(i, frame, outputs, iter);
        }
    }

    fn set(outputs: &mut Outputs, i: NodeIdx, v: Vec<Spread>) {
        outputs[i] = Some(Arc::new(v));
    }

    fn run_compute(&mut self, i: NodeIdx, inputs: &[Spread], frame: &FrameCtx) -> Vec<Spread> {
        let node = &mut self.graph.nodes[i];
        let out_arity = node.out_arity;
        let Some(imp) = node.imp.as_mut() else {
            return vec![empty(); out_arity];
        };
        let mut cx = EvalCtx {
            node_id: &node.id,
            frame,
            tick_frame: self.tick_frame,
            params: self.params,
            saved_params: &node.saved_params,
        };
        match imp.eval(inputs, &mut cx) {
            Ok(out) => out,
            Err(msg) => {
                self.errors.push((i, msg));
                vec![empty(); out_arity]
            }
        }
    }

    fn eval_node(
        &mut self,
        i: NodeIdx,
        frame: &FrameCtx,
        outputs: &mut Outputs,
        iter: Option<&IterCtx>,
    ) {
        let node = &self.graph.nodes[i];

        if node.is_context {
            let out = self.run_compute(i, &[], frame);
            Self::set(outputs, i, out);
            return;
        }

        match node.role {
            Some(RegionRole::SplicerIn | RegionRole::LoopVar) => {
                let v = iter
                    .and_then(|c| c.splicer_in.get(&i).cloned())
                    .unwrap_or_else(empty);
                Self::set(outputs, i, vec![v]);
                return;
            }
            Some(RegionRole::SplicerOut) => {
                let inputs = self.gather(i, outputs);
                Self::set(
                    outputs,
                    i,
                    vec![inputs.first().cloned().unwrap_or_else(empty)],
                );
                return;
            }
            Some(RegionRole::AccumulatorRead) => {
                let cur = iter
                    .and_then(|c| c.acc.get(&i).cloned())
                    .unwrap_or(Value::Undefined);
                Self::set(outputs, i, vec![one(cur)]);
                return;
            }
            Some(RegionRole::AccumulatorWrite) => {
                Self::set(outputs, i, vec![one(Value::Undefined)]);
                return;
            }
            None => {}
        }

        if node.region_kind.is_some() {
            self.run_region(i, frame, outputs);
            return;
        }

        if node.is_state_read() {
            let write_ty = if node.ty == "state/read" {
                "state/write"
            } else {
                "state/write-spread"
            };
            let persisted = slot_key(self.params.get(&node.id, "slot"))
                .and_then(|slot| self.slot_writers.get(&format!("{write_ty}:{slot}")))
                .and_then(|&w| self.graph.nodes[w].state_value.clone());
            match persisted {
                Some(v) => Self::set(outputs, i, vec![v]),
                None => {
                    let inputs = self.gather(i, outputs);
                    Self::set(
                        outputs,
                        i,
                        vec![inputs.first().cloned().unwrap_or_else(empty)],
                    );
                }
            }
            return;
        }

        if node.stateful {
            if node.out_arity == 0 {
                Self::set(outputs, i, Vec::new());
                return;
            }
            match node.state_value.clone() {
                Some(v) => Self::set(outputs, i, vec![v]),
                None => {
                    let inputs = self.gather(i, outputs);
                    let init = inputs.first().cloned().unwrap_or_else(empty);
                    self.graph.nodes[i].state_value = Some(init.clone());
                    Self::set(outputs, i, vec![init]);
                }
            }
            return;
        }

        if node.imp.is_none() {
            let n = node.out_arity;
            Self::set(outputs, i, vec![empty(); n]);
            return;
        }
        let inputs = self.gather(i, outputs);
        let out = self.run_compute(i, &inputs, frame);
        Self::set(outputs, i, out);
    }

    fn run_region(&mut self, region: NodeIdx, frame: &FrameCtx, outputs: &mut Outputs) {
        let Some(inner) = self.graph.scopes.get(&Some(region)).cloned() else {
            let n = self.graph.nodes[region].out_arity;
            Self::set(outputs, region, vec![empty(); n]);
            self.stats.insert(
                region,
                RegionStats {
                    iterations: 0,
                    input_lengths: vec![],
                    bottleneck: None,
                },
            );
            return;
        };
        let outer = self.gather(region, outputs);
        let (kind, static_in) = {
            let r = &self.graph.nodes[region];
            (r.region_kind.expect("region"), r.region_static_in)
        };
        let get = |k: usize| outer.get(k).cloned().unwrap_or_else(empty);

        let mut delay = 0.0;
        if kind == RegionKind::Foreach {
            if let Some(Value::Num(d)) = get(0).first() {
                if d.is_finite() {
                    delay = *d;
                }
            }
        }
        let n_in = inner.splicer_in.len();
        let n_acc = inner.accumulators.len();
        let n_out = inner.splicer_out.len();
        let input_lengths: Vec<usize> = (0..n_in).map(|k| get(static_in + k).len()).collect();

        if kind == RegionKind::Cache {
            let hash = hash_inputs(&outer);
            if let Some((h, cached)) = &self.graph.nodes[region].cache {
                if *h == hash {
                    let it = input_lengths.iter().copied().min().unwrap_or(1);
                    Self::set(outputs, region, cached.clone());
                    self.stats.insert(
                        region,
                        RegionStats {
                            iterations: it,
                            input_lengths,
                            bottleneck: None,
                        },
                    );
                    return;
                }
            }
        }

        if kind == RegionKind::If {
            let cond = get(0).first().map(Value::truthy).unwrap_or(false);
            if !cond {
                let mut empties: Vec<Spread> = vec![empty(); n_out];
                for k in 0..n_acc {
                    empties.push(one(get(static_in + n_in + k)
                        .first()
                        .cloned()
                        .unwrap_or(Value::Undefined)));
                }
                Self::set(outputs, region, empties);
                self.stats.insert(
                    region,
                    RegionStats {
                        iterations: 0,
                        input_lengths,
                        bottleneck: None,
                    },
                );
                return;
            }
        }

        let mut n: usize;
        let mut bottleneck = None;
        if kind == RegionKind::If {
            n = 1;
        } else if n_in == 0 {
            n = if kind == RegionKind::Foreach { 0 } else { 1 };
        } else {
            n = usize::MAX;
            for (k, &len) in input_lengths.iter().enumerate() {
                if len < n {
                    n = len;
                    bottleneck = Some(k);
                }
            }
            if input_lengths.iter().all(|&l| l == n) {
                bottleneck = None;
            }
        }

        let mut acc_values: HashMap<NodeIdx, Value> = HashMap::new();
        for k in 0..n_acc {
            acc_values.insert(
                inner.accumulators[k],
                get(static_in + n_in + k)
                    .first()
                    .cloned()
                    .unwrap_or(Value::Undefined),
            );
        }
        let mut out_bufs: Vec<Vec<Value>> = vec![Vec::new(); n_out];
        let mut in_bufs: Vec<Vec<Value>> = vec![Vec::new(); n_in];
        let mut acc_bufs: Vec<Vec<Value>> = vec![Vec::new(); n_acc];
        let mut special: std::collections::HashSet<NodeIdx> = inner
            .splicer_in
            .iter()
            .chain(&inner.splicer_out)
            .chain(&inner.accumulators)
            .copied()
            .collect();
        for &(r, w) in inner.acc_pairs.values() {
            special.insert(r);
            special.insert(w);
        }
        // node → per-port per-iteration slot-0 values, in first-seen order.
        let mut body: indexmap::IndexMap<NodeIdx, Vec<Vec<Value>>> = indexmap::IndexMap::new();

        let mut inner_out: Outputs = outputs.clone();
        let rid = self.graph.nodes[region].id.clone();
        let lv = [
            self.graph.idx(&loop_var_id(&rid, "index")),
            self.graph.idx(&loop_var_id(&rid, "count")),
            self.graph.idx(&loop_var_id(&rid, "fraction")),
        ];

        if n == usize::MAX {
            n = 0;
        }
        for it in 0..n {
            let mut splicer_in = HashMap::new();
            for (k, buf) in in_bufs.iter_mut().enumerate() {
                let slot = get(static_in + k)
                    .get(it)
                    .cloned()
                    .unwrap_or(Value::Undefined);
                buf.push(slot.clone());
                splicer_in.insert(inner.splicer_in[k], one(slot));
            }
            for (k, buf) in acc_bufs.iter_mut().enumerate() {
                buf.push(
                    acc_values
                        .get(&inner.accumulators[k])
                        .cloned()
                        .unwrap_or(Value::Undefined),
                );
            }
            let fraction = if n > 1 {
                it as f64 / (n - 1) as f64
            } else {
                0.0
            };
            for (idx, v) in lv.iter().zip([it as f64, n as f64, fraction]) {
                if let Some(idx) = idx {
                    splicer_in.insert(*idx, one(Value::Num(v)));
                }
            }
            let ctx = IterCtx {
                splicer_in,
                acc: acc_values.clone(),
            };
            let iter_frame;
            let f = if delay != 0.0 {
                iter_frame = FrameCtx {
                    t: frame.t - it as f64 * delay,
                    ..frame.clone()
                };
                &iter_frame
            } else {
                frame
            };
            self.eval_scope(&inner, f, &mut inner_out, Some(&ctx));

            for (k, &id) in inner.splicer_out.iter().enumerate() {
                let v = inner_out[id]
                    .as_ref()
                    .and_then(|o| o.first())
                    .and_then(|s| s.first().cloned())
                    .unwrap_or(Value::Undefined);
                out_bufs[k].push(v);
            }
            for &id in &inner.order {
                if special.contains(&id) {
                    continue;
                }
                let Some(v) = inner_out[id].clone() else {
                    continue;
                };
                let buf = body.entry(id).or_insert_with(|| vec![Vec::new(); v.len()]);
                for (p, port) in v.iter().enumerate() {
                    if let Some(b) = buf.get_mut(p) {
                        b.push(port.first().cloned().unwrap_or(Value::Undefined));
                    }
                }
            }
            for &anchor in &inner.accumulators {
                let Some(&(_, write)) = inner.acc_pairs.get(&anchor) else {
                    continue;
                };
                let w_in = self.gather(write, &inner_out);
                let v = w_in
                    .first()
                    .and_then(|s| s.first().cloned())
                    .unwrap_or(Value::Undefined);
                if !v.is_undefined() {
                    acc_values.insert(anchor, v);
                }
            }
        }

        for (id, buf) in body {
            Self::set(outputs, id, buf.into_iter().map(Arc::from).collect());
        }
        for (k, &id) in inner.splicer_in.iter().enumerate() {
            Self::set(
                outputs,
                id,
                vec![Arc::from(std::mem::take(&mut in_bufs[k]))],
            );
        }
        for (k, &id) in inner.splicer_out.iter().enumerate() {
            Self::set(outputs, id, vec![Arc::from(out_bufs[k].clone())]);
        }
        for (k, &anchor) in inner.accumulators.iter().enumerate() {
            let Some(&(read, write)) = inner.acc_pairs.get(&anchor) else {
                continue;
            };
            Self::set(
                outputs,
                read,
                vec![Arc::from(std::mem::take(&mut acc_bufs[k]))],
            );
            Self::set(
                outputs,
                write,
                vec![one(acc_values
                    .get(&anchor)
                    .cloned()
                    .unwrap_or(Value::Undefined))],
            );
        }
        let mut finals: Vec<Spread> = out_bufs.into_iter().map(Arc::from).collect();
        for anchor in &inner.accumulators {
            finals.push(one(acc_values
                .get(anchor)
                .cloned()
                .unwrap_or(Value::Undefined)));
        }
        Self::set(outputs, region, finals.clone());
        self.stats.insert(
            region,
            RegionStats {
                iterations: n,
                input_lengths,
                bottleneck,
            },
        );
        if kind == RegionKind::Cache {
            self.graph.nodes[region].cache = Some((hash_inputs(&outer), finals));
        }
    }

    /// `gatherInputs`: defaults / overrides / constants, then wires.
    fn gather(&mut self, i: NodeIdx, outputs: &Outputs) -> Vec<Spread> {
        let node: &RuntimeNode = &self.graph.nodes[i];
        let mut result: Vec<Spread> = Vec::with_capacity(node.in_arity);
        let mut multi_acc: Vec<Option<Vec<Value>>> = vec![None; node.in_arity];
        for k in 0..node.in_arity {
            if node.constant[k] {
                result.push(
                    node.constants[k]
                        .clone()
                        .unwrap_or_else(|| node.defaults[k].clone()),
                );
            } else if node.multi[k] {
                multi_acc[k] = Some(Vec::new());
                result.push(empty());
            } else {
                let ov = node
                    .sources
                    .as_ref()
                    .and_then(|s| source_value(&s[k], &node.id, self.params));
                result.push(
                    ov.unwrap_or_else(|| node.defaults.get(k).cloned().unwrap_or_else(empty)),
                );
            }
        }
        let mut captured: Vec<(usize, Spread)> = Vec::new();
        for e in &self.graph.edges_by_target[i] {
            let Some(up) = &outputs[e.from] else { continue };
            let Some(v) = up.get(e.from_port) else {
                continue;
            };
            if node.constant[e.to_port] {
                // Constant pins sample their wire once (first tick), then
                // stay frozen — the documented behaviour the TS runtime
                // never implemented (it ignored wires into constant pins).
                if self.capture_constants {
                    captured.push((e.to_port, v.clone()));
                }
                continue;
            }
            if let Some(acc) = multi_acc[e.to_port].as_mut() {
                acc.extend(v.iter().cloned());
            } else {
                result[e.to_port] = v.clone();
            }
        }
        for (k, acc) in multi_acc.into_iter().enumerate() {
            if let Some(items) = acc {
                result[k] = Arc::from(items);
            }
        }
        for (port, v) in captured {
            self.graph.nodes[i].constants[port] = Some(v.clone());
            result[port] = v;
        }
        result
    }
}

/// Cache-region input hash (`JSON.stringify`-like; closures hash alike).
fn hash_inputs(inputs: &[Spread]) -> String {
    let mut s = String::new();
    for port in inputs {
        s.push('[');
        for v in port.iter() {
            write_hash(v, &mut s);
            s.push(',');
        }
        s.push(']');
    }
    s
}

fn write_hash(v: &Value, s: &mut String) {
    use std::fmt::Write;
    match v {
        Value::Undefined | Value::Null => s.push_str("null"),
        // TS's replacer hashes every function (style) as "function".
        Value::Style(_) => s.push_str("\"function\""),
        Value::Scene(n) => write_hash(&n.to_value(), s),
        Value::Bool(b) => {
            let _ = write!(s, "{b}");
        }
        Value::Num(n) if n.is_finite() => {
            let _ = write!(s, "{n:?}");
        }
        Value::Num(_) => s.push_str("null"),
        Value::Str(t) => {
            let _ = write!(s, "{t:?}");
        }
        Value::Vec2(p) => {
            let _ = write!(s, "{{x:{:?},y:{:?}}}", p.x, p.y);
        }
        Value::Color(c) => {
            let _ = write!(s, "{{r:{:?},g:{:?},b:{:?},a:{:?}}}", c.r, c.g, c.b, c.a);
        }
        Value::List(items) => {
            s.push('[');
            for x in items.iter() {
                write_hash(x, s);
                s.push(',');
            }
            s.push(']');
        }
        Value::Object(map) => {
            s.push('{');
            for (k, x) in map.iter() {
                if x.is_undefined() {
                    continue;
                }
                let _ = write!(s, "{k:?}:");
                write_hash(x, s);
                s.push(',');
            }
            s.push('}');
        }
    }
}
