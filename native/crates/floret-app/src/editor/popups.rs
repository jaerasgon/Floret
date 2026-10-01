//! Canvas overlays: the node-search popover and right-click menus, drawn
//! as foreground areas in Floret's dark theme.

use super::favorites::Favorites;
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
        // A press and release in one frame leaves no press origin.
        let at = i.pointer.press_origin().or(i.pointer.latest_pos());
        i.pointer.any_pressed() && at.is_some_and(|p| !rect.contains(p))
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

/// A ★ / ☆ toggle at the right of a list row. Returns whether it was hit.
fn star(ui: &egui::Ui, row: Rect, on: bool) -> bool {
    let r = Rect::from_center_size(row.right_center() - vec2(12.0, 0.0), vec2(18.0, 18.0));
    let resp = ui.interact(r, ui.id().with(("star", row.min.y as i32)), Sense::click());
    let color = if on {
        GOLD
    } else if resp.hovered() {
        FG_1
    } else {
        FG_2
    };
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        if on { "★" } else { "☆" },
        FontId::proportional(13.0),
        color,
    );
    resp.on_hover_text(if on {
        "Unfavorite"
    } else {
        "Favorite (adds to Q menu)"
    })
    .clicked()
}

/// The node-search popover. ↑/↓ move, Enter picks, Esc / click outside
/// closes; ☆ stars a type. Returns the picked node type.
pub fn search_popup(
    ctx: &egui::Context,
    bounds: Rect,
    cat: &Catalogue,
    favs: &mut Favorites,
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
                        // Focus before the field runs, so the first
                        // keystroke after opening isn't lost.
                        let id = Id::new("floret-node-search-input");
                        ui.memory_mut(|m| m.request_focus(id));
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut st.query)
                                .id(id)
                                .hint_text(hint)
                                .desired_width(SEARCH_W - 40.0)
                                .frame(egui::Frame::NONE),
                        );
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
                                    rect.right_center() - vec2(28.0, 0.0),
                                    Align2::RIGHT_CENTER,
                                    &s.ty,
                                    FontId::monospace(10.5),
                                    FG_2,
                                );
                                if star(ui, rect, favs.contains(&s.ty)) {
                                    favs.toggle(&s.ty);
                                    continue;
                                }
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

pub struct FavoritesState {
    pub screen: Pos2,
    pub at: Position,
    pub highlight: usize,
}

const DIGITS: [Key; 9] = [
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

/// The favorites quick-menu (Q): ↑/↓ + Enter, or 1–9 to place at once.
pub fn favorites_popup(
    ctx: &egui::Context,
    bounds: Rect,
    cat: &Catalogue,
    favs: &Favorites,
    st: &mut FavoritesState,
) -> Outcome<String> {
    let defs: Vec<_> = favs
        .types()
        .iter()
        .filter_map(|t| cat.get(t))
        .filter(|s| !s.hidden)
        .collect();
    let n = defs.len();
    if st.highlight >= n.max(1) {
        st.highlight = 0;
    }
    let (esc, down, up, enter, digit) = ctx.input_mut(|i| {
        let digit = DIGITS
            .iter()
            .position(|k| i.consume_key(egui::Modifiers::NONE, *k));
        (
            i.consume_key(egui::Modifiers::NONE, Key::Escape),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::Enter),
            digit,
        )
    });
    if esc {
        return Outcome::Close;
    }
    if let Some(d) = digit.and_then(|d| defs.get(d)) {
        return Outcome::Pick(d.ty.clone());
    }
    if n > 0 {
        if down {
            st.highlight = (st.highlight + 1) % n;
        }
        if up {
            st.highlight = (st.highlight + n - 1) % n;
        }
        if enter {
            return Outcome::Pick(defs[st.highlight].ty.clone());
        }
    }
    let w = 260.0;
    let pos = clamp(st.screen, vec2(w, n.max(1) as f32 * ROW_H + 44.0), bounds);
    let mut picked = None;
    let area = egui::Area::new(Id::new("floret-favorites"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(theme::BG_1)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(CornerRadius::same(9))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(w);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("★ Favorites").color(GOLD).small());
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| ui.label(egui::RichText::new("Q").color(FG_2).small()),
                        );
                    });
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if defs.is_empty() {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "No favorites yet — click the ☆ on a node in the palette or search.",
                            )
                            .color(FG_2)
                            .small(),
                        );
                    }
                    for (i, s) in defs.iter().enumerate() {
                        let (rect, resp) = ui
                            .allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                        if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                            st.highlight = i;
                        }
                        let on = i == st.highlight;
                        if on {
                            ui.painter().rect_filled(rect, 5.0, BG_3);
                        }
                        let p = ui.painter();
                        if i < 9 {
                            p.text(
                                rect.left_center() + vec2(9.0, 0.0),
                                Align2::CENTER_CENTER,
                                (i + 1).to_string(),
                                FontId::monospace(11.0),
                                FG_2,
                            );
                        }
                        p.circle_filled(
                            rect.left_center() + vec2(22.0, 0.0),
                            3.0,
                            theme::category_color(&s.category),
                        );
                        p.text(
                            rect.left_center() + vec2(32.0, 0.0),
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
                        if resp.clicked() {
                            picked = Some(s.ty.clone());
                        }
                    }
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

