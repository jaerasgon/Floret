//! Node gravity: dragging nodes gently pulls the rest of their scope along,
//! scaled by how related each node is to the dragged ones. Not a physics
//! simulation: each pulled node gets one weight at drag start and follows
//! `weight × drag distance`, easing towards it so neighbours trail a little.
//! On release every pulled node lands exactly on its target.
//!
//! Weights (the strongest relation to any dragged node wins):
//! - wired directly to a dragged node: [`LINKED`];
//! - two wires away (a shared source or target, or a chain): [`NEAR`];
//! - anything else: [`FAR`] · e^(−distance / [`FALLOFF`]), dropped below
//!   [`MIN_WEIGHT`]; distance between box centres, in canvas units.
//!
//! Only nodes in the dragged nodes' scope (same parent) are pulled; a wire
//! into a region counts as a wire to the region.

use crate::editor::ops;
use crate::editor::ports::{find_node, find_node_mut};
use floret_engine::patch::{PatchFile, Position};
use std::collections::{HashMap, HashSet};

pub const LINKED: f64 = 0.35;
pub const NEAR: f64 = 0.25;
pub const FAR: f64 = 0.12;
pub const FALLOFF: f64 = 350.0;
pub const MIN_WEIGHT: f64 = 0.02;
/// Fraction of the remaining distance covered per 60 Hz frame.
pub const EASE: f64 = 0.28;

#[derive(Clone, Debug)]
struct Pulled {
    id: String,
    w: f64,
    /// Absolute position at drag start.
    start: Position,
    /// Current offset from `start` (eases towards `w × delta`).
    cur: (f64, f64),
}

#[derive(Clone, Debug, Default)]
pub struct Gravity {
    pulled: Vec<Pulled>,
    /// The latest drag delta, canvas units.
    delta: (f64, f64),
}

/// The member of scope `parent` that `id` is, or sits inside.
fn member_of<'a>(patch: &'a PatchFile, id: &'a str, parent: Option<&str>) -> Option<&'a str> {
    let mut cur = find_node(patch, id)?;
    for _ in 0..64 {
        if cur.parent_id.as_deref() == parent {
            return Some(&cur.id);
        }
        cur = find_node(patch, cur.parent_id.as_deref()?)?;
    }
    None
}

/// Each pulled node's weight. `centre` gives a node's box centre in canvas
/// units (when it's laid out).
pub fn weights(
    patch: &PatchFile,
    moved: &[String],
    parent: Option<&str>,
    centre: impl Fn(&str) -> Option<(f64, f64)>,
) -> Vec<(String, f64)> {
    let moved_set: HashSet<&str> = moved.iter().map(String::as_str).collect();
    let mut adj: HashMap<&str, HashSet<&str>> = HashMap::new();
    for e in &patch.edges {
        let (Some(a), Some(b)) = (
            member_of(patch, &e.source, parent),
            member_of(patch, &e.target, parent),
        ) else {
            continue;
        };
        if a != b {
            adj.entry(a).or_default().insert(b);
            adj.entry(b).or_default().insert(a);
        }
    }
    let hops = |id: &str| -> Option<u8> {
        let first = adj.get(id)?;
        if first.iter().any(|n| moved_set.contains(n)) {
            return Some(1);
        }
        first
            .iter()
            .any(|n| {
                adj.get(n)
                    .is_some_and(|s| s.iter().any(|m| moved_set.contains(m)))
            })
            .then_some(2)
    };
    let moved_centres: Vec<(f64, f64)> = moved.iter().filter_map(|id| centre(id)).collect();
    patch
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == parent && !moved_set.contains(n.id.as_str()))
        .filter_map(|n| {
            let w = match hops(&n.id) {
                Some(1) => LINKED,
                Some(_) => NEAR,
                None => {
                    let c = centre(&n.id)?;
                    let d = moved_centres
                        .iter()
                        .map(|m| (c.0 - m.0).hypot(c.1 - m.1))
                        .fold(f64::INFINITY, f64::min);
                    FAR * (-d / FALLOFF).exp()
                }
            };
            (w >= MIN_WEIGHT).then(|| (n.id.clone(), w))
        })
        .collect()
}

impl Gravity {
    /// Start pulling for a drag of `moved` (all in scope `parent`).
    pub fn start(
        patch: &PatchFile,
        moved: &[String],
        parent: Option<&str>,
        centre: impl Fn(&str) -> Option<(f64, f64)>,
    ) -> Gravity {
        let pulled = weights(patch, moved, parent, centre)
            .into_iter()
            .map(|(id, w)| Pulled {
                start: ops::absolute_pos(patch, &id),
                id,
                w,
                cur: (0.0, 0.0),
            })
            .collect();
        Gravity {
            pulled,
            delta: (0.0, 0.0),
        }
    }

    pub fn ids(&self) -> impl Iterator<Item = &String> {
        self.pulled.iter().map(|p| &p.id)
    }

    /// Ease the pulled nodes towards the drag `delta` (canvas units) over
    /// `dt` seconds (`instant`: no easing). True while any is still moving.
    pub fn step(
        &mut self,
        patch: &mut PatchFile,
        delta: (f64, f64),
        dt: f32,
        instant: bool,
    ) -> bool {
        self.delta = delta;
        let k = if instant {
            1.0
        } else {
            1.0 - (1.0 - EASE).powf(f64::from(dt.clamp(0.0, 0.25)) * 60.0)
        };
        let mut moving = false;
        for p in &mut self.pulled {
            let target = (p.w * delta.0, p.w * delta.1);
            p.cur.0 += (target.0 - p.cur.0) * k;
            p.cur.1 += (target.1 - p.cur.1) * k;
            if (target.0 - p.cur.0).abs() > 0.25 || (target.1 - p.cur.1).abs() > 0.25 {
                moving = true;
            } else {
                p.cur = target;
            }
            let at = Position {
                x: p.start.x + p.cur.0,
                y: p.start.y + p.cur.1,
            };
            place(patch, &p.id, at);
        }
        moving
    }

