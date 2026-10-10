//! The node interface: how a node type computes, and the context it sees.

use crate::spec::{NodeSpec, Params, PortKind, PortSpec};
use crate::value::{Spread, Value};
use std::collections::HashMap;

/// Per-frame input to the engine (mirrors the TS `FrameCtx`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameCtx {
    /// Seconds since the runtime started.
    pub t: f64,
    /// Seconds since the previous frame.
    pub dt: f64,
    pub frame_count: f64,
    /// Canvas size in pixels.
    pub width: f64,
    pub height: f64,
    /// Mouse position, canvas-centre origin.
    pub mouse_x: f64,
    pub mouse_y: f64,
    pub mouse_is_pressed: bool,
    /// Last key pressed (lowercase), empty if none.
    pub key: String,
    pub key_is_pressed: bool,
    /// Every key held down now, named like `key` (lowercase letters and
    /// digits, `" "` for the space bar, `ArrowLeft`, `Enter` …).
    pub keys_down: Vec<String>,
    /// Text typed since the previous frame, in order: the characters
    /// (case and punctuation as typed), `'\n'` for Enter and `'\u{8}'`
    /// for Backspace.
    pub typed: String,
    /// The patch's loop length in seconds (the time node's `loop` phase).
    /// 0 when the caller leaves it to the engine, which fills in its
    /// patch's (see [`crate::Engine::loop_length`]).
    pub loop_length: f64,
}

/// Live parameter values, keyed by node id — the editor writes here while
/// a slider is dragged so the runtime picks values up without a recompile.
///
/// Every write bumps a version (per node and overall), which is how the
/// tick's memo notices a param change on a node that reads it.
#[derive(Clone, Debug, Default)]
pub struct ParamStore {
    by_node: HashMap<String, (Params, u64)>,
    version: u64,
}

impl ParamStore {
    pub fn get(&self, node: &str, key: &str) -> Option<&serde_json::Value> {
        self.by_node.get(node)?.0.get(key)
    }

    pub fn set(&mut self, node: &str, key: &str, value: serde_json::Value) {
        self.version += 1;
        let entry = self.by_node.entry(node.to_owned()).or_default();
        entry.0.insert(key.to_owned(), value);
        entry.1 = self.version;
    }

    pub fn set_node(&mut self, node: &str, params: Params) {
        self.version += 1;
        self.by_node.insert(node.to_owned(), (params, self.version));
    }

    pub fn replace_all<'a>(&mut self, entries: impl IntoIterator<Item = (&'a str, &'a Params)>) {
        self.version += 1;
        self.by_node.clear();
        for (id, p) in entries {
            self.by_node
                .insert(id.to_owned(), (p.clone(), self.version));
        }
    }

    /// Bumped by every write.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The store's [`version`](Self::version) when `node`'s params last
    /// changed (0 if never set).
    pub fn node_version(&self, node: &str) -> u64 {
        self.by_node.get(node).map_or(0, |e| e.1)
    }

    /// True when `node`'s params haven't changed since the store was at
    /// `version`.
    pub fn unchanged_since(&self, node: &str, version: u64) -> bool {
        self.version == version || self.node_version(node) <= version
    }
}

/// What a node sees while it computes.
pub struct EvalCtx<'a> {
    pub node_id: &'a str,
    /// The frame for this evaluation. Inside a ForEach with `delay`, `t` is
    /// shifted per iteration.
    pub frame: &'a FrameCtx,
    /// The tick's unshifted frame (what TS nodes read from `frameCtxStore`).
    pub tick_frame: &'a FrameCtx,
    pub params: &'a mut ParamStore,
    /// The node's params as saved in the patch (fallback after `params`).
    pub saved_params: &'a Params,
}

impl EvalCtx<'_> {
    /// `paramCtx.param(key)`: live value first, then the saved params.
    pub fn param(&self, key: &str) -> Option<&serde_json::Value> {
        self.params
            .get(self.node_id, key)
            .or_else(|| self.saved_params.get(key))
    }

    pub fn param_value(&self, key: &str) -> Value {
        self.param(key)
            .map(Value::from_json)
            .unwrap_or(Value::Undefined)
    }

    /// The app's services for this tick (rasterising, fetching), if any.
    /// See `crate::host`.
    pub fn host(&self) -> Option<std::sync::Arc<dyn crate::host::Host>> {
        crate::host::current()
    }

    /// The node's own `seed` with the patch seed mixed in — the one place
    /// every random / noise node gets its seed from (see [`crate::seed`]).
    /// Returns `own` unchanged while the patch seed is 0.
    pub fn seed(&self, own: f64) -> f64 {
        crate::seed::mix(own)
    }

    /// Report a problem on the node (shown like an error) while still
    /// returning normal outputs, e.g. a failed load.
    pub fn warn(&self, msg: impl Into<String>) {
        WARNINGS.with(|w| w.borrow_mut().push(msg.into()));
    }
}

