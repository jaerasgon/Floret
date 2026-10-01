//! The node canvas: pan / zoom, nodes, pins, wires, selection, dragging,
//! and live output values from the running engine.

use crate::theme::{self, BG_0, BG_2, BORDER, FG_0, FG_1, FG_2, FG_3, GOLD};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2,
};
use floret_engine::patch::{parse_handle, Handle, PatchFile, PortDirection};
use floret_engine::spec::{Catalogue, NodeSpec};
use floret_engine::tick::Outputs;
use floret_engine::{Engine, Value};
use std::collections::HashMap;

const NODE_W: f32 = 168.0;
const HEADER_H: f32 = 26.0;
const ROW_H: f32 = 20.0;
const PIN_R: f32 = 4.5;

/// Canvas camera + interaction state.
pub struct View {
    pub pan: Vec2,
    pub zoom: f32,
    pub selected: Option<String>,
    dragging: Option<String>,
}

impl Default for View {
    fn default() -> Self {
        View {
            pan: vec2(40.0, 40.0),
            zoom: 1.0,
            selected: None,
            dragging: None,
        }
    }
}

struct Pin {
    /// Handle name (`in-<id>` / `out-<id>`).
    id: String,
    label: String,
    ty: String,
}

struct NodeBox {
    rect: Rect,
    label: String,
    category: String,
    inputs: Vec<Pin>,
    outputs: Vec<Pin>,
    region: bool,
    unimplemented: bool,
}

impl View {
    fn to_screen(&self, origin: Pos2, p: Pos2) -> Pos2 {
        origin + (p.to_vec2() + self.pan) * self.zoom
    }

    /// Fit the whole patch into `area`.
    pub fn fit(&mut self, patch: &PatchFile, area: Rect) {
        let mut bb = Rect::NOTHING;
        for n in &patch.nodes {
            let p = absolute_pos(patch, &n.id);
            let (w, h) = (
                n.width.unwrap_or(NODE_W as f64) as f32,
                n.height.unwrap_or(80.0) as f32,
            );
            bb = bb.union(Rect::from_min_size(p, vec2(w, h)));
        }
        if !bb.is_finite() || bb.width() <= 0.0 {
            *self = View::default();
            return;
        }
        let margin = 60.0;
        self.zoom = ((area.width() - margin) / bb.width())
            .min((area.height() - margin) / bb.height())
            .clamp(0.2, 1.4);
        let content = bb.size() * self.zoom;
        let offset = (area.size() - content) * 0.5;
        self.pan = offset / self.zoom - bb.min.to_vec2();
    }
}

/// Position in canvas space. React Flow stores children relative to their
/// parent region, so walk up the parent chain.
fn absolute_pos(patch: &PatchFile, id: &str) -> Pos2 {
    let mut p = Vec2::ZERO;
    let mut cur = patch.nodes.iter().find(|n| n.id == id);
    while let Some(n) = cur {
        p += vec2(n.position.x as f32, n.position.y as f32);
        cur = n
            .parent_id
            .as_ref()
            .and_then(|pid| patch.nodes.iter().find(|m| &m.id == pid));
    }
    p.to_pos2()
}

fn layout(
    patch: &PatchFile,
    engine: &Engine,
    cat: &Catalogue,
    view: &View,
    origin: Pos2,
) -> HashMap<String, NodeBox> {
    let mut boxes = HashMap::new();
    for n in &patch.nodes {
        let Some(spec) = cat.get(&n.def_type) else {
            continue;
        };
        let region = spec.region_kind.is_some();
        let (inputs, outputs) = pins(n, spec, cat, region);
        let rows = inputs.len().max(outputs.len()) as f32;
        let size = if region {
            vec2(
                n.width.unwrap_or(320.0) as f32,
                n.height.unwrap_or(200.0) as f32,
            )
        } else {
            vec2(NODE_W, HEADER_H + rows * ROW_H + 8.0)
        };
        let min = view.to_screen(origin, absolute_pos(patch, &n.id));
        let unimplemented = engine
            .graph
            .idx(&n.id)
            .is_some_and(|i| engine.graph.nodes[i].unimplemented);
        boxes.insert(
            n.id.clone(),
            NodeBox {
                rect: Rect::from_min_size(min, size * view.zoom),
                label: spec.label.clone(),
                category: spec.category.clone(),
                inputs,
                outputs,
                region,
                unimplemented,
            },
        );
    }
    boxes
}