/// A single-line text field floating over the canvas (inline value or
/// port-name editing). `Pick` on Enter / focus loss, `Close` on Esc.
pub fn inline_text(ctx: &egui::Context, rect: Rect, text: &mut String, first: bool) -> Outcome<()> {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        return Outcome::Close;
    }
    let mut out = Outcome::Open;
    egui::Area::new(Id::new("floret-inline-text"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            let edit = egui::TextEdit::singleline(text)
                .font(FontId::monospace((rect.height() * 0.7).clamp(9.0, 16.0)))
                .desired_width(rect.width().max(40.0))
                .margin(egui::Margin::symmetric(3, 1))
                .background_color(theme::BG_0);
            let resp = ui.add(edit);
            if first {
                resp.request_focus();
                if let Some(mut state) = egui::TextEdit::load_state(ctx, resp.id) {
                    let all = egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(text.chars().count()),
                    );
                    state.cursor.set_char_range(Some(all));
                    state.store(ctx, resp.id);
                }
            } else if resp.lost_focus() {
                out = Outcome::Pick(());
            }
        });
    out
}

pub struct PortEditorState {
    pub node: String,
    pub port: String,
    pub label: String,
    pub ty: floret_engine::spec::PortType,
    pub text: String,
    pub placeholder: String,
    pub screen: Pos2,
    pub first: bool,
}

/// The floating port-value editor (right-click a pin): Enter sets,
/// empty clears, Esc cancels, click outside commits.
pub fn port_editor(ctx: &egui::Context, bounds: Rect, st: &mut PortEditorState) -> Outcome<()> {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        return Outcome::Close;
    }
    let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
    let w = 200.0;
    let pos = clamp(st.screen, vec2(w, 80.0), bounds);
    let first = std::mem::take(&mut st.first);
    let area = egui::Area::new(Id::new("floret-port-editor"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(theme::BG_1)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(w);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&st.label).color(FG_0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(super::ports::type_name(st.ty))
                                    .color(FG_2)
                                    .monospace()
                                    .small(),
                            )
                        });
                    });
                    let id = Id::new("floret-port-editor-input");
                    if first {
                        ui.memory_mut(|m| m.request_focus(id));
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut st.text)
                            .id(id)
                            .hint_text(st.placeholder.as_str())
                            .desired_width(f32::INFINITY),
                    );
                    ui.label(
                        egui::RichText::new("Enter to set · empty to clear · Esc")
                            .color(FG_2)
                            .small(),
                    );
                });
        });
    if enter || pressed_outside(ctx, area.response.rect) {
        return Outcome::Pick(());
    }
    Outcome::Open
}

pub struct ColorState {
    pub node: String,
    pub key: super::values::EditKey,
    pub color: egui::Color32,
    pub hex: String,
    pub screen: Pos2,
    /// The value before editing (restored on Esc).
    pub before: serde_json::Value,
}

/// Colour picker for an inline colour field. Returns `(changed, outcome)`;
/// `Pick` (commit) on click outside or Enter, `Close` (revert) on Esc.
pub fn color_popup(ctx: &egui::Context, bounds: Rect, st: &mut ColorState) -> (bool, Outcome<()>) {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        return (false, Outcome::Close);
    }
    let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
    let mut changed = false;
    let pos = clamp(st.screen, vec2(230.0, 280.0), bounds);
    let area = egui::Area::new(Id::new("floret-color-popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(theme::BG_1)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    if egui::color_picker::color_picker_color32(
                        ui,
                        &mut st.color,
                        egui::color_picker::Alpha::OnlyBlend,
                    ) {
                        changed = true;
                        st.hex = super::values::hex(rgba_of(st.color));
                    }
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut st.hex)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                    if resp.changed() {
                        if let Some(c) = super::values::parse_hex(&st.hex) {
                            st.color = color32(c);
                            changed = true;
                        }
                    }
                });
        });
    if enter || pressed_outside(ctx, area.response.rect) {
        return (changed, Outcome::Pick(()));
    }
    (changed, Outcome::Open)
}

/// 0..1 channels of a colour (unmultiplied).
pub fn rgba_of(c: egui::Color32) -> [f64; 4] {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let f = |v: u8| f64::from(v) / 255.0;
    [f(r), f(g), f(b), f(a)]
}

pub fn color32([r, g, b, a]: [f64; 4]) -> egui::Color32 {
    let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgba_unmultiplied(c(r), c(g), c(b), c(a))
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
