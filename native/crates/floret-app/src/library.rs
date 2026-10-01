//! Library panel (`LibraryPanel.tsx`): the built-in samples by category
//! and the user's own saved patches, as a collapsible list or a tile grid,
//! with one search across all of them. "Save to library" stores the
//! current patch (name + description) as a `.flora` file in the app data
//! dir, next to a small JSON index.

use crate::theme;
use eframe::egui::{self, vec2, Color32, RichText};
use floret_engine::patch::PatchFile;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A saved patch in the user library.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub file: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// What the user picked.
pub enum Pick {
    Sample(usize),
    Patch(PatchFile),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    List,
    Grid,
}

/// A file-name-safe slug (`my-patch`), never empty.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() {
        "patch".into()
    } else {
        out
    }
}

/// `<slug>.flora`, numbered if taken.
pub fn unique_file(entries: &[Entry], name: &str) -> String {
    let base = slug(name);
    let taken = |f: &str| entries.iter().any(|e| e.file == f);
    let mut file = format!("{base}.flora");
    let mut n = 2;
    while taken(&file) {
        file = format!("{base}-{n}.flora");
        n += 1;
    }
    file
}

/// Case-insensitive match on any of `fields`.
pub fn matches(query: &str, fields: &[&str]) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(&q))
}

/// A stable hue per name (`hueFor`), for tile accents.
pub fn hue_for(s: &str) -> f32 {
    let mut h: u32 = 0;
    for c in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(c));
    }
    (h % 360) as f32
}

fn index_path(dir: &Path) -> PathBuf {
    dir.join("library.json")
}

