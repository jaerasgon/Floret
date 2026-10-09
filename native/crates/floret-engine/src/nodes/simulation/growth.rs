//! `growth`: differential growth, a curve that grows like coral or the
//! folds of a brain.
//!
//! The curve starts as the first outline of the wired `shape` (closed or
//! open, read when the simulation starts) or a circle of `radius`. Every
//! step each node is pushed away from every other node within `repel`
//! (harder the closer) and pulled `align` of the way towards the middle of
//! its two neighbours (which keeps the curve smooth), moving at most
//! `speed` units; then `grow` new nodes a second go in at the middles of
//! edges picked at random, and every edge longer than `maxEdge` splits in
//! two, until there are `maxNodes`. A new node crowds its neighbours, they
//! push apart, the extra length has nowhere to go but sideways, and the
//! curve folds. Nodes stay inside
//! `size`; the ends of an open curve move like the rest.
//!
//! The layer draws the curve as one path in the `style` (a white line
//! unwired); with `colors` wired the edges are coloured by age, oldest
//! first, over the fill (if the style has one). `points` are the nodes in
//! order, `values` their age (0 = from the start, 1 = newest), `from` /
//! `to` the edges.

use super::{line, sim_rng, Ins, Out, Rng, Sim, Styles, STEP};
use crate::scene::SceneNode;
use crate::value::Vec2;
use std::f64::consts::TAU;
use std::sync::Arc;

/// Most nodes a curve may have.
pub(crate) const MAX_NODES: usize = 8000;

pub(crate) struct Growth {
    seed: u64,
    size: Vec2,
    closed: bool,
    rng: Rng,
    pos: Vec<Vec2>,
    /// When each node was made (steps).
    born: Vec<u32>,
    steps: u32,
    /// New nodes owed by `grow` (fractional).
    owed: f64,
    /// The start's outline (to notice a new one).
    start: Vec<Vec2>,
}

fn size(ins: &Ins) -> Vec2 {
    ins.size(Vec2 { x: 600.0, y: 600.0 })
}

/// The starting curve: the first outline of `shape` with at least two
/// points, else a circle of `radius` round the origin.
fn start_curve(ins: &Ins) -> (Vec<Vec2>, bool) {
    let contours = crate::nodes::geometry::layer_contours(&ins.spread("shape"));
    if let Some(c) = contours.iter().find(|c| c.pts.len() >= 2) {
        let pts: Vec<Vec2> = c
            .pts
            .iter()
            .copied()
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .collect();
        if pts.len() >= 2 {
            return (pts, c.closed);
        }
    }
    let r = ins.num("radius", 40.0).abs().clamp(1.0, 10_000.0);
    // A node every 4 units or so (`maxEdge` splits it further).
    let n = ((TAU * r / 4.0).ceil() as usize).clamp(8, 512);
    let pts = (0..n)
        .map(|i| {
            let a = TAU * i as f64 / n as f64;
            Vec2 {
                x: r * a.cos(),
                y: r * a.sin(),
            }
        })
        .collect();
    (pts, true)
}

fn max_edge(ins: &Ins) -> f64 {
    ins.num("maxEdge", 5.0).clamp(0.5, 1000.0)
}

impl Growth {
    pub(crate) fn new(ins: &Ins) -> Growth {
        let (start, closed) = start_curve(ins);
        let mut g = Growth {
            seed: ins.seed(),
            size: size(ins),
            closed,
            rng: sim_rng(ins.seed()),
            pos: Vec::new(),
            born: Vec::new(),
            steps: 0,
            owed: 0.0,
            start: start.clone(),
        };
        // Long edges of a wired shape split straight away.
        g.pos = start;
        g.born = vec![0; g.pos.len()];
        g.split(max_edge(ins), ins.count("maxNodes", 2000, 3, MAX_NODES), 0);
        g
    }

    /// Split every edge longer than `edge` and `extra` edges picked at
    /// random (while under `cap` nodes).
    #[allow(clippy::needless_range_loop)] // pos, born and picked in step
    fn split(&mut self, edge: f64, cap: usize, extra: usize) {
        let n = self.pos.len();
        if n < 2 {
            return;
        }
        let edges = if self.closed { n } else { n - 1 };
        let mut picked = vec![false; edges];
        for _ in 0..extra.min(edges) {
            let e = (self.rng.f64() * edges as f64) as usize;
            picked[e.min(edges - 1)] = true;
        }
        let mut pos = Vec::with_capacity(n * 2);
        let mut born = Vec::with_capacity(n * 2);
        let mut room = cap.saturating_sub(n);
        for i in 0..n {
            pos.push(self.pos[i]);
            born.push(self.born[i]);
            if i < edges && room > 0 {
                let (a, b) = (self.pos[i], self.pos[(i + 1) % n]);
                let d = (b.x - a.x).hypot(b.y - a.y);
                if d > edge || picked[i] {
                    pos.push(Vec2 {
                        x: (a.x + b.x) / 2.0,
                        y: (a.y + b.y) / 2.0,
                    });
                    born.push(self.steps);
                    room -= 1;
                }
            }
        }
        self.pos = pos;
        self.born = born;
    }
}

impl Sim for Growth {
    fn stale(&self, ins: &Ins) -> bool {
        ins.seed() != self.seed || size(ins) != self.size || start_curve(ins).0 != self.start
    }

