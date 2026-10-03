//! Keyboard shortcuts (the `?` overlay and Help → Keyboard Shortcuts) and
//! About (#74).
//!
//! [`groups`] lists every shortcut the app implements: the menu commands
//! (from [`crate::menu`]) plus the canvas's own keys and pointer gestures
//! (`canvas.rs`, `editor/popups.rs`, `preview.rs`). Both the overlay and
//! the window draw from it, so keep it in step with them.

use crate::menu::Command;
use crate::theme::{BG_0, BG_2, BORDER, FG_0, FG_1, FG_2, GOLD, GOLD_TEXT};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};

/// How a shortcut is triggered.
pub enum Trigger {
    Keys(KeyboardShortcut),
    /// Free text (pointer gestures, key combinations egui can't express).
    Text(&'static str),
}

impl Trigger {
    fn text(&self) -> String {
        match self {
            Trigger::Keys(sc) => crate::menu::shortcut_text(sc),
            Trigger::Text(s) => (*s).to_owned(),
        }
    }
}

pub struct Section {
    pub title: &'static str,
    pub rows: Vec<(Trigger, &'static str)>,
}

fn keys(m: Modifiers, k: Key) -> Trigger {
    Trigger::Keys(KeyboardShortcut::new(m, k))
}

fn menu(c: Command) -> (Trigger, &'static str) {
    let sc = c.shortcut().expect("menu command has a shortcut");
    (Trigger::Keys(sc), c.label())
}

/// Every shortcut, grouped.
pub fn groups() -> Vec<Section> {
    use Command::*;
    let none = Modifiers::NONE;
    let mut file = vec![
        menu(New),
        menu(Open),
        menu(BrowseSamples),
        menu(Save),
        menu(SaveAs),
        menu(Preferences),
    ];
    if Quit.shortcut().is_some() {
        file.push(menu(Quit));
    }
    let alt_redo = if cfg!(target_os = "windows") {
        Trigger::Keys(KeyboardShortcut::new(
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::Z,
        ))
    } else {
        keys(Modifiers::COMMAND, Key::Y)
    };
    vec![
        Section {
            title: "Canvas",
            rows: vec![
                menu(FitView),
                (Trigger::Text("Double-click canvas"), "Node search"),
                (Trigger::Text("Scroll / two-finger drag"), "Pan"),
                (Trigger::Text("Middle-drag · Alt-drag · Space-drag"), "Pan"),
                (
                    Trigger::Text("Pinch · Ctrl/⌘ + scroll"),
                    "Zoom at the pointer",
                ),
                (
                    Trigger::Text("Shift + click / drag"),
                    "Add to the selection",
                ),
                menu(SelectAll),
                menu(Undo),
                menu(Redo),
                (alt_redo, "Redo"),
                (
                    keys(none, Key::Escape),
                    "Clear selection / cancel the draw tool / close popups",
                ),
                (Trigger::Text("Right-click"), "Context menu"),
            ],
        },
        Section {
            title: "Nodes",
            rows: vec![
                (keys(none, Key::Tab), "Node search at the pointer"),
                (keys(none, Key::Q), "Favorites menu (1–9 places a favorite)"),
                menu(Cut),
                menu(Copy),
                menu(Paste),
                menu(Duplicate),
                (keys(none, Key::Delete), "Delete selection"),
                (keys(none, Key::Backspace), "Delete selection"),
                (Trigger::Text("Alt + drag a node"), "Drag out a duplicate"),
            ],
        },
        Section {
            title: "Wires",
            rows: vec![
                (
                    Trigger::Text("Drag from a pin"),
                    "New wire (drop on empty canvas to search)",
                ),
                (
                    Trigger::Text("Double-click wire"),
                    "Insert a node into the wire",
                ),
                (
                    keys(none, Key::OpenBracket),
                    "Move the selected wire earlier on its pin",
                ),
                (
                    keys(none, Key::CloseBracket),
                    "Move the selected wire later on its pin",
                ),
                (Trigger::Text("Alt + drop a wire"), "Probe the value"),
            ],
        },
        Section {
            title: "Regions",
            rows: vec![
                menu(Group),
                (keys(none, Key::I), "Enter the selected subpatch"),
                (keys(none, Key::O), "Exit the subpatch"),
                (
                    Trigger::Text("Double-click region title"),
                    "Rename the region",
                ),
                (
                    Trigger::Text("Hold Space while dropping"),
                    "Move into / out of a region",
                ),
            ],
        },
        Section {
            title: "Preview",
            rows: vec![
                menu(Restart),
                menu(NewVariation),
                (Trigger::Text("Space over the preview"), "Pause / play"),
                menu(ExportPng),
                (keys(none, Key::Escape), "Leave focus mode / close output"),
            ],
        },
        Section {
            title: "File",
            rows: file,
        },
        Section {
            title: "View",
            rows: vec![
                menu(ToggleNodes),
                menu(ZoomIn),
                menu(ZoomOut),
                menu(ZoomReset),
                (Trigger::Text("?"), "Shortcuts overlay"),
                menu(Shortcuts),
            ],
        },
    ]
}

/// The Keyboard Shortcuts window (Help menu).
pub fn shortcuts_window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Keyboard Shortcuts")
        .open(open)
        .collapsible(false)
        .default_size([560.0, 560.0])
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for g in groups() {
                    ui.add_space(6.0);
                    section(ui, &g, 150.0);
                }
            });
        });
}

