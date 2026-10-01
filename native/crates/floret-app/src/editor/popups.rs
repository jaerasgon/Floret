//! Canvas overlays: the node-search popover and right-click menus, drawn
//! as foreground areas in Floret's dark theme.

use super::ports::Side;
use super::search::{search, Fit};
use crate::theme::{self, BG_2, BG_3, BORDER, DANGER, FG_0, FG_1, FG_2, GOLD};
use eframe::egui::{
    self, pos2, vec2, Align2, CornerRadius, FontId, Id, Key, Pos2, Rect, Sense, Stroke,
};
use floret_engine::patch::{PatchEdge, Position};
use floret_engine::spec::Catalogue;

const SEARCH_W: f32 = 300.0;
const SEARCH_LIST_H: f32 = 300.0;
const ROW_H: f32 = 24.0;
const MENU_W: f32 = 210.0;

/// Why the search is open: what to do with the picked node.
#[derive(Clone, Debug)]
pub enum Purpose {
    Plain,
    /// A wire dragged from this pin was dropped on empty canvas.
    Wire {
        node: String,
        side: Side,
        port: String,
    },
    /// Double-clicked wire: split it with the new node.
    Insert {
        edge: PatchEdge,
    },
}

pub struct SearchState {
    pub screen: Pos2,
    /// Where the new node goes (canvas coordinates).
    pub at: Position,
    pub query: String,
    pub highlight: usize,
    pub fit: Fit,
    pub purpose: Purpose,
}

impl SearchState {
    pub fn new(screen: Pos2, at: Position, fit: Fit, purpose: Purpose) -> SearchState {
        SearchState {
            screen,
            at,
            query: String::new(),
            highlight: 0,
            fit,
            purpose,
        }
    }
}

pub enum Outcome<T> {
    Open,
    Close,
    Pick(T),
}

fn pressed_outside(ctx: &egui::Context, rect: Rect) -> bool {
    ctx.input(|i| {
        i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| !rect.contains(p))
    })
}

/// Clamp a popup's top-left so a `size` box stays inside `bounds`.
fn clamp(p: Pos2, size: egui::Vec2, bounds: Rect) -> Pos2 {
    pos2(
        p.x.min(bounds.right() - size.x - 8.0)
            .max(bounds.left() + 4.0),
        p.y.min(bounds.bottom() - size.y - 8.0)
            .max(bounds.top() + 4.0),
    )
}

/// The node-search popover. ↑/↓ move, Enter picks, Esc / click outside
/// closes. Returns the picked node type.
pub fn search_popup(
    ctx: &egui::Context,
    bounds: Rect,
    cat: &Catalogue,
    st: &mut SearchState,
) -> Outcome<String> {
    let results = search(cat, &st.query, &st.fit);
    if st.highlight >= results.len() {
        st.highlight = 0;
    }
    let (esc, down, up, enter) = ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, Key::Escape),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::Enter),
        )
    });
    if esc {
        return Outcome::Close;
    }
    let n = results.len();
    if n > 0 {
        if down {
            st.highlight = (st.highlight + 1) % n;
        }
        if up {
            st.highlight = (st.highlight + n - 1) % n;
        }
        if enter {
            return Outcome::Pick(results[st.highlight].ty.clone());
        }
    }
    let pos = clamp(st.screen, vec2(SEARCH_W, SEARCH_LIST_H + 48.0), bounds);
    let mut picked = None;
    let area = egui::Area::new(Id::new("floret-node-search"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(theme::BG_1)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(CornerRadius::same(9))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(SEARCH_W);
                    ui.horizontal(|ui| {
                        let hint = match st.fit {
                            Fit::Any => "add node…",
                            _ => "add compatible node…",
                        };
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut st.query)
                                .hint_text(hint)
                                .desired_width(SEARCH_W - 40.0)
                                .frame(egui::Frame::NONE),
                        );
                        edit.request_focus();
                        if edit.changed() {
                            st.highlight = 0;
                        }
                        ui.label(egui::RichText::new("Esc").color(FG_2).small());
                    });
                    ui.add_space(2.0);
                    let r = ui.available_rect_before_wrap();
                    ui.painter()
                        .line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, BORDER));
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height(SEARCH_LIST_H)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            if results.is_empty() {
                                ui.label(egui::RichText::new("no matches").color(FG_2));
                            }
                            for (i, s) in results.iter().enumerate() {
                                let (rect, resp) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), ROW_H),
                                    Sense::click(),
                                );
                                if resp.hovered()
                                    && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO)
                                {
                                    st.highlight = i;
                                }
                                let on = i == st.highlight;
                                if on {
                                    ui.painter().rect_filled(rect, 5.0, BG_3);
                                    if down || up {
                                        resp.scroll_to_me(None);
                                    }
                                }
                                let p = ui.painter();
                                p.circle_filled(
                                    rect.left_center() + vec2(9.0, 0.0),
                                    3.0,
                                    theme::category_color(&s.category),
                                );
                                p.text(
                                    rect.left_center() + vec2(20.0, 0.0),
                                    Align2::LEFT_CENTER,
                                    &s.label,
                                    FontId::proportional(13.0),
                                    if on { GOLD } else { FG_0 },
                                );
                                p.text(
                                    rect.right_center() - vec2(8.0, 0.0),
                                    Align2::RIGHT_CENTER,
                                    &s.ty,
                                    FontId::monospace(10.5),
                                    FG_2,
                                );
                                let resp = match &s.description {
                                    Some(d) => resp.on_hover_text(d),
                                    None => resp,
                                };
                                if resp.clicked() {
                                    picked = Some(s.ty.clone());
                                }
                            }
                        });
                });
        });
    if let Some(ty) = picked {
        return Outcome::Pick(ty);
    }
    if pressed_outside(ctx, area.response.rect) {
        return Outcome::Close;
    }
    Outcome::Open
}