    /// Release: every pulled node lands exactly on its target (rounded, or
    /// on the grid when `snap` is `Some((step, origin))`).
    pub fn settle(&mut self, patch: &mut PatchFile, snap: Option<(f64, Position)>) {
        let d = self.delta;
        for p in &mut self.pulled {
            p.cur = (p.w * d.0, p.w * d.1);
            let mut at = Position {
                x: (p.start.x + p.cur.0).round(),
                y: (p.start.y + p.cur.1).round(),
            };
            if let Some((step, origin)) = snap {
                at = ops::snap_point(at, step, origin);
            }
            place(patch, &p.id, at);
        }
    }
}

/// Put `id` at absolute position `at`.
fn place(patch: &mut PatchFile, id: &str, at: Position) {
    let parent = find_node(patch, id).and_then(|n| n.parent_id.clone());
    let base = parent
        .map(|p| ops::absolute_pos(patch, &p))
        .unwrap_or_default();
    if let Some(n) = find_node_mut(patch, id) {
        n.position = Position {
            x: at.x - base.x,
            y: at.y - base.y,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// a → b → c, d far away, e near b but unwired; r is a region holding x,
    /// which is wired to b.
    fn patch() -> PatchFile {
        let node = |id: &str, x: f64, y: f64, parent: Option<&str>| {
            json!({"id": id, "defType": "math/add", "position": {"x": x, "y": y},
                   "parentId": parent, "data": {}})
        };
        let edge = |id: &str, s: &str, t: &str| {
            json!({"id": id, "source": s, "target": t,
                   "sourceHandle": "out-out", "targetHandle": "in-a"})
        };
        serde_json::from_value(json!({
            "kind": "floret.patch",
            "version": 3,
            "nodes": [
                node("a", 0.0, 0.0, None),
                node("b", 200.0, 0.0, None),
                node("c", 400.0, 0.0, None),
                node("d", 5000.0, 0.0, None),
                node("e", 200.0, 150.0, None),
                {"id": "r", "defType": "region/foreach", "position": {"x": 0, "y": 400}, "data": {}},
                node("x", 20.0, 20.0, Some("r")),
            ],
            "edges": [edge("1", "a", "b"), edge("2", "b", "c"), edge("3", "x", "b")],
        }))
        .expect("patch")
    }

    fn centre(p: &PatchFile) -> impl Fn(&str) -> Option<(f64, f64)> + '_ {
        |id| {
            let at = ops::absolute_pos(p, id);
            Some((at.x + 80.0, at.y + 30.0))
        }
    }

    #[test]
    fn weights_follow_wires_then_distance() {
        let p = patch();
        let w: HashMap<String, f64> = weights(&p, &["b".into()], None, centre(&p))
            .into_iter()
            .collect();
        assert_eq!(w["a"], LINKED);
        assert_eq!(w["c"], LINKED);
        // The region is wired to b through its child x.
        assert_eq!(w["r"], LINKED);
        assert!(
            w["e"] < LINKED && w["e"] > MIN_WEIGHT,
            "near but unwired: {}",
            w["e"]
        );
        assert!(!w.contains_key("d"), "too far");
        assert!(!w.contains_key("b"), "the dragged node isn't pulled");
        assert!(!w.contains_key("x"), "other scopes aren't pulled");

        let w: HashMap<String, f64> = weights(&p, &["a".into()], None, centre(&p))
            .into_iter()
            .collect();
        assert_eq!(w["b"], LINKED);
        assert_eq!(w["c"], NEAR, "two wires away");
        assert_eq!(w["r"], NEAR);
    }

    #[test]
    fn pulled_nodes_ease_then_settle_exactly() {
        let mut p = patch();
        let mut g = Gravity::start(&p, &["b".into()], None, centre(&p));
        let delta = (100.0, -40.0);
        assert!(g.step(&mut p, delta, 1.0 / 60.0, false), "still easing");
        let a = ops::absolute_pos(&p, "a");
        assert!(a.x > 0.0 && a.x < 35.0, "trails behind: {}", a.x);
        for _ in 0..200 {
            if !g.step(&mut p, delta, 1.0 / 60.0, false) {
                break;
            }
        }
        g.settle(&mut p, None);
        let a = ops::absolute_pos(&p, "a");
        assert_eq!((a.x, a.y), (35.0, -14.0));
        // The region's child moves with the region, not on its own.
        assert_eq!(find_node(&p, "x").map(|n| n.position.x), Some(20.0));
        assert_eq!(ops::absolute_pos(&p, "x").x, 20.0 + 35.0);
        assert_eq!(ops::absolute_pos(&p, "d").x, 5000.0);
    }

    #[test]
    fn instant_steps_land_on_target() {
        let mut p = patch();
        let mut g = Gravity::start(&p, &["b".into()], None, centre(&p));
        assert!(!g.step(&mut p, (100.0, 0.0), 1.0 / 60.0, true));
        assert_eq!(ops::absolute_pos(&p, "c").x, 435.0);
    }
}
