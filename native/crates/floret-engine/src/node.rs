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
}

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
pub struct Scalar<F> {
    f: F,
    in_kinds: Vec<PortKind>,
    out_kinds: Vec<PortKind>,
}

pub fn scalar<F>(init: &NodeInit, f: F) -> Box<dyn NodeImpl>
where
    F: FnMut(&[Value]) -> Result<Vec<Value>, String> + Send + 'static,
{
    Box::new(Scalar {
        f,
        in_kinds: init.inputs.iter().map(|p| p.kind).collect(),
        out_kinds: init.spec.outputs.iter().map(|p| p.kind).collect(),
    })
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
        let arg = |(v, k): (&Spread, &PortKind)| match k {
            PortKind::Single => v.first().cloned().unwrap_or(Value::Undefined),
            PortKind::Spread | PortKind::Multi => Value::List(v.clone()),
        };
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