/// One group: its title, then a grid of key chips and what they do.
fn section(ui: &mut egui::Ui, g: &Section, key_width: f32) {
    ui.label(RichText::new(g.title).color(GOLD_TEXT).strong());
    ui.add_space(2.0);
    egui::Grid::new(("shortcuts", g.title))
        .num_columns(2)
        .spacing([14.0, 5.0])
        .min_col_width(key_width)
        .show(ui, |ui| {
            for (t, what) in &g.rows {
                kbd(ui, &t.text(), false);
                ui.label(RichText::new(*what).color(FG_1));
                ui.end_row();
            }
        });
}

/// A group in a narrow column: keys in a fixed-width cell, descriptions
/// wrapping beside them.
fn compact_section(ui: &mut egui::Ui, g: &Section) {
    ui.label(RichText::new(g.title).color(GOLD_TEXT).strong());
    ui.add_space(2.0);
    let key_w = (ui.available_width() * 0.42).min(220.0);
    for (t, what) in &g.rows {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(key_w, 0.0),
                egui::Layout::left_to_right(egui::Align::Min),
                |ui| {
                    ui.set_width(key_w);
                    kbd(ui, &t.text(), true);
                },
            );
            ui.add(egui::Label::new(RichText::new(*what).color(FG_1)).wrap());
        });
        ui.add_space(1.0);
    }
}

/// The `?` overlay: every group in columns over `rect` (the canvas). A
/// click closes it; the app handles `?` and Esc ([`question_typed`]).
pub fn overlay(ctx: &egui::Context, rect: egui::Rect, open: &mut bool) {
    if !*open {
        return;
    }
    let area = egui::Area::new(egui::Id::new("floret-shortcuts-overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            ui.set_min_size(rect.size());
            ui.set_max_size(rect.size());
            let bg = ui.allocate_rect(rect, egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, BG_0.gamma_multiply(0.94));
            let inner = rect.shrink2(egui::vec2(28.0, 20.0));
            let mut content = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            content.horizontal(|ui| {
                ui.label(RichText::new("Keyboard shortcuts").size(16.0).color(FG_0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Esc or ? to close").small().color(FG_2));
                });
            });
            content.add_space(10.0);
            let all = groups();
            let cols = ((inner.width() / 330.0).floor() as usize).clamp(1, 4);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(&mut content, |ui| {
                    ui.columns(cols, |columns| {
                        for (col, ids) in columns.iter_mut().zip(balance(&all, cols)) {
                            for i in ids {
                                compact_section(col, &all[i]);
                                col.add_space(12.0);
                            }
                        }
                    });
                });
            bg.clicked()
        });
    if area.inner {
        *open = false;
    }
}

