//! Painting the laid-out canvas: region frames with their port pills,
//! node boxes with rows and inline fields, pins, chips and tooltips.

use super::geom::{
    value_text, Field, FieldLabel, Look, NodeBox, PinGeom, BOX_PAD, CHECK, CHECK_GAP, HEADER_H,
    LABEL_FONT, PIN_R, SELECT_ARROW, SWATCH_W, VALUE_FONT,
};
use crate::editor::ports::type_name;
use crate::editor::values::{self, FieldKind};
use crate::theme::{
    self, BG_0, BG_2, BG_3, BORDER, DANGER, FG_0, FG_1, FG_2, FG_3, GOLD, GOLD_TEXT, SPREAD_PIN,
};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Id, LayerId, Order, Painter, Pos2,
    Rect, Stroke, StrokeKind, Vec2,
};
use floret_engine::spec::RegionKind;

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
        Some(RegionKind::If) => *theme::REGION_IF,
        Some(RegionKind::Cache) => *theme::REGION_CACHE,
        Some(RegionKind::Subpatch) => *theme::REGION_SUBPATCH,
        _ => *GOLD,
    }
}

/// The frame accent as title text (ForEach's gold is deeper as text).
fn frame_text_color(kind: Option<RegionKind>) -> Color32 {
    match kind {
        Some(RegionKind::If | RegionKind::Cache | RegionKind::Subpatch) => frame_color(kind),
        _ => *GOLD_TEXT,
    }
}

/// Linear blend `a·t + b·(1−t)`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (f32::from(x) * t + f32::from(y) * (1.0 - t)).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// How a pin sits this frame, apart from the wire-drag looks.
#[derive(Clone, Copy, Default)]
pub struct PinState {
    /// A wire is plugged in.
    pub connected: bool,
    /// Under the pointer.
    pub hovered: bool,
}

/// Below this zoom unwired pins draw filled too.
pub const FILL_PINS_BELOW: f32 = 0.3;

/// A pin: a filled dot when wired, a hollow ring in its type colour when
/// not (filled too when zoomed far out); a touch bigger under the pointer. While a wire is dragged, the
/// pins it can reach light up and the rest dim.
pub fn pin(p: &Painter, g: &PinGeom, look: PinLook, st: PinState, pulse: f32, z: f32) {
    let pos = g.pos;
    let mut color = if g.spread {
        *SPREAD_PIN
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
            let r = PIN_R * z + if st.hovered { 1.0 } else { 0.0 };
            // Far out a ring is a speck: every pin is a filled dot.
            if st.connected || z < FILL_PINS_BELOW {
                p.circle(pos, r, color, Stroke::new(1.5, BG_2));
            } else {
                // The ring's outer edge where a wired dot's fill ends.
                let w = (1.75 * z).clamp(1.0, 3.0);
                p.circle(pos, r - w / 2.0, BG_2, Stroke::new(w, color));
            }
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
    /// A section's tint (`None` for other frames).
    pub tint: Option<Color32>,
    pub selected: bool,
    pub drop_target: bool,
    pub selected_ports: &'a [String],
}

/// A new section's tint (the theme's blue).
pub const SECTION_DEFAULT: &str = "#7cb6ff";

/// A section's tint from its `color` param; `None` for other nodes.
pub fn section_tint(n: &floret_engine::patch::PatchNode) -> Option<Color32> {
    (n.def_type == "meta/section").then(|| {
        let [r, g, b, _] = n
            .params
            .get("color")
            .and_then(|v| v.as_str())
            .and_then(values::parse_hex)
            .or_else(|| values::parse_hex(SECTION_DEFAULT))
            .unwrap_or([0.5, 0.7, 1.0, 1.0]);
        let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color32::from_rgb(c(r), c(g), c(b))
    })
}

/// A section: a flat, tinted card behind its nodes with the title on a
/// tinted strip, no pins or pills.
fn section(p: &Painter, b: &NodeBox, st: &FrameState, tint: Color32, z: f32) {
    let card = b.header.union(b.rect);
    let r = (9.0 * z).min(255.0) as u8;
    let (fill, stroke) = if st.drop_target {
        (tint.gamma_multiply(0.16), Stroke::new(1.5, tint))
    } else if st.selected {
        (tint.gamma_multiply(0.09), Stroke::new(1.25, tint))
    } else {
        (
            tint.gamma_multiply(0.07),
            Stroke::new(0.75, tint.gamma_multiply(0.45)),
        )
    };
    p.rect(
        card,
        CornerRadius::same(r),
        fill,
        Stroke::NONE,
        StrokeKind::Inside,
    );
    p.rect(
        b.header,
        CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        },
        tint.gamma_multiply(0.14),
        Stroke::NONE,
        StrokeKind::Inside,
    );
    p.rect(
        card,
        CornerRadius::same(r),
        Color32::TRANSPARENT,
        stroke,
        StrokeKind::Inside,
    );
    if z >= 0.2 {
        p.text(
            pos2(b.header.left() + 10.0 * z, b.header.center().y),
            Align2::LEFT_CENTER,
            &b.label,
            theme::medium(12.0 * z),
            mix(tint, *FG_0, 0.55),
        );
    }
    if let Some((r, _)) = &b.warning {
        chip(p, *r);
    }
    // Resize grip.
    let c = b.rect.right_bottom() - vec2(4.0, 4.0) * z;
    for k in [4.0, 8.0] {
        p.line_segment(
            [c - vec2(k * z, 0.0), c - vec2(0.0, k * z)],
            Stroke::new(1.0, tint.gamma_multiply(0.6)),
        );
    }
}