pub fn load_index(dir: &Path) -> Vec<Entry> {
    std::fs::read_to_string(index_path(dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_index(dir: &Path, entries: &[Entry]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(entries).map_err(std::io::Error::other)?;
    std::fs::write(index_path(dir), text)
}

/// Save `patch` into the library at `dir` and return its entry.
pub fn save_patch(
    dir: &Path,
    entries: &mut Vec<Entry>,
    patch: &PatchFile,
    name: &str,
    description: &str,
) -> std::io::Result<Entry> {
    std::fs::create_dir_all(dir)?;
    let mut patch = patch.clone();
    let mut meta = patch.meta.take().unwrap_or_default();
    meta.name = Some(name.trim().to_owned());
    patch.meta = Some(meta);
    let entry = Entry {
        file: unique_file(entries, name),
        name: name.trim().to_owned(),
        description: description.trim().to_owned(),
    };
    std::fs::write(dir.join(&entry.file), patch.to_json())?;
    entries.push(entry.clone());
    save_index(dir, entries)?;
    Ok(entry)
}

pub fn remove_entry(dir: &Path, entries: &mut Vec<Entry>, file: &str) -> std::io::Result<()> {
    entries.retain(|e| e.file != file);
    let _ = std::fs::remove_file(dir.join(file));
    save_index(dir, entries)
}

/// The library's own state.
pub struct Library {
    mode: Mode,
    query: String,
    /// The "Save to library" form: (name, description).
    save: Option<(String, String)>,
    dir: Option<PathBuf>,
    entries: Vec<Entry>,
    status: Option<String>,
}

impl Default for Library {
    fn default() -> Self {
        let dir = crate::editor::favorites::config_dir().map(|d| d.join("library"));
        let entries = dir.as_deref().map(load_index).unwrap_or_default();
        Library {
            mode: Mode::List,
            query: String::new(),
            save: None,
            dir,
            entries,
            status: None,
        }
    }
}

impl Library {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        samples: &[crate::Sample],
        current: Option<usize>,
        patch: &PatchFile,
    ) -> Option<Pick> {
        let mut pick = None;
        ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Search all libraries…")
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            for (m, label) in [(Mode::List, "List"), (Mode::Grid, "Tiles")] {
                if ui
                    .add(egui::Button::new(RichText::new(label).small()).selected(self.mode == m))
                    .clicked()
                {
                    self.mode = m;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("Save to library…").small())
                    .on_hover_text("Save the current patch to your library")
                    .clicked()
                {
                    let name = patch
                        .meta
                        .as_ref()
                        .and_then(|m| m.name.clone())
                        .unwrap_or_default();
                    self.save = Some((name, String::new()));
                }
            });
        });
        self.save_form(ui, patch);
        if let Some(s) = &self.status {
            ui.label(RichText::new(s).color(theme::FG_2).small());
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(p) = self.user_section(ui) {
                    pick = Some(p);
                }
                let mut cats: Vec<&str> = Vec::new();
                for s in samples {
                    if !cats.contains(&s.category) {
                        cats.push(s.category);
                    }
                }
                for cat in cats {
                    let items: Vec<(usize, &crate::Sample)> = samples
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| {
                            s.category == cat
                                && matches(&self.query, &[s.name, s.category, s.description, s.id])
                        })
                        .collect();
                    if items.is_empty() {
                        continue;
                    }
                    let chosen = self
                        .folder(ui, cat, items.len(), |ui| {
                            self.items(ui, cat, &items, current)
                        })
                        .flatten();
                    if let Some(i) = chosen {
                        pick = Some(Pick::Sample(i));
                    }
                }
            });
        pick
    }

    /// A collapsible folder (searching opens everything).
    fn folder<R>(
        &self,
        ui: &mut egui::Ui,
        name: &str,
        count: usize,
        body: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<R> {
        let searching = !self.query.trim().is_empty();
        egui::CollapsingHeader::new(
            RichText::new(format!("{name}  ·  {count}"))
                .color(theme::FG_1)
                .small(),
        )
        .id_salt(("floret-library", name))
        .default_open(true)
        .open(searching.then_some(true))
        .show(ui, body)
        .body_returned
    }

    fn items(
        &self,
        ui: &mut egui::Ui,
        cat: &str,
        items: &[(usize, &crate::Sample)],
        current: Option<usize>,
    ) -> Option<usize> {
        let mut picked = None;
        match self.mode {
            Mode::List => {
                for (i, s) in items {
                    let on = current == Some(*i);
                    if ui
                        .selectable_label(on, format!("   {}", s.name))
                        .on_hover_text(s.description)
                        .clicked()
                    {
                        picked = Some(*i);
                    }
                }
            }
            Mode::Grid => {
                ui.horizontal_wrapped(|ui| {
                    for (i, s) in items {
                        if tile(ui, s.name, cat, current == Some(*i))
                            .on_hover_text(s.description)
                            .clicked()
                        {
                            picked = Some(*i);
                        }
                    }
                });
            }
        }
        ui.add_space(4.0);
        picked
    }

    fn user_section(&mut self, ui: &mut egui::Ui) -> Option<Pick> {
        let shown: Vec<Entry> = self
            .entries
            .iter()
            .filter(|e| matches(&self.query, &[&e.name, &e.description]))
            .cloned()
            .collect();
        if shown.is_empty() {
            return None;
        }
        let mut pick = None;
        let mut remove = None;
        let mode = self.mode;
        self.folder(ui, "My patches", shown.len(), |ui| {
            match mode {
                Mode::List => {
                    for e in &shown {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(false, format!("   {}", e.name))
                                .on_hover_text(&e.description)
                                .clicked()
                            {
                                pick = Some(e.file.clone());
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button("×")
                                        .on_hover_text("Remove from library")
                                        .clicked()
                                    {
                                        remove = Some(e.file.clone());
                                    }
                                },
                            );
                        });
                    }
                }
                Mode::Grid => {
                    ui.horizontal_wrapped(|ui| {
                        for e in &shown {
                            if tile(ui, &e.name, &e.name, false)
                                .on_hover_text(&e.description)
                                .clicked()
                            {
                                pick = Some(e.file.clone());
                            }
                        }
                    });
                }
            }
            ui.add_space(4.0);
        });
        if let (Some(file), Some(dir)) = (remove, self.dir.clone()) {
            if let Err(e) = remove_entry(&dir, &mut self.entries, &file) {
                self.status = Some(format!("Couldn't update the library: {e}"));
            }
        }
        let (file, dir) = (pick?, self.dir.clone()?);
        match std::fs::read_to_string(dir.join(&file))
            .map_err(|e| e.to_string())
            .and_then(|t| PatchFile::load(&t).map_err(|e| e.to_string()))
        {
            Ok(p) => Some(Pick::Patch(p)),
            Err(e) => {
                self.status = Some(format!("Couldn't open {file}: {e}"));
                None
            }
        }
    }

    fn save_form(&mut self, ui: &mut egui::Ui, patch: &PatchFile) {
        let Some((name, desc)) = self.save.as_mut() else {
            return;
        };
        let mut done = None;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new("Save to library").color(theme::FG_1).small());
            ui.add(egui::TextEdit::singleline(name).hint_text("Name"));
            ui.add(
                egui::TextEdit::multiline(desc)
                    .hint_text("Description (optional)")
                    .desired_rows(2),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!name.trim().is_empty(), egui::Button::new("Save"))
                    .clicked()
                {
                    done = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    done = Some(false);
                }
            });
        });
        match done {
            Some(true) => {
                let (name, desc) = self.save.take().expect("open");
                self.status = Some(match &self.dir {
                    None => "No app data directory to save into.".into(),
                    Some(dir) => match save_patch(dir, &mut self.entries, patch, &name, &desc) {
                        Ok(e) => format!("Saved “{}”.", e.name),
                        Err(e) => format!("Couldn't save: {e}"),
                    },
                });
            }
            Some(false) => self.save = None,
            None => {}
        }
    }
}

