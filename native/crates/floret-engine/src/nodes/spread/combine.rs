//! Combiners, filters and partitions: `combine.ts`, `filter.ts`,
//! `partition.ts`.

use super::{array_len, first, get, get_f, int_count, int_d, ok, slice, sp};
use crate::js;
use crate::node::{EvalCtx, NodeResult};
use crate::value::{Spread, Value};
use std::collections::HashSet;

// ─── combine.ts ──────────────────────────────────────────────────────────────

/// a0 b0 a1 b1 … up to the shorter length (concatenation if either is empty).
pub fn interleave(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let a = sp(inputs, 0);
    let b = sp(inputs, 1);
    let n = a.len().min(b.len());
    if n == 0 {
        return ok(vec![a.iter().chain(b).cloned().collect()]);
    }
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        out.push(a[i].clone());
        out.push(b[i].clone());
    }
    ok(vec![out])
}

pub fn uninterleave(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let even = src.iter().step_by(2).cloned().collect();
    let odd = src.iter().skip(1).step_by(2).cloned().collect();
    ok(vec![even, odd])
}

/// Cartesian product as two parallel spreads.
pub fn cross(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let a = sp(inputs, 0);
    let b = sp(inputs, 1);
    let mut out_a = Vec::with_capacity(a.len() * b.len());
    let mut out_b = Vec::with_capacity(a.len() * b.len());
    for x in a {
        for y in b {
            out_a.push(x.clone());
            out_b.push(y.clone());
        }
    }
    ok(vec![out_a, out_b])
}

pub fn cons(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let mut out = vec![first(inputs, 0)];
    out.extend_from_slice(sp(inputs, 1));
    ok(vec![out])
}

pub fn append(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let mut out = sp(inputs, 0).to_vec();
    out.push(first(inputs, 1));
    ok(vec![out])
}

/// Inject `marks` at `count` evenly spaced positions.
pub fn inject(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let marks = sp(inputs, 1);
    let n = int_count(&first(inputs, 2));
    if n == 0.0 || marks.is_empty() {
        return ok(vec![src.to_vec()]);
    }
    let total_f = src.len() as f64 + n;
    // NaN: the placement loop doesn't run and `new Array(NaN)` throws.
    // Infinity would spin forever in JS; treat it as the same RangeError.
    let total = array_len(total_f)?;
    let mut mark_at: HashSet<i64> = HashSet::new();
    let mut k = 0.0;
    while k < n {
        let mut pos = (((k + 0.5) * total_f) / n).floor();
        while mark_at.contains(&(pos as i64)) && pos < total_f - 1.0 {
            pos += 1.0;
        }
        while mark_at.contains(&(pos as i64)) && pos > 0.0 {
            pos -= 1.0;
        }
        mark_at.insert(pos as i64);
        k += 1.0;
    }
    let mut out = Vec::with_capacity(total);
    let (mut si, mut mi) = (0, 0);
    for i in 0..total {
        if mark_at.contains(&(i as i64)) {
            out.push(marks[mi % marks.len()].clone());
            mi += 1;
        } else {
            out.push(get(src, si));
            si += 1;
        }
    }
    ok(vec![out])
}

// ─── filter.ts ───────────────────────────────────────────────────────────────

/// `eqKey`: primitives as themselves, anything else as its JSON text.
#[derive(Clone, Debug)]
enum Key {
    Undefined,
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
}

fn eq_key(v: &Value) -> Key {
    match v {
        Value::Undefined => Key::Undefined,
        Value::Null => Key::Null,
        Value::Bool(b) => Key::Bool(*b),
        Value::Num(n) => Key::Num(*n),
        Value::Str(s) => Key::Str(s.to_string()),
        other => Key::Str(json_stringify(other)),
    }
}

impl Key {
    /// `===`.
    fn strict_eq(&self, other: &Key) -> bool {
        match (self, other) {
            (Key::Undefined, Key::Undefined) | (Key::Null, Key::Null) => true,
            (Key::Bool(a), Key::Bool(b)) => a == b,
            (Key::Num(a), Key::Num(b)) => a == b,
            (Key::Str(a), Key::Str(b)) => a == b,
            _ => false,
        }
    }
}

/// Hashable form for `Set` membership (SameValueZero: NaN equals NaN,
/// -0 equals +0).
#[derive(PartialEq, Eq, Hash)]
enum SetKey {
    Undefined,
    Null,
    Bool(bool),
    Num(u64),
    Str(String),
}

impl From<Key> for SetKey {
    fn from(k: Key) -> SetKey {
        match k {
            Key::Undefined => SetKey::Undefined,
            Key::Null => SetKey::Null,
            Key::Bool(b) => SetKey::Bool(b),
            Key::Num(n) if n.is_nan() => SetKey::Num(f64::NAN.to_bits()),
            Key::Num(n) => SetKey::Num((n + 0.0).to_bits()),
            Key::Str(s) => SetKey::Str(s),
        }
    }
}

/// `JSON.stringify` for the object shapes a spread can carry.
fn json_stringify(v: &Value) -> String {
    let mut out = String::new();
    write_json(v, &mut out);
    out
}

