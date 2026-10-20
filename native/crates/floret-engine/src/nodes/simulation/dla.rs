//! `dla`: diffusion-limited aggregation.
//!
//! The cluster starts from the `seeds` points (the origin when none are
//! wired; read when the simulation starts). One walker at a time sets off
//! from a circle just outside the cluster and wanders in steps of
//! `radius` until it touches a particle (centres `2 · radius` apart); it
//! then sticks with probability `stickiness`, placed exactly touching the
//! particle it hit, which becomes its parent. Away from particles it takes
//! the longest step that can't reach one (outside the cluster's circle,
//! or where a coarse grid has none nearby), and it starts over if it
//! strays past twice the launch circle or out of `size`. Up to `rate` particles stick
//! per second (walking is budgeted per step, so a slow frame never
//! stalls) until there are `count`.
//!
//! The layer draws each particle's link to its parent; `values` is the
//! growth order (0 = first, 1 = newest) and picks the `colors` ramp.

use super::{line, sim_rng, Ins, Out, Rng, Sim, Styles, STEP};
use crate::value::Vec2;
use std::f64::consts::TAU;

const MAX_COUNT: usize = 50_000;
/// Walker moves allowed per step.
const WALK_BUDGET: u32 = 4000;
const NONE: u32 = u32::MAX;
/// Fine cells per coarse cell side.
const COARSE: usize = 4;

pub(crate) struct Dla {
    seed: u64,
    radius: f64,
    size: Vec2,
    seeds: Vec<Vec2>,
    rng: Rng,
    pos: Vec<Vec2>,
    /// The particle each one stuck to (`NONE` for seeds).
    parent: Vec<u32>,
    centre: Vec2,
    /// Farthest particle from `centre`.
    reach: f64,
    /// Cell-linked list over `size`, cells `2 · radius` wide.
    cols: usize,
    rows: usize,
    head: Vec<u32>,
    next: Vec<u32>,
    /// Particles per coarse cell (`COARSE` fine cells a side).
    coarse: Vec<u32>,
    walker: Option<Vec2>,
    /// Particles owed by `rate` (fractional).
    owed: f64,
}

fn radius(ins: &Ins) -> f64 {
    ins.num("radius", 2.5).clamp(0.25, 100.0)
}

fn size(ins: &Ins) -> Vec2 {
    ins.size(Vec2 { x: 600.0, y: 600.0 })
}

fn dist(a: Vec2, b: Vec2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

impl Dla {
    pub(crate) fn new(ins: &Ins) -> Dla {
        let (seed, radius, size) = (ins.seed(), radius(ins), size(ins));
        let cols = ((size.x / (2.0 * radius)).ceil() as usize).clamp(1, 2048);
        let rows = ((size.y / (2.0 * radius)).ceil() as usize).clamp(1, 2048);
        let mut seeds = ins.points("seeds");
        if seeds.is_empty() {
            seeds.push(Vec2::default());
        }
        seeds.truncate(MAX_COUNT);
        let n = seeds.len() as f64;
        let centre = Vec2 {
            x: seeds.iter().map(|p| p.x).sum::<f64>() / n,
            y: seeds.iter().map(|p| p.y).sum::<f64>() / n,
        };
        let mut d = Dla {
            seed,
            radius,
            size,
            seeds: seeds.clone(),
            rng: sim_rng(seed),
            pos: Vec::new(),
            parent: Vec::new(),
            centre,
            reach: 0.0,
            cols,
            rows,
            head: vec![NONE; cols * rows],
            next: Vec::new(),
            coarse: vec![0; cols.div_ceil(COARSE) * rows.div_ceil(COARSE)],
            walker: None,
            owed: 0.0,
        };
        for p in seeds {
            d.add(p, NONE);
        }
        d
    }

    fn cell(&self, p: Vec2) -> (usize, usize) {
        let w = 2.0 * self.radius;
        let cx = ((p.x + self.size.x / 2.0) / w).floor().max(0.0) as usize;
        let cy = ((p.y + self.size.y / 2.0) / w).floor().max(0.0) as usize;
        (cx.min(self.cols - 1), cy.min(self.rows - 1))
    }

    fn add(&mut self, p: Vec2, parent: u32) {
        let i = self.pos.len() as u32;
        let (cx, cy) = self.cell(p);
        let c = cy * self.cols + cx;
        self.next.push(self.head[c]);
        self.head[c] = i;
        self.coarse[(cy / COARSE) * self.cols.div_ceil(COARSE) + cx / COARSE] += 1;
        self.pos.push(p);
        self.parent.push(parent);
        self.reach = self.reach.max(dist(p, self.centre));
    }

    /// The nearest particle touching `p`, if any.
    fn touching(&self, p: Vec2) -> Option<u32> {
        let contact = 2.0 * self.radius;
        let (cx, cy) = self.cell(p);
        let mut best: Option<(f64, u32)> = None;
        for gy in cy.saturating_sub(1)..=(cy + 1).min(self.rows - 1) {
            for gx in cx.saturating_sub(1)..=(cx + 1).min(self.cols - 1) {
                let mut j = self.head[gy * self.cols + gx];
                while j != NONE {
                    let d = dist(p, self.pos[j as usize]);
                    if d <= contact && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, j));
                    }
                    j = self.next[j as usize];
                }
            }
        }
        best.map(|(_, j)| j)
    }

    /// No particle within `COARSE · 2 · radius` of `p`: its coarse cell
    /// and their neighbours are empty.
    fn clear_around(&self, p: Vec2) -> bool {
        let (cx, cy) = self.cell(p);
        let (cx, cy) = (cx / COARSE, cy / COARSE);
        let (cc, cr) = (self.cols.div_ceil(COARSE), self.rows.div_ceil(COARSE));
        (cy.saturating_sub(1)..=(cy + 1).min(cr - 1)).all(|gy| {
            (cx.saturating_sub(1)..=(cx + 1).min(cc - 1)).all(|gx| self.coarse[gy * cc + gx] == 0)
        })
    }

    fn inside(&self, p: Vec2) -> bool {
        p.x.abs() <= self.size.x / 2.0 && p.y.abs() <= self.size.y / 2.0
    }

    fn launch_radius(&self) -> f64 {
        self.reach + 6.0 * self.radius
    }

    /// A walker on the launch circle (inside `size`), if one fits.
    fn launch(&mut self) -> Option<Vec2> {
        let r = self.launch_radius();
        for _ in 0..16 {
            let a = self.rng.f64() * TAU;
            let p = Vec2 {
                x: self.centre.x + r * a.cos(),
                y: self.centre.y + r * a.sin(),
            };
            if self.inside(p) {
                return Some(p);
            }
        }
        None
    }
}

