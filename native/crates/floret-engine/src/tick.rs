//! Per-frame evaluation. A port of `src/runtime/tick.ts`.

use crate::compile::source_value;
use crate::graph::{NodeIdx, RuntimeGraph, RuntimeNode, Scope};
use crate::node::{EvalCtx, FrameCtx, ParamStore};
use crate::profile::Profile;
use crate::spec::{RegionKind, RegionRole};
use crate::value::{empty, one, Spread, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
struct IterCtx<'a> {
    /// Zero-based iteration, which keys the memo of nodes in the body.
    index: usize,
    /// Splicer-in / loop-var node → this iteration's value (a spread).
    splicer_in: &'a [(NodeIdx, Spread)],
    /// Accumulator anchor → current value.
    acc: &'a HashMap<NodeIdx, Value>,
}

/// A pure node's last evaluation at one loop iteration (TS
/// `runtimeState.memo`).
struct MemoEntry {
    inputs: Vec<Spread>,
    outputs: Arc<Vec<Spread>>,
    /// Param store version after the compute.
    params: u64,
}

#[derive(Default)]
struct NodeMemo {
    /// Slot 0 is the root scope; slot `i + 1` is loop iteration `i`.
    slots: Vec<Option<MemoEntry>>,
    /// Unwired pins' override values, so an unchanged override hands out
    /// the same spread every tick.
    sources: Vec<Option<(SourceKey, Option<Spread>)>>,
}

/// What an input override was read from.
enum SourceKey {
    /// A `portValues` entry.
    Port(Value),
    /// A param, at this node params version.
    Param(u64),
}

/// The engine's mutable state across ticks: the compiled graph plus the
/// live param store.
pub struct Engine {
    pub graph: RuntimeGraph,
    pub params: ParamStore,
    /// Wired constant pins whose value has been captured (first tick).
    constants_wired: bool,
    host: Option<Arc<dyn crate::host::Host>>,
    audio: Option<Arc<dyn crate::audio::AudioBackend>>,
    memo: Vec<NodeMemo>,
    /// Each node's params version, refreshed when the store changes.
    param_versions: Vec<u64>,
    /// Store version `param_versions` was read at.
    params_seen: Option<u64>,
    profile: Profile,
    /// The patch's loop length in seconds, for frames that don't set one
    /// (see [`FrameCtx::loop_length`]). Change it freely between ticks.
    pub loop_length: f64,
    /// Mixed into every random / noise node's seed (see `crate::seed`).
    patch_seed: u32,
}

impl Engine {
    pub fn new(graph: RuntimeGraph, params: ParamStore) -> Engine {
        Engine {
            graph,
            params,
            constants_wired: false,
            host: None,
            audio: None,
            memo: Vec::new(),
            param_versions: Vec::new(),
            params_seen: None,
            profile: Profile::default(),
            loop_length: crate::patch::DEFAULT_LOOP_LENGTH,
            patch_seed: 0,
        }
    }

    /// Lend the engine the app's rasteriser / fetcher (see `crate::host`).
    pub fn set_host(&mut self, host: Option<Arc<dyn crate::host::Host>>) {
        self.host = host;
    }

    /// Connect the sound nodes to the app's audio engine (see
    /// `crate::audio`).
    pub fn set_audio(&mut self, audio: Option<Arc<dyn crate::audio::AudioBackend>>) {
        self.audio = audio;
    }

    /// Set the patch seed (see `crate::seed`). A change forgets memoised
    /// outputs, which may have been drawn with the old seed.
    pub fn set_patch_seed(&mut self, seed: u32) {
        if seed != self.patch_seed {
            self.patch_seed = seed;
            self.memo.clear();
        }
    }

    pub fn patch_seed(&self) -> u32 {
        self.patch_seed
    }

    /// Time every node from the next tick on (see [`Engine::profile`]).
    pub fn set_profiling(&mut self, on: bool) {
        self.profile.enabled = on;
    }