/// An icon tile: a hue swatch from the group name, the title below.
fn tile(ui: &mut egui::Ui, title: &str, group: &str, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(96.0, 74.0), egui::Sense::click());
    let p = ui.painter();
    let swatch = egui::Rect::from_min_size(rect.min, vec2(rect.width(), 48.0));
    let accent = egui::ecolor::Hsva::new(hue_for(group) / 360.0, 0.45, 0.55, 1.0);
    p.rect_filled(swatch, 5.0, Color32::from(accent).gamma_multiply(0.55));
    let stroke = if on {
        egui::Stroke::new(1.5, theme::GOLD)
    } else if resp.hovered() {
        egui::Stroke::new(1.0, theme::FG_2)
    } else {
        egui::Stroke::new(1.0, theme::BORDER)
    };
    p.rect_stroke(swatch, 5.0, stroke, egui::StrokeKind::Inside);
    let g = p.layout(
        title.to_owned(),
        egui::FontId::proportional(10.5),
        *theme::FG_1,
        rect.width(),
    );
    p.with_clip_rect(rect).galley(
        egui::pos2(rect.left(), swatch.bottom() + 3.0),
        g,
        *theme::FG_1,
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_unique_names() {
        assert_eq!(slug("  My Patch!  v2 "), "my-patch-v2");
        assert_eq!(slug("???"), "patch");
        let e = |f: &str| Entry {
            file: f.into(),
            name: String::new(),
            description: String::new(),
        };
        let entries = vec![e("a.flora"), e("a-2.flora")];
        assert_eq!(unique_file(&entries, "A"), "a-3.flora");
        assert_eq!(unique_file(&entries, "b"), "b.flora");
    }

    #[test]
    fn search_and_hues() {
        assert!(matches("", &["x"]));
        assert!(matches("SLID", &["05 · Slider"]));
        assert!(!matches("zzz", &["a", "b"]));
        assert_eq!(hue_for("Tutorials"), hue_for("Tutorials"));
        assert!(hue_for("x") < 360.0);
    }

    #[test]
    fn saves_and_removes_patches() {
        let dir = std::env::temp_dir().join(format!("floret-lib-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut entries = Vec::new();
        let patch = crate::editor::test_util::patch(vec![], vec![]);
        let a = save_patch(&dir, &mut entries, &patch, "Night Sky", "stars").unwrap();
        let b = save_patch(&dir, &mut entries, &patch, "Night Sky", "").unwrap();
        assert_eq!(
            (a.file.as_str(), b.file.as_str()),
            ("night-sky.flora", "night-sky-2.flora")
        );
        assert_eq!(load_index(&dir), entries);
        let text = std::fs::read_to_string(dir.join(&a.file)).unwrap();
        let back = PatchFile::load(&text).unwrap();
        assert_eq!(back.meta.and_then(|m| m.name).as_deref(), Some("Night Sky"));
        remove_entry(&dir, &mut entries, &a.file).unwrap();
        assert_eq!(load_index(&dir).len(), 1);
        assert!(!dir.join(&a.file).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
