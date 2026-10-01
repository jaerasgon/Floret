//! `spread/*` nodes. Ported from `src/nodes/spread-ops/`.
//!
//! Every spread node is a `computeRaw` node: it sees whole input spreads
//! and reads slot 0 itself (`inputs[i]?.[0]`). The helpers below reproduce
//! the JS idioms those nodes use — `Number(v ?? d)`, `intCount`,
//! `new Array(n)` (which throws on a bad length), `Array.prototype.slice`
//! and reads past the end (`undefined`).

mod automata;
mod combine;
mod generators;
mod jssort;
mod ops;

#[cfg(test)]
mod tests;

use super::Registrar;
use crate::js;
use crate::node::{raw, EvalCtx, NodeResult};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Spread, Value};
use std::sync::Arc;

type RawFn = fn(&[Spread], &mut EvalCtx) -> NodeResult;

const NODES: &[(&str, RawFn)] = &[
    // generators.ts
    ("spread/linear", generators::linear),
    ("spread/range", generators::range),
    ("spread/repeat", generators::repeat),
    ("spread/pick", generators::pick),
    ("spread/randomize", generators::randomize),
    ("spread/sine", generators::sine),
    ("spread/geometric", generators::geometric),
    ("spread/noise", generators::noise),
    // vec2-generators.ts
    ("spread/grid", generators::grid),
    ("spread/ring", generators::ring),
    ("spread/spiral", generators::spiral),
    ("spread/fibonacci", generators::fibonacci),
    // ops.ts
    ("spread/count", ops::count),
    ("spread/reverse", ops::reverse),
    ("spread/take", ops::take),
    ("spread/skip", ops::skip),
    ("spread/slice", ops::slice_node),
    ("spread/cycle", ops::cycle),
    ("spread/stack", ops::stack),
    ("spread/at", ops::at),
    ("spread/neighbor", ops::neighbor),
    ("spread/resample", ops::resample),
    ("spread/sort", ops::sort),
    ("spread/shuffle", ops::shuffle),
    // reorder.ts
    ("spread/rotate", ops::rotate),
    ("spread/permute", ops::permute),
    ("spread/sort-by-key", ops::sort_by_key),
    // slice-mutation.ts
    ("spread/set-slice", ops::set_slice),
    ("spread/insert-slice", ops::insert_slice),
    ("spread/remove-slice", ops::remove_slice),
    ("spread/swap", ops::swap),
    // combine.ts
    ("spread/interleave", combine::interleave),
    ("spread/uninterleave", combine::uninterleave),
    ("spread/cross", combine::cross),
    ("spread/cons", combine::cons),
    ("spread/append", combine::append),
    ("spread/inject", combine::inject),
    // filter.ts
    ("spread/sift", combine::sift),
    ("spread/unique", combine::unique),
    ("spread/index-of", combine::index_of),
    ("spread/count-where", combine::count_where),
    ("spread/indices-by-value", combine::indices_by_value),
    // partition.ts
    ("spread/partition", combine::partition),
    ("spread/pairwise", combine::pairwise),
    ("spread/at-group", combine::at_group),
    ("spread/windows", combine::windows),
    ("spread/ungroup", combine::ungroup),
    // merge.ts
    ("spread/merge", merge),
    // elementary-ca.ts
    ("spread/elementary-ca", automata::elementary_ca),
    ("spread/ca-seed", automata::ca_seed),
    ("spread/scroll-up", automata::scroll_up),
    ("spread/last-row", automata::last_row),
    ("spread/ca-grid-seed", automata::ca_grid_seed),
    // gol-step.ts
    ("spread/gol-step", automata::gol_step),
    ("spread/gol-seed", automata::gol_seed),
    // walk-step.ts
    ("spread/walk-step", automata::walk_step),
];

pub fn register(r: &mut Registrar) {
    // None of these nodes keep state across ticks, so a factory only has to
    // look the function up by type.
    for &(ty, _) in NODES {
        r.node(ty, |init| {
            let f = lookup(&init.spec.ty).expect("registered spread node");
            raw(f)
        });
    }
    r.dynamic("spread/merge", merge_slots);
}

fn lookup(ty: &str) -> Option<RawFn> {
    NODES.iter().find(|(t, _)| *t == ty).map(|&(_, f)| f)
}

// ─── merge.ts ────────────────────────────────────────────────────────────────

