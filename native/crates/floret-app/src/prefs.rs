//! Preferences (#74): the web app's `prefs.ts` / `SettingsModal.tsx`,
//! persisted as `prefs.json` in the config dir (`FLORET_CONFIG_DIR`
//! overrides). Keys keep the web app's names. Every control applies and
//! saves immediately; there is no Apply / Cancel step.

use crate::theme::{BG_3, FG_0, FG_1, FG_2};
use eframe::egui::{self, RichText};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePref {
    Dark,
    Light,
    System,
}

impl ThemePref {
    pub fn egui(self) -> egui::ThemePreference {
        match self {
            ThemePref::Dark => egui::ThemePreference::Dark,
            ThemePref::Light => egui::ThemePreference::Light,
            ThemePref::System => egui::ThemePreference::System,
        }
    }
}

/// What opens on launch (after any crash recovery).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Startup {
    /// The most recent file.
    Last,
    /// The starter sample.
    Starter,
    /// An empty patch (and the welcome screen).
    Blank,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub theme: ThemePref,
    /// Interface zoom (egui's zoom factor).
    pub ui_scale: f32,
    /// Off by default. (A new key, so the old default-on setting doesn't
    /// carry over.)
    pub flow_animation: bool,
    pub snap_to_grid: bool,
    /// Snap step and dot spacing, in canvas units.
    pub grid_size: u32,
    /// The dot grid on the canvas.
    pub show_grid: bool,
    /// The overview map in the canvas corner.
    pub show_minimap: bool,
    /// The slow generative drift behind the graph.
    pub ambient_motion: bool,
    /// The web app follows the OS's reduced-motion setting, which winit
    /// doesn't expose; here it's a preference. Stops camera moves, node
    /// entrances, the selection breath and ambient drift.
    pub reduce_motion: bool,
    /// The Nodes browser. Hidden by default (a new key, so earlier
    /// default-on settings don't carry over); Tab / double-click search
    /// adds nodes.
    #[serde(rename = "nodesPanelVisible")]
    pub palette_visible: bool,
    pub default_render_width: u32,
    pub default_render_height: u32,
    pub startup_patch: Startup,
    pub autosave_enabled: bool,
    pub autosave_interval_secs: u32,
    /// Show the welcome screen on an empty canvas.
    pub show_welcome: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: ThemePref::Dark,
            ui_scale: 1.0,
            flow_animation: false,
            snap_to_grid: false,
            grid_size: crate::editor::ops::GRID as u32,
            show_grid: true,
            show_minimap: true,
            ambient_motion: true,
            reduce_motion: false,
            palette_visible: false,
            default_render_width: 1080,
            default_render_height: 1920,
            startup_patch: Startup::Last,
            autosave_enabled: true,
            autosave_interval_secs: 30,
            show_welcome: true,
        }
    }
}

pub const UI_SCALES: [f32; 7] = [0.75, 0.875, 1.0, 1.125, 1.25, 1.5, 2.0];
const RENDER_PRESETS: [(&str, u32, u32); 4] = [
    ("Portrait — 1080 × 1920", 1080, 1920),
    ("Square — 1080 × 1080", 1080, 1080),
    ("Landscape — 1920 × 1080", 1920, 1080),
    ("HD — 1280 × 720", 1280, 720),
];
const AUTOSAVE_INTERVALS: [u32; 5] = [10, 30, 60, 120, 300];

impl Prefs {
    pub fn load() -> Prefs {
        Self::load_from(path().as_deref())
    }

    pub fn load_from(path: Option<&std::path::Path>) -> Prefs {
        let mut p: Prefs = path
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        p.sanitize();
        p
    }

    /// Best effort: a read-only config dir just loses persistence.
    pub fn save(&self) {
        if let Some(p) = path() {
            self.save_to(&p);
        }
    }

