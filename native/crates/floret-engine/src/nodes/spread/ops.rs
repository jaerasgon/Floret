//! Spread operators: `ops.ts`, `reorder.ts`, `slice-mutation.ts`.

use super::{
    array_len, first, get, get_f, hash32, int_count, int_d, jssort, num_d, ok, rel_index, slice, sp,
};
use crate::js;
use crate::node::{EvalCtx, NodeResult};
use crate::value::{Spread, Value};

// ─── ops.ts ──────────────────────────────────────────────────────────────────

pub fn count(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    ok(vec![vec![Value::Num(sp(inputs, 0).len() as f64)]])
}

pub fn reverse(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    ok(vec![sp(inputs, 0).iter().rev().cloned().collect()])
}

pub fn take(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let n = int_count(&first(inputs, 1));
    if n == 0.0 {
        return ok(vec![vec![]]);
    }
    ok(vec![slice(src, 0.0, Some(n))])
}

pub fn skip(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let n = int_count(&first(inputs, 1));
    ok(vec![slice(src, n, None)])
}

/// `start..end`, `end ≤ 0` counting from the end (0 = to the end).
pub fn slice_node(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let len = src.len() as f64;
    let start = int_d(&first(inputs, 1), 0.0);
    let end_in = int_d(&first(inputs, 2), 0.0);
    let end = if end_in == 0.0 { len } else { end_in };
    ok(vec![slice(
        src,
        start,
        Some(if end < 0.0 { len + end } else { end }),
    )])
}

pub fn cycle(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let n = int_count(&first(inputs, 1));
    if n == 0.0 || src.is_empty() {
        return ok(vec![vec![]]);
    }
    let n = array_len(n)?;
    ok(vec![src.iter().cycle().take(n).cloned().collect()])
}

pub fn stack(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let mut out = sp(inputs, 0).to_vec();
    out.extend_from_slice(sp(inputs, 1));
    ok(vec![out])
}

/// One element by index (negative from the end); out of range → null.
pub fn at(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    if src.is_empty() {
        return ok(vec![vec![Value::Null]]);
    }
    let len = src.len() as f64;
    let mut i = int_d(&first(inputs, 1), 0.0);
    if i < 0.0 {
        i += len;
    }
    if i < 0.0 || i >= len {
        return ok(vec![vec![Value::Null]]);
    }
    // NaN slips through both checks: `src[NaN]` is undefined.
    ok(vec![vec![get_f(src, i)]])
}

/// `src[index + offset]` with clamp / wrap / zero edges.
pub fn neighbor(inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let n = src.len() as f64;
    if src.is_empty() {
        return ok(vec![vec![Value::Null]]);
    }
    let idx = int_d(&first(inputs, 1), 0.0);
    let off = int_d(&first(inputs, 2), 0.0);
    let mut i = idx + off;
    // `params.param('edges', 'clamp')`; anything but wrap / zero clamps.
    let mode = cx
        .param("edges")
        .and_then(|v| v.as_str())
        .unwrap_or("clamp");
    if mode == "wrap" {
        i = ((i % n) + n) % n;
    } else if mode == "zero" {
        if i < 0.0 || i >= n {
            return ok(vec![vec![Value::Null]]);
        }
    } else if i < 0.0 {
        i = 0.0;
    } else if i >= n {
        i = n - 1.0;
    }
    ok(vec![vec![get_f(src, i)]])
}

/// Linear-interpolated resample to `count` values.
pub fn resample(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let count = int_count(&first(inputs, 1));
    if count == 0.0 || src.is_empty() {
        return ok(vec![vec![]]);
    }
    let num_at = |i: usize| num_d(&get(src, i), 0.0);
    if src.len() == 1 {
        let n = array_len(count)?;
        return ok(vec![vec![Value::Num(num_at(0)); n]]);
    }
    let n = array_len(count)?;
    if count == 1.0 {
        return ok(vec![vec![Value::Num(num_at(0))]]);
    }
    let last = (src.len() - 1) as f64;
    ok(vec![(0..n)
        .map(|i| {
            let t = (i as f64 / (count - 1.0)) * last;
            let lo = t.floor();
            let hi = js::min(last, lo + 1.0);
            let f = t - lo;
            Value::Num(num_at(lo as usize) * (1.0 - f) + num_at(hi as usize) * f)
        })
        .collect()])
}

/// Numeric sort with `(a, b) => a - b` (or `b - a`), V8 TimSort.
pub fn sort(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let desc = first(inputs, 1).truthy();
    let out = jssort::sort_values(src, |a, b| {
        if desc {
            b.to_number() - a.to_number()
        } else {
            a.to_number() - b.to_number()
        }
    });
    ok(vec![out])
}

/// Deterministic shuffle by `hash32(i, 0, seed) % (i + 1)`.
pub fn shuffle(inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
    let mut out = sp(inputs, 0).to_vec();
    let seed = f64::from(js::to_int32(cx.seed(num_d(&first(inputs, 1), 0.0))));
    for i in (1..out.len()).rev() {
        let j = hash32(i as f64, 0.0, seed) as usize % (i + 1);
        out.swap(i, j);
    }
    ok(vec![out])
}

