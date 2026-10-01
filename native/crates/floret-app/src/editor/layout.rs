//! Auto-arrange: a left-to-right layered ("Sugiyama-lite") layout. Port of
//! `src/editor/autoLayout.ts`, step for step (same gaps, same sweeps, same
//! tie-breaking), so a patch tidies the same way in both apps.

use floret_engine::patch::{PatchFile, PatchNode, Position};
use indexmap::IndexMap;
use std::collections::HashMap;

const H_GAP: f64 = 40.0;
const V_GAP: f64 = 58.0;
const REGION_PAD: f64 = 30.0;
const REGION_TOP: f64 = 50.0;
const FALLBACK: (f64, f64) = (200.0, 100.0);

fn median(xs: &mut [f64]) -> f64 {
    if xs.is_empty() {
        return -1.0;
    }
    xs.sort_by(|a, b| a.total_cmp(b));
    let m = xs.len() / 2;
    if xs.len() % 2 == 1 {
        xs[m]
    } else {
        (xs[m - 1] + xs[m]) / 2.0
    }
}

struct Ctx<'a> {
    patch: &'a PatchFile,
    size_of_node: &'a dyn Fn(&PatchNode) -> (f64, f64),
    by_parent: IndexMap<Option<String>, Vec<usize>>,
    positions: HashMap<String, Position>,
    region_size: HashMap<String, (f64, f64)>,
}