impl Sim for Dla {
    fn stale(&self, ins: &Ins) -> bool {
        let mut seeds = ins.points("seeds");
        if seeds.is_empty() {
            seeds.push(Vec2::default());
        }
        seeds.truncate(MAX_COUNT);
        ins.seed() != self.seed
            || radius(ins) != self.radius
            || size(ins) != self.size
            || seeds != self.seeds
    }

    fn step(&mut self, ins: &Ins) {
        let max = ins.count("count", 3000, 0, MAX_COUNT);
        if self.pos.len() >= max {
            self.owed = 0.0;
            return;
        }
        let stickiness = ins.num("stickiness", 1.0).clamp(0.0, 1.0);
        self.owed = (self.owed + ins.num("rate", 300.0).max(0.0) * STEP).min(1e6);
        let r = self.radius;
        let mut budget = WALK_BUDGET;
        while self.owed >= 1.0 && budget > 0 && self.pos.len() < max && stickiness > 0.0 {
            let mut w = match self.walker.take().or_else(|| self.launch()) {
                Some(w) => w,
                // The cluster fills the field.
                None => break,
            };
            let kill = 2.0 * self.launch_radius() + 10.0 * r;
            let stuck = loop {
                if budget == 0 {
                    self.walker = Some(w);
                    break false;
                }
                budget -= 1;
                // Nothing is nearer than `gap + 2r`: far out, jump most of it.
                let gap = dist(w, self.centre) - self.reach - 2.0 * r;
                let clear = gap > 2.0 * r || self.clear_around(w);
                let len = if gap > 2.0 * r {
                    gap - r
                } else if clear {
                    // Lands at least 2.5r from every particle.
                    (COARSE as f64 * 2.0 - 2.5) * r
                } else {
                    r
                };
                let a = self.rng.f64() * TAU;
                w.x += len * a.cos();
                w.y += len * a.sin();
                if !self.inside(w) || dist(w, self.centre) > kill {
                    match self.launch() {
                        Some(p) => w = p,
                        None => break false,
                    }
                    continue;
                }
                if clear {
                    continue;
                }
                let Some(hit) = self.touching(w) else {
                    continue;
                };
                if stickiness < 1.0 && self.rng.f64() >= stickiness {
                    continue;
                }
                // Sit exactly touching the particle it hit.
                let q = self.pos[hit as usize];
                let d = dist(w, q);
                let p = if d > 1e-9 {
                    Vec2 {
                        x: q.x + (w.x - q.x) / d * 2.0 * r,
                        y: q.y + (w.y - q.y) / d * 2.0 * r,
                    }
                } else {
                    Vec2 {
                        x: q.x + 2.0 * r,
                        y: q.y,
                    }
                };
                self.add(p, hit);
                break true;
            };
            if stuck {
                self.owed -= 1.0;
            } else if self.walker.is_none() {
                break;
            }
        }
    }

    fn output(&self, ins: &Ins) -> Out {
        let styles = Styles::new(ins, false);
        let n = self.pos.len();
        let order = |i: usize| {
            if n > 1 {
                i as f64 / (n - 1) as f64
            } else {
                0.0
            }
        };
        let mut out = Out {
            points: self.pos.clone(),
            values: (0..n).map(order).collect(),
            ..Out::default()
        };
        for (i, (&p, &parent)) in self.pos.iter().zip(&self.parent).enumerate() {
            if parent != NONE {
                let q = self.pos[parent as usize];
                out.layer.push(line(q, p, styles.at(order(i))));
                out.segments.push((q, p));
            }
        }
        out
    }
}