pub fn frame(p: &Painter, b: &NodeBox, st: &FrameState, z: f32) {
    if let (Some(RegionKind::Section), Some(tint)) = (st.kind, st.tint) {
        return section(p, b, st, tint, z);
    }
    let accent = frame_color(st.kind);
    let (fill, stroke) = if st.drop_target {
        (accent.gamma_multiply(0.12), Stroke::new(1.5, accent))
    } else if st.selected {
        (
            accent.gamma_multiply(0.05),
            Stroke::new(1.25, accent.gamma_multiply(0.95)),
        )
    } else {
        (
            accent.gamma_multiply(0.04),
            Stroke::new(0.75, accent.gamma_multiply(0.5)),
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
        frame_text_color(st.kind),
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
        *FG_2,
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
            *FG_0,
        );
        if *capped {
            p.text(
                r.right_center() - vec2(5.0 * z, 0.0),
                Align2::RIGHT_CENTER,
                "⚠",
                FontId::proportional(9.0 * z),
                *DANGER,
            );
        }
    }
    if let Some((r, _)) = &b.warning {
        chip(p, *r);
    }
    // Top-left stack: static inputs and loop variables, as small flat
    // cards (card fill, hairline, the cards' corner rounding).
    for t in &b.top {
        p.rect(
            t.rect,
            4.0 * z,
            BG_2,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let color = if t.loop_var {
            theme::port_color("number")
        } else {
            *FG_1
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
            if on { GOLD.gamma_multiply(0.25) } else { *BG_2 },
            Stroke::new(if on { 1.5 } else { 1.0 }, if on { *GOLD } else { color }),
            StrokeKind::Inside,
        );
        p.text(
            pl.rect.center(),
            Align2::CENTER_CENTER,
            &pl.name,
            FontId::proportional(9.0 * z),
            *FG_0,
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
    /// The selected glow's slow breath, 0–1 (0 with reduced motion).
    pub breathe: f32,
    pub subtitle: Option<&'a str>,
    /// The field being typed into (drawn by the text editor instead).
    pub editing: Option<&'a Field>,
    /// Custom-body state (pointer, active control, probe scroll …).
    pub body: super::custom::BodyState,
}

pub fn node(p: &Painter, b: &NodeBox, st: &NodeState, z: f32) {
    node_card(p, b, st, z);
    if let Some(on) = b.bypass {
        bypass(p, b, st, on, z);
    }
}

/// A bypassed node is dimmed under its lit bypass button; the button
/// shows on the others while hovered or selected.
fn bypass(p: &Painter, b: &NodeBox, st: &NodeState, on: bool, z: f32) {
    let r = super::custom::radius(z);
    if on {
        p.rect_filled(b.rect.shrink(1.0), r, BG_2.gamma_multiply(0.6));
    }
    let node_hot = st.body.hover.is_some_and(|h| b.rect.contains(h));
    if z <= 0.25 || !(on || node_hot || st.selected) {
        return;
    }
    let rect = super::geom::bypass_rect(b.header, z);
    let hot = st.body.hover.is_some_and(|h| rect.contains(h));
    let ink = if on {
        p.rect_filled(rect, 4.0 * z, theme::AMBER.gamma_multiply(0.22));
        *theme::AMBER
    } else if hot {
        p.rect_filled(rect, 4.0 * z, *BG_3);
        *FG_0
    } else {
        *FG_2
    };
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::POWER,
        theme::icon_font(12.0 * z),
        ink,
    );
}

fn node_card(p: &Painter, b: &NodeBox, st: &NodeState, z: f32) {
    match b.look {
        Look::Probe => return super::custom::draw_probe(p, b, &st.body, z),
        Look::Comment => return super::custom::draw_comment(p, b, &st.body, z),
        _ => {}
    }
    let r = super::custom::radius(z);
    let hovered = st.body.hover.is_some_and(|h| b.rect.contains(h));
    super::custom::lift(p, b.rect, hovered, z);
    let accent = if b.look == Look::Collapsed {
        frame_color(Some(RegionKind::Subpatch))
    } else {
        theme::category_color(&b.group)
    };
    if st.selected {
        // Soft gold glow (the web app's --glow-gold) that slowly breathes
        // (`node-breathe`) around the crisp gold border.
        let glow = egui::Shadow {
            offset: [0, 0],
            blur: (14.0 + 6.0 * st.breathe) as u8,
            spread: 0,
            color: GOLD.gamma_multiply(0.12 + 0.06 * st.breathe),
        };
        p.add(glow.as_shape(b.rect, r));
    }
    let border = if st.selected {
        Stroke::new(1.5, *GOLD)
    } else if b.warning.is_some() {
        Stroke::new(1.0, DANGER.gamma_multiply(0.6))
    } else if hovered {
        // The hover ring takes the category accent.
        Stroke::new(1.0, mix(accent, *BORDER, 0.55))
    } else {
        Stroke::new(1.0, *BORDER)
    };
    // A flat card: header and rows share the fill, split by a hairline.
    p.rect_filled(b.rect, r, *BG_2);
    if z > 0.2 {
        let y = p.round_to_pixel_center(b.rect.top() + HEADER_H * z);
        p.hline(b.rect.x_range(), y, Stroke::new(1.0, *BORDER));
    }
    p.rect_stroke(b.rect, r, border, StrokeKind::Inside);
    let header = Rect::from_min_size(b.rect.min, vec2(b.rect.width(), HEADER_H * z));
    p.circle_filled(header.left_center() + vec2(14.0 * z, 0.0), 3.2 * z, accent);
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
            theme::medium(12.5 * z)
        };
        let galley = p.layout_no_wrap(title, font, *FG_0);
        let mut x = header.left() + TITLE_X * z + galley.size().x + 6.0 * z;
        p.galley(
            pos2(
                header.left() + TITLE_X * z,
                header.center().y - galley.size().y / 2.0,
            ),
            galley,
            *FG_0,
        );
        if let Some(slot) = &b.slot {
            let g = p.layout_no_wrap(slot.clone(), FontId::monospace(10.0 * z), *GOLD_TEXT);
            let rr = Rect::from_min_size(
                pos2(x, header.center().y - 7.0 * z),
                vec2(g.size().x + 8.0 * z, 14.0 * z),
            );
            p.rect(rr, 7.0 * z, BG_3, Stroke::NONE, StrokeKind::Inside);
            p.galley(rr.center() - g.size() / 2.0, g, *GOLD_TEXT);
            x = rr.right() + 4.0 * z;
        }
        let right = if let Some((r, _)) = &b.warning {
            chip(p, *r);
            r.left() - 4.0 * z
        } else if b.bypass.is_some() {
            header.right() - (12.0 + super::geom::BYPASS_ROOM) * z
        } else {
            header.right() - 12.0 * z
        };
        let tag = if b.unimplemented {
            Some("not ported")
        } else {
            b.header_value.as_deref().or(st.subtitle)
        };
        if let Some(tag) = tag {
            if right - x > 30.0 * z {
                p.text(
                    pos2(right, header.center().y),
                    Align2::RIGHT_CENTER,
                    tag,
                    FontId::proportional(9.5 * z),
                    *FG_2,
                );
            }
        }
    }
    super::custom::draw_body(p, b, &st.body, z);
    if z <= 0.3 {
        for f in &b.fields {
            field(p, f, z, None, false);
        }
        return;
    }
    let font = FontId::proportional(LABEL_FONT * z);
    for row in &b.rows {
        if let Some(label) = &row.label {
            let color = *FG_1;
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
                    *FG_2,
                );
                x += 9.0 * z;
            }
            if row.integer {
                p.text(
                    pos2(x, row.y),
                    Align2::LEFT_CENTER,
                    "⌊⌋",
                    FontId::monospace(9.0 * z),
                    *FG_2,
                );
            }
        }
        if let Some((label, _)) = &row.output {
            // Output rows show the port name (as in the web app); the live
            // value is in the pin's hover tooltip. Keep clear of an inline
            // field on the same row.
            let left = b
                .fields
                .iter()
                .filter(|f| (f.rect.center().y - row.y).abs() < 1.0)
                .map(|f| f.rect.right() + 4.0 * z)
                .fold(b.rect.left(), f32::max);
            let right = b.rect.right() - 10.0 * z;
            let g = p.layout_no_wrap(label.clone(), FontId::proportional(LABEL_FONT * z), *FG_1);
            if right - g.size().x >= left {
                p.galley(pos2(right - g.size().x, row.y - g.size().y / 2.0), g, *FG_1);
            }
        }
    }
    for f in &b.fields {
        let editing = st.editing.is_some_and(|e| e.rect == f.rect);
        // A value box keeps its box and name under the text editor.
        if editing && f.label.is_none() {
            continue;
        }
        field(p, f, z, st.body.hover, editing);
    }
}