thread_local! {
    static WARNINGS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Warnings reported since the last call (see [`EvalCtx::warn`]).
pub(crate) fn take_warnings() -> Vec<String> {
    WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

pub type NodeResult = Result<Vec<Spread>, String>;

/// A node instance. One is created per node at compile time, so any
/// cross-tick state a node needs lives in its own struct.
///
/// # Memoisation
///
/// Like the TS runtime, the tick skips `eval` for a pure node whose inputs
/// (and params) are unchanged since its last evaluation at the same loop
/// iteration, reusing its previous outputs. Context, stateful and
/// effectful node types (flags in `nodes.json`) and nodes with an `image`
/// or `audio` port always run. A node that reads anything else besides its
/// inputs and params — `cx.frame` / `cx.tick_frame`, devices, the host,
/// a clock — or must run every tick for a side effect must either be
/// flagged that way in its spec or return `false` from [`NodeImpl::memoise`].
pub trait NodeImpl: Send {
    /// Compute outputs from gathered inputs. Context (source) nodes receive
    /// an empty `inputs`. An `Err` makes the runtime emit empty outputs for
    /// this tick (the node retries next tick).
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult;

    /// Whether the tick may skip `eval` when inputs and params are
    /// unchanged. Only consulted for node types the spec allows to be
    /// memoised.
    fn memoise(&self) -> bool {
        true
    }

    /// State worth keeping across an edit of the patch: the app recompiles
    /// on every structural edit (a wire, a mode, a typed pin value), and
    /// hands what this returns to the rebuilt node of the same id and type
    /// through [`NodeImpl::adopt`] (see [`crate::Engine::adopt_state`]).
    /// Most nodes start fresh instead (`None`).
    fn carry(&mut self) -> Option<CarriedState> {
        None
    }

    /// Take over the state a node of the same id and type [`carry`]-ed
    /// out of the previous engine.
    ///
    /// [`carry`]: NodeImpl::carry
    fn adopt(&mut self, _state: CarriedState) {}

    /// Switch on per-element evaluation over spreads: a spread on a
    /// single-value pin runs the node once per element (see
    /// [`lift_len`] for the rule) instead of reading its slot 0. The
    /// compiler calls this on nodes of the types [`crate::nodes::lifts`]
    /// lists, in patches saved from v4 on (older files keep slot 0, see
    /// `PatchFile::legacy_slot0`). Returns whether the node now lifts.
    fn lift_spreads(&mut self) -> bool {
        false
    }
}

/// Element count for per-element evaluation over the pins marked `single`.
/// Lifting starts only when a spread of two or more values arrives: then
/// it's Expression's rule — the longest spread wins, shorter ones cycle
/// (`s[i % len]`), an empty one empties the result (0). Until then it is 1,
/// the plain one-value path, where an empty pin reads as "nothing" (an
/// upstream error or a device that isn't there falls back as it always did).
pub fn lift_len(inputs: &[Spread], single: impl Fn(usize) -> bool) -> usize {
    let (mut n, mut empty) = (1, false);
    for (k, s) in inputs.iter().enumerate() {
        if single(k) {
            n = n.max(s.len());
            empty |= s.is_empty();
        }
    }
    if n > 1 && empty {
        0
    } else {
        n
    }
}

/// A node's state handed from one compile of a patch to the next.
pub type CarriedState = Box<dyn std::any::Any + Send>;

/// Everything a factory needs to build a node instance.
pub struct NodeInit<'a> {
    pub id: &'a str,
    pub spec: &'a NodeSpec,
    pub params: &'a Params,
    /// Resolved input ports (after dynamic inputs).
    pub inputs: &'a [PortSpec],
}

pub type Factory = fn(&NodeInit) -> Box<dyn NodeImpl>;

// ─── Scalar nodes (`compute` in TS, wrapped by `applyKinded`) ────────────────

/// Adapts a per-call function over unwrapped values into a node:
/// `single` inputs arrive as their slot-0 value (`Undefined` when empty),
/// `spread` / `multi` inputs arrive as `Value::List`. Outputs are wrapped
/// the other way: a `spread` output must be a `List` (anything else is `[]`).
///
/// Lifted (see [`NodeImpl::lift_spreads`]), a spread on a `single` pin
/// calls `f` once per element instead and every output carries one value
/// per call (a `spread` output the items of each call's list, in order).
/// While every `single` pin holds exactly one value it takes the plain path
/// above, so the common case costs nothing extra.
pub struct Scalar<F> {
    f: F,
    in_kinds: Vec<PortKind>,
    out_kinds: Vec<PortKind>,
    lift: bool,
}

pub fn scalar<F>(init: &NodeInit, f: F) -> Box<dyn NodeImpl>
where
    F: FnMut(&[Value]) -> Result<Vec<Value>, String> + Send + 'static,
{
    Box::new(Scalar {
        f,
        in_kinds: init.inputs.iter().map(|p| p.kind).collect(),
        out_kinds: init.spec.outputs.iter().map(|p| p.kind).collect(),
        lift: false,
    })
}

fn scalar_arg((v, k): (&Spread, &PortKind)) -> Value {
    match k {
        PortKind::Single => v.first().cloned().unwrap_or(Value::Undefined),
        PortKind::Spread | PortKind::Multi => Value::List(v.clone()),
    }
}

impl<F> Scalar<F>
where
    F: FnMut(&[Value]) -> Result<Vec<Value>, String> + Send,
{
    /// `f` once per element `0..n` (see [`lift_len`]).
    fn eval_lifted(&mut self, inputs: &[Spread], n: usize) -> NodeResult {
        let mut outs: Vec<Vec<Value>> = self
            .out_kinds
            .iter()
            .map(|_| Vec::with_capacity(n))
            .collect();
        if n > 0 {
            let mut args: Vec<Value> = inputs.iter().zip(&self.in_kinds).map(scalar_arg).collect();
            for i in 0..n {
                for (slot, (s, k)) in args.iter_mut().zip(inputs.iter().zip(&self.in_kinds)) {
                    if *k == PortKind::Single && s.len() > 1 {
                        *slot = s[i % s.len()].clone();
                    }
                }
                let results = (self.f)(&args)?;
                if results.len() != self.out_kinds.len() {
                    return Err(format!(
                        "applyKinded output mismatch: expected {}, got {}",
                        self.out_kinds.len(),
                        results.len()
                    ));
                }
                for ((out, r), k) in outs.iter_mut().zip(results).zip(&self.out_kinds) {
                    match (k, r) {
                        (PortKind::Spread, Value::List(items)) => out.extend(items.iter().cloned()),
                        (PortKind::Spread, _) => {}
                        (_, r) => out.push(r),
                    }
                }
            }
        }
        Ok(outs.into_iter().map(Spread::from).collect())
    }
}

impl<F> NodeImpl for Scalar<F>
where
    F: FnMut(&[Value]) -> Result<Vec<Value>, String> + Send,
{
    fn eval(&mut self, inputs: &[Spread], _cx: &mut EvalCtx) -> NodeResult {
        if inputs.len() != self.in_kinds.len() {
            return Err(format!(
                "applyKinded arity mismatch: expected {} inputs, got {}",
                self.in_kinds.len(),
                inputs.len()
            ));
        }
        if self.lift {
            let n = lift_len(inputs, |k| self.in_kinds[k] == PortKind::Single);
            if n != 1 {
                return self.eval_lifted(inputs, n);
            }
        }
        let arg = scalar_arg;
        // Arguments on the stack for the usual small arities.
        const STACK: usize = 8;
        let results = if inputs.len() <= STACK {
            let mut args: [Value; STACK] = std::array::from_fn(|_| Value::Undefined);
            for (slot, a) in args.iter_mut().zip(inputs.iter().zip(&self.in_kinds)) {
                *slot = arg(a);
            }
            (self.f)(&args[..inputs.len()])?
        } else {
            let args: Vec<Value> = inputs.iter().zip(&self.in_kinds).map(arg).collect();
            (self.f)(&args)?
        };
        if results.len() != self.out_kinds.len() {
            return Err(format!(
                "applyKinded output mismatch: expected {}, got {}",
                self.out_kinds.len(),
                results.len()
            ));
        }
        Ok(results
            .into_iter()
            .zip(&self.out_kinds)
            .map(|(r, k)| match k {
                PortKind::Spread => match r {
                    Value::List(items) => items,
                    _ => crate::value::empty(),
                },
                _ => crate::value::one(r),
            })
            .collect())
    }

    fn lift_spreads(&mut self) -> bool {
        self.lift = true;
        true
    }
}

/// Per-element evaluation for a node written against single values that
/// isn't a [`Scalar`] (it reads params, or keeps state): lifted, a spread on
/// a `single` pin runs the inner node once per element with one-value
/// inputs (Expression's rule, see [`lift_len`]) and concatenates what each
/// run outputs. Unlifted, or while every `single` pin holds one value, the
/// inner node sees the inputs untouched.
pub struct PerElement {
    single: Vec<bool>,
    outputs: usize,
    lift: bool,
    /// One node for every element (pure nodes), or one per element.
    nodes: Vec<Box<dyn NodeImpl>>,
    /// Builds a node per element: state per element (see [`per_element_state`]).
    make: Option<Box<dyn FnMut() -> Box<dyn NodeImpl> + Send>>,
}

/// [`PerElement`] over a pure node: one instance runs every element.
pub fn per_element(init: &NodeInit, inner: Box<dyn NodeImpl>) -> Box<dyn NodeImpl> {
    Box::new(PerElement {
        single: init
            .inputs
            .iter()
            .map(|p| p.kind == PortKind::Single)
            .collect(),
        outputs: init.spec.outputs.len(),
        lift: false,
        nodes: vec![inner],
        make: None,
    })
}

/// [`PerElement`] keeping state per element: element `i` always runs the
/// `i`-th node `make` built (element 0 is the node an unlifted patch runs);
/// a spread that grows starts the new elements fresh, one that shrinks drops
/// the state of the elements gone.
pub fn per_element_state<M>(init: &NodeInit, mut make: M) -> Box<dyn NodeImpl>
where
    M: FnMut() -> Box<dyn NodeImpl> + Send + 'static,
{
    Box::new(PerElement {
        single: init
            .inputs
            .iter()
            .map(|p| p.kind == PortKind::Single)
            .collect(),
        outputs: init.spec.outputs.len(),
        lift: false,
        nodes: vec![make()],
        make: Some(Box::new(make)),
    })
}

impl NodeImpl for PerElement {
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        let n = if self.lift {
            lift_len(inputs, |k| self.single.get(k).copied().unwrap_or(false))
        } else {
            1
        };
        if self.make.is_some() {
            self.nodes.truncate(n.max(1));
        }
        if n == 1 {
            return self.nodes[0].eval(inputs, cx);
        }
        let mut outs: Vec<Vec<Value>> = vec![Vec::with_capacity(n); self.outputs];
        let mut args: Vec<Spread> = inputs.to_vec();
        for i in 0..n {
            for (k, (arg, s)) in args.iter_mut().zip(inputs).enumerate() {
                if self.single.get(k).copied().unwrap_or(false) && s.len() > 1 {
                    *arg = crate::value::one(s[i % s.len()].clone());
                }
            }
            let node = match &mut self.make {
                Some(make) => {
                    if self.nodes.len() <= i {
                        self.nodes.push(make());
                    }
                    &mut self.nodes[i]
                }
                None => &mut self.nodes[0],
            };
            let res = node.eval(&args, cx)?;
            if outs.len() < res.len() {
                outs.resize_with(res.len(), Vec::new);
            }
            for (out, r) in outs.iter_mut().zip(res) {
                out.extend(r.iter().cloned());
            }
        }
        Ok(outs.into_iter().map(Spread::from).collect())
    }

    fn memoise(&self) -> bool {
        self.nodes[0].memoise()
    }

    fn carry(&mut self) -> Option<CarriedState> {
        self.nodes[0].carry()
    }

    fn adopt(&mut self, state: CarriedState) {
        self.nodes[0].adopt(state);
    }

    fn lift_spreads(&mut self) -> bool {
        self.lift = true;
        true
    }
}

/// A node defined by a plain function over whole spreads (`computeRaw`).
pub struct Raw<F>(pub F);

impl<F> NodeImpl for Raw<F>
where
    F: FnMut(&[Spread], &mut EvalCtx) -> NodeResult + Send,
{
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        (self.0)(inputs, cx)
    }
}

pub fn raw<F>(f: F) -> Box<dyn NodeImpl>
where
    F: FnMut(&[Spread], &mut EvalCtx) -> NodeResult + Send + 'static,
{
    Box::new(Raw(f))
}

/// Opts a node out of the tick's memo (see [`NodeImpl::memoise`]): it is
/// evaluated every tick, e.g. because it polls a load or a device.
pub fn always_eval(inner: Box<dyn NodeImpl>) -> Box<dyn NodeImpl> {
    struct AlwaysEval(Box<dyn NodeImpl>);
    impl NodeImpl for AlwaysEval {
        fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
            self.0.eval(inputs, cx)
        }
        fn memoise(&self) -> bool {
            false
        }
        fn lift_spreads(&mut self) -> bool {
            self.0.lift_spreads()
        }
    }
    Box::new(AlwaysEval(inner))
}
