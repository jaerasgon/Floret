//! Write → Read pairs on the canvas: the partner of a hovered or selected
//! half lights up, and a selected half shows a dashed "teleport" arc to
//! its partner. Drawn only: the arc is not a wire and is never hit-tested.

use super::geom::NodeBox;
use crate::editor::pairs;
use crate::editor::ports::find_node;
use crate::theme::GOLD;
use eframe::egui::{pos2, vec2, Painter, Pos2, Shape, Stroke, StrokeKind};
use floret_engine::patch::PatchFile;
use indexmap::IndexMap;
use std::collections::HashSet;

#[derive(Default)]
pub struct PairLinks {
    /// (Write, Read) arcs to draw, both on screen.
    links: Vec<(String, String)>,
    /// Partners to light up (not themselves selected).
    lit: HashSet<String>,
}

impl PairLinks {
    /// The links of the selected halves and the partners of the selected
    /// or hovered ones, among the nodes on screen.
    pub fn find(
        patch: &PatchFile,
        boxes: &IndexMap<String, NodeBox>,
        hovered: Option<&str>,
        selection: &[String],
    ) -> Self {
        let mut out = Self::default();
        let is_half =
            |id: &str| find_node(patch, id).is_some_and(|n| pairs::is_pair_type(&n.def_type));
        for id in selection.iter().map(String::as_str).chain(hovered) {
            if !boxes.contains_key(id) || !is_half(id) {
                continue;
            }
            let write = find_node(patch, id).is_some_and(|n| pairs::is_write(&n.def_type));
            for p in pairs::partners(patch, id) {
                if !boxes.contains_key(&p) {
                    continue;
                }
                if !selection.contains(&p) {
                    out.lit.insert(p.clone());
                }
                if selection.iter().any(|s| s == id) {
                    let link = if write {
                        (id.to_owned(), p)
                    } else {
                        (p, id.to_owned())
                    };
                    if !out.links.contains(&link) {
                        out.links.push(link);
                    }
                }
            }
        }
        out
    }

    /// The dashed arcs, Write → Read, over the tops of the two cards
    /// (drawn under the nodes).
    pub fn draw_links(&self, p: &Painter, boxes: &IndexMap<String, NodeBox>, z: f32) {
        let color = GOLD.gamma_multiply(0.6);
        for (w, r) in &self.links {
            let (Some(wb), Some(rb)) = (boxes.get(w), boxes.get(r)) else {
                continue;
            };
            let a = pos2(wb.rect.center().x, wb.rect.top());
            let b = pos2(rb.rect.center().x, rb.rect.top());
            let lift = (36.0 + 0.12 * (a - b).length() / z.max(0.01)).min(140.0) * z;
            let top = a.y.min(b.y) - lift;
            let (c1, c2) = (pos2(a.x, top), pos2(b.x, top));
            let pts: Vec<Pos2> = (0..=48)
                .map(|i| cubic(a, c1, c2, b, i as f32 / 48.0))
                .collect();
            let stroke = Stroke::new(1.4, color);
            p.extend(Shape::dashed_line(&pts, stroke, 6.0 * z, 5.0 * z));
            p.circle_filled(a, 2.6 * z, color);
            // A small arrowhead where the value lands: on the Read.
            let tip = b;
            let dir = (b - pts[pts.len() - 4]).normalized();
            let side = vec2(-dir.y, dir.x);
            let back = tip - dir * 7.0 * z;
            p.add(Shape::convex_polygon(
                vec![tip, back + side * 4.0 * z, back - side * 4.0 * z],
                color,
                Stroke::NONE,
            ));
        }
    }

    /// A soft ring around each lit partner (drawn over the nodes).
    pub fn draw_lit(&self, p: &Painter, boxes: &IndexMap<String, NodeBox>, z: f32) {
        for id in &self.lit {
            if let Some(b) = boxes.get(id) {
                p.rect_stroke(
                    b.rect.expand(3.0 * z),
                    super::custom::radius(z),
                    Stroke::new(1.5, GOLD.gamma_multiply(0.55)),
                    StrokeKind::Outside,
                );
            }
        }
    }
}

fn cubic(a: Pos2, b: Pos2, c: Pos2, d: Pos2, t: f32) -> Pos2 {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    pos2(
        w[0] * a.x + w[1] * b.x + w[2] * c.x + w[3] * d.x,
        w[0] * a.y + w[1] * b.y + w[2] * c.y + w[3] * d.y,
    )
}