// ─── reorder.ts ──────────────────────────────────────────────────────────────

/// Cyclic shift by N (negative shifts left).
pub fn rotate(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let n = src.len() as f64;
    if src.is_empty() {
        return ok(vec![vec![]]);
    }
    let mut s = int_d(&first(inputs, 1), 1.0) % n;
    if s < 0.0 {
        s += n;
    }
    if s == 0.0 {
        return ok(vec![src.to_vec()]);
    }
    // A NaN shift reads `src[NaN]` (undefined) for every slot.
    ok(vec![(0..src.len())
        .map(|i| get_f(src, (i as f64 - s + n) % n))
        .collect()])
}

/// Reorder by an index spread (negative from the end; out of range → null).
pub fn permute(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let idx = sp(inputs, 1);
    if src.is_empty() || idx.is_empty() {
        return ok(vec![vec![]]);
    }
    let n = src.len() as f64;
    ok(vec![idx
        .iter()
        .map(|v| {
            let mut k = num_d(v, 0.0).floor();
            if k < 0.0 {
                k += n;
            }
            if k >= 0.0 && k < n {
                src[k as usize].clone()
            } else {
                Value::Null
            }
        })
        .collect()])
}

/// Sort a spread by a parallel key spread (missing keys count as 0).
pub fn sort_by_key(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let keys = sp(inputs, 1);
    let desc = first(inputs, 2).truthy();
    if src.is_empty() {
        return ok(vec![vec![]]);
    }
    let mut pairs: Vec<(Value, f64)> = src
        .iter()
        .enumerate()
        .map(|(i, v)| (v.clone(), num_d(&get(keys, i), 0.0)))
        .collect();
    jssort::sort_by(&mut pairs, |a, b| if desc { b.1 - a.1 } else { a.1 - b.1 });
    ok(vec![pairs.into_iter().map(|p| p.0).collect()])
}

// ─── slice-mutation.ts ───────────────────────────────────────────────────────

/// `resolveIndex`: negative from the end, clamped to `[0, clamp_max]`.
/// NaN passes through unchanged.
fn resolve_index(i: f64, len: usize, clamp_max: usize) -> f64 {
    let mut idx = i;
    if idx < 0.0 {
        idx += len as f64;
    }
    if idx < 0.0 {
        idx = 0.0;
    }
    if idx > clamp_max as f64 {
        idx = clamp_max as f64;
    }
    idx
}

/// `out.splice(start, delete, ...items)` with JS argument coercion.
fn splice(out: &mut Vec<Value>, start: f64, delete: f64, items: &[Value]) {
    let start = rel_index(start, out.len());
    let delete = if delete.is_nan() { 0.0 } else { delete.trunc() };
    let delete = delete.clamp(0.0, (out.len() - start) as f64) as usize;
    out.splice(start..start + delete, items.iter().cloned());
}

pub fn set_slice(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let idx = resolve_index(
        int_d(&first(inputs, 1), 0.0),
        src.len(),
        src.len().saturating_sub(1),
    );
    let rep = sp(inputs, 2);
    if src.is_empty() {
        return ok(vec![rep.to_vec()]);
    }
    if rep.is_empty() {
        return ok(vec![src.to_vec()]);
    }
    let mut out = src.to_vec();
    splice(&mut out, idx, 1.0, rep);
    ok(vec![out])
}

pub fn insert_slice(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let ins = sp(inputs, 2);
    if ins.is_empty() {
        return ok(vec![src.to_vec()]);
    }
    let idx = resolve_index(int_d(&first(inputs, 1), 0.0), src.len(), src.len());
    let mut out = src.to_vec();
    splice(&mut out, idx, 0.0, ins);
    ok(vec![out])
}

pub fn remove_slice(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    if src.is_empty() {
        return ok(vec![vec![]]);
    }
    let idx = resolve_index(int_d(&first(inputs, 1), 0.0), src.len(), src.len() - 1);
    let n = int_count(&first(inputs, 2));
    if n == 0.0 {
        return ok(vec![src.to_vec()]);
    }
    let mut out = src.to_vec();
    splice(&mut out, idx, n, &[]);
    ok(vec![out])
}

pub fn swap(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    if src.len() < 2 {
        return ok(vec![src.to_vec()]);
    }
    let last = src.len() - 1;
    let i = resolve_index(int_d(&first(inputs, 1), 0.0), src.len(), last);
    let j = resolve_index(int_d(&first(inputs, 2), 1.0), src.len(), last);
    if i == j {
        return ok(vec![src.to_vec()]);
    }
    // A NaN index is a non-element property in JS: it reads `undefined`
    // and a write to it doesn't touch the elements.
    let mut out = src.to_vec();
    let tmp = get_f(&out, i);
    let vj = get_f(&out, j);
    if !i.is_nan() {
        out[i as usize] = vj;
    }
    if !j.is_nan() {
        out[j as usize] = tmp;
    }
    ok(vec![out])
}