    pub fn save_to(&self, path: &std::path::Path) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = crate::files::write_atomic(path, &text);
        }
    }

    fn sanitize(&mut self) {
        let clamp = |v: u32| v.clamp(floret_render::MIN_CANVAS, floret_render::MAX_CANVAS);
        self.default_render_width = clamp(self.default_render_width);
        self.default_render_height = clamp(self.default_render_height);
        self.ui_scale = if self.ui_scale.is_finite() {
            self.ui_scale
                .clamp(UI_SCALES[0], UI_SCALES[UI_SCALES.len() - 1])
        } else {
            1.0
        };
        self.autosave_interval_secs = self.autosave_interval_secs.clamp(5, 3600);
        self.grid_size = self.grid_size.clamp(4, 128);
    }

    /// The next / previous step of the UI scale ladder.
    pub fn step_scale(&mut self, dir: i32) {
        let i = UI_SCALES
            .iter()
            .position(|s| *s >= self.ui_scale - 1e-3)
            .unwrap_or(2) as i32;
        let j = (i + dir).clamp(0, UI_SCALES.len() as i32 - 1);
        self.ui_scale = UI_SCALES[j as usize];
    }

    pub fn autosave_interval(&self) -> Option<std::time::Duration> {
        self.autosave_enabled
            .then(|| std::time::Duration::from_secs(u64::from(self.autosave_interval_secs)))
    }
}