fn pins(
    n: &floret_engine::patch::PatchNode,
    spec: &NodeSpec,
    cat: &Catalogue,
    region: bool,
) -> (Vec<Pin>, Vec<Pin>) {
    if region {
        let ports = n.ports.clone().unwrap_or_default();
        let side = |d: PortDirection| {
            ports
                .iter()
                .filter(|p| p.direction == d)
                .map(|p| Pin {
                    id: p.id.clone(),
                    label: p.name.clone(),
                    ty: p.ty.clone(),
                })
                .collect()
        };
        return (side(PortDirection::Input), side(PortDirection::Output));
    }
    let pin = |p: &floret_engine::spec::PortSpec| Pin {
        id: p.name.clone(),
        label: p.label.clone().unwrap_or_else(|| p.name.clone()),
        ty: format!("{:?}", p.ty).to_ascii_lowercase(),
    };
    (
        cat.resolved_inputs(spec, &n.params)
            .iter()
            .map(pin)
            .collect(),
        spec.outputs.iter().map(pin).collect(),
    )
}

fn pin_pos(b: &NodeBox, side: Handle, zoom: f32) -> Option<Pos2> {
    let (list, x) = match side {
        Handle::In(_) => (&b.inputs, b.rect.left()),
        Handle::Out(_) => (&b.outputs, b.rect.right()),
    };
    let name = match side {
        Handle::In(n) | Handle::Out(n) => n,
    };
    if let Some(stripped) = name.strip_prefix("__loopvar_") {
        let i = ["index", "count", "fraction"]
            .iter()
            .position(|v| *v == stripped)? as f32;
        return Some(pos2(
            b.rect.left() + 10.0 * zoom,
            b.rect.top() + (HEADER_H + 10.0 + i * ROW_H) * zoom,
        ));
    }
    let i = list.iter().position(|p| p.id.as_str() == name)? as f32;
    Some(pos2(
        x,
        b.rect.top() + (HEADER_H + ROW_H * (i + 0.5)) * zoom,
    ))
}