    /// Timing and memo counters for the last tick.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn tick(&mut self, frame: &FrameCtx) -> TickResult {
        let start = Instant::now();
        let looped;
        let frame = if frame.loop_length > 0.0 {
            frame
        } else {
            looped = FrameCtx {
                loop_length: self.loop_length,
                ..frame.clone()
            };
            &looped
        };
        let _host = crate::host::install(self.host.clone());
        let _seed = crate::seed::install(self.patch_seed);
        let _audio = crate::audio::begin(self.audio.clone(), self.sound_nodes());
        let n = self.graph.nodes.len();
        self.memo.resize_with(n, NodeMemo::default);
        if self.params_seen != Some(self.params.version()) || self.param_versions.len() != n {
            self.param_versions = (self.graph.nodes.iter())
                .map(|node| self.params.node_version(&node.id))
                .collect();
            self.params_seen = Some(self.params.version());
        }
        self.profile.begin(n);
        let mut t = Ticker {
            graph: &mut self.graph,
            params: &mut self.params,
            tick_frame: frame,
            stats: HashMap::new(),
            errors: Vec::new(),
            slot_writers: HashMap::new(),
            capture_constants: !self.constants_wired,
            memo: &mut self.memo,
            param_versions: &self.param_versions,
            profile: &mut self.profile,
            child_time: Duration::ZERO,
            scratch: Vec::new(),
        };
        t.slot_writers = t.build_slot_writers();
        let mut outputs: Outputs = vec![None; n];
        if let Some(root) = t.graph.scopes.get(&None).cloned() {
            t.eval_scope(&root, frame, &mut outputs, None);
            t.capture_stateful_next(&root, &outputs);
        }
        self.constants_wired = true;
        let (stats, errors) = (t.stats, t.errors);
        self.profile.end(start.elapsed());
        TickResult {
            outputs,
            region_stats: stats,
            errors,
        }
    }
}

impl Engine {
    /// Carry node state over from the engine this one replaces (a
    /// recompile after an edit): every node that keeps state across edits
    /// ([`crate::node::NodeImpl::carry`]) hands it to the node with the
    /// same id and type here. A restart builds a fresh engine instead.
    pub fn adopt_state(&mut self, old: &mut Engine) {
        for node in &mut self.graph.nodes {
            let Some(imp) = node.imp.as_mut() else {
                continue;
            };
            let Some(prev) = old.graph.idx(&node.id).map(|i| &mut old.graph.nodes[i]) else {
                continue;
            };
            if prev.ty != node.ty {
                continue;
            }
            if let Some(state) = prev.imp.as_mut().and_then(|p| p.carry()) {
                imp.adopt(state);
            }
        }
    }

