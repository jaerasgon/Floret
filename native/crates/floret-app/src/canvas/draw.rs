//! Painting the laid-out canvas: region frames with their port pills,
//! node boxes with rows and inline fields, pins, chips and tooltips.

use super::geom::{Field, Look, NodeBox, PinGeom, HEADER_H, LABEL_FONT, PIN_R};
use crate::editor::ports::type_name;
use crate::editor::values::{self, FieldKind};
use crate::theme::{self, BG_0, BG_2, BG_3, BORDER, DANGER, FG_0, FG_1, FG_2, GOLD};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Id, LayerId, Order, Painter, Pos2,
    Rect, Stroke, StrokeKind,
};
use floret_engine::spec::RegionKind;

pub const SPREAD_PIN: Color32 = Color32::from_rgb(0xf0, 0xd8, 0x00);

#[derive(Clone, Copy)]
pub enum PinLook {
    Normal,
    Source,
    Match,
    Dim,
}

/// Frame accent per region kind (`.region-foreach` / `-if` / `-cache`).
pub fn frame_color(kind: Option<RegionKind>) -> Color32 {
    match kind {
        Some(RegionKind::If) => Color32::from_rgb(255, 200, 100),
        Some(RegionKind::Cache) => Color32::from_rgb(124, 182, 255),
        Some(RegionKind::Subpatch) => Color32::from_rgb(190, 160, 255),
        _ => GOLD,
    }
}

/// Linear blend `a·t + b·(1−t)`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (f32::from(x) * t + f32::from(y) * (1.0 - t)).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn pin(p: &Painter, g: &PinGeom, look: PinLook, pulse: f32, z: f32) {
    let pos = g.pos;
    let mut color = if g.spread {
        SPREAD_PIN
    } else {
        theme::port_color(type_name(g.ty))
    };
    if g.constant {
        // Constant pins: set once at compile.
        color = color.gamma_multiply(0.55);
    }
    match look {
        PinLook::Dim => {
            p.circle(pos, PIN_R * z, color.gamma_multiply(0.18), Stroke::NONE);
        }
        PinLook::Match => {
            p.circle_filled(
                pos,
                (PIN_R + 3.0 + 2.0 * pulse) * z,
                GOLD.gamma_multiply(0.35 + 0.2 * pulse),
            );
            p.circle(pos, PIN_R * z, color, Stroke::new(1.5, BG_2));
        }
        PinLook::Source => {
            p.circle(pos, (PIN_R + 1.0) * z, GOLD, Stroke::new(1.5, BG_2));
        }
        PinLook::Normal => {
            p.circle(pos, PIN_R * z, color, Stroke::new(1.5, BG_2));
        }
    }
}

fn chip(p: &Painter, r: Rect) {
    p.circle_filled(r.center(), r.height() / 2.0, DANGER);
    p.text(
        r.center(),
        Align2::CENTER_CENTER,
        "!",
        FontId::proportional(r.height() * 0.75),
        Color32::WHITE,
    );
}

pub struct FrameState<'a> {
    pub kind: Option<RegionKind>,
    pub def_type: &'a str,
    pub selected: bool,
    pub drop_target: bool,
    pub selected_ports: &'a [String],
}

