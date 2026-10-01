//! Help → Keyboard Shortcuts and About (#74).
//!
//! The reference lists every shortcut the app implements: the menu
//! commands (from [`crate::menu`]) plus the canvas's own keys and pointer
//! gestures (`canvas.rs`, `editor/popups.rs`). Keep it in step with them.

use crate::menu::Command;
use crate::theme::{BG_0, BORDER, FG_0, FG_1, FG_2, GOLD};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};

/// How a shortcut is triggered.
pub enum Trigger {
    Keys(KeyboardShortcut),
    /// Free text (pointer gestures, key combinations egui can't express).
    Text(&'static str),
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
        menu(ExportPng),
        menu(Preferences),
    ];
    if Quit.shortcut().is_some() {
        file.push(menu(Quit));
    }
    vec![
        Section {
            title: "File",
            rows: file,
        },
        Section {
            title: "Edit",
            rows: vec![
                menu(Undo),
                (
                    if cfg!(target_os = "windows") {
                        Trigger::Keys(KeyboardShortcut::new(
                            Modifiers::COMMAND | Modifiers::SHIFT,
                            Key::Z,
                        ))
                    } else {
                        keys(Modifiers::COMMAND, Key::Y)
                    },
                    "Redo",
                ),
                menu(Redo),
                menu(Cut),
                menu(Copy),
                menu(Paste),
                menu(Duplicate),
                (keys(none, Key::Delete), "Delete selection"),
                (keys(none, Key::Backspace), "Delete selection"),
                menu(SelectAll),
                menu(Group),
                (
                    keys(none, Key::Escape),
                    "Clear selection / cancel the draw tool / close popups",
                ),
            ],
        },
        Section {
            title: "Nodes",
            rows: vec![
                (keys(none, Key::Tab), "Node search at the pointer"),
                (Trigger::Text("Double-click canvas"), "Node search"),
                (
                    Trigger::Text("Double-click wire"),
                    "Insert a node into the wire",
                ),
                (keys(none, Key::Q), "Favorites menu (1–9 places a favorite)"),
                (keys(none, Key::I), "Enter the selected subpatch"),
                (keys(none, Key::O), "Exit the subpatch"),
                (
                    keys(none, Key::OpenBracket),
                    "Move the selected wire earlier on its pin",
                ),
                (
                    keys(none, Key::CloseBracket),
                    "Move the selected wire later on its pin",
                ),
                (
                    Trigger::Text("Double-click region title"),
                    "Rename the region",
                ),
                (
                    Trigger::Text("Drag from a pin"),
                    "New wire (drop on empty canvas to search)",
                ),
                (Trigger::Text("Alt + drop a wire"), "Probe the value"),
            ],
        },
        Section {
            title: "Canvas",
            rows: vec![
                menu(FitView),
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
                (Trigger::Text("Alt + drag a node"), "Drag out a duplicate"),
                (
                    Trigger::Text("Hold Space while dropping"),
                    "Move into / out of a region",
                ),
                (Trigger::Text("Right-click"), "Context menu"),
            ],
        },
        Section {
            title: "View",
            rows: vec![
                menu(ToggleNodes),
                menu(ZoomIn),
                menu(ZoomOut),
                menu(ZoomReset),
                menu(Restart),
                menu(Shortcuts),
            ],
        },
    ]
}

/// The Keyboard Shortcuts window.
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
                    ui.label(RichText::new(g.title).color(GOLD).strong());
                    ui.add_space(2.0);
                    egui::Grid::new(g.title)
                        .num_columns(2)
                        .spacing([18.0, 6.0])
                        .min_col_width(150.0)
                        .show(ui, |ui| {
                            for (t, what) in &g.rows {
                                let text = match t {
                                    Trigger::Keys(sc) => crate::menu::shortcut_text(sc),
                                    Trigger::Text(s) => (*s).to_owned(),
                                };
                                kbd(ui, &text);
                                ui.label(RichText::new(*what).color(FG_1));
                                ui.end_row();
                            }
                        });
                }
            });
        });
}

fn kbd(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(*BG_0)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).monospace().color(FG_0));
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
                    .color(FG_2),
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
            Command::Undo,
            Command::Redo,
            Command::Copy,
            Command::Paste,
            Command::Duplicate,
            Command::Group,
            Command::FitView,
            Command::Preferences,
            Command::Shortcuts,
        ] {
            assert!(listed(c.label()), "{} missing", c.label());
        }
    }
}