pub fn show(
    ui: &mut egui::Ui,
    patch: &mut PatchFile,
    engine: &Engine,
    outputs: &Outputs,
    cat: &Catalogue,
    view: &mut View,
) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let area = resp.rect;
    let origin = area.min;
    painter.rect_filled(area, 0.0, BG_0);

    // ── Camera: scroll pans, pinch / ctrl+scroll zooms around the pointer.
    if resp.hovered() {
        let (scroll, zoom_delta, pointer) =
            ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
        if zoom_delta != 1.0 {
            if let Some(p) = pointer {
                let before = (p - origin) / view.zoom - view.pan;
                view.zoom = (view.zoom * zoom_delta).clamp(0.15, 3.0);
                view.pan = (p - origin) / view.zoom - before;
            }
        } else if scroll != Vec2::ZERO {
            view.pan += scroll / view.zoom;
        }
    }

    // ── Dot grid.
    let step = 24.0 * view.zoom;
    if step > 6.0 {
        let start = origin
            + vec2(
                (view.pan.x * view.zoom).rem_euclid(step),
                (view.pan.y * view.zoom).rem_euclid(step),
            );
        let dot = FG_3.gamma_multiply(0.55);
        let mut y = start.y;
        while y < area.bottom() {
            let mut x = start.x;
            while x < area.right() {
                painter.circle_filled(pos2(x, y), 0.9, dot);
                x += step;
            }
            y += step;
        }
    }

    let boxes = layout(patch, engine, cat, view, origin);
    let z = view.zoom;

    // ── Interaction: drag nodes, select, pan on empty space.
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let hit = |p: Pos2| -> Option<String> {
        // Topmost: non-region nodes beat regions.
        let mut best: Option<(&String, bool)> = None;
        for (id, b) in &boxes {
            let target = if b.region {
                Rect::from_min_size(b.rect.min, vec2(b.rect.width(), HEADER_H * z))
            } else {
                b.rect
            };
            if target.contains(p) && (best.is_none() || best.is_some_and(|(_, r)| r && !b.region)) {
                best = Some((id, b.region));
            }
        }
        best.map(|(id, _)| id.clone())
    };
    if resp.drag_started() {
        view.dragging = pointer.and_then(hit);
        if view.dragging.is_some() {
            view.selected = view.dragging.clone();
        }
    }
    if resp.dragged() {
        let d = resp.drag_delta() / z;
        match &view.dragging {
            Some(id) => {
                if let Some(n) = patch.nodes.iter_mut().find(|n| &n.id == id) {
                    n.position.x += f64::from(d.x);
                    n.position.y += f64::from(d.y);
                }
            }
            None => view.pan += d,
        }
    }
    if resp.drag_stopped() {
        view.dragging = None;
    }
    if resp.clicked() {
        view.selected = pointer.and_then(hit);
    }

    // ── Regions (behind everything).
    for (id, b) in boxes.iter().filter(|(_, b)| b.region) {
        let selected = view.selected.as_ref() == Some(id);
        painter.rect(
            b.rect,
            CornerRadius::same((9.0 * z) as u8),
            GOLD.gamma_multiply(0.05),
            Stroke::new(
                if selected { 2.0 } else { 1.5 },
                GOLD.gamma_multiply(if selected { 0.95 } else { 0.55 }),
            ),
            egui::StrokeKind::Inside,
        );
        painter.text(
            b.rect.min + vec2(12.0, 7.0) * z,
            Align2::LEFT_TOP,
            &b.label,
            FontId::proportional(12.0 * z),
            GOLD,
        );
    }

    // ── Wires.
    for e in &patch.edges {
        let (Some(src), Some(dst)) = (boxes.get(&e.source), boxes.get(&e.target)) else {
            continue;
        };
        let (Some(sh), Some(th)) = (
            parse_handle(e.source_handle.as_deref()),
            parse_handle(e.target_handle.as_deref()),
        ) else {
            continue;
        };
        let (Some(a), Some(bp)) = (pin_pos(src, sh, z), pin_pos(dst, th, z)) else {
            continue;
        };
        let ty = match sh {
            Handle::Out(name) => src
                .outputs
                .iter()
                .find(|p| p.id.as_str() == name)
                .map(|p| p.ty.as_str()),
            Handle::In(_) => None,
        }
        .unwrap_or("any");
        let dx = ((bp.x - a.x).abs() * 0.5).max(40.0 * z);
        let curve = egui::epaint::CubicBezierShape::from_points_stroke(
            [a, a + vec2(dx, 0.0), bp - vec2(dx, 0.0), bp],
            false,
            Color32::TRANSPARENT,
            Stroke::new(
                (1.8 * z).clamp(1.0, 3.0),
                theme::port_color(ty).gamma_multiply(0.75),
            ),
        );
        painter.add(curve);
    }

    // ── Nodes.
    let values = |id: &str| -> Option<Vec<String>> {
        let idx = engine.graph.idx(id)?;
        let out = outputs.get(idx)?.as_ref()?;
        Some(out.iter().map(format_spread).collect())
    };
    for (id, b) in boxes.iter().filter(|(_, b)| !b.region) {
        let selected = view.selected.as_ref() == Some(id);
        let r = CornerRadius::same((6.0 * z) as u8);
        if selected {
            painter.rect_stroke(
                b.rect.expand(3.0),
                r,
                Stroke::new(3.0, GOLD.gamma_multiply(0.25)),
                egui::StrokeKind::Outside,
            );
        }
        painter.rect(
            b.rect,
            r,
            BG_2,
            Stroke::new(
                if selected { 1.5 } else { 1.0 },
                if selected { GOLD } else { BORDER },
            ),
            egui::StrokeKind::Inside,
        );
        let header = Rect::from_min_size(b.rect.min, vec2(b.rect.width(), HEADER_H * z));
        painter.line_segment(
            [header.left_bottom(), header.right_bottom()],
            Stroke::new(1.0, BORDER),
        );
        let accent = theme::category_color(&b.category);
        painter.circle_filled(header.left_center() + vec2(12.0 * z, 0.0), 3.2 * z, accent);
        if z > 0.35 {
            painter.text(
                header.left_center() + vec2(22.0 * z, 0.0),
                Align2::LEFT_CENTER,
                &b.label,
                FontId::proportional(12.5 * z),
                FG_0,
            );
            if b.unimplemented {
                painter.text(
                    header.right_center() - vec2(8.0 * z, 0.0),
                    Align2::RIGHT_CENTER,
                    "not ported",
                    FontId::proportional(9.5 * z),
                    FG_2,
                );
            }
        }
        let vals = values(id);
        for (i, p) in b.inputs.iter().enumerate() {
            let y = b.rect.top() + (HEADER_H + ROW_H * (i as f32 + 0.5)) * z;
            painter.circle(
                pos2(b.rect.left(), y),
                PIN_R * z,
                theme::port_color(&p.ty),
                Stroke::new(1.5, BG_2),
            );
            if z > 0.45 {
                painter.text(
                    pos2(b.rect.left() + 10.0 * z, y),
                    Align2::LEFT_CENTER,
                    p.label.as_str(),
                    FontId::proportional(11.0 * z),
                    FG_1,
                );
            }
        }
        for (i, p) in b.outputs.iter().enumerate() {
            let y = b.rect.top() + (HEADER_H + ROW_H * (i as f32 + 0.5)) * z;
            painter.circle(
                pos2(b.rect.right(), y),
                PIN_R * z,
                theme::port_color(&p.ty),
                Stroke::new(1.5, BG_2),
            );
            if z > 0.45 {
                let text = match vals.as_ref().and_then(|v| v.get(i)) {
                    Some(v) if !v.is_empty() => v.clone(),
                    _ => p.label.as_str().to_owned(),
                };
                painter.text(
                    pos2(b.rect.right() - 10.0 * z, y),
                    Align2::RIGHT_CENTER,
                    text,
                    FontId::monospace(10.5 * z),
                    if vals.is_some() { FG_0 } else { FG_1 },
                );
            }
        }
    }

    if resp.double_clicked() && pointer.and_then(hit).is_none() {
        view.fit(patch, area);
    }
    let _ = painter.add(Shape::Noop);
}

/// Short live-value text for an output pin.
fn format_spread(s: &floret_engine::Spread) -> String {
    match s.len() {
        0 => String::new(),
        1 => format_value(&s[0]),
        n => format!("[{n}]"),
    }
}

fn format_value(v: &Value) -> String {
    match v {
        Value::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 1e9 {
                format!("{n:.0}")
            } else {
                format!("{n:.3}")
            }
        }
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => {
            let s: String = s.chars().take(14).collect();
            format!("\"{s}\"")
        }
        Value::Vec2(p) => format!("{:.1}, {:.1}", p.x, p.y),
        Value::Color(c) => format!(
            "#{:02x}{:02x}{:02x}",
            (c.r.clamp(0.0, 1.0) * 255.0) as u8,
            (c.g.clamp(0.0, 1.0) * 255.0) as u8,
            (c.b.clamp(0.0, 1.0) * 255.0) as u8
        ),
        Value::List(items) => format!("[{}]", items.len()),
        Value::Undefined | Value::Null => String::new(),
        Value::Object(_) => "{…}".into(),
    }
}