    fn step(&mut self, ins: &Ins) {
        let n = self.pos.len();
        if n < 2 {
            return;
        }
        let repel = ins.num("repel", 10.0).clamp(0.5, 1000.0);
        let align = ins.num("align", 0.5).clamp(0.0, 1.0);
        let force = ins.num("force", 1.0).clamp(0.0, 10.0);
        let speed = ins.num("speed", 1.0).clamp(0.0, 100.0);
        // Bucket the nodes `repel` wide over the bounds (plus a margin).
        let (hw, hh) = (self.size.x / 2.0, self.size.y / 2.0);
        let cols = ((self.size.x / repel).ceil() as usize).clamp(1, 1024);
        let rows = ((self.size.y / repel).ceil() as usize).clamp(1, 1024);
        let (cw, ch) = (self.size.x / cols as f64, self.size.y / rows as f64);
        let cell = |p: Vec2| {
            let i = (((p.x + hw) / cw).floor().max(0.0) as usize).min(cols - 1);
            let j = (((p.y + hh) / ch).floor().max(0.0) as usize).min(rows - 1);
            (i, j)
        };
        let mut head = vec![u32::MAX; cols * rows];
        let mut next = vec![u32::MAX; n];
        for (k, p) in self.pos.iter().enumerate() {
            let (i, j) = cell(*p);
            next[k] = head[j * cols + i];
            head[j * cols + i] = k as u32;
        }
        let r2 = repel * repel;
        let reach_i = (repel / cw).ceil() as isize;
        let reach_j = (repel / ch).ceil() as isize;
        let mut moved = Vec::with_capacity(n);
        for k in 0..n {
            let p = self.pos[k];
            // Repulsion from every node in reach (neighbours included:
            // that is what keeps edges from collapsing).
            let (mut rx, mut ry) = (0.0, 0.0);
            let (ci, cj) = cell(p);
            for dj in -reach_j..=reach_j {
                for di in -reach_i..=reach_i {
                    let (i, j) = (ci as isize + di, cj as isize + dj);
                    if i < 0 || j < 0 || i as usize >= cols || j as usize >= rows {
                        continue;
                    }
                    let mut q = head[j as usize * cols + i as usize];
                    while q != u32::MAX {
                        let o = self.pos[q as usize];
                        let (dx, dy) = (p.x - o.x, p.y - o.y);
                        let d2 = dx * dx + dy * dy;
                        if q as usize != k && d2 < r2 && d2 > 1e-18 {
                            let d = d2.sqrt();
                            let w = (repel - d) / repel;
                            rx += dx / d * w;
                            ry += dy / d * w;
                        }
                        q = next[q as usize];
                    }
                }
            }
            // Towards the middle of the two neighbours.
            let (prev, nxt) = if self.closed {
                (Some((k + n - 1) % n), Some((k + 1) % n))
            } else {
                ((k > 0).then(|| k - 1), (k + 1 < n).then_some(k + 1))
            };
            let (mut ax, mut ay) = (0.0, 0.0);
            if let (Some(a), Some(b)) = (prev, nxt) {
                let (a, b) = (self.pos[a], self.pos[b]);
                ax = (a.x + b.x) / 2.0 - p.x;
                ay = (a.y + b.y) / 2.0 - p.y;
            }
            // A whisper of noise breaks perfect symmetry.
            let jx = (self.rng.f64() - 0.5) * 0.02;
            let jy = (self.rng.f64() - 0.5) * 0.02;
            let mut fx = rx * force + ax * align + jx;
            let mut fy = ry * force + ay * align + jy;
            let m = fx.hypot(fy);
            if m > 1.0 {
                fx /= m;
                fy /= m;
            }
            moved.push(Vec2 {
                x: (p.x + fx * speed).clamp(-hw, hw),
                y: (p.y + fy * speed).clamp(-hh, hh),
            });
        }
        self.pos = moved;
        self.steps += 1;
        // New nodes at `grow` a second, on edges picked at random.
        self.owed += ins.num("grow", 40.0).clamp(0.0, 10_000.0) * STEP;
        let extra = self.owed.floor();
        self.owed -= extra;
        let cap = ins.count("maxNodes", 2000, 3, MAX_NODES);
        self.split(max_edge(ins), cap, extra as usize);
    }

    fn output(&self, ins: &Ins) -> Out {
        let n = self.pos.len();
        let newest = self.born.iter().copied().max().unwrap_or(0).max(1) as f64;
        let age = |k: usize| f64::from(self.born[k]) / newest;
        let edges: Vec<(usize, usize)> = match n {
            0 | 1 => Vec::new(),
            _ => {
                let m = if self.closed { n } else { n - 1 };
                (0..m).map(|k| (k, (k + 1) % n)).collect()
            }
        };
        let mut out = Out {
            points: self.pos.clone(),
            values: (0..n).map(age).collect(),
            segments: edges
                .iter()
                .map(|&(a, b)| (self.pos[a], self.pos[b]))
                .collect(),
            ..Out::default()
        };
        let styles = Styles::new(ins, false);
        let base = styles.base();
        let coloured = !ins.colors().is_empty();
        // The whole curve as one path (just its fill when the edges take
        // the colours).
        let path_style = if coloured {
            base.fill.as_ref().map(|_| {
                Arc::new(crate::scene::ResolvedStyle {
                    stroke: None,
                    ..(**base).clone()
                })
            })
        } else {
            Some(base.clone())
        };
        if let Some(style) = path_style {
            if n >= 2 {
                out.layer.push(Arc::new(SceneNode::Polygon {
                    points: Arc::from(self.pos.clone()),
                    closed: self.closed,
                    style,
                    tension: None,
                }));
            }
        }
        if coloured {
            for &(a, b) in &edges {
                let t = age(a).max(age(b));
                out.layer.push(line(self.pos[a], self.pos[b], styles.at(t)));
            }
        }
        out
    }
}