/// Split sections into `cols` columns of similar height, keeping order.
fn balance(all: &[Section], cols: usize) -> Vec<Vec<usize>> {
    let weight = |s: &Section| s.rows.len() + 2;
    let total: usize = all.iter().map(weight).sum();
    let per = total.div_ceil(cols);
    let mut out = vec![Vec::new(); cols];
    let (mut col, mut used) = (0, 0);
    for (i, s) in all.iter().enumerate() {
        if used > 0 && used + weight(s) / 2 > per && col + 1 < cols {
            col += 1;
            used = 0;
        }
        out[col].push(i);
        used += weight(s);
    }
    out
}

/// True when `?` was typed this frame (outside a text field).
pub fn question_typed(ctx: &egui::Context) -> bool {
    if ctx.egui_wants_keyboard_input() {
        return false;
    }
    ctx.input(|i| {
        i.events.iter().any(|e| match e {
            egui::Event::Text(t) => t == "?",
            _ => false,
        })
    })
}

/// A key cap. `wrap`: break long gestures onto more lines.
fn kbd(ui: &mut egui::Ui, text: &str, wrap: bool) {
    egui::Frame::new()
        .fill(*BG_2)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            let label = egui::Label::new(RichText::new(text).monospace().color(FG_0));
            ui.add(if wrap { label.wrap() } else { label.extend() });
        });
}

/// The About window. `gpu` describes the adapter in use.
pub fn about_window(ctx: &egui::Context, open: &mut bool, gpu: &str) {
    egui::Window::new("About Floret")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(340.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("✿").size(40.0).color(GOLD));
                ui.label(RichText::new("Floret").size(20.0).strong());
                ui.label(
                    RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(FG_1),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "A node-based creative tool: wire shapes, motion, sound and data together.",
                    )
                    .color(FG_1),
                );
                ui.add_space(6.0);
                ui.label(RichText::new(gpu).small().color(FG_2));
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_editing_and_app_command_is_listed() {
        let all = groups();
        let listed = |label: &str| all.iter().flat_map(|g| &g.rows).any(|(_, w)| *w == label);
        for c in [
            Command::New,
            Command::Open,
            Command::Save,
            Command::SaveAs,
            Command::ExportPng,
            Command::Undo,
            Command::Redo,
            Command::Cut,
            Command::Copy,
            Command::Paste,
            Command::Duplicate,
            Command::SelectAll,
            Command::Group,
            Command::FitView,
            Command::ToggleNodes,
            Command::Restart,
            Command::Preferences,
            Command::Shortcuts,
        ] {
            assert!(listed(c.label()), "{} missing", c.label());
        }
    }

    #[test]
    fn overlay_groups_and_columns() {
        let all = groups();
        let titles: Vec<_> = all.iter().map(|g| g.title).collect();
        for t in ["Canvas", "Nodes", "Wires", "Regions", "Preview", "File"] {
            assert!(titles.contains(&t), "{t} group");
        }
        for cols in 1..=4 {
            let split = balance(&all, cols);
            let flat: Vec<usize> = split.concat();
            assert_eq!(flat, (0..all.len()).collect::<Vec<_>>(), "{cols} columns");
            assert!(split.iter().all(|c| !c.is_empty()));
        }
    }

    #[test]
    fn question_mark_opens_unless_typing() {
        let ctx = egui::Context::default();
        let raw = || egui::RawInput {
            events: vec![egui::Event::Text("?".into())],
            ..Default::default()
        };
        let mut seen = false;
        ctx.run_ui(raw(), |ui| seen = question_typed(ui.ctx()))
            .textures_delta
            .clear();
        assert!(seen);
    }
}
