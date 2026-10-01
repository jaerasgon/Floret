//! Cellular automata and walkers: `elementary-ca.ts`, `gol-step.ts`,
//! `walk-step.ts`.

use super::{array_len, first, get, int_d, num_d, ok, sp};
use crate::js;
use crate::node::{EvalCtx, NodeResult};
use crate::value::{Spread, Value, Vec2};

/// `Math.max(1, Math.floor(Number(v ?? d)))`.
fn at_least_one(v: &Value, d: f64) -> f64 {
    js::max(1.0, int_d(v, d))
}

/// `out[idx] = v` where `idx` may be any number: only an in-range integer
/// index touches an element.
fn set_f(out: &mut [Value], idx: f64, v: Value) {
    if idx >= 0.0 && idx < out.len() as f64 && idx.fract() == 0.0 {
        out[idx as usize] = v;
    }
}

/// One step of a 1-D elementary CA (Wolfram rule), wrapping at the edges.
pub fn elementary_ca(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let row = sp(inputs, 0);
    let rule = js::max(0.0, js::min(255.0, int_d(&first(inputs, 1), 90.0)));
    let rule = js::to_int32(rule);
    let n = row.len();
    let bit = |i: usize| i32::from(row[i].truthy());
    ok(vec![(0..n)
        .map(|i| {
            let pattern = (bit((i + n - 1) % n) << 2) | (bit(i) << 1) | bit((i + 1) % n);
            Value::Bool((rule >> pattern) & 1 == 1)
        })
        .collect()])
}

/// A CA row with a single live cell at `index` (negative = middle).
pub fn ca_seed(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let count = at_least_one(&first(inputs, 0), 64.0);
    let idx_in = int_d(&first(inputs, 1), -1.0);
    let idx = if idx_in < 0.0 {
        (count / 2.0).floor()
    } else {
        js::min(count - 1.0, idx_in)
    };
    let mut out = vec![Value::Bool(false); array_len(count)?];
    set_f(&mut out, idx, Value::Bool(true));
    ok(vec![out])
}

/// Drop the first row of a flat grid and append `newRow` at the bottom.
pub fn scroll_up(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let grid = sp(inputs, 0);
    let new_row = sp(inputs, 1);
    let cols = at_least_one(&first(inputs, 2), 64.0);
    let len = grid.len() as f64;
    // `len % cols !== 0` is also true for a NaN / infinite `cols`.
    if grid.is_empty() || len % cols != 0.0 {
        return ok(vec![vec![]]);
    }
    let cols = cols as usize;
    let mut out: Vec<Value> = grid[cols..]
        .iter()
        .map(|v| Value::Bool(v.truthy()))
        .collect();
    out.extend((0..cols).map(|i| Value::Bool(get(new_row, i).truthy())));
    ok(vec![out])
}

/// The last `cols` cells of a flat grid.
pub fn last_row(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let grid = sp(inputs, 0);
    let cols = at_least_one(&first(inputs, 1), 64.0);
    if (grid.len() as f64) < cols {
        return ok(vec![vec![]]);
    }
    let cols = array_len(cols)?;
    ok(vec![grid[grid.len() - cols..]
        .iter()
        .map(|v| Value::Bool(v.truthy()))
        .collect()])
}

/// A flat grid with one live cell in the bottom row.
pub fn ca_grid_seed(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let cols = at_least_one(&first(inputs, 0), 64.0);
    let rows = at_least_one(&first(inputs, 1), 32.0);
    let idx_in = int_d(&first(inputs, 2), -1.0);
    let idx = if idx_in < 0.0 {
        (cols / 2.0).floor()
    } else {
        js::min(cols - 1.0, idx_in)
    };
    let mut out = vec![Value::Bool(false); array_len(cols * rows)?];
    set_f(&mut out, (rows - 1.0) * cols + idx, Value::Bool(true));
    ok(vec![out])
}

/// One Game-of-Life generation on a toroidal `cols × rows` grid.
pub fn gol_step(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let grid = sp(inputs, 0);
    let cols = at_least_one(&first(inputs, 1), 16.0);
    let rows = at_least_one(&first(inputs, 2), 16.0);
    if grid.len() as f64 != cols * rows {
        return ok(vec![vec![]]);
    }
    let (cols, rows) = (cols as usize, rows as usize);
    let alive = |x: usize, y: usize| grid[y * cols + x].truthy();
    let mut next = Vec::with_capacity(grid.len());
    for y in 0..rows {
        for x in 0..cols {
            let mut live = 0;
            for dy in [-1isize, 0, 1] {
                for dx in [-1isize, 0, 1] {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = (x as isize + dx + cols as isize) as usize % cols;
                    let ny = (y as isize + dy + rows as isize) as usize % rows;
                    if alive(nx, ny) {
                        live += 1;
                    }
                }
            }
            let cur = alive(x, y);
            next.push(Value::Bool(if cur {
                live == 2 || live == 3
            } else {
                live == 3
            }));
        }
    }
    ok(vec![next])
}

/// Random live cells (mulberry32 stream, `density` chance each).
pub fn gol_seed(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let cols = at_least_one(&first(inputs, 0), 16.0);
    let rows = at_least_one(&first(inputs, 1), 16.0);
    let density = js::max(0.0, js::min(1.0, num_d(&first(inputs, 2), 0.35)));
    let mut s = js::to_int32(num_d(&first(inputs, 3), 1.0)) as u32;
    if s == 0 {
        s = 1;
    }
    let n = array_len(cols * rows)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        s = s.wrapping_add(0x6d2b_79f5);
        let mut t = s;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        let r = f64::from((t ^ (t >> 14)) % 1_000_000) / 1_000_000.0;
        out.push(Value::Bool(r < density));
    }
    ok(vec![out])
}

/// Advance a vec2 trail by one hashed random-walk step.
pub fn walk_step(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let trail = sp(inputs, 0);
    let t = num_d(&first(inputs, 1), 0.0);
    let step = num_d(&first(inputs, 2), 4.0);
    let seed = int_d(&first(inputs, 3), 1.0);
    let bounds = js::max(1.0, num_d(&first(inputs, 4), 250.0));
    if trail.is_empty() {
        return ok(vec![vec![]]);
    }
    // The last entry that is a real `{x, y}` (null entries are skipped).
    let last = trail
        .iter()
        .rev()
        .find_map(|v| match v {
            Value::Vec2(p) => Some(*p),
            Value::Object(_) => match (v.get("x"), v.get("y")) {
                (Value::Num(x), Value::Num(y)) => Some(Vec2 { x, y }),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_default();
    // `(floor(t·60)·0x9e3779b9 + seed·0x85ebca6b) | 0` is float math first.
    let h = js::to_int32((t * 60.0).floor() * 2_654_435_769.0 + seed * 2_246_822_507.0) as u32;
    let h = (h ^ (h >> 16)).wrapping_mul(0x85eb_ca6b);
    let h = (h ^ (h >> 13)).wrapping_mul(0xc2b2_ae35);
    let angle = f64::from(h ^ (h >> 16)) / 4_294_967_295.0 * std::f64::consts::PI * 2.0;
    let mut nx = last.x + angle.cos() * step;
    let mut ny = last.y + angle.sin() * step;
    if nx < 250.0 - bounds || nx > 250.0 + bounds {
        nx = last.x - angle.cos() * step;
    }
    if ny < 250.0 - bounds || ny > 250.0 + bounds {
        ny = last.y - angle.sin() * step;
    }
    let mut next = trail[1..].to_vec();
    next.push(Value::Vec2(Vec2 { x: nx, y: ny }));
    ok(vec![next])
}