pub fn frame(p: &Painter, b: &NodeBox, st: &FrameState, z: f32) {
    let accent = frame_color(st.kind);
    let (fill, stroke) = if st.drop_target {
        (accent.gamma_multiply(0.12), Stroke::new(2.5, accent))
    } else if st.selected {
        (
            accent.gamma_multiply(0.05),
            Stroke::new(2.0, accent.gamma_multiply(0.95)),
        )
    } else {
        (
            accent.gamma_multiply(0.04),
            Stroke::new(1.5, accent.gamma_multiply(0.55)),
        )
    };
    p.rect(
        b.rect,
        CornerRadius::same((9.0 * z).min(255.0) as u8),
        fill,
        stroke,
        StrokeKind::Inside,
    );
    if z < 0.2 {
        return;
    }
    let mid = b.header.center().y;
    p.text(
        pos2(b.header.left() + 8.0 * z, mid),
        Align2::LEFT_CENTER,
        &b.label,
        FontId::monospace(12.0 * z),
        accent,
    );
    p.text(
        pos2(b.header.right() - 4.0 * z, mid),
        Align2::RIGHT_CENTER,
        if b.look == Look::Frame {
            "subpatch"
        } else {
            st.def_type
        },
        FontId::monospace(9.5 * z),
        FG_2,
    );
    if let Some((r, text, _, capped)) = &b.badge {
        p.rect(
            r.to_owned(),
            7.0 * z,
            BG_3,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        p.text(
            r.left_center() + vec2(6.0 * z, 0.0),
            Align2::LEFT_CENTER,
            text,
            FontId::monospace(10.0 * z),
            FG_0,
        );
        if *capped {
            p.text(
                r.right_center() - vec2(5.0 * z, 0.0),
                Align2::RIGHT_CENTER,
                "⚠",
                FontId::proportional(9.0 * z),
                DANGER,
            );
        }
    }
    if let Some((r, _)) = &b.warning {
        chip(p, *r);
    }
    // Top-left stack: static inputs and loop variables.
    for t in &b.top {
        p.rect(
            t.rect,
            t.rect.height() / 2.0,
            BG_2,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let color = if t.loop_var {
            theme::port_color("number")
        } else {
            FG_1
        };
        p.text(
            t.rect.center(),
            Align2::CENTER_CENTER,
            &t.label,
            FontId::proportional(9.0 * z),
            color,
        );
    }
    for pl in &b.pills {
        let color = theme::port_color(type_name(pl.ty));
        let on = st.selected_ports.contains(&pl.port);
        p.rect(
            pl.rect,
            pl.rect.height() / 2.0,
            if on { GOLD.gamma_multiply(0.25) } else { BG_2 },
            Stroke::new(if on { 1.5 } else { 1.0 }, if on { GOLD } else { color }),
            StrokeKind::Inside,
        );
        p.text(
            pl.rect.center(),
            Align2::CENTER_CENTER,
            &pl.name,
            FontId::proportional(9.0 * z),
            FG_0,
        );
    }
    // Resize grip.
    let c = b.rect.right_bottom() - vec2(4.0, 4.0) * z;
    for k in [4.0, 8.0] {
        p.line_segment(
            [c - vec2(k * z, 0.0), c - vec2(0.0, k * z)],
            Stroke::new(1.0, accent.gamma_multiply(0.6)),
        );
    }
}

pub struct NodeState<'a> {
    pub selected: bool,
    pub subtitle: Option<&'a str>,
    /// Live value text for an output port.
    pub out_text: &'a dyn Fn(&str) -> Option<String>,
    /// The field being typed into (drawn by the text editor instead).
    pub editing: Option<&'a Field>,
}

pub fn node(p: &Painter, b: &NodeBox, st: &NodeState, z: f32) {
    let r = CornerRadius::same((6.0 * z).min(255.0) as u8);
    if st.selected {
        // Soft gold glow (the web app's --glow-gold), not a second ring.
        let glow = egui::Shadow {
            offset: [0, 0],
            blur: 14,
            spread: 0,
            color: GOLD.gamma_multiply(0.22),
        };
        p.add(glow.as_shape(b.rect, r));
    }
    let border = if st.selected {
        GOLD.gamma_multiply(0.85)
    } else if b.warning.is_some() {
        DANGER.gamma_multiply(0.6)
    } else {
        BORDER
    };
    p.rect(
        b.rect,
        r,
        BG_2,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    let header = Rect::from_min_size(b.rect.min, vec2(b.rect.width(), HEADER_H * z));
    p.line_segment(
        [header.left_bottom(), header.right_bottom()],
        Stroke::new(1.0, BORDER),
    );
    let accent = if b.look == Look::Collapsed {
        frame_color(Some(RegionKind::Subpatch))
    } else {
        theme::category_color(&b.category)
    };
    p.circle_filled(header.left_center() + vec2(12.0 * z, 0.0), 3.2 * z, accent);
    if z > 0.25 {
        let title = if b.look == Look::Collapsed {
            format!("▶ {}", b.label)
        } else {
            b.label.clone()
        };
        // Math symbols (⌊x⌋, ‖v‖ …) only exist in the monospace font.
        let font = if b.math {
            FontId::monospace(13.0 * z)
        } else {
            FontId::proportional(12.5 * z)
        };
        let galley = p.layout_no_wrap(title, font, FG_0);
        let mut x = header.left() + 22.0 * z + galley.size().x + 6.0 * z;
        p.galley(
            pos2(
                header.left() + 22.0 * z,
                header.center().y - galley.size().y / 2.0,
            ),
            galley,
            FG_0,
        );
        if let Some(slot) = &b.slot {
            let g = p.layout_no_wrap(slot.clone(), FontId::monospace(10.0 * z), GOLD);
            let rr = Rect::from_min_size(
                pos2(x, header.center().y - 7.0 * z),
                vec2(g.size().x + 8.0 * z, 14.0 * z),
            );
            p.rect(rr, 7.0 * z, BG_3, Stroke::NONE, StrokeKind::Inside);
            p.galley(rr.center() - g.size() / 2.0, g, GOLD);
            x = rr.right() + 4.0 * z;
        }
        let right = if let Some((r, _)) = &b.warning {
            chip(p, *r);
            r.left() - 4.0 * z
        } else {
            header.right() - 8.0 * z
        };
        let tag = if b.unimplemented {
            Some("not ported")
        } else {
            st.subtitle
        };
        if let Some(tag) = tag {
            if right - x > 30.0 * z {
                p.text(
                    pos2(right, header.center().y),
                    Align2::RIGHT_CENTER,
                    tag,
                    FontId::proportional(9.5 * z),
                    FG_2,
                );
            }
        }
    }
    if let (Some(text), true) = (&b.comment, z > 0.3) {
        let body = b.rect.shrink2(vec2(10.0, 0.0) * z);
        let galley = p.layout(
            text.clone(),
            FontId::proportional(11.5 * z),
            FG_1,
            body.width(),
        );
        let clip = Rect::from_min_max(
            pos2(body.left(), header.bottom() + 6.0 * z),
            pos2(body.right(), b.rect.bottom() - 6.0 * z),
        );
        p.with_clip_rect(clip).galley(clip.min, galley, FG_1);
    }
    if z <= 0.3 {
        return;
    }
    let font = FontId::proportional(LABEL_FONT * z);
    for row in &b.rows {
        if let Some(label) = &row.label {
            let color = if row.expanded {
                GOLD
            } else if row.toggle.is_some() {
                FG_1
            } else {
                mix(FG_1, FG_2, 0.6)
            };
            let g = p.layout_no_wrap(label.clone(), font.clone(), color);
            let mut x = b.rect.left() + 10.0 * z;
            let w = g.size().x;
            p.galley(pos2(x, row.y - g.size().y / 2.0), g, color);
            x += w + 3.0 * z;
            if row.constant {
                p.text(
                    pos2(x, row.y),
                    Align2::LEFT_CENTER,
                    "C",
                    FontId::proportional(8.5 * z),
                    FG_2,
                );
                x += 9.0 * z;
            }
            if row.integer && row.readout.is_none() && !row.expanded {
                p.text(
                    pos2(x, row.y),
                    Align2::LEFT_CENTER,
                    "⌊⌋",
                    FontId::monospace(9.0 * z),
                    FG_2,
                );
            }
            if let Some(ro) = &row.readout {
                p.text(
                    pos2(x + 3.0 * z, row.y),
                    Align2::LEFT_CENTER,
                    ro,
                    FontId::monospace(10.0 * z),
                    GOLD.gamma_multiply(0.8),
                );
            }
        }
        if let Some((label, port)) = &row.output {
            let live = (st.out_text)(port).filter(|t| !t.is_empty());
            let (text, color) = match &live {
                Some(t) => (t.clone(), FG_0),
                None => (label.clone(), FG_1),
            };
            // Keep clear of an inline field on the same row.
            let left = b
                .fields
                .iter()
                .filter(|f| (f.rect.center().y - row.y).abs() < 1.0)
                .map(|f| f.rect.right() + 4.0 * z)
                .fold(b.rect.left(), f32::max);
            let clip = Rect::from_min_max(
                pos2(left, row.y - 10.0 * z),
                pos2(b.rect.right() - 6.0 * z, row.y + 10.0 * z),
            );
            p.with_clip_rect(clip).text(
                pos2(b.rect.right() - 10.0 * z, row.y),
                Align2::RIGHT_CENTER,
                text,
                FontId::monospace(10.5 * z),
                color,
            );
        }
    }
    for f in &b.fields {
        if st.editing.is_some_and(|e| e.rect == f.rect) {
            continue;
        }
        field(p, f, z);
    }
}

pub fn field(p: &Painter, f: &Field, z: f32) {
    let r = f.rect;
    let mono = FontId::monospace(10.0 * z);
    let boxed = |p: &Painter| {
        p.rect(
            r,
            3.0 * z,
            BG_0,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
    };
    match &f.kind {
        FieldKind::Bool => {
            boxed(p);
            if values::as_bool(&f.value) {
                let c = r.center();
                let s = r.height() * 0.3;
                p.line(
                    vec![
                        c + vec2(-s, 0.0),
                        c + vec2(-s * 0.3, s * 0.7),
                        c + vec2(s, -s * 0.8),
                    ],
                    Stroke::new(1.6 * z, GOLD),
                );
            }
        }
        FieldKind::Number { .. } | FieldKind::Vec2 => {
            boxed(p);
            let v = match f.axis {
                Some(a) => {
                    let (x, y) = values::as_vec2(&f.value);
                    if a == 0 {
                        x
                    } else {
                        y
                    }
                }
                None => values::as_f64(&f.value),
            };
            p.with_clip_rect(r).text(
                r.center(),
                Align2::CENTER_CENTER,
                values::num(v, 3),
                mono,
                FG_0,
            );
        }
        FieldKind::Text => {
            boxed(p);
            p.with_clip_rect(r.shrink(2.0)).text(
                r.left_center() + vec2(4.0 * z, 0.0),
                Align2::LEFT_CENTER,
                values::as_text(&f.value),
                mono,
                FG_0,
            );
        }
        FieldKind::Color => {
            boxed(p);
            let rgba = values::as_rgba(&f.value);
            let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let swatch = Rect::from_min_size(
                r.min + vec2(2.0, 2.0) * z,
                vec2(10.0, r.height() / z - 4.0) * z,
            );
            p.rect_filled(
                swatch,
                2.0 * z,
                Color32::from_rgb(c(rgba[0]), c(rgba[1]), c(rgba[2])),
            );
            if r.width() < 56.0 * z {
                return;
            }
            p.with_clip_rect(r).text(
                swatch.right_center() + vec2(4.0 * z, 0.0),
                Align2::LEFT_CENTER,
                values::hex(rgba),
                mono,
                FG_0,
            );
        }
        FieldKind::Select(_) => {
            boxed(p);
            p.with_clip_rect(r.shrink(2.0)).text(
                r.left_center() + vec2(4.0 * z, 0.0),
                Align2::LEFT_CENTER,
                values::as_text(&f.value),
                mono,
                FG_0,
            );
            p.text(
                r.right_center() - vec2(4.0 * z, 0.0),
                Align2::RIGHT_CENTER,
                "▾",
                FontId::monospace(9.0 * z),
                FG_1,
            );
        }
    }
}

/// A small value / warning tooltip on the tooltip layer.
pub fn tooltip(ctx: &egui::Context, at: Pos2, title: &str, kind: &str, body: &str) {
    let p = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("floret-canvas-tip")));
    let title_g = p.layout_no_wrap(title.to_owned(), FontId::proportional(11.5), FG_0);
    let kind_g = p.layout_no_wrap(kind.to_owned(), FontId::monospace(10.0), FG_2);
    let body_g = p.layout(body.to_owned(), FontId::monospace(11.0), GOLD, 360.0);
    let w = (title_g.size().x + kind_g.size().x + 14.0)
        .max(body_g.size().x)
        .max(80.0)
        + 16.0;
    let h = title_g.size().y + body_g.size().y + 18.0;
    let screen = ctx.content_rect();
    let mut min = at + vec2(14.0, -h - 6.0);
    min.x = min.x.min(screen.right() - w - 4.0);
    if min.y < screen.top() + 4.0 {
        min.y = at.y + 14.0;
    }
    let r = Rect::from_min_size(min, vec2(w, h));
    p.rect(
        r,
        6.0,
        theme::BG_1,
        Stroke::new(1.0, BORDER),
        StrokeKind::Inside,
    );
    let kind_w = kind_g.size().x;
    p.galley(r.min + vec2(8.0, 6.0), title_g, FG_0);
    p.galley(pos2(r.right() - 8.0 - kind_w, r.top() + 7.0), kind_g, FG_2);
    let line_y = r.top() + 6.0 + 15.0;
    p.line_segment(
        [pos2(r.left() + 6.0, line_y), pos2(r.right() - 6.0, line_y)],
        Stroke::new(1.0, BORDER),
    );
    p.galley(pos2(r.left() + 8.0, line_y + 4.0), body_g, GOLD);
}

/// Translucent rectangle while drawing a new region.
pub fn region_preview(p: &Painter, r: Rect, kind: Option<RegionKind>, label: &str) {
    let accent = frame_color(kind);
    p.rect(
        r,
        6.0,
        accent.gamma_multiply(0.08),
        Stroke::new(1.5, accent.gamma_multiply(0.9)),
        StrokeKind::Inside,
    );
    p.text(
        r.left_top() + vec2(8.0, 6.0),
        Align2::LEFT_TOP,
        label,
        FontId::monospace(11.0),
        accent,
    );
}

/// The ghost of a palette node being dragged over the canvas.
pub fn drop_ghost(p: &Painter, at: Pos2, label: &str, category: &str, z: f32) {
    let r = Rect::from_min_size(at, vec2(super::geom::NODE_W, HEADER_H + 8.0) * z);
    p.rect(
        r,
        6.0 * z,
        BG_2.gamma_multiply(0.85),
        Stroke::new(1.0, GOLD.gamma_multiply(0.7)),
        StrokeKind::Inside,
    );
    p.circle_filled(
        r.left_top() + vec2(12.0, HEADER_H / 2.0) * z,
        3.2 * z,
        theme::category_color(category),
    );
    p.text(
        r.left_top() + vec2(22.0, HEADER_H / 2.0) * z,
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.5 * z),
        FG_0,
    );
}

pub fn mix_wire(base: Color32) -> Color32 {
    mix(base, FG_2, 0.75)
}