/// Where a node's title starts (after the category lamp), canvas units.
pub const TITLE_X: f32 = 24.0;

/// A value box's fill, lighter under the pointer.
fn box_fill(hot: bool) -> Color32 {
    if hot {
        mix(*FG_3, *BG_3, 0.4)
    } else {
        mix(*FG_3, *BG_3, 0.25)
    }
}

/// `s` cut with "…" to fit `max` wide.
fn fit(p: &Painter, s: &str, font: &FontId, max: f32) -> String {
    let w = |t: &str| p.layout_no_wrap(t.to_owned(), font.clone(), *FG_0).size().x;
    if w(s) <= max {
        return s.to_owned();
    }
    let mut chars: Vec<char> = s.chars().take(64).collect();
    while chars.pop().is_some() {
        let t = format!("{}…", chars.iter().collect::<String>());
        if w(&t) <= max {
            return t;
        }
    }
    String::new()
}

/// A row's value box (Blender style): one rounded box, the name at its
/// left, the value right-aligned; a checkbox and its name for a bool; X
/// and Y cells for a vec2. `editing`: the text editor covers the value.
fn value_box(p: &Painter, f: &Field, tag: &FieldLabel, z: f32, hover: Option<Pos2>, editing: bool) {
    let hot = |r: Rect| hover.is_some_and(|h| r.contains(h));
    // Far out: the boxes alone, no text.
    let text = z > 0.3;
    let label_font = FontId::proportional(LABEL_FONT * z);
    let mono = FontId::monospace(VALUE_FONT * z);
    // The name (and `C` mark) from `x`.
    let name = |x: f32| {
        let g = p.layout_no_wrap(tag.text.clone(), label_font.clone(), *FG_2);
        let w = g.size().x;
        let y = tag.frame.center().y;
        p.galley(pos2(x, y - g.size().y / 2.0), g, *FG_2);
        if tag.constant {
            p.text(
                pos2(x + w + 3.0 * z, y),
                Align2::LEFT_CENTER,
                "C",
                FontId::proportional(8.5 * z),
                *FG_2,
            );
        }
    };
    let radius = 4.0 * z;
    if f.kind == FieldKind::Bool {
        let check = Rect::from_center_size(
            pos2(f.rect.left() + CHECK * z / 2.0, f.rect.center().y),
            vec2(CHECK, CHECK) * z,
        );
        p.rect_filled(check, 3.0 * z, box_fill(hot(f.rect)));
        if values::as_bool(&f.value) {
            let c = check.center();
            let s = check.height() * 0.3;
            p.line(
                vec![
                    c + vec2(-s, 0.0),
                    c + vec2(-s * 0.3, s * 0.7),
                    c + vec2(s, -s * 0.8),
                ],
                Stroke::new(1.6 * z, GOLD),
            );
        }
        if text {
            name(check.right() + CHECK_GAP * z);
        }
        return;
    }
    // A vec2's box and name come with its X cell.
    if f.axis != Some(1) {
        let whole = f.axis.is_none() && hot(tag.frame);
        p.rect_filled(tag.frame, radius, box_fill(whole));
        if text {
            name(tag.frame.left() + BOX_PAD * z);
        }
    }
    let vr = f.value_rect;
    let clip = p.with_clip_rect(vr);
    match &f.kind {
        FieldKind::Vec2 => {
            p.rect_filled(f.rect, 3.0 * z, mix(*BG_2, box_fill(hot(f.rect)), 0.55));
            if text && !editing {
                let (x, y) = values::as_vec2(&f.value);
                let v = if f.axis == Some(1) { y } else { x };
                clip.text(
                    vr.center(),
                    Align2::CENTER_CENTER,
                    values::num(v, 3),
                    mono,
                    *GOLD_TEXT,
                );
            }
        }
        _ if !text || editing => {}
        FieldKind::Number { .. } => {
            clip.text(
                vr.right_center(),
                Align2::RIGHT_CENTER,
                value_text(&f.kind, &f.value),
                mono,
                *GOLD_TEXT,
            );
        }
        FieldKind::Text => {
            let s = fit(p, &value_text(&f.kind, &f.value), &mono, vr.width());
            clip.text(vr.right_center(), Align2::RIGHT_CENTER, s, mono, *FG_0);
        }
        FieldKind::Select(_) => {
            clip.text(
                vr.right_center(),
                Align2::RIGHT_CENTER,
                "▾",
                FontId::monospace(9.0 * z),
                *FG_1,
            );
            let s = value_text(&f.kind, &f.value);
            let room = vr.width() - SELECT_ARROW * z;
            let s = fit(p, &s, &mono, room);
            clip.text(
                vr.right_center() - vec2(SELECT_ARROW * z, 0.0),
                Align2::RIGHT_CENTER,
                s,
                mono,
                *FG_0,
            );
        }
        FieldKind::Color => {
            let rgba = values::as_rgba(&f.value);
            let g = p.layout_no_wrap(value_text(&f.kind, &f.value), mono, *FG_0);
            let hex_w = g.size().x;
            // The hex at the right, its swatch before it; just the swatch
            // when the hex won't fit.
            let swatch_right = if (SWATCH_W + 4.0) * z + hex_w <= vr.width() + 0.5 {
                let at = pos2(vr.right() - hex_w, vr.center().y - g.size().y / 2.0);
                clip.galley(at, g, *FG_0);
                at.x - 4.0 * z
            } else {
                vr.right()
            };
            let swatch = Rect::from_center_size(
                pos2(swatch_right - SWATCH_W * z / 2.0, vr.center().y),
                vec2(SWATCH_W, SWATCH_W) * z,
            );
            let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            p.rect_filled(
                swatch,
                2.0 * z,
                Color32::from_rgb(c(rgba[0]), c(rgba[1]), c(rgba[2])),
            );
        }
        FieldKind::Bool => {}
    }
}