fn path() -> Option<PathBuf> {
    crate::editor::favorites::config_dir().map(|d| d.join("prefs.json"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    Appearance,
    Editor,
    Canvas,
    Files,
}

/// The Preferences window: a category rail and the selected page.
pub struct PrefsWindow {
    pub open: bool,
    cat: Category,
}

impl Default for PrefsWindow {
    fn default() -> Self {
        PrefsWindow {
            open: false,
            cat: Category::Appearance,
        }
    }
}

impl PrefsWindow {
    /// Draw (when open). Returns true when a pref changed (already saved).
    pub fn show(&mut self, ctx: &egui::Context, prefs: &mut Prefs) -> bool {
        if !self.open {
            return false;
        }
        let before = prefs.clone();
        let mut open = self.open;
        egui::Window::new("Preferences")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(620.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(150.0);
                        ui.set_min_height(300.0);
                        for (c, label) in [
                            (Category::Appearance, "Appearance"),
                            (Category::Editor, "Editor"),
                            (Category::Canvas, "Canvas"),
                            (Category::Files, "Files"),
                        ] {
                            let on = self.cat == c;
                            let text = RichText::new(label).color(if on { FG_0 } else { FG_1 });
                            let b = egui::Button::new(text)
                                .fill(if on {
                                    BG_3.get()
                                } else {
                                    egui::Color32::TRANSPARENT
                                })
                                .frame_when_inactive(on)
                                .min_size(egui::vec2(150.0, 26.0));
                            if ui.add(b).clicked() {
                                self.cat = c;
                            }
                        }
                        ui.add_space(ui.available_height() - 30.0);
                        if ui
                            .button(RichText::new("Reset to defaults").color(FG_2))
                            .on_hover_text("Restore all settings to their defaults")
                            .clicked()
                        {
                            *prefs = Prefs::default();
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.set_width(440.0);
                        self.page(ui, prefs);
                    });
                });
            });
        self.open = open;
        prefs.sanitize();
        let changed = *prefs != before;
        if changed {
            prefs.save();
        }
        changed
    }

    fn page(&mut self, ui: &mut egui::Ui, p: &mut Prefs) {
        match self.cat {
            Category::Appearance => {
                row(
                    ui,
                    "Theme",
                    "Choose a colour theme, or follow your system.",
                    |ui| {
                        egui::ComboBox::from_id_salt("pref-theme")
                            .selected_text(match p.theme {
                                ThemePref::Dark => "Dark",
                                ThemePref::Light => "Light",
                                ThemePref::System => "Use system setting",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut p.theme, ThemePref::Light, "Light");
                                ui.selectable_value(&mut p.theme, ThemePref::Dark, "Dark");
                                ui.selectable_value(
                                    &mut p.theme,
                                    ThemePref::System,
                                    "Use system setting",
                                );
                            });
                    },
                );
                row(ui, "Interface size", "Scale the whole interface.", |ui| {
                    egui::ComboBox::from_id_salt("pref-scale")
                        .selected_text(format!("{:.0}%", p.ui_scale * 100.0))
                        .show_ui(ui, |ui| {
                            for s in UI_SCALES {
                                ui.selectable_value(
                                    &mut p.ui_scale,
                                    s,
                                    format!("{:.0}%", s * 100.0),
                                );
                            }
                        });
                });
                row(
                    ui,
                    "Welcome screen",
                    "Show the welcome screen on an empty canvas.",
                    |ui| {
                        ui.checkbox(&mut p.show_welcome, "");
                    },
                );
                row(
                    ui,
                    "Ambient canvas motion",
                    "A slow generative drift behind the graph on a quiet patch.",
                    |ui| {
                        ui.checkbox(&mut p.ambient_motion, "");
                    },
                );
                row(
                    ui,
                    "Show minimap",
                    "The overview map in the canvas corner.",
                    |ui| {
                        ui.checkbox(&mut p.show_minimap, "");
                    },
                );
                row(
                    ui,
                    "Show dot grid",
                    "The dotted background grid on the canvas.",
                    |ui| {
                        ui.checkbox(&mut p.show_grid, "");
                    },
                );
                row(
                    ui,
                    "Reduce motion",
                    "No camera glides, node entrances or ambient drift.",
                    |ui| {
                        ui.checkbox(&mut p.reduce_motion, "");
                    },
                );
            }
            Category::Editor => {
                row(
                    ui,
                    "Wire flow animation",
                    "Marching dashes along wires.",
                    |ui| {
                        ui.checkbox(&mut p.flow_animation, "");
                    },
                );
                row(
                    ui,
                    "Snap to grid",
                    "Snap nodes to the grid while dragging.",
                    |ui| {
                        ui.checkbox(&mut p.snap_to_grid, "");
                    },
                );
                row(
                    ui,
                    "Nodes panel",
                    "Show the node palette on the left.",
                    |ui| {
                        ui.checkbox(&mut p.palette_visible, "");
                    },
                );
            }
            Category::Canvas => {
                row(
                    ui,
                    "Default render size",
                    "Resolution of a newly added Renderer node. Pick a preset or type a size (32–4096).",
                    |ui| {
                        ui.vertical(|ui| {
                            let current = RENDER_PRESETS
                                .iter()
                                .find(|(_, w, h)| {
                                    (*w, *h) == (p.default_render_width, p.default_render_height)
                                })
                                .map_or("Custom", |x| x.0);
                            egui::ComboBox::from_id_salt("pref-size")
                                .selected_text(current)
                                .width(200.0)
                                .show_ui(ui, |ui| {
                                    for (label, w, h) in RENDER_PRESETS {
                                        if ui.selectable_label(current == label, label).clicked() {
                                            p.default_render_width = w;
                                            p.default_render_height = h;
                                        }
                                    }
                                });
                            ui.horizontal(|ui| {
                                let range = floret_render::MIN_CANVAS..=floret_render::MAX_CANVAS;
                                ui.add(egui::DragValue::new(&mut p.default_render_width).range(range.clone()));
                                ui.label(RichText::new("×").color(FG_2));
                                ui.add(egui::DragValue::new(&mut p.default_render_height).range(range));
                            });
                        });
                    },
                );
                row(
                    ui,
                    "Grid step",
                    "Spacing of the dot grid, used when Snap to grid is on.",
                    |ui| {
                        egui::ComboBox::from_id_salt("pref-grid")
                            .selected_text(format!("{} px", p.grid_size))
                            .show_ui(ui, |ui| {
                                for n in crate::editor::ops::GRID_STEPS {
                                    ui.selectable_value(&mut p.grid_size, n, format!("{n} px"));
                                }
                            });
                    },
                );
            }
            Category::Files => {
                row(
                    ui,
                    "On startup, open",
                    "What loads when you launch Floret.",
                    |ui| {
                        egui::ComboBox::from_id_salt("pref-startup")
                            .selected_text(match p.startup_patch {
                                Startup::Last => "Last file",
                                Startup::Starter => "Starter sample",
                                Startup::Blank => "Blank canvas",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut p.startup_patch,
                                    Startup::Last,
                                    "Last file",
                                );
                                ui.selectable_value(
                                    &mut p.startup_patch,
                                    Startup::Starter,
                                    "Starter sample",
                                );
                                ui.selectable_value(
                                    &mut p.startup_patch,
                                    Startup::Blank,
                                    "Blank canvas",
                                );
                            });
                    },
                );
                row(
                    ui,
                    "Autosave",
                    "Save unsaved changes to a recovery file, offered again if Floret quits unexpectedly.",
                    |ui| {
                        ui.checkbox(&mut p.autosave_enabled, "");
                    },
                );
                ui.add_enabled_ui(p.autosave_enabled, |ui| {
                    row(
                        ui,
                        "Autosave every",
                        "How often, while there are unsaved changes.",
                        |ui| {
                            egui::ComboBox::from_id_salt("pref-autosave")
                                .selected_text(interval_label(p.autosave_interval_secs))
                                .show_ui(ui, |ui| {
                                    for s in AUTOSAVE_INTERVALS {
                                        ui.selectable_value(
                                            &mut p.autosave_interval_secs,
                                            s,
                                            interval_label(s),
                                        );
                                    }
                                });
                        },
                    );
                });
                ui.add_space(8.0);
                let dir = crate::recovery::dir();
                ui.label(
                    RichText::new(format!("Recovery files: {}", dir.display()))
                        .color(FG_2)
                        .small(),
                );
            }
        }
    }
}

