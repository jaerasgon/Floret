//! `spread/nearest` (Lookup's `nearest` op): for each of `points`, the
//! index of the nearest item of `spread`.
//!
//! A native-only node. The items go into a uniform grid of about one item a
//! cell; each query searches rings of cells outward from its own and stops
//! once no cell further out can hold anything nearer, so thousands of
//! points against thousands of items stay cheap. Ties go to the lower
//! index. Items that aren't points (null, numbers …) are never nearest; an
//! empty list gives −1 for every point.

use super::{ok, sp};
use crate::node::{EvalCtx, NodeResult};
use crate::nodes::shared::to_vec2;
use crate::value::{Value, Vec2};

/// Most grid cells along a side.
const MAX_SIDE: usize = 1024;

pub fn nearest(inputs: &[crate::value::Spread], _: &mut EvalCtx) -> NodeResult {
    let items: Vec<(usize, Vec2)> = sp(inputs, 0)
        .iter()
        .enumerate()
        .filter_map(|(i, v)| point(v).map(|p| (i, p)))
        .collect();
    let queries: Vec<Vec2> = sp(inputs, 1).iter().map(to_vec2).collect();
    let out = nearest_indices(&items, &queries)
        .into_iter()
        .map(|i| Value::Num(i.map_or(-1.0, |i| i as f64)))
        .collect();
    ok(vec![out])
}

/// A point value (vec2 or `{x, y}`), with finite coordinates.
fn point(v: &Value) -> Option<Vec2> {
    let p = match v {
        Value::Vec2(p) => *p,
        Value::Object(_) => match (v.get("x"), v.get("y")) {
            (Value::Num(x), Value::Num(y)) => Vec2 { x, y },
            _ => return None,
        },
        _ => return None,
    };
    (p.x.is_finite() && p.y.is_finite()).then_some(p)
}

/// The original index of the item nearest each query (`None` with no
/// items).
pub(crate) fn nearest_indices(items: &[(usize, Vec2)], queries: &[Vec2]) -> Vec<Option<usize>> {
    if items.is_empty() {
        return vec![None; queries.len()];
    }
    let grid = Grid::new(items);
    queries
        .iter()
        .map(|q| Some(grid.nearest(items, *q)))
        .collect()
}

/// Items bucketed by cell (`starts` / `order`, a counting sort).
struct Grid {
    x0: f64,
    y0: f64,
    cell: f64,
    cols: usize,
    rows: usize,
    /// `order[starts[c]..starts[c + 1]]`: the items (positions in `items`)
    /// in cell `c`.
    starts: Vec<usize>,
    order: Vec<usize>,
}

