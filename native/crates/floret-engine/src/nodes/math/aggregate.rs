//! Aggregations over a number spread. Port of `src/nodes/math/aggregate.ts`.
//!
//! Every element is coerced with `Number(x)`; non-numeric elements are NaN.

use crate::node::{raw, NodeImpl, NodeInit};
use crate::nodes::shared::first;
use crate::value::{one, Spread, Value};
use std::sync::Arc;

fn as_numbers(inputs: &[Spread]) -> Vec<f64> {
    inputs
        .first()
        .map(|s| s.iter().map(Value::to_number).collect())
        .unwrap_or_default()
}

fn num_spread(xs: Vec<f64>) -> Spread {
    Arc::from(xs.into_iter().map(Value::Num).collect::<Vec<_>>())
}

fn sum_of(src: &[f64]) -> f64 {
    let mut s = 0.0;
    for x in src {
        s += x;
    }
    s
}

pub fn sum(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| Ok(vec![one(Value::Num(sum_of(&as_numbers(inputs))))]))
}

pub fn mean(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let src = as_numbers(inputs);
        let m = if src.is_empty() {
            0.0
        } else {
            sum_of(&src) / src.len() as f64
        };
        Ok(vec![one(Value::Num(m))])
    })
}

/// Min / max by `<` / `>` scans (a NaN first element sticks; later NaNs
/// are skipped) — not `Math.min`.
fn extreme(src: &[f64], better: fn(f64, f64) -> bool) -> f64 {
    let Some((&first, rest)) = src.split_first() else {
        return 0.0;
    };
    let mut m = first;
    for &x in rest {
        if better(x, m) {
            m = x;
        }
    }
    m
}

pub fn min_spread(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let m = extreme(&as_numbers(inputs), |x, m| x < m);
        Ok(vec![one(Value::Num(m))])
    })
}

pub fn max_spread(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let m = extreme(&as_numbers(inputs), |x, m| x > m);
        Ok(vec![one(Value::Num(m))])
    })
}

pub fn median(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let mut sorted = as_numbers(inputs);
        if sorted.is_empty() {
            return Ok(vec![one(Value::Num(0.0))]);
        }
        sort_by_difference(&mut sorted);
        let mid = sorted.len() >> 1;
        let m = if sorted.len() % 2 == 0 {
            (sorted[mid - 1] + sorted[mid]) / 2.0
        } else {
            sorted[mid]
        };
        Ok(vec![one(Value::Num(m))])
    })
}

/// `xs.sort((a, b) => a - b)` as V8 runs it.
///
/// Without NaNs the comparator is a consistent total preorder, so any
/// stable sort gives V8's (stable TimSort) result. A NaN compares "equal"
/// to everything, making the order algorithm-dependent: for that case this
/// reproduces V8's TimSort for arrays shorter than 64 (one run, extended by
/// binary insertion sort). Longer arrays containing NaN go through the same
/// insertion pass, which can differ from V8's merge phase.
pub fn sort_by_difference(xs: &mut [f64]) {
    if !xs.iter().any(|x| x.is_nan()) {
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        return;
    }
    let n = xs.len();
    if n < 2 {
        return;
    }
    // V8 maps a NaN comparator result to 0 ("equal").
    let cmp = |a: f64, b: f64| {
        let d = a - b;
        if d.is_nan() {
            0.0
        } else {
            d
        }
    };
    // CountAndMakeRun(0, n)
    let descending = cmp(xs[1], xs[0]) < 0.0;
    let mut run = 2;
    let mut prev = xs[1];
    for &cur in &xs[2..] {
        let order = cmp(cur, prev);
        let stop = if descending {
            order >= 0.0
        } else {
            order < 0.0
        };
        if stop {
            break;
        }
        prev = cur;
        run += 1;
    }
    if descending {
        xs[..run].reverse();
    }
    // BinaryInsertionSort(0, run, n)
    for start in run..n {
        let pivot = xs[start];
        let (mut left, mut right) = (0, start);
        while left < right {
            let mid = left + ((right - left) >> 1);
            if cmp(pivot, xs[mid]) < 0.0 {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        xs.copy_within(left..start, left + 1);
        xs[left] = pivot;
    }
}

pub fn stddev(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let src = as_numbers(inputs);
        if src.is_empty() {
            return Ok(vec![one(Value::Num(0.0))]);
        }
        let mean = sum_of(&src) / src.len() as f64;
        let mut v = 0.0;
        for x in &src {
            let d = x - mean;
            v += d * d;
        }
        Ok(vec![one(Value::Num((v / src.len() as f64).sqrt()))])
    })
}

pub fn cumulative(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let mut s = 0.0;
        let out: Vec<f64> = as_numbers(inputs)
            .into_iter()
            .map(|x| {
                s += x;
                s
            })
            .collect();
        Ok(vec![num_spread(out)])
    })
}

pub fn delta(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let src = as_numbers(inputs);
        let out: Vec<f64> = src.windows(2).map(|w| w[1] - w[0]).collect();
        Ok(vec![num_spread(out)])
    })
}

/// `Number(inputs[i]?.[0] ?? fallback)`.
fn num_or(inputs: &[Spread], i: usize, fallback: f64) -> f64 {
    match first(inputs, i) {
        Value::Undefined | Value::Null => fallback,
        v => v.to_number(),
    }
}

pub fn histogram(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let src = as_numbers(inputs);
        let bins = crate::js::max(1.0, num_or(inputs, 1, 10.0).floor());
        // `new Array(bins)` throws a RangeError for NaN / huge lengths.
        if bins.is_nan() || bins > 4_294_967_295.0 {
            return Err("RangeError: Invalid array length".into());
        }
        let nbins = bins as usize;
        let min = num_or(inputs, 2, 0.0);
        let max = num_or(inputs, 3, 1.0);
        let mut counts = vec![0.0; nbins];
        if !src.is_empty() && max != min {
            let span = max - min;
            for x in &src {
                let t = (x - min) / span;
                if !t.is_finite() {
                    continue;
                }
                let b = (t * bins).floor();
                let b = if b < 0.0 {
                    0
                } else if b >= bins {
                    nbins - 1
                } else {
                    b as usize
                };
                counts[b] += 1.0;
            }
        }
        Ok(vec![num_spread(counts)])
    })
}