impl Ctx<'_> {
    fn node(&self, id: &str) -> Option<&PatchNode> {
        self.patch.nodes.iter().find(|n| n.id == id)
    }

    fn size(&self, id: &str) -> (f64, f64) {
        if let Some(r) = self.region_size.get(id) {
            return *r;
        }
        let Some(n) = self.node(id) else {
            return FALLBACK;
        };
        let (w, h) = (self.size_of_node)(n);
        (
            if w > 0.0 { w } else { FALLBACK.0 },
            if h > 0.0 { h } else { FALLBACK.1 },
        )
    }

    /// Lay out one scope; returns its content extent (right, bottom).
    fn layout_scope(&mut self, parent: Option<&str>) -> (f64, f64) {
        let scope: Vec<&PatchNode> = self
            .by_parent
            .get(&parent.map(str::to_owned))
            .map(|v| v.iter().map(|&i| &self.patch.nodes[i]).collect())
            .unwrap_or_default();
        if scope.is_empty() {
            return (0.0, 0.0);
        }
        let ids: Vec<&str> = scope.iter().map(|n| n.id.as_str()).collect();
        let in_scope = |id: &str| ids.contains(&id);
        let mut outs: IndexMap<&str, Vec<&str>> = ids.iter().map(|&i| (i, vec![])).collect();
        let mut ins: IndexMap<&str, Vec<&str>> = ids.iter().map(|&i| (i, vec![])).collect();
        for e in &self.patch.edges {
            if in_scope(&e.source) && in_scope(&e.target) && e.source != e.target {
                outs[e.source.as_str()].push(e.target.as_str());
                ins[e.target.as_str()].push(e.source.as_str());
            }
        }

        // 1. Longest-path layering (Kahn); insertion order mirrors the JS Map.
        let mut indeg: HashMap<&str, usize> = ids.iter().map(|&i| (i, ins[i].len())).collect();
        let mut depth: IndexMap<&str, usize> = IndexMap::new();
        let mut queue = std::collections::VecDeque::new();
        for &id in &ids {
            if indeg[id] == 0 {
                depth.insert(id, 0);
                queue.push_back(id);
            }
        }
        while let Some(id) = queue.pop_front() {
            let d = depth[id];
            for &t in &outs[id] {
                let cur = depth.get(t).copied().unwrap_or(0);
                depth.insert(t, cur.max(d + 1));
                let left = indeg.get_mut(t).expect("in scope");
                *left -= 1;
                if *left == 0 {
                    queue.push_back(t);
                }
            }
        }
        for &id in &ids {
            depth.entry(id).or_insert(0); // cycle / detached → column 0
        }
        let mut max_d = depth.values().copied().max().unwrap_or(0);
        for n in &scope {
            if n.def_type == "meta/comment" {
                depth.insert(n.id.as_str(), max_d + 1);
            }
        }
        max_d = depth.values().copied().max().unwrap_or(0);

        let mut cols: Vec<Vec<&str>> = vec![Vec::new(); max_d + 1];
        for (&id, &d) in &depth {
            cols[d].push(id);
        }
        // Seed order by current Y (keeps hand-tuned layouts roughly stable).
        let pos = |id: &str| self.node(id).map(|n| n.position).unwrap_or_default();
        for c in &mut cols {
            c.sort_by(|a, b| {
                let (pa, pb) = (pos(a), pos(b));
                pa.y.total_cmp(&pb.y).then(pa.x.total_cmp(&pb.x))
            });
        }

        // 2. Crossing minimisation: median sweeps.
        for sweep in 0..6 {
            let down = sweep % 2 == 0;
            let rank: HashMap<&str, f64> = cols
                .iter()
                .flat_map(|c| c.iter().enumerate().map(|(i, &id)| (id, i as f64)))
                .collect();
            let order: Vec<usize> = if down {
                (1..=max_d).collect()
            } else {
                (0..max_d).rev().collect()
            };
            for d in order {
                let neigh = if down { &ins } else { &outs };
                let mut keyed: Vec<(&str, f64)> = cols[d]
                    .iter()
                    .enumerate()
                    .map(|(i, &id)| {
                        let mut rs: Vec<f64> = neigh[id]
                            .iter()
                            .filter_map(|n| rank.get(n).copied())
                            .filter(|&v| v >= 0.0)
                            .collect();
                        let m = median(&mut rs);
                        (id, if m >= 0.0 { m } else { i as f64 })
                    })
                    .collect();
                keyed.sort_by(|a, b| a.1.total_cmp(&b.1));
                cols[d] = keyed.into_iter().map(|(id, _)| id).collect();
            }
        }

        // 3. Coordinates.
        let col_w: Vec<f64> = cols
            .iter()
            .map(|c| {
                c.iter()
                    .map(|id| self.size(id).0)
                    .fold(FALLBACK.0, f64::max)
            })
            .collect();
        let mut col_x = Vec::with_capacity(cols.len());
        let mut x = 0.0;
        for w in &col_w {
            col_x.push(x);
            x += w + H_GAP;
        }
        let mut center_y: HashMap<&str, f64> = HashMap::new();
        for c in &cols {
            let mut prev_bottom = f64::NEG_INFINITY;
            for &id in c {
                let h = self.size(id).1;
                let bary: Vec<f64> = ins[id]
                    .iter()
                    .filter_map(|n| center_y.get(n).copied())
                    .collect();
                let mut cy = if !bary.is_empty() {
                    bary.iter().sum::<f64>() / bary.len() as f64
                } else if prev_bottom == f64::NEG_INFINITY {
                    h / 2.0
                } else {
                    prev_bottom + V_GAP + h / 2.0
                };
                if cy - h / 2.0 < prev_bottom + V_GAP {
                    cy = prev_bottom + V_GAP + h / 2.0;
                }
                center_y.insert(id, cy);
                prev_bottom = cy + h / 2.0;
            }
        }
        let (ox, oy) = if parent.is_some() {
            (REGION_PAD, REGION_TOP)
        } else {
            (0.0, 0.0)
        };
        let min_top = ids
            .iter()
            .map(|id| center_y[id] - self.size(id).1 / 2.0)
            .fold(f64::INFINITY, f64::min);
        let shift = oy - if min_top.is_finite() { min_top } else { 0.0 };

        let (mut right, mut bottom) = (0.0f64, 0.0f64);
        let mut placed = Vec::new();
        for (d, c) in cols.iter().enumerate() {
            for &id in c {
                let (w, h) = self.size(id);
                let px = ox + col_x[d] + (col_w[d] - w) / 2.0;
                let py = center_y[id] - h / 2.0 + shift;
                placed.push((id.to_owned(), Position { x: px, y: py }));
                right = right.max(px + w);
                bottom = bottom.max(py + h);
            }
        }
        self.positions.extend(placed);
        (right, bottom)
    }

    /// Post-order: inner regions are laid out and sized before their scope.
    fn process(&mut self, parent: Option<&str>) -> (f64, f64) {
        let children: Vec<String> = self
            .by_parent
            .get(&parent.map(str::to_owned))
            .map(|v| v.iter().map(|&i| self.patch.nodes[i].id.clone()).collect())
            .unwrap_or_default();
        for id in children {
            if self.by_parent.contains_key(&Some(id.clone())) {
                let (r, b) = self.process(Some(&id));
                self.region_size
                    .insert(id, (r + REGION_PAD, b + REGION_PAD));
            }
        }
        self.layout_scope(parent)
    }
}