    fn sound_nodes(&self) -> Vec<Arc<str>> {
        if self.audio.is_none() {
            return Vec::new();
        }
        self.graph
            .nodes
            .iter()
            .filter(|n| n.ty.starts_with("sound/"))
            .map(|n| Arc::from(n.id.as_str()))
            .collect()
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
    memo: &'a mut [NodeMemo],
    param_versions: &'a [u64],
    profile: &'a mut Profile,
    /// Profiling: time spent in nodes evaluated inside the current one.
    child_time: Duration,
    /// Spare inputs buffer for memoised nodes.
    scratch: Vec<Spread>,
}

pub(crate) fn slot_key(v: Option<&serde_json::Value>) -> Option<String> {
    v.map(|j| match j {
        serde_json::Value::String(s) => s.clone(),
        other => crate::value::Value::from_json(other)
            .to_number()
            .to_string(),
    })
}

/// TS `shallowInputsEqual`: per port, the same spread, or the same length
/// with every element `===` ([`Value::same`]).
fn same_inputs(a: &[Spread], b: &[Spread]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            Arc::ptr_eq(x, y)
                || (x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| p.same(q)))
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
            if self.profile.enabled {
                self.eval_node_timed(i, frame, outputs, iter);
            } else {
                self.eval_node(i, frame, outputs, iter);
            }
        }
    }

    /// `eval_node`, charging its self time (minus nodes evaluated inside
    /// it, i.e. a region's body) to the node.
    fn eval_node_timed(
        &mut self,
        i: NodeIdx,
        frame: &FrameCtx,
        outputs: &mut Outputs,
        iter: Option<&IterCtx>,
    ) {
        let start = Instant::now();
        let outer = std::mem::take(&mut self.child_time);
        self.eval_node(i, frame, outputs, iter);
        let elapsed = start.elapsed();
        self.profile.nodes[i].time += elapsed.saturating_sub(self.child_time);
        self.child_time = outer + elapsed;
    }

    fn set(outputs: &mut Outputs, i: NodeIdx, v: Vec<Spread>) {
        outputs[i] = Some(Arc::new(v));
    }

    /// `set(outputs, i, vec![v])`, reusing the node's previous outputs when
    /// nothing else holds them (a loop body's splicers, every iteration).
    fn set_one(outputs: &mut Outputs, i: NodeIdx, v: Spread) {
        if let Some(Some(prev)) = outputs.get_mut(i) {
            if let Some(prev) = Arc::get_mut(prev) {
                prev.clear();
                prev.push(v);
                return;
            }
        }
        outputs[i] = Some(Arc::new(vec![v]));
    }

    /// Run a node's compute. `false` with the empty fallback when it
    /// failed (TS `safeCompute`; such a result isn't memoised).
    fn run_compute(
        &mut self,
        i: NodeIdx,
        inputs: &[Spread],
        frame: &FrameCtx,
    ) -> (Vec<Spread>, bool) {
        self.profile.nodes[i].evals += 1;
        let node = &mut self.graph.nodes[i];
        let out_arity = node.out_arity;
        let Some(imp) = node.imp.as_mut() else {
            return (vec![empty(); out_arity], true);
        };
        let mut cx = EvalCtx {
            node_id: &node.id,
            frame,
            tick_frame: self.tick_frame,
            params: self.params,
            saved_params: &node.saved_params,
        };
        let result = imp.eval(inputs, &mut cx);
        self.errors
            .extend(crate::node::take_warnings().into_iter().map(|m| (i, m)));
        match result {
            Ok(out) => (out, true),
            Err(msg) => {
                self.errors.push((i, msg));
                (vec![empty(); out_arity], false)
            }
        }
    }

    /// The pure-compute memo (TS `evalNode`): reuse the outputs from this
    /// node's last evaluation at the same loop iteration when its inputs
    /// are [`same_inputs`] and its params haven't changed since.
    fn eval_memoised(
        &mut self,
        i: NodeIdx,
        frame: &FrameCtx,
        outputs: &mut Outputs,
        iter: Option<&IterCtx>,
    ) {
        // Inputs go into a reused buffer; a hit allocates nothing.
        let mut inputs = std::mem::take(&mut self.scratch);
        self.gather_into(i, outputs, &mut inputs);
        let slot = iter.map_or(0, |c| c.index + 1);
        if let Some(Some(prev)) = self.memo[i].slots.get(slot) {
            if self.param_versions[i] <= prev.params && same_inputs(&prev.inputs, &inputs) {
                outputs[i] = Some(prev.outputs.clone());
                self.profile.nodes[i].hits += 1;
                inputs.clear();
                self.scratch = inputs;
                return;
            }
        }
        let (out, ok) = self.run_compute(i, &inputs, frame);
        let out = Arc::new(out);
        if ok {
            let params = self.params.version();
            let slots = &mut self.memo[i].slots;
            if slots.len() <= slot {
                slots.resize_with(slot + 1, || None);
            }
            match &mut slots[slot] {
                Some(prev) => {
                    let mut old = std::mem::replace(&mut prev.inputs, inputs);
                    prev.outputs = out.clone();
                    prev.params = params;
                    old.clear();
                    self.scratch = old;
                }
                empty => {
                    *empty = Some(MemoEntry {
                        inputs,
                        outputs: out.clone(),
                        params,
                    });
                }
            }
        } else {
            inputs.clear();
            self.scratch = inputs;
        }
        outputs[i] = Some(out);
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
            let out = self.run_compute(i, &[], frame).0;
            Self::set(outputs, i, out);
            return;
        }

        match node.role {
            Some(RegionRole::SplicerIn | RegionRole::LoopVar) => {
                let v = iter
                    .and_then(|c| c.splicer_in.iter().find(|(k, _)| *k == i))
                    .map_or_else(empty, |(_, v)| v.clone());
                Self::set_one(outputs, i, v);
                return;
            }
            Some(RegionRole::SplicerOut) => {
                let mut inputs = std::mem::take(&mut self.scratch);
                self.gather_into(i, outputs, &mut inputs);
                let v = inputs.first().cloned().unwrap_or_else(empty);
                Self::set_one(outputs, i, v);
                inputs.clear();
                self.scratch = inputs;
                return;
            }
            Some(RegionRole::AccumulatorRead) => {
                let cur = iter
                    .and_then(|c| c.acc.get(&i).cloned())
                    .unwrap_or(Value::Undefined);
                Self::set_one(outputs, i, one(cur));
                return;
            }
            Some(RegionRole::AccumulatorWrite) => {
                Self::set_one(outputs, i, one(Value::Undefined));
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
        if node.memoise {
            self.eval_memoised(i, frame, outputs, iter);
            return;
        }
        let inputs = self.gather(i, outputs);
        let out = self.run_compute(i, &inputs, frame).0;
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

        let mut cache_hash = None;
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
            cache_hash = Some(hash);
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
        let in_spreads: Vec<Spread> = (0..n_in).map(|k| get(static_in + k)).collect();
        if n == usize::MAX {
            n = 0;
        }
        let mut out_bufs: Vec<Vec<Value>> = (0..n_out).map(|_| Vec::with_capacity(n)).collect();
        let mut acc_bufs: Vec<Vec<Value>> = (0..n_acc).map(|_| Vec::with_capacity(n)).collect();
        let mut special: Vec<NodeIdx> = (inner.splicer_in.iter())
            .chain(&inner.splicer_out)
            .chain(&inner.accumulators)
            .copied()
            .collect();
        for &(r, w) in inner.acc_pairs.values() {
            special.extend([r, w]);
        }
        // Body nodes (by position in `inner.order`) → per-port
        // per-iteration slot-0 values.
        let body_nodes: Vec<(usize, NodeIdx)> = (inner.order.iter().copied().enumerate())
            .filter(|(_, id)| !special.contains(id))
            .collect();
        let mut body: Vec<Option<Vec<Vec<Value>>>> = vec![None; inner.order.len()];

        let mut inner_out: Outputs = outputs.clone();
        let mut splicer_in = Vec::with_capacity(n_in + 3);
        for it in 0..n {
            splicer_in.clear();
            for (k, spread) in in_spreads.iter().enumerate() {
                let slot = spread.get(it).cloned().unwrap_or(Value::Undefined);
                splicer_in.push((inner.splicer_in[k], one(slot)));
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
            for (idx, v) in inner.loop_vars.iter().zip([it as f64, n as f64, fraction]) {
                if let Some(idx) = idx {
                    splicer_in.push((*idx, one(Value::Num(v))));
                }
            }
            let ctx = IterCtx {
                index: it,
                splicer_in: &splicer_in,
                acc: &acc_values,
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
            for &(pos, id) in &body_nodes {
                let Some(v) = &inner_out[id] else {
                    continue;
                };
                let buf = body[pos]
                    .get_or_insert_with(|| (0..v.len()).map(|_| Vec::with_capacity(n)).collect());
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

        for (pos, buf) in body.into_iter().enumerate() {
            if let Some(buf) = buf {
                Self::set(
                    outputs,
                    inner.order[pos],
                    buf.into_iter().map(Arc::from).collect(),
                );
            }
        }
        for (k, &id) in inner.splicer_in.iter().enumerate() {
            let src = &in_spreads[k];
            let v = if src.len() == n {
                src.clone()
            } else {
                (0..n)
                    .map(|it| src.get(it).cloned().unwrap_or(Value::Undefined))
                    .collect()
            };
            Self::set(outputs, id, vec![v]);
        }
        let finals_out: Vec<Spread> = out_bufs.into_iter().map(Arc::from).collect();
        for (k, &id) in inner.splicer_out.iter().enumerate() {
            Self::set(outputs, id, vec![finals_out[k].clone()]);
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
        let mut finals = finals_out;
        for anchor in &inner.accumulators {
            finals.push(one(acc_values
                .get(anchor)
                .cloned()
                .unwrap_or(Value::Undefined)));
        }
        let finals = Arc::new(finals);
        outputs[region] = Some(finals.clone());
        self.stats.insert(
            region,
            RegionStats {
                iterations: n,
                input_lengths,
                bottleneck,
            },
        );
        if let Some(hash) = cache_hash {
            self.graph.nodes[region].cache = Some((hash, finals.to_vec()));
        }
    }

    /// `gatherInputs`: defaults / overrides / constants, then wires.
    fn gather(&mut self, i: NodeIdx, outputs: &Outputs) -> Vec<Spread> {
        let mut result = Vec::with_capacity(self.graph.nodes[i].in_arity);
        self.gather_into(i, outputs, &mut result);
        result
    }

    /// [`Ticker::gather`] into an empty `result`.
    fn gather_into(&mut self, i: NodeIdx, outputs: &Outputs, result: &mut Vec<Spread>) {
        let node: &RuntimeNode = &self.graph.nodes[i];
        let n = node.in_arity;
        result.reserve(n);
        for k in 0..n {
            result.push(if node.constant[k] {
                node.constants[k]
                    .clone()
                    .unwrap_or_else(|| node.defaults[k].clone())
            } else if node.multi[k] {
                empty()
            } else {
                node.defaults.get(k).cloned().unwrap_or_else(empty)
            });
        }

        // Wires. Pins without one fall back to their override below.
        let mut wired = vec![false; if n > 64 { n } else { 0 }];
        let mut wired_bits = 0u64;
        let mut multi_acc: Vec<Option<Vec<Value>>> = if node.multi.contains(&true) {
            (node.multi.iter())
                .map(|&m| if m { Some(Vec::new()) } else { None })
                .collect()
        } else {
            Vec::new()
        };
        let mut captured: Vec<(usize, Spread)> = Vec::new();
        for e in &self.graph.edges_by_target[i] {
            let Some(up) = &outputs[e.from] else { continue };
            let Some(v) = up.get(e.from_port) else {
                continue;
            };
            let k = e.to_port;
            if node.constant[k] {
                // Constant pins sample their wire once (first tick), then
                // stay frozen — the documented behaviour the TS runtime
                // never implemented (it ignored wires into constant pins).
                if self.capture_constants {
                    captured.push((k, v.clone()));
                }
                continue;
            }
            if let Some(Some(acc)) = multi_acc.get_mut(k) {
                acc.extend(v.iter().cloned());
                continue;
            }
            result[k] = v.clone();
            if n > 64 {
                wired[k] = true;
            } else {
                wired_bits |= 1 << k;
            }
        }
        for (k, acc) in multi_acc.into_iter().enumerate() {
            if let Some(items) = acc {
                result[k] = Arc::from(items);
            }
        }

        if let Some(sources) = &node.sources {
            let memo = &mut self.memo[i];
            for (k, src) in sources.iter().enumerate().take(n) {
                let is_wired = if n > 64 {
                    wired[k]
                } else {
                    wired_bits & (1 << k) != 0
                };
                if is_wired || node.constant[k] || node.multi[k] {
                    continue;
                }
                if src.port_value.is_none() && src.param.is_none() {
                    continue;
                }
                let version = self.param_versions[i];
                if let Some(v) = override_value(memo, node, self.params, version, k) {
                    result[k] = v;
                }
            }
        }

        for (port, v) in captured {
            self.graph.nodes[i].constants[port] = Some(v.clone());
            result[port] = v;
        }
    }
}

/// An unwired pin's override (`portValues`, else its param), reused while
/// unchanged.
fn override_value(
    memo: &mut NodeMemo,
    node: &RuntimeNode,
    params: &ParamStore,
    version: u64,
    k: usize,
) -> Option<Spread> {
    let src = &node.sources.as_ref()?[k];
    let cache = &mut memo.sources;
    if cache.len() <= k {
        cache.resize_with(node.in_arity.max(k + 1), || None);
    }
    let fresh = match (&cache[k], &src.param, &src.port_value) {
        (Some((SourceKey::Param(seen), _)), Some(_), _) => *seen != version,
        (Some((SourceKey::Port(seen), _)), None, Some(v)) => !seen.same(v),
        _ => true,
    };
    if fresh {
        let key = if src.param.is_some() {
            SourceKey::Param(version)
        } else if let Some(v) = &src.port_value {
            SourceKey::Port(v.clone())
        } else {
            SourceKey::Param(version)
        };
        cache[k] = Some((key, source_value(src, &node.id, params)));
    }
    cache[k].as_ref().and_then(|(_, v)| v.clone())
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
        // TS stringifies a canvas as `{}`, so a cache never saw new pixels.
        // Hash the image's generation id instead.
        Value::Image(i) => {
            let _ = write!(s, "{{\"$image\":{}}}", i.id());
        }
        Value::Handle(h) => match h.to_value() {
            Some(obj) => write_hash(&obj, s),
            None => s.push_str("\"function\""),
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneNode;
    use serde_json::{json, Value as J};

    /// A patch from `(id, type, portValues)` nodes and
    /// `(from, out-port, to, in-port)` wires.
    fn engine(nodes: &[(&str, &str, J)], wires: &[(&str, &str, &str, &str)]) -> Engine {
        let nodes: Vec<J> = (nodes.iter())
            .map(|(id, ty, pv)| {
                json!({"id": id, "defType": ty, "position": {"x": 0, "y": 0},
                       "params": pv, "portValues": pv})
            })
            .collect();
        let edges: Vec<J> = (wires.iter().enumerate())
            .map(|(i, (s, sp, t, tp))| {
                json!({"id": format!("e{i}"), "source": s, "sourceHandle": format!("out-{sp}"),
                       "target": t, "targetHandle": format!("in-{tp}")})
            })
            .collect();
        let text = json!({"kind": "floret.patch", "version": 2, "nodes": nodes, "edges": edges});
        Engine::load(&text.to_string()).unwrap()
    }

    fn frame(i: u32) -> FrameCtx {
        FrameCtx {
            t: f64::from(i) / 60.0,
            dt: 1.0 / 60.0,
            frame_count: f64::from(i),
            ..FrameCtx::default()
        }
    }

    fn out(e: &Engine, r: &TickResult, id: &str) -> Arc<Vec<Spread>> {
        r.outputs[e.graph.idx(id).unwrap()].clone().unwrap()
    }

    fn counts(e: &Engine, id: &str) -> (u32, u32) {
        let p = &e.profile().nodes[e.graph.idx(id).unwrap()];
        (p.evals, p.hits)
    }

    #[test]
    fn pure_nodes_reuse_outputs_while_inputs_are_unchanged() {
        let mut e = engine(
            &[
                ("t", "time/seconds", json!({})),
                ("k", "math/add", json!({"a": 1, "b": 2})),
                ("live", "math/add", json!({"b": 1})),
            ],
            &[("t", "t", "live", "a")],
        );
        let r0 = e.tick(&frame(0));
        let r1 = e.tick(&frame(1));
        assert!(Arc::ptr_eq(&out(&e, &r0, "k"), &out(&e, &r1, "k")));
        assert!(!Arc::ptr_eq(&out(&e, &r0, "live"), &out(&e, &r1, "live")));
        assert_eq!(counts(&e, "k"), (0, 1));
        assert_eq!(counts(&e, "live"), (1, 0));
    }

    #[test]
    fn a_param_edit_invalidates_the_memo() {
        let mut e = engine(&[("txt", "draw/text", json!({}))], &[]);
        e.params.set("txt", "string", json!("a"));
        let text = |r: &TickResult, e: &Engine| match &out(e, r, "txt")[0][0] {
            Value::Scene(n) => match &**n {
                SceneNode::Text { text, .. } => text.to_string(),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        };
        let r = e.tick(&frame(0));
        assert_eq!(text(&r, &e), "a");
        e.params.set("txt", "string", json!("b"));
        let r = e.tick(&frame(1));
        assert_eq!(text(&r, &e), "b");
        let r2 = e.tick(&frame(2));
        assert!(Arc::ptr_eq(&out(&e, &r, "txt"), &out(&e, &r2, "txt")));
    }

    #[test]
    fn render_as_keeps_the_batch_it_built_like_ts() {
        // TS memoises render-as on input identity: the emitter hands out the
        // same (mutated) buffer every tick, so the batch keeps the count it
        // was built with while the buffer moves on.
        let mut e = engine(
            &[
                ("t", "time/seconds", json!({})),
                (
                    "em",
                    "particles/emitter",
                    json!({"rate": 120, "maxCount": 100}),
                ),
                ("ra", "particles/render-as", json!({})),
            ],
            &[
                ("t", "t", "em", "time"),
                ("em", "particles", "ra", "particles"),
            ],
        );
        let mut seen = Vec::new();
        for i in 0..3 {
            let r = e.tick(&frame(i));
            let Value::Scene(sn) = &out(&e, &r, "ra")[0][0] else {
                panic!()
            };
            let SceneNode::ParticleBatch { count, buffer, .. } = &**sn else {
                panic!()
            };
            seen.push((*count, buffer.read().count));
        }
        assert_eq!(seen, vec![(2.0, 2), (2.0, 4), (2.0, 6)]);
    }

    #[test]
    fn a_static_patch_recomputes_nothing() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../samples/tut-07-grid.flora");
        let mut e = Engine::load(&std::fs::read_to_string(path).unwrap()).unwrap();
        e.set_profiling(true);
        let r0 = e.tick(&frame(0));
        let r1 = e.tick(&frame(1));
        let computed: Vec<&str> = (e.graph.nodes.iter().zip(&e.profile().nodes))
            .filter(|(_, p)| p.evals > 0)
            .map(|(n, _)| n.ty.as_str())
            .collect();
        assert_eq!(computed, vec!["meta/comment"]);
        assert!(e.profile().totals().1 > 0);
        let renderer = e.graph.renderer().unwrap();
        let (a, b) = (r0.layers(renderer), r1.layers(renderer));
        assert!(a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.same(y)));
        assert!(e.profile().tick_avg_ms > 0.0);
    }
}
