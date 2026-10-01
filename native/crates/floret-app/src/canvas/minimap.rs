//! The overview map in the canvas corner (React Flow's `<MiniMap pannable
//! zoomable>` as `Canvas.tsx` configures it): every visible node as a
//! small block in its category colour, the area outside the view masked,
//! drag to pan and scroll to zoom.

use super::geom::{Look, NodeBox};
use crate::theme::{self, BG_1, BORDER};
use eframe::egui::{self, pos2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use indexmap::IndexMap;

/// `.react-flow__minimap` default size and its panel margin.
const SIZE: Vec2 = Vec2::new(200.0, 150.0);
const MARGIN: f32 = 15.0;
/// React Flow's `offsetScale`.
const OFFSET: f32 = 5.0;

/// A node's block in canvas units, with its colour.
pub struct Block {
    pub rect: Rect,
    pub color: Color32,
}

/// The view and its contents in canvas units.
pub struct World {
    pub blocks: Vec<Block>,
    pub view: Rect,
}

/// Visible nodes and the viewport, in canvas units.
pub fn world_rects(boxes: &IndexMap<String, NodeBox>, area: Rect, pan: Vec2, zoom: f32) -> World {
    let to_canvas = |p: Pos2| ((p - area.min) / zoom - pan).to_pos2();
    let blocks = boxes
        .values()
        .map(|b| {
            let r = if b.look.is_frame() {
                b.rect.union(b.header)
            } else {
                b.rect
            };
            Block {
                rect: Rect::from_min_max(to_canvas(r.min), to_canvas(r.max)),
                color: match b.look {
                    Look::Region | Look::Frame => theme::FG_3.gamma_multiply(0.6),
                    _ => theme::category_color(&b.category),
                },
            }
        })
        .collect();
    World {
        blocks,
        view: Rect::from_min_max(to_canvas(area.min), to_canvas(area.max)),
    }
}

/// The canvas region the map shows (React Flow's `viewBox`): the nodes
/// and the view, centred, with a little room around.
pub fn view_box(world: &World) -> Rect {
    let bounds = world
        .blocks
        .iter()
        .fold(world.view, |acc, b| acc.union(b.rect));
    let scale = (bounds.width() / SIZE.x)
        .max(bounds.height() / SIZE.y)
        .max(1e-3);
    let size = SIZE * scale;
    let offset = OFFSET * scale;
    Rect::from_min_size(
        bounds.min - (size - bounds.size()) / 2.0 - Vec2::splat(offset),
        size + Vec2::splat(2.0 * offset),
    )
}

/// The map's screen rect inside the canvas area.
pub fn map_rect(area: Rect) -> Rect {
    Rect::from_min_size(area.max - SIZE - Vec2::splat(MARGIN), SIZE)
}

/// Pan that moves the view by `drag` screen pixels on the map.
pub fn drag_pan(pan: Vec2, drag: Vec2, view_box: Rect, map: Rect) -> Vec2 {
    let scale = view_box.width() / map.width().max(1.0);
    pan - drag * scale
}

#[derive(Default)]
pub struct MiniMap {
    /// The view box frozen while dragging, so the map doesn't rescale
    /// under the pointer.
    held: Option<Rect>,
}

impl MiniMap {
    /// Draw the map; returns a new (pan, zoom) when it was dragged or
    /// scrolled.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        area: Rect,
        world: &World,
        pan: Vec2,
        zoom: f32,
    ) -> Option<(Vec2, f32)> {
        let map = map_rect(area);
        if area.width() < SIZE.x * 2.5 || area.height() < SIZE.y * 2.0 {
            return None;
        }
        let resp = ui.interact(map, ui.id().with("floret-minimap"), Sense::click_and_drag());
        let vb = match self.held {
            Some(vb) if resp.dragged() => vb,
            _ => view_box(world),
        };
        self.held = resp.dragged().then_some(vb);
        let to_map = |p: Pos2| {
            pos2(
                map.left() + (p.x - vb.left()) / vb.width() * map.width(),
                map.top() + (p.y - vb.top()) / vb.height() * map.height(),
            )
        };
        let map_of = |r: Rect| Rect::from_min_max(to_map(r.min), to_map(r.max));

        let p = painter.with_clip_rect(map);
        p.rect_filled(map, 4.0, BG_1.gamma_multiply(0.92));
        for b in &world.blocks {
            let r = map_of(b.rect);
            p.rect(
                r,
                2.0,
                b.color,
                Stroke::new(1.0, Color32::from_black_alpha(150)),
                StrokeKind::Inside,
            );
        }
        // Mask everything outside the view.
        let v = map_of(world.view).intersect(map);
        let mask = Color32::from_black_alpha(if theme::is_light() { 40 } else { 150 });
        for r in [
            Rect::from_min_max(map.min, pos2(map.right(), v.top())),
            Rect::from_min_max(pos2(map.left(), v.bottom()), map.max),
            Rect::from_min_max(pos2(map.left(), v.top()), pos2(v.left(), v.bottom())),
            Rect::from_min_max(pos2(v.right(), v.top()), pos2(map.right(), v.bottom())),
        ] {
            if r.is_positive() {
                p.rect_filled(r, 0.0, mask);
            }
        }
        p.rect_stroke(
            v,
            0.0,
            Stroke::new(1.0, theme::FG_2.gamma_multiply(0.7)),
            StrokeKind::Inside,
        );
        painter.rect_stroke(map, 4.0, Stroke::new(1.0, *BORDER), StrokeKind::Inside);

        if resp.dragged() {
            return Some((drag_pan(pan, resp.drag_delta(), vb, map), zoom));
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                // Zoom about the view's centre.
                let z = (zoom * (1.0 + scroll * 0.002)).clamp(0.1, 3.0);
                let half = area.size() * 0.5;
                let centre = half / zoom - pan;
                return Some((half / z - centre, z));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::vec2;

    #[test]
    fn view_box_holds_nodes_and_view() {
        let world = World {
            blocks: vec![Block {
                rect: Rect::from_min_size(pos2(1000.0, 0.0), vec2(100.0, 50.0)),
                color: Color32::WHITE,
            }],
            view: Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0)),
        };
        let vb = view_box(&world);
        assert!(vb.contains_rect(world.view));
        assert!(vb.contains_rect(world.blocks[0].rect));
        // Same aspect as the map.
        assert!((vb.aspect_ratio() - SIZE.x / SIZE.y).abs() < 0.05);
        let map = map_rect(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0)));
        assert_eq!(map.max, pos2(985.0, 785.0));
        // Dragging right on the map moves the view right (pan goes down).
        let pan = drag_pan(Vec2::ZERO, vec2(10.0, 0.0), vb, map);
        assert!(pan.x < 0.0 && pan.y == 0.0);
    }
}