/// Tidy every scope of the patch in place. `size_of` gives a node's
/// on-canvas size (zero → the 200×100 fallback). Regions with contents
/// are resized to fit them.
pub fn auto_layout(patch: &mut PatchFile, size_of: &dyn Fn(&PatchNode) -> (f64, f64)) {
    let mut by_parent: IndexMap<Option<String>, Vec<usize>> = IndexMap::new();
    for (i, n) in patch.nodes.iter().enumerate() {
        by_parent.entry(n.parent_id.clone()).or_default().push(i);
    }
    let (positions, region_size) = {
        let mut ctx = Ctx {
            patch,
            size_of_node: size_of,
            by_parent,
            positions: HashMap::new(),
            region_size: HashMap::new(),
        };
        ctx.process(None);
        (ctx.positions, ctx.region_size)
    };
    for n in &mut patch.nodes {
        if let Some(p) = positions.get(&n.id) {
            n.position = *p;
        }
        if let Some(&(w, h)) = region_size.get(&n.id) {
            n.width = Some(w);
            n.height = Some(h);
            for (k, v) in [("width", w), ("height", h)] {
                if n.params.contains_key(k) {
                    n.params.insert(k.into(), v.into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{edge, node, patch};

    fn headless(_: &PatchNode) -> (f64, f64) {
        (0.0, 0.0)
    }

    fn at(p: &PatchFile, id: &str) -> Position {
        p.nodes.iter().find(|n| n.id == id).unwrap().position
    }

    fn e(s: &str, t: &str) -> floret_engine::patch::PatchEdge {
        edge(s, "out-result", t, "in-a")
    }

    #[test]
    fn chain_left_to_right() {
        let mut p = patch(
            vec![
                node("a", "math/add"),
                node("b", "math/add"),
                node("c", "math/add"),
            ],
            vec![e("a", "b"), e("b", "c")],
        );
        auto_layout(&mut p, &headless);
        assert!(at(&p, "a").x < at(&p, "b").x);
        assert!(at(&p, "b").x < at(&p, "c").x);
        assert_eq!(at(&p, "a"), Position { x: 0.0, y: 0.0 });
        assert_eq!(at(&p, "b").x, 240.0);
    }

    #[test]
    fn same_layer_stacks_and_fan_in_centres() {
        let mut p = patch(
            vec![
                node("a", "math/add"),
                node("b", "math/add"),
                node("c", "math/add"),
            ],
            vec![e("a", "c"), e("b", "c")],
        );
        auto_layout(&mut p, &headless);
        let (a, b, c) = (at(&p, "a"), at(&p, "b"), at(&p, "c"));
        assert!((a.x - b.x).abs() < 1.0);
        assert!((a.y - b.y).abs() >= 100.0);
        assert!(c.y > a.y.min(b.y) - 1.0 && c.y < a.y.max(b.y) + 1.0);
        // Exact numbers from the TS algorithm: b sits 100 + 58 below a,
        // c at the barycentre.
        assert_eq!(b.y - a.y, 158.0);
        assert_eq!(c.y, 79.0);
    }

    #[test]
    fn region_grows_to_fit_children() {
        let mut r = node("r", "region/foreach");
        r.width = Some(100.0);
        r.height = Some(100.0);
        let mut c1 = node("c1", "math/add");
        c1.parent_id = Some("r".into());
        let mut c2 = node("c2", "math/add");
        c2.parent_id = Some("r".into());
        let mut p = patch(vec![r, c1, c2], vec![]);
        auto_layout(&mut p, &|n| match (n.width, n.height) {
            (Some(w), Some(h)) => (w, h),
            _ => (0.0, 0.0),
        });
        let r = p.nodes.iter().find(|n| n.id == "r").unwrap();
        assert!(r.width.unwrap() > 100.0);
        assert!(r.height.unwrap() > 100.0);
        assert_eq!(at(&p, "c1").x, REGION_PAD);
        assert_eq!(at(&p, "c1").y, REGION_TOP);
    }

    #[test]
    fn comments_park_far_right() {
        let mut p = patch(
            vec![
                node("a", "math/add"),
                node("b", "math/add"),
                node("cm", "meta/comment"),
            ],
            vec![e("a", "b")],
        );
        auto_layout(&mut p, &headless);
        assert!(at(&p, "cm").x > at(&p, "b").x);
    }

    #[test]
    fn deterministic_and_idempotent() {
        let mk = || {
            patch(
                (0..12)
                    .map(|i| {
                        let mut n = node(&format!("n{i}"), "math/add");
                        n.position = Position {
                            x: (i * 37 % 11) as f64 * 50.0,
                            y: (i * 53 % 7) as f64 * 40.0,
                        };
                        n
                    })
                    .collect(),
                vec![
                    e("n0", "n3"),
                    e("n1", "n3"),
                    e("n2", "n4"),
                    e("n3", "n5"),
                    e("n4", "n5"),
                    e("n5", "n6"),
                    e("n7", "n6"),
                    e("n8", "n9"),
                    e("n9", "n10"),
                    e("n2", "n10"),
                ],
            )
        };
        let (mut p1, mut p2) = (mk(), mk());
        auto_layout(&mut p1, &headless);
        auto_layout(&mut p2, &headless);
        let pos = |p: &PatchFile| p.nodes.iter().map(|n| n.position).collect::<Vec<_>>();
        assert_eq!(pos(&p1), pos(&p2));
        let once = pos(&p1);
        auto_layout(&mut p1, &headless);
        assert_eq!(pos(&p1), once, "re-arranging a tidy patch changes nothing");
    }
}