pub fn field(p: &Painter, f: &Field, z: f32, hover: Option<Pos2>, editing: bool) {
    if let Some(tag) = &f.label {
        return value_box(p, f, tag, z, hover, editing);
    }
    let r = f.rect;
    let mono = FontId::monospace(10.0 * z);
    if f.bare && !super::custom::bare_hot(hover, r) {
        // Plain text until hovered (`.iobox-range-input`).
        let text = match &f.kind {
            FieldKind::Number { .. } => values::num(values::as_f64(&f.value), 3),
            _ => values::as_text(&f.value),
        };
        if z > 0.3 {
            p.with_clip_rect(r).text(
                r.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::monospace(9.0 * z),
                *FG_2,
            );
        }
        if matches!(f.kind, FieldKind::Text) {
            p.extend(egui::Shape::dashed_line(
                &[r.left_bottom(), r.right_bottom()],
                Stroke::new(1.0, BORDER),
                3.0 * z,
                2.0 * z,
            ));
        }
        return;
    }
    if f.multiline {
        p.rect(
            r,
            3.0 * z,
            BG_0,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        if z > 0.3 {
            let text = values::as_text(&f.value);
            let (text, color) = if text.trim().is_empty() {
                let hint = match &f.key {
                    values::EditKey::Param(k) if k == "expr" => "formula, e.g. sin(x * 3) + t",
                    _ => "<svg>…</svg>",
                };
                (hint.to_owned(), FG_2)
            } else {
                (text, FG_0)
            };
            let g = p.layout(
                text,
                FontId::monospace(9.0 * z),
                *color,
                r.width() - 6.0 * z,
            );
            p.with_clip_rect(r.shrink(2.0))
                .galley(r.min + vec2(3.0, 3.0) * z, g, *color);
        }
        return;
    }
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
                *FG_0,
            );
        }
        FieldKind::Text => {
            boxed(p);
            p.with_clip_rect(r.shrink(2.0)).text(
                r.left_center() + vec2(4.0 * z, 0.0),
                Align2::LEFT_CENTER,
                values::as_text(&f.value),
                mono,
                *FG_0,
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
                *FG_0,
            );
        }
        FieldKind::Select(_) => {
            boxed(p);
            // The option text stops short of the ▾.
            let text = Rect::from_min_max(r.min, pos2(r.right() - 12.0 * z, r.bottom()));
            p.with_clip_rect(text.shrink(2.0)).text(
                r.left_center() + vec2(4.0 * z, 0.0),
                Align2::LEFT_CENTER,
                values::option_label(&values::as_text(&f.value)),
                mono,
                *FG_0,
            );
            p.text(
                r.right_center() - vec2(4.0 * z, 0.0),
                Align2::RIGHT_CENTER,
                "▾",
                FontId::monospace(9.0 * z),
                *FG_1,
            );
        }
    }
}