/// `buildSlots`: `slotCount` numbered spread pins `s1…sN` (at least one).
fn merge_slots(params: &Params) -> Vec<PortSpec> {
    let v = params
        .get("slotCount")
        .map(Value::from_json)
        .unwrap_or(Value::Undefined);
    // `Number(v) || 2`, floored, at least 1.
    let n = v.to_number();
    let n = if n == 0.0 || n.is_nan() { 2.0 } else { n };
    let n = js::max(1.0, n.floor());
    (1..=n as usize)
        .map(|i| {
            PortSpec::new(format!("s{i}"), PortType::Any)
                .label(i.to_string())
                .kind(PortKind::Spread)
        })
        .collect()
}

/// Concatenate every slot, in order.
fn merge(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let out: Vec<Value> = inputs.iter().flat_map(|s| s.iter().cloned()).collect();
    ok(vec![out])
}

// ─── JS helpers shared by the spread nodes ───────────────────────────────────

/// `inputs[i] ?? []`.
fn sp(inputs: &[Spread], i: usize) -> &[Value] {
    inputs.get(i).map(|s| &s[..]).unwrap_or(&[])
}

/// `inputs[i]?.[0]`.
fn first(inputs: &[Spread], i: usize) -> Value {
    super::shared::first(inputs, i)
}

/// `arr[i]` for an integer index (`undefined` past the end).
fn get(arr: &[Value], i: usize) -> Value {
    arr.get(i).cloned().unwrap_or(Value::Undefined)
}

/// `arr[x]` for any number: only an exact in-range integer is an element;
/// anything else (NaN, fractions, out of range) reads `undefined`.
fn get_f(arr: &[Value], x: f64) -> Value {
    if x >= 0.0 && x < arr.len() as f64 && x.fract() == 0.0 {
        arr[x as usize].clone()
    } else {
        Value::Undefined
    }
}

/// `Number(v ?? d)`.
fn num_d(v: &Value, d: f64) -> f64 {
    if v.is_nullish() {
        d
    } else {
        v.to_number()
    }
}

/// `Math.floor(Number(v ?? d))`.
fn int_d(v: &Value, d: f64) -> f64 {
    num_d(v, d).floor()
}

/// `intCount`: `Math.max(0, Math.floor(Number(v ?? 0)))` (NaN stays NaN).
fn int_count(v: &Value) -> f64 {
    js::max(0.0, int_d(v, 0.0))
}

/// `new Array(n)`: a RangeError unless `n` is a valid array length.
fn array_len(n: f64) -> Result<usize, String> {
    if n >= 0.0 && n <= f64::from(u32::MAX) && n.fract() == 0.0 {
        Ok(n as usize)
    } else {
        Err("RangeError: Invalid array length".into())
    }
}

/// `ToIntegerOrInfinity` then the relative-index clamp `slice`/`splice`
/// apply to a start/end argument.
fn rel_index(x: f64, len: usize) -> usize {
    let len_f = len as f64;
    let i = if x.is_nan() { 0.0 } else { x.trunc() };
    if i < 0.0 {
        js::max(len_f + i, 0.0) as usize
    } else {
        i.min(len_f) as usize
    }
}

/// `arr.slice(start, end)` (`end = None` → `undefined` → length).
fn slice(arr: &[Value], start: f64, end: Option<f64>) -> Vec<Value> {
    let from = rel_index(start, arr.len());
    let to = end.map_or(arr.len(), |e| rel_index(e, arr.len()));
    if from < to {
        arr[from..to].to_vec()
    } else {
        Vec::new()
    }
}

/// Wrap output columns into spreads.
fn ok(outs: Vec<Vec<Value>>) -> NodeResult {
    Ok(outs.into_iter().map(Arc::from).collect())
}

/// `hash32` from `src/nodes/random/prng.ts`, bit-exact.
fn hash32(x: f64, y: f64, seed: f64) -> u32 {
    let i = |v: f64| js::to_int32(v) as u32;
    let mut h = i(x).wrapping_mul(2_654_435_761);
    h = (h ^ i(y)).wrapping_mul(2_246_822_519);
    h = (h ^ i(seed)).wrapping_mul(3_266_489_917);
    h ^= h >> 16;
    h = h.wrapping_mul(668_265_263);
    h ^= h >> 13;
    h = h.wrapping_mul(374_761_393);
    h ^= h >> 16;
    h
}

/// `hashUnit`: hash → [0, 1).
fn hash_unit(x: f64, y: f64, seed: f64) -> f64 {
    f64::from(hash32(x, y, seed)) / 4_294_967_296.0
}
