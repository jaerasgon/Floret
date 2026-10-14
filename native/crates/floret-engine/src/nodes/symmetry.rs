//! `transform/symmetry` (Symmetry): a layer repeated by a symmetry group.
//! Native-only.
//!
//! - `rotate`: `count` copies turned evenly round `center` (n-fold
//!   rotation, the cyclic group Cn), the first at `angle`.
//! - `kaleidoscope`: the same with `count` mirrors through `center` (the
//!   dihedral group Dn): `2 × count` copies, every other one reflected.
//! - `wallpaper`: one of the 17 plane groups (`group`, in the usual
//!   short names: p1, p2, pm, pg, cm, pmm, pmg, pgg, cmm, p4, p4m, p4g,
//!   p3, p3m1, p31m, p6, p6m) over a lattice of `cell`-wide cells
//!   (square, or hexagonal for the p3 / p6 groups), turned by `angle`,
//!   filling `size` round `center`. Mirrors and rotation centres pass
//!   through `center`: draw the motif near it, within about a cell.
//!
//! `clip` keeps each copy to its own wedge in the round modes (the layer
//! is masked to the wedge between `angle` and the next mirror or turn, so
//! the copies meet edge to edge like a kaleidoscope's), and keeps the
//! pattern inside `size` in wallpaper mode.

use super::geometry::{int_pin, num_pin, vec2_pin};
use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::{self, Affine, RectF, SceneNode};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Value, Vec2};
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("transform/symmetry", symmetry);
    r.dynamic("transform/symmetry", inputs);
}

const MODE: &str = "kaleidoscope";
/// The wallpaper groups, in dropdown order.
pub const GROUPS: [&str; 17] = [
    "p1", "p2", "pm", "pg", "cm", "pmm", "pmg", "pgg", "cmm", "p4", "p4m", "p4g", "p3", "p3m1",
    "p31m", "p6", "p6m",
];
/// Most copies drawn.
pub const MAX_COPIES: usize = 20_000;
/// Most copies round a centre.
const MAX_COUNT: f64 = 360.0;

fn mode_of(params: &Params) -> &str {
    match params.get("mode").and_then(|v| v.as_str()) {
        Some(m @ ("rotate" | "kaleidoscope" | "wallpaper")) => m,
        _ => MODE,
    }
}

fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = vec![PortSpec::new("layer", PortType::Layer).kind(PortKind::Spread)];
    let clip = |on: bool| PortSpec::new("clip", PortType::Boolean).default(Value::Bool(on));
    match mode_of(params) {
        "wallpaper" => {
            let mut g = PortSpec::new("group", PortType::String).default(Value::str("p4m"));
            g.param = Some("group".into());
            v.push(g);
            v.push(num_pin("cell", 80.0));
            v.push(vec2_pin("center", 0.0, 0.0));
            v.push(num_pin("angle", 0.0));
            v.push(vec2_pin("size", 600.0, 600.0));
            v.push(clip(true));
        }
        m => {
            v.push(int_pin("count", 6.0));
            v.push(vec2_pin("center", 0.0, 0.0));
            v.push(num_pin("angle", 0.0));
            v.push(clip(m == "kaleidoscope"));
        }
    }
    v
}

// ─── Transforms ──────────────────────────────────────────────────────────────

/// A 2×2 linear map and a translation, in the order `[a, b, c, d]` maps
/// (x, y) to (a·x + c·y, b·x + d·y) like [`Affine`].
type Lin = [f64; 4];

fn lin_mul(p: &Lin, q: &Lin) -> Lin {
    [
        p[0] * q[0] + p[2] * q[1],
        p[1] * q[0] + p[3] * q[1],
        p[0] * q[2] + p[2] * q[3],
        p[1] * q[2] + p[3] * q[3],
    ]
}

fn lin_apply(m: &Lin, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y, m[1] * x + m[3] * y)
}

fn lin_inv(m: &Lin) -> Option<Lin> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-12 || !det.is_finite() {
        return None;
    }
    Some([m[3] / det, -m[1] / det, -m[2] / det, m[0] / det])
}

fn rotation(a: f64) -> Lin {
    let (s, c) = a.sin_cos();
    [c, s, -s, c]
}