/// A small value / warning tooltip on the tooltip layer.
pub fn tooltip(ctx: &egui::Context, at: Pos2, title: &str, kind: &str, body: &str) {
    let p = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("floret-canvas-tip")));
    let title_g = p.layout_no_wrap(title.to_owned(), FontId::proportional(11.5), *FG_0);
    let kind_g = p.layout_no_wrap(kind.to_owned(), FontId::monospace(10.0), *FG_2);
    let body_g = p.layout(body.to_owned(), FontId::monospace(11.0), *GOLD_TEXT, 360.0);
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
    p.galley(r.min + vec2(8.0, 6.0), title_g, *FG_0);
    p.galley(pos2(r.right() - 8.0 - kind_w, r.top() + 7.0), kind_g, *FG_2);
    let line_y = r.top() + 6.0 + 15.0;
    p.line_segment(
        [pos2(r.left() + 6.0, line_y), pos2(r.right() - 6.0, line_y)],
        Stroke::new(1.0, BORDER),
    );
    p.galley(pos2(r.left() + 8.0, line_y + 4.0), body_g, *GOLD_TEXT);
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
        frame_text_color(kind),
    );
}

/// The ghost of a palette node being dragged over the canvas: the card's
/// own size, title (`math`: a header symbol, in the monospace font) and
/// lamp, so it lands exactly where it's shown.
pub fn drop_ghost(
    p: &Painter,
    at: Pos2,
    size: Vec2,
    (label, math): (&str, bool),
    group: &str,
    z: f32,
) {
    let r = Rect::from_min_size(at, size * z);
    let radius = super::custom::radius(z);
    p.rect(
        r,
        radius,
        BG_2.gamma_multiply(0.85),
        Stroke::new(1.0, GOLD.gamma_multiply(0.7)),
        StrokeKind::Inside,
    );
    if r.height() > HEADER_H * z + 2.0 {
        let y = p.round_to_pixel_center(r.top() + HEADER_H * z);
        p.hline(
            r.x_range(),
            y,
            Stroke::new(1.0, BORDER.gamma_multiply(0.85)),
        );
    }
    p.circle_filled(
        r.left_top() + vec2(14.0, HEADER_H / 2.0) * z,
        3.2 * z,
        theme::category_color(group),
    );
    let font = if math {
        FontId::monospace(13.0 * z)
    } else {
        theme::medium(12.5 * z)
    };
    p.text(
        r.left_top() + vec2(TITLE_X, HEADER_H / 2.0) * z,
        Align2::LEFT_CENTER,
        label,
        font,
        *FG_0,
    );
}

/// A resting wire: its type colour, muted toward the canvas so the nodes
/// read first.
pub fn mix_wire(base: Color32) -> Color32 {
    mix(mix(base, *FG_2, 0.6), *BG_0, 0.45)
}