fn interval_label(s: u32) -> String {
    if s < 60 {
        format!("{s} seconds")
    } else if s == 60 {
        "1 minute".into()
    } else {
        format!("{} minutes", s / 60)
    }
}

/// A settings row: label + description on the left, control on the right.
fn row(ui: &mut egui::Ui, label: &str, desc: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(230.0);
            ui.label(RichText::new(label).color(FG_0));
            ui.label(RichText::new(desc).color(FG_2).small());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), control);
    });
    ui.add_space(6.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_fill_missing_keys_and_clamp() {
        let dir = std::env::temp_dir().join(format!("floret-prefs-{}", std::process::id()));
        let path = dir.join("prefs.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &path,
            r#"{"theme":"light","defaultRenderWidth":99999,"uiScale":9}"#,
        )
        .unwrap();
        let p = Prefs::load_from(Some(&path));
        assert_eq!(p.theme, ThemePref::Light);
        assert_eq!(p.default_render_width, floret_render::MAX_CANVAS);
        assert_eq!(p.default_render_height, 1920);
        assert_eq!(p.ui_scale, 2.0);
        assert!(p.autosave_enabled);
        assert!(p.show_grid && p.show_minimap && p.ambient_motion && !p.reduce_motion);
        assert_eq!(p.grid_size, 24);
        let mut q = p.clone();
        q.startup_patch = Startup::Blank;
        q.save_to(&path);
        assert_eq!(Prefs::load_from(Some(&path)), q);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"startupPatch\": \"blank\""), "{text}");
        assert_eq!(Prefs::load_from(None), Prefs::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn scale_steps() {
        let mut p = Prefs::default();
        p.step_scale(1);
        assert_eq!(p.ui_scale, 1.125);
        p.step_scale(-2);
        assert_eq!(p.ui_scale, 0.875);
        for _ in 0..10 {
            p.step_scale(-1);
        }
        assert_eq!(p.ui_scale, 0.75);
    }
}