/// The reflection in the line through the origin at angle `a`.
fn reflection(a: f64) -> Lin {
    let (s, c) = (2.0 * a).sin_cos();
    [c, s, s, -c]
}

/// `p ↦ center + m·(p − center) + t`.
fn about(center: Vec2, m: &Lin, t: (f64, f64)) -> Affine {
    let (mx, my) = lin_apply(m, center.x, center.y);
    [
        m[0],
        m[1],
        m[2],
        m[3],
        center.x - mx + t.0,
        center.y - my + t.1,
    ]
}

/// The copies round a centre: `count` turns, each with its mirror image
/// when `mirror`.
pub(crate) fn rosette(count: usize, center: Vec2, angle: f64, mirror: bool) -> Vec<Affine> {
    let n = count.max(1);
    let mut out = Vec::with_capacity(n * 2);
    // Mirrors at angle + k·π/n: reflecting in the one at angle + π/n
    // takes the wedge [angle, angle + π/n] to the next one round.
    let flip = reflection(angle + PI / n as f64);
    for k in 0..n {
        let r = rotation(TAU * k as f64 / n as f64);
        out.push(about(center, &r, (0.0, 0.0)));
        if mirror {
            out.push(about(center, &lin_mul(&r, &flip), (0.0, 0.0)));
        }
    }
    out
}

/// A wallpaper group's operations in lattice (fractional) coordinates:
/// `(matrix, translation)`, and whether the lattice is hexagonal.
/// A wallpaper operation: a linear map and a translation.
type Op = (Lin, (f64, f64));

fn group_ops(group: &str) -> (Vec<Op>, bool) {
    // Fractional maps (x, y) ↦ (r0·x + r1·y, r2·x + r3·y) + t, written
    // row by row as the International Tables list them.
    let op = |r: [f64; 4], t: (f64, f64)| -> (Lin, (f64, f64)) { ([r[0], r[2], r[1], r[3]], t) };
    let id = op([1.0, 0.0, 0.0, 1.0], (0.0, 0.0));
    let rot2 = op([-1.0, 0.0, 0.0, -1.0], (0.0, 0.0));
    let mx = |t| op([-1.0, 0.0, 0.0, 1.0], t); // x ↦ −x
    let my = |t| op([1.0, 0.0, 0.0, -1.0], t); // y ↦ −y
    let r4 = op([0.0, -1.0, 1.0, 0.0], (0.0, 0.0)); // (−y, x)
    let r4i = op([0.0, 1.0, -1.0, 0.0], (0.0, 0.0)); // (y, −x)
    let d1 = |t| op([0.0, 1.0, 1.0, 0.0], t); // (y, x)
    let d2 = |t| op([0.0, -1.0, -1.0, 0.0], t); // (−y, −x)
    let h = (0.5, 0.5);
    let o = (0.0, 0.0);
    let shift = |ops: Vec<(Lin, (f64, f64))>, t: (f64, f64)| -> Vec<(Lin, (f64, f64))> {
        let mut all = ops.clone();
        all.extend(ops.into_iter().map(|(m, s)| (m, (s.0 + t.0, s.1 + t.1))));
        all
    };
    // Hexagonal: a = (1, 0), b at 120°.
    let h3 = [
        op([1.0, 0.0, 0.0, 1.0], o),
        op([0.0, -1.0, 1.0, -1.0], o), // (−y, x − y)
        op([-1.0, 1.0, -1.0, 0.0], o), // (−x + y, −x)
    ];
    let h6 = [
        op([-1.0, 0.0, 0.0, -1.0], o), // (−x, −y)
        op([0.0, 1.0, -1.0, 1.0], o),  // (y, −x + y)
        op([1.0, -1.0, 1.0, 0.0], o),  // (x − y, x)
    ];
    let m31 = [
        op([0.0, 1.0, 1.0, 0.0], o),   // (y, x)
        op([1.0, -1.0, 0.0, -1.0], o), // (x − y, −y)
        op([-1.0, 0.0, -1.0, 1.0], o), // (−x, −x + y)
    ];
    let m3 = [
        op([0.0, -1.0, -1.0, 0.0], o), // (−y, −x)
        op([-1.0, 1.0, 0.0, 1.0], o),  // (−x + y, y)
        op([1.0, 0.0, 1.0, -1.0], o),  // (x, x − y)
    ];
    match group {
        "p1" => (vec![id], false),
        "p2" => (vec![id, rot2], false),
        "pm" => (vec![id, mx(o)], false),
        "pg" => (vec![id, mx((0.0, 0.5))], false),
        "cm" => (shift(vec![id, mx(o)], h), false),
        "pmm" => (vec![id, rot2, mx(o), my(o)], false),
        "pmg" => (vec![id, rot2, mx((0.5, 0.0)), my((0.5, 0.0))], false),
        "pgg" => (vec![id, rot2, mx(h), my(h)], false),
        "cmm" => (shift(vec![id, rot2, mx(o), my(o)], h), false),
        "p4" => (vec![id, rot2, r4, r4i], false),
        "p4m" => (vec![id, rot2, r4, r4i, mx(o), my(o), d1(o), d2(o)], false),
        "p4g" => (vec![id, rot2, r4, r4i, mx(h), my(h), d1(h), d2(h)], false),
        "p3" => (h3.to_vec(), true),
        "p3m1" => ([h3, m3].concat(), true),
        "p31m" => ([h3, m31].concat(), true),
        "p6" => ([h3, h6].concat(), true),
        "p6m" => ([h3, h6, m3, m31].concat(), true),
        _ => (vec![id], false),
    }
}