fn write_json(v: &Value, out: &mut String) {
    let num = |n: f64, out: &mut String| {
        if n.is_finite() {
            out.push_str(&js::num_to_string(n));
        } else {
            out.push_str("null");
        }
    };
    let fields = |pairs: &[(&str, f64)], out: &mut String| {
        out.push('{');
        for (i, (k, n)) in pairs.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{k}\":"));
            num(*n, out);
        }
        out.push('}');
    };
    match v {
        // Functions serialise like undefined (null inside arrays).
        Value::Undefined | Value::Null | Value::Style(_) => out.push_str("null"),
        Value::Scene(n) => write_json(&n.to_value(), out),
        Value::Handle(h) => match h.to_value() {
            Some(obj) => write_json(&obj, out),
            None => out.push_str("null"),
        },
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Num(n) => num(*n, out),
        Value::Str(s) => out.push_str(&serde_json::to_string(&**s).unwrap_or_default()),
        Value::Vec2(p) => fields(&[("x", p.x), ("y", p.y)], out),
        Value::Color(c) => fields(&[("r", c.r), ("g", c.g), ("b", c.b), ("a", c.a)], out),
        Value::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            let mut first = true;
            for (k, item) in map.iter() {
                if item.is_undefined() {
                    continue;
                }
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                write_json(item, out);
            }
            out.push('}');
        }
    }
}

/// Keep elements where the (cycled) mask is truthy.
pub fn sift(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let keep = sp(inputs, 1);
    if src.is_empty() || keep.is_empty() {
        return ok(vec![vec![]]);
    }
    ok(vec![src
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[i % keep.len()].truthy())
        .map(|(_, v)| v.clone())
        .collect()])
}

/// Drop duplicates (by `eqKey`), keeping first occurrences.
pub fn unique(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let mut seen: HashSet<SetKey> = HashSet::new();
    ok(vec![sp(inputs, 0)
        .iter()
        .filter(|v| seen.insert(eq_key(v).into()))
        .cloned()
        .collect()])
}

pub fn index_of(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let k = eq_key(&first(inputs, 1));
    let found = sp(inputs, 0)
        .iter()
        .position(|v| eq_key(v).strict_eq(&k))
        .map_or(-1.0, |i| i as f64);
    ok(vec![vec![Value::Num(found)]])
}

pub fn count_where(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let n = sp(inputs, 0).iter().filter(|v| v.truthy()).count();
    ok(vec![vec![Value::Num(n as f64)]])
}

pub fn indices_by_value(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let k = eq_key(&first(inputs, 1));
    ok(vec![sp(inputs, 0)
        .iter()
        .enumerate()
        .filter(|(_, v)| eq_key(v).strict_eq(&k))
        .map(|(i, _)| Value::Num(i as f64))
        .collect()])
}

// ─── partition.ts ────────────────────────────────────────────────────────────

/// Fixed-size groups: the flat spread plus each group's size.
pub fn partition(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let bin = js::max(1.0, int_count(&first(inputs, 1)));
    if src.is_empty() {
        return ok(vec![vec![], vec![]]);
    }
    let mut sizes = Vec::new();
    let mut remaining = src.len() as f64;
    while remaining > 0.0 {
        let s = js::min(bin, remaining);
        sizes.push(Value::Num(s));
        remaining -= s;
    }
    ok(vec![src.to_vec(), sizes])
}

/// Adjacent pairs: `s[0..n-1]` and `s[1..n]`.
pub fn pairwise(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    if src.len() < 2 {
        return ok(vec![vec![], vec![]]);
    }
    ok(vec![src[..src.len() - 1].to_vec(), src[1..].to_vec()])
}

/// The Nth group of a flat + groupSize pair.
pub fn at_group(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let flat = sp(inputs, 0);
    let sizes = sp(inputs, 1);
    if flat.is_empty() || sizes.is_empty() {
        return ok(vec![vec![]]);
    }
    let n = sizes.len() as f64;
    let mut idx = int_d(&first(inputs, 2), 0.0);
    if idx < 0.0 {
        idx += n;
    }
    if idx < 0.0 || idx >= n {
        return ok(vec![vec![]]);
    }
    let size_at = |v: &Value| js::max(0.0, int_d(v, 0.0));
    let mut start = 0.0;
    let mut i = 0.0;
    while i < idx {
        start += size_at(&get_f(sizes, i));
        i += 1.0;
    }
    let len = size_at(&get_f(sizes, idx));
    ok(vec![slice(flat, start, Some(start + len))])
}

/// Sliding windows of `size` (stride 1), flattened, plus each window's size.
pub fn windows(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let src = sp(inputs, 0);
    let size = js::max(1.0, int_count(&first(inputs, 1)));
    if src.is_empty() || size > src.len() as f64 {
        return ok(vec![vec![], vec![]]);
    }
    let num_win = src.len() as f64 - size + 1.0;
    array_len(num_win * size)?;
    let (num_win, size) = (num_win as usize, size as usize);
    let mut flat = Vec::with_capacity(num_win * size);
    for w in 0..num_win {
        flat.extend_from_slice(&src[w..w + size]);
    }
    ok(vec![flat, vec![Value::Num(size as f64); num_win]])
}

/// Fan a spread-of-spreads out to one pin per group (`count` pins, 1–12).
pub fn ungroup(inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
    let groups = sp(inputs, 0);
    let v = cx.param("count").map_or(Value::Num(3.0), Value::from_json);
    let n = ungroup_count(&v);
    ok((0..n)
        .map(|i| match get(groups, i) {
            Value::List(items) => items.to_vec(),
            Value::Undefined => Vec::new(),
            g => vec![g],
        })
        .collect())
}

/// `max(1, min(12, floor(Number(v ?? 1)) || 1))`.
fn ungroup_count(v: &Value) -> usize {
    let n = int_d(v, 1.0);
    let n = if n == 0.0 || n.is_nan() { 1.0 } else { n };
    js::max(1.0, js::min(12.0, n)) as usize
}