impl Grid {
    fn new(items: &[(usize, Vec2)]) -> Grid {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (_, p) in items {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        let (w, h) = ((x1 - x0).max(1e-9), (y1 - y0).max(1e-9));
        // About one item a cell.
        let cell = ((w * h / items.len() as f64).sqrt())
            .max(w.max(h) / MAX_SIDE as f64)
            .max(1e-9);
        let cols = ((w / cell).floor() as usize + 1).clamp(1, MAX_SIDE);
        let rows = ((h / cell).floor() as usize + 1).clamp(1, MAX_SIDE);
        let mut g = Grid {
            x0,
            y0,
            cell,
            cols,
            rows,
            starts: vec![0; cols * rows + 1],
            order: vec![0; items.len()],
        };
        let cells: Vec<usize> = items.iter().map(|(_, p)| g.cell_of(*p)).collect();
        for &c in &cells {
            g.starts[c + 1] += 1;
        }
        for c in 0..cols * rows {
            g.starts[c + 1] += g.starts[c];
        }
        let mut fill = g.starts.clone();
        for (k, &c) in cells.iter().enumerate() {
            g.order[fill[c]] = k;
            fill[c] += 1;
        }
        g
    }

    /// The cell (column, row) a point falls in, clamped to the grid.
    fn coords(&self, p: Vec2) -> (usize, usize) {
        let cx = ((p.x - self.x0) / self.cell).floor();
        let cy = ((p.y - self.y0) / self.cell).floor();
        let clamp = |v: f64, n: usize| {
            if v.is_nan() || v < 0.0 {
                0
            } else {
                (v as usize).min(n - 1)
            }
        };
        (clamp(cx, self.cols), clamp(cy, self.rows))
    }

    fn cell_of(&self, p: Vec2) -> usize {
        let (cx, cy) = self.coords(p);
        cy * self.cols + cx
    }

    /// Search rings of cells outward from the query's (clamped) cell. Any
    /// cell outside ring `r` is at least `r · cell` from the query, so the
    /// search stops once the best is nearer than that.
    fn nearest(&self, items: &[(usize, Vec2)], q: Vec2) -> usize {
        let (cx, cy) = self.coords(q);
        let (cx, cy) = (cx as isize, cy as isize);
        let mut best = (f64::INFINITY, usize::MAX);
        let max_ring = self.cols.max(self.rows) as isize;
        for r in 0..=max_ring {
            for gy in (cy - r)..=(cy + r) {
                if gy < 0 || gy >= self.rows as isize {
                    continue;
                }
                let edge_row = gy == cy - r || gy == cy + r;
                let mut gx = cx - r;
                while gx <= cx + r {
                    if gx >= 0 && gx < self.cols as isize {
                        let c = gy as usize * self.cols + gx as usize;
                        for &k in &self.order[self.starts[c]..self.starts[c + 1]] {
                            let (i, p) = items[k];
                            let d = (p.x - q.x).powi(2) + (p.y - q.y).powi(2);
                            if (d, i) < best {
                                best = (d, i);
                            }
                        }
                    }
                    // Inside the ring only its two side cells are new.
                    gx += if edge_row || r == 0 { 1 } else { 2 * r };
                }
            }
            let reach = r as f64 * self.cell;
            if best.1 != usize::MAX && best.0 <= reach * reach {
                break;
            }
        }
        best.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2 { x, y }
    }

    fn brute(items: &[(usize, Vec2)], q: Vec2) -> usize {
        items
            .iter()
            .map(|(i, p)| ((p.x - q.x).powi(2) + (p.y - q.y).powi(2), *i))
            .fold(
                (f64::INFINITY, usize::MAX),
                |a, b| if b < a { b } else { a },
            )
            .1
    }

    #[test]
    fn finds_the_nearest_item() {
        let items = vec![(0, v(0.0, 0.0)), (1, v(10.0, 0.0)), (2, v(0.0, 10.0))];
        let got = nearest_indices(&items, &[v(9.0, 1.0), v(-5.0, -5.0), v(1.0, 8.0)]);
        assert_eq!(got, vec![Some(1), Some(0), Some(2)]);
    }

    #[test]
    fn ties_go_to_the_lower_index() {
        let items = vec![(0, v(-1.0, 0.0)), (1, v(1.0, 0.0))];
        assert_eq!(nearest_indices(&items, &[v(0.0, 0.0)]), vec![Some(0)]);
        let same = vec![(3, v(2.0, 2.0)), (5, v(2.0, 2.0))];
        assert_eq!(nearest_indices(&same, &[v(0.0, 0.0)]), vec![Some(3)]);
    }

    #[test]
    fn no_items_gives_none() {
        assert_eq!(nearest_indices(&[], &[v(0.0, 0.0)]), vec![None]);
    }

    #[test]
    fn agrees_with_brute_force_inside_and_outside_the_grid() {
        // A clumped set (dense cluster plus a few far outliers) and queries
        // well beyond its bounds.
        let mut s = 12345u64;
        let mut rnd = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut items = Vec::new();
        for i in 0..400 {
            let p = if i % 50 == 0 {
                v(rnd() * 2000.0 - 1000.0, rnd() * 2000.0 - 1000.0)
            } else {
                v(rnd() * 60.0, rnd() * 40.0)
            };
            items.push((i * 2, p));
        }
        for _ in 0..500 {
            let q = v(rnd() * 3000.0 - 1500.0, rnd() * 3000.0 - 1500.0);
            assert_eq!(
                nearest_indices(&items, &[q])[0],
                Some(brute(&items, q)),
                "{q:?}"
            );
        }
    }

    #[test]
    fn skips_values_that_are_not_points() {
        use crate::nodes::value::harness::node;
        let mut n = node("spread/nearest", serde_json::json!({}));
        let out = n
            .eval(vec![
                vec![Value::Null, Value::Vec2(v(5.0, 5.0)), Value::Num(3.0)],
                vec![Value::Vec2(v(0.0, 0.0)), Value::Vec2(v(9.0, 9.0))],
            ])
            .unwrap();
        let nums = |o: &[Value]| o.iter().map(Value::to_number).collect::<Vec<_>>();
        assert_eq!(nums(&out[0]), vec![1.0, 1.0]);
        let out = n
            .eval(vec![vec![], vec![Value::Vec2(v(0.0, 0.0))]])
            .unwrap();
        assert_eq!(nums(&out[0]), vec![-1.0]);
    }
}