/// The copies of a wallpaper group filling `size` round `center`, and
/// whether they stopped at [`MAX_COPIES`].
pub(crate) fn wallpaper(
    group: &str,
    cell: f64,
    center: Vec2,
    angle: f64,
    size: Vec2,
) -> (Vec<Affine>, bool) {
    let (w, h) = (size.x.abs(), size.y.abs());
    let finite = [cell, center.x, center.y, angle, w, h]
        .iter()
        .all(|v| v.is_finite());
    if !finite || cell <= 0.0 || w <= 0.0 || h <= 0.0 {
        return (Vec::new(), false);
    }
    let (ops, hex) = group_ops(group);
    // Lattice basis (columns a, b), turned by `angle`.
    let base: Lin = if hex {
        [cell, 0.0, -cell / 2.0, cell * 3f64.sqrt() / 2.0]
    } else {
        [cell, 0.0, 0.0, cell]
    };
    let b = lin_mul(&rotation(angle), &base);
    let Some(bi) = lin_inv(&b) else {
        return (Vec::new(), false);
    };
    // Lattice cells covering the area (corners in lattice coordinates).
    let corners =
        [(-w, -h), (w, -h), (w, h), (-w, h)].map(|(x, y)| lin_apply(&bi, x / 2.0, y / 2.0));
    let span = |f: fn(&(f64, f64)) -> f64| {
        let lo = corners.iter().map(f).fold(f64::INFINITY, f64::min);
        let hi = corners.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
        // Two cells' margin for motifs that spill out of their cell; a
        // range capped so the loop stays bounded.
        let cap = (MAX_COPIES as f64).sqrt();
        (
            (lo.floor() - 2.0).max(-cap) as i64,
            (hi.ceil() + 2.0).min(cap) as i64,
        )
    };
    let (lo_i, hi_i) = span(|c| c.0);
    let (lo_j, hi_j) = span(|c| c.1);
    // Cartesian ops: B·M·B⁻¹ and B·t.
    let cart: Vec<(Lin, (f64, f64))> = ops
        .iter()
        .map(|(m, t)| (lin_mul(&b, &lin_mul(m, &bi)), lin_apply(&b, t.0, t.1)))
        .collect();
    // Keep copies whose motif centre lands within the area plus margin.
    let (rx, ry) = (w / 2.0 + 2.0 * cell, h / 2.0 + 2.0 * cell);
    let mut out = Vec::new();
    let mut capped = false;
    'all: for j in lo_j..=hi_j {
        for i in lo_i..=hi_i {
            let (lx, ly) = lin_apply(&b, i as f64, j as f64);
            for (m, t) in &cart {
                let (tx, ty) = (lx + t.0, ly + t.1);
                if tx.abs() > rx || ty.abs() > ry {
                    continue;
                }
                if out.len() >= MAX_COPIES {
                    capped = true;
                    break 'all;
                }
                out.push(about(center, m, (tx, ty)));
            }
        }
    }
    (out, capped)
}