/// One row of a context menu (`action: None` draws a separator).
pub struct MenuItem<A> {
    pub label: String,
    pub shortcut: Option<&'static str>,
    pub enabled: bool,
    pub danger: bool,
    pub action: Option<A>,
}

impl<A> MenuItem<A> {
    pub fn new(label: impl Into<String>, action: A) -> Self {
        MenuItem {
            label: label.into(),
            shortcut: None,
            enabled: true,
            danger: false,
            action: Some(action),
        }
    }

    pub fn separator() -> Self {
        MenuItem {
            label: String::new(),
            shortcut: None,
            enabled: false,
            danger: false,
            action: None,
        }
    }

    pub fn key(mut self, s: &'static str) -> Self {
        self.shortcut = Some(s);
        self
    }

    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

/// A right-click menu at `at`; returns the picked action.
pub fn menu_popup<A: Clone>(
    ctx: &egui::Context,
    bounds: Rect,
    at: Pos2,
    items: &[MenuItem<A>],
) -> Outcome<A> {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        return Outcome::Close;
    }
    let h: f32 = items
        .iter()
        .map(|i| if i.action.is_some() { ROW_H } else { 9.0 })
        .sum();
    let pos = clamp(at, vec2(MENU_W, h + 12.0), bounds);
    let mut picked = None;
    let area = egui::Area::new(Id::new("floret-canvas-menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style())
                .fill(theme::BG_1)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(5))
                .show(ui, |ui| {
                    ui.set_width(MENU_W);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for item in items {
                        let Some(action) = &item.action else {
                            let (r, _) = ui.allocate_exact_size(vec2(MENU_W, 9.0), Sense::hover());
                            ui.painter().line_segment(
                                [r.left_center(), r.right_center()],
                                Stroke::new(1.0, BORDER),
                            );
                            continue;
                        };
                        let sense = if item.enabled {
                            Sense::click()
                        } else {
                            Sense::hover()
                        };
                        let (rect, resp) = ui.allocate_exact_size(vec2(MENU_W, ROW_H), sense);
                        if item.enabled && resp.hovered() {
                            ui.painter().rect_filled(rect, 5.0, BG_3);
                        }
                        let color = match (item.enabled, item.danger) {
                            (false, _) => FG_2,
                            (true, true) => DANGER,
                            (true, false) => FG_0,
                        };
                        ui.painter().text(
                            rect.left_center() + vec2(9.0, 0.0),
                            Align2::LEFT_CENTER,
                            &item.label,
                            FontId::proportional(13.0),
                            color,
                        );
                        if let Some(k) = item.shortcut {
                            ui.painter().text(
                                rect.right_center() - vec2(9.0, 0.0),
                                Align2::RIGHT_CENTER,
                                k,
                                FontId::proportional(11.0),
                                FG_1,
                            );
                        }
                        if resp.clicked() {
                            picked = Some(action.clone());
                        }
                    }
                });
        });
    if let Some(a) = picked {
        return Outcome::Pick(a);
    }
    if pressed_outside(ctx, area.response.rect) {
        return Outcome::Close;
    }
    Outcome::Open
}

/// Small pill-shaped toggle / button for the canvas corner toolbar.
pub fn chip(ui: &mut egui::Ui, label: &str, on: bool, tip: &str) -> egui::Response {
    let font = FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, FG_0);
    let size = vec2(galley.size().x + 18.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if resp.hovered() { BG_3 } else { BG_2 };
    let stroke = if on {
        Stroke::new(1.0, GOLD.gamma_multiply(0.8))
    } else {
        Stroke::new(1.0, BORDER)
    };
    ui.painter()
        .rect(rect, 6.0, fill, stroke, egui::StrokeKind::Inside);
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        if on { GOLD } else { FG_1 },
    );
    if resp.clicked() {
        // Don't keep keyboard focus: canvas shortcuts (Tab, F, …) stay live.
        resp.surrender_focus();
    }
    resp.on_hover_text(tip)
}