/// The wedge `[from, from + span]` round `center`, out past `radius`.
fn wedge(center: Vec2, from: f64, span: f64, radius: f64) -> Arc<SceneNode> {
    let steps = ((span / TAU * 96.0).ceil() as usize).clamp(2, 96);
    let mut pts = vec![center];
    for i in 0..=steps {
        let a = from + span * i as f64 / steps as f64;
        pts.push(Vec2 {
            x: center.x + radius * a.cos(),
            y: center.y + radius * a.sin(),
        });
    }
    Arc::new(SceneNode::Polygon {
        points: Arc::from(pts),
        closed: true,
        style: scene::default_style().clone(),
        tension: None,
    })
}

fn group(
    children: Vec<Arc<SceneNode>>,
    transform: Option<Affine>,
    clip: Option<RectF>,
) -> Arc<SceneNode> {
    Arc::new(SceneNode::Group {
        children: Arc::from(children),
        transform,
        blend_mode: None,
        clip,
    })
}

fn symmetry(init: &NodeInit) -> Box<dyn NodeImpl> {
    let mode = mode_of(init.params).to_owned();
    let names: Vec<String> = init.inputs.iter().map(|p| p.name.clone()).collect();
    raw(move |inputs, cx| {
        let at = |name: &str| {
            names
                .iter()
                .position(|n| n == name)
                .and_then(|i| inputs.get(i))
        };
        let value = |name: &str| {
            at(name)
                .and_then(|s| s.first())
                .cloned()
                .unwrap_or(Value::Undefined)
        };
        let num = |name: &str, d: f64| {
            let v = value(name).to_number();
            if v.is_finite() {
                v
            } else {
                d
            }
        };
        let vec2 = |name: &str, d: Vec2| match value(name) {
            v @ (Value::Vec2(_) | Value::Object(_)) => {
                let p = super::shared::to_vec2(&v);
                if p.x.is_finite() && p.y.is_finite() {
                    p
                } else {
                    d
                }
            }
            _ => d,
        };
        let layers: Vec<Arc<SceneNode>> = at("layer")
            .map(|s| {
                s.iter()
                    .filter_map(|v| match v {
                        Value::Scene(n) => Some(n.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let empty = || {
            Ok(vec![one(Value::Scene(Arc::new(SceneNode::group(
                Vec::new(),
            ))))])
        };
        if layers.is_empty() {
            return empty();
        }
        let motif = if layers.len() == 1 {
            layers[0].clone()
        } else {
            Arc::new(SceneNode::group(layers))
        };
        let center = vec2("center", Vec2::default());
        let angle = num("angle", 0.0);
        let clip = value("clip").truthy();
        let out = if mode == "wallpaper" {
            let name = match value("group") {
                Value::Str(s) if GROUPS.contains(&&*s) => s.to_string(),
                _ => "p4m".to_owned(),
            };
            let size = vec2("size", Vec2 { x: 600.0, y: 600.0 });
            let (copies, capped) = wallpaper(&name, num("cell", 80.0).abs(), center, angle, size);
            if capped {
                cx.warn(format!(
                    "Stopped at {MAX_COPIES} copies: make `cell` bigger."
                ));
            }
            let kids = copies
                .into_iter()
                .map(|m| group(vec![motif.clone()], Some(m), None))
                .collect();
            let rect = RectF {
                x: center.x - size.x.abs() / 2.0,
                y: center.y - size.y.abs() / 2.0,
                w: size.x.abs(),
                h: size.y.abs(),
            };
            group(kids, None, clip.then_some(rect))
        } else {
            let count = num("count", 6.0).clamp(1.0, MAX_COUNT) as usize;
            let mirror = mode == "kaleidoscope";
            let span = if mirror { PI } else { TAU } / count as f64;
            // The motif cut to its wedge (a hair wider, so no seam shows
            // between neighbours).
            let piece = if clip && (mirror || count != 1) {
                Arc::new(SceneNode::Mask {
                    content: motif,
                    mask: wedge(center, angle - 0.01, span + 0.02, 1.0e5),
                    luminance: false,
                    invert: false,
                })
            } else {
                motif
            };
            let kids = rosette(count, center, angle, mirror)
                .into_iter()
                .map(|m| group(vec![piece.clone()], Some(m), None))
                .collect();
            group(kids, None, None)
        };
        Ok(vec![one(Value::Scene(out))])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::value::harness::node;
    use serde_json::json;

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2 { x, y }
    }

    fn apply(m: &Affine, p: Vec2) -> Vec2 {
        v(
            m[0] * p.x + m[2] * p.y + m[4],
            m[1] * p.x + m[3] * p.y + m[5],
        )
    }

    fn det(m: &Affine) -> f64 {
        m[0] * m[3] - m[1] * m[2]
    }

    /// Lengths and angles kept: the linear part is orthonormal.
    fn isometry(m: &Affine) -> bool {
        let (a, b, c, d) = (m[0], m[1], m[2], m[3]);
        (a * a + b * b - 1.0).abs() < 1e-9
            && (c * c + d * d - 1.0).abs() < 1e-9
            && (a * c + b * d).abs() < 1e-9
    }

    #[test]
    fn rosettes_turn_and_mirror_round_the_centre() {
        let c = v(30.0, -20.0);
        let rot = rosette(5, c, 0.3, false);
        assert_eq!(rot.len(), 5);
        assert!(rot.iter().all(|m| isometry(m) && det(m) > 0.0));
        let kal = rosette(6, c, 0.3, true);
        assert_eq!(kal.len(), 12);
        assert_eq!(kal.iter().filter(|m| det(m) < 0.0).count(), 6);
        for m in kal.iter().chain(&rot) {
            let p = apply(m, c);
            assert!((p.x - c.x).abs() < 1e-9 && (p.y - c.y).abs() < 1e-9);
        }
        // The first mirror reflects the wedge's far edge onto itself.
        let edge = v(
            c.x + (0.3 + PI / 6.0).cos() * 50.0,
            c.y + (0.3 + PI / 6.0).sin() * 50.0,
        );
        let q = apply(&kal[1], edge);
        assert!((q.x - edge.x).abs() < 1e-9 && (q.y - edge.y).abs() < 1e-9);
        // Every image of a point is different.
        let p = v(c.x + 40.0, c.y + 7.0);
        let imgs: Vec<Vec2> = kal.iter().map(|m| apply(m, p)).collect();
        for (i, a) in imgs.iter().enumerate() {
            for b in &imgs[i + 1..] {
                assert!((a.x - b.x).hypot(a.y - b.y) > 1e-6);
            }
        }
    }

    #[test]
    fn every_wallpaper_group_is_closed_and_has_its_order() {
        let order = [
            ("p1", 1),
            ("p2", 2),
            ("pm", 2),
            ("pg", 2),
            ("cm", 4),
            ("pmm", 4),
            ("pmg", 4),
            ("pgg", 4),
            ("cmm", 8),
            ("p4", 4),
            ("p4m", 8),
            ("p4g", 8),
            ("p3", 3),
            ("p3m1", 6),
            ("p31m", 6),
            ("p6", 6),
            ("p6m", 12),
        ];
        assert_eq!(order.len(), GROUPS.len());
        let int = |x: f64| (x - x.round()).abs() < 1e-9;
        for (g, n) in order {
            let (ops, _) = group_ops(g);
            assert_eq!(ops.len(), n, "{g}");
            // Composing any two ops gives one of them, up to a lattice
            // translation (fractional coordinates).
            for (m1, t1) in &ops {
                for (m2, t2) in &ops {
                    let m = lin_mul(m1, m2);
                    let (ax, ay) = lin_apply(m1, t2.0, t2.1);
                    let t = (ax + t1.0, ay + t1.1);
                    let found = ops.iter().any(|(mk, tk)| {
                        mk.iter().zip(&m).all(|(a, b)| (a - b).abs() < 1e-9)
                            && int(t.0 - tk.0)
                            && int(t.1 - tk.1)
                    });
                    assert!(found, "{g}: {m:?} + {t:?} isn't in the group");
                }
            }
            // Every copy is an isometry of the plane.
            let (copies, capped) = wallpaper(g, 50.0, v(0.0, 0.0), 0.4, v(400.0, 300.0));
            assert!(!capped && !copies.is_empty(), "{g}");
            assert!(copies.iter().all(isometry), "{g}");
        }
        // Unknown names fall back to p1; bad sizes give nothing.
        assert_eq!(group_ops("nope").0.len(), 1);
        assert!(wallpaper("p4m", 0.0, v(0.0, 0.0), 0.0, v(100.0, 100.0))
            .0
            .is_empty());
        assert!(
            wallpaper("p4m", f64::NAN, v(0.0, 0.0), 0.0, v(100.0, 100.0))
                .0
                .is_empty()
        );
        // A tiny cell stops at the cap.
        let (many, capped) = wallpaper("p6m", 0.5, v(0.0, 0.0), 0.0, v(2000.0, 2000.0));
        assert!(capped && many.len() == MAX_COPIES);
    }

    #[test]
    fn wallpaper_fills_the_area() {
        // p1 at cell 50 over 400 × 300: a copy's centre near every cell.
        let (copies, _) = wallpaper("p1", 50.0, v(0.0, 0.0), 0.0, v(400.0, 300.0));
        let centres: Vec<Vec2> = copies.iter().map(|m| apply(m, v(0.0, 0.0))).collect();
        for j in 0..6 {
            for i in 0..8 {
                let p = v(-175.0 + i as f64 * 50.0, -125.0 + j as f64 * 50.0);
                assert!(centres.iter().any(|c| (c.x - p.x).hypot(c.y - p.y) <= 36.0));
            }
        }
    }

    fn eval(params: serde_json::Value, layer: Vec<Value>) -> Arc<SceneNode> {
        let mut n = node("transform/symmetry", params.clone());
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("transform/symmetry").unwrap();
        let ports = cat.resolved_inputs(spec, params.as_object().unwrap());
        let mut inputs: Vec<Vec<Value>> = ports
            .iter()
            .map(|p| crate::compile::default_for_port(p).to_vec())
            .collect();
        inputs[0] = layer;
        match &n.eval(inputs).unwrap()[0][0] {
            Value::Scene(s) => s.clone(),
            other => panic!("{other:?}"),
        }
    }

    fn dot() -> Value {
        Value::Scene(Arc::new(SceneNode::Ellipse {
            x: 40.0,
            y: 10.0,
            w: 10.0,
            h: 10.0,
            mode: Arc::from("center"),
            style: scene::default_style().clone(),
        }))
    }

    #[test]
    fn the_node_wraps_copies_and_clips() {
        let kids = |n: &SceneNode| match n {
            SceneNode::Group { children, clip, .. } => (children.to_vec(), *clip),
            other => panic!("{other:?}"),
        };
        // Kaleidoscope (default): 12 copies, each the dot masked to its wedge.
        let (k, clip) = kids(&eval(json!({}), vec![dot()]));
        assert_eq!(k.len(), 12);
        assert!(clip.is_none());
        let (inner, _) = kids(&k[0]);
        assert!(matches!(&*inner[0], SceneNode::Mask { .. }));
        // Rotate, unclipped: the dot itself.
        let (k, _) = kids(&eval(json!({"mode": "rotate"}), vec![dot()]));
        assert_eq!(k.len(), 6);
        let (inner, _) = kids(&k[0]);
        assert!(matches!(&*inner[0], SceneNode::Ellipse { .. }));
        // Wallpaper: clipped to its size.
        let (k, clip) = kids(&eval(json!({"mode": "wallpaper"}), vec![dot()]));
        assert!(k.len() > 50);
        assert_eq!(
            clip,
            Some(RectF {
                x: -300.0,
                y: -300.0,
                w: 600.0,
                h: 600.0
            })
        );
        // Nothing wired: an empty layer.
        let (k, _) = kids(&eval(json!({}), vec![]));
        assert!(k.is_empty());
    }
}
