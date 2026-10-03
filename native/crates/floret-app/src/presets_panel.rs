//! The Presets panel (#26): the patch seed with "New variation", saved
//! presets to recall, and a morph between two of them. What a preset holds
//! is decided in `floret_engine::preset`; presets and the seed are saved in
//! the `.flora` file and every change here is an undo step.

use crate::canvas::View;
use crate::editor::values::LiveEdit;
use crate::theme;
use eframe::egui::{self, RichText};
use floret_engine::patch::PatchFile;
use floret_engine::preset::{self, Field, Key};
use floret_engine::spec::Catalogue;

#[derive(Default)]
pub struct PresetsPanel {
    /// Morph endpoints (indices into `patch.presets`).
    a: usize,
    b: usize,
    /// Morph position, 0 = A, 1 = B.
    t: f32,
    /// A preset being renamed, and the text so far.
    renaming: Option<(usize, String)>,
}

/// Re-roll the patch seed: an undo step, and the engine is rebuilt.
pub fn new_variation(patch: &mut PatchFile, view: &mut View) {
    let entropy = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    patch.seed = floret_engine::seed::reroll(patch.seed, entropy);
    view.commit(patch);
}

/// The live engine edits that put `fields` into effect without a rebuild.
fn live_edits(fields: &[Field]) -> Vec<LiveEdit> {
    fields
        .iter()
        .map(|f| match &f.key {
            Key::Param(k) => LiveEdit::Param {
                node: f.node.clone(),
                key: k.clone(),
                value: f.value.clone(),
            },
            Key::Port(p) => LiveEdit::Port {
                node: f.node.clone(),
                port: p.clone(),
                value: f.value.clone(),
            },
        })
        .collect()
}

/// A preset name not used yet: "Preset 1", "Preset 2", …
fn fresh_name(patch: &PatchFile) -> String {
    (1..)
        .map(|i| format!("Preset {i}"))
        .find(|n| !patch.presets.iter().any(|p| &p.name == n))
        .expect("unbounded")
}

impl PresetsPanel {
    pub fn ui(&mut self, ui: &mut egui::Ui, patch: &mut PatchFile, view: &mut View) {
        let cat = Catalogue::builtin();
        self.seed_row(ui, patch, view);
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(2.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Presets").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Save")
                    .on_hover_text(
                        "Save the current values (unwired pins, sliders, toggles, colours, \
                         modes) and the seed as a new preset",
                    )
                    .clicked()
                {
                    let name = fresh_name(patch);
                    patch.presets.push(preset::capture(cat, patch, name));
                    view.commit_meta(patch);
                }
            });
        });
        if patch.presets.is_empty() {
            ui.label(
                RichText::new(
                    "Save the values you like as a preset, then recall it or morph between two.",
                )
                .color(*theme::FG_2)
                .small(),
            );
            return;
        }
        self.list(ui, patch, view);
        if patch.presets.len() >= 2 {
            ui.add_space(6.0);
            ui.separator();
            self.morph(ui, patch, view);
        }
    }

    fn seed_row(&mut self, ui: &mut egui::Ui, patch: &mut PatchFile, view: &mut View) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Seed").strong());
            let mut seed = patch.seed;
            let r = ui
                .add(
                    egui::DragValue::new(&mut seed)
                        .range(0..=u32::MAX)
                        .speed(1.0),
                )
                .on_hover_text(
                    "Patch seed: mixed into every random and noise node's own seed. \
                     0 keeps each node's seed as it is.",
                );
            if r.changed() && seed != patch.seed {
                patch.seed = seed;
                view.commit_seed(patch);
            }
            if patch.seed == 0 {
                ui.label(RichText::new("original").color(*theme::FG_2).small());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tip = format!(
                    "New variation: re-roll the patch seed ({})",
                    crate::menu::shortcut_text(
                        &crate::menu::Command::NewVariation
                            .shortcut()
                            .expect("has a shortcut")
                    )
                );
                if ui.button("Vary").on_hover_text(tip).clicked() {
                    new_variation(patch, view);
                }
            });
        });
    }

    fn list(&mut self, ui: &mut egui::Ui, patch: &mut PatchFile, view: &mut View) {
        let cat = Catalogue::builtin();
        let mut recall = None;
        let mut update = None;
        let mut delete = None;
        let mut renamed = None;
        for (i, p) in patch.presets.iter().enumerate() {
            ui.horizontal(|ui| {
                match &mut self.renaming {
                    Some((j, text)) if *j == i => {
                        let r = ui.add(egui::TextEdit::singleline(text).desired_width(140.0));
                        if !r.has_focus() && !r.lost_focus() {
                            r.request_focus();
                        }
                        if r.lost_focus() {
                            let name = text.trim().to_owned();
                            if !ui.input(|inp| inp.key_pressed(egui::Key::Escape))
                                && !name.is_empty()
                            {
                                renamed = Some((i, name));
                            }
                            self.renaming = None;
                        }
                    }
                    _ => {
                        let seed = if p.seed == 0 {
                            String::new()
                        } else {
                            format!(" · seed {}", p.seed)
                        };
                        let r = ui
                            .add(egui::Label::new(&p.name).sense(egui::Sense::click()))
                            .on_hover_text(format!("Double-click to rename{seed}"));
                        if r.double_clicked() {
                            self.renaming = Some((i, p.name.clone()));
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("✕")
                        .on_hover_text("Delete this preset")
                        .clicked()
                    {
                        delete = Some(i);
                    }
                    if ui
                        .small_button("Update")
                        .on_hover_text("Replace this preset with the current values")
                        .clicked()
                    {
                        update = Some(i);
                    }
                    if ui
                        .small_button("Recall")
                        .on_hover_text("Set the patch to this preset's values")
                        .clicked()
                    {
                        recall = Some(i);
                    }
                });
            });
        }
        if let Some(i) = recall {
            let p = patch.presets[i].clone();
            self.go_to(patch, view, &p, &p, 0.0);
            view.commit(patch);
            // Morphing now starts from here.
            self.t = if i == self.b && i != self.a { 1.0 } else { 0.0 };
        }
        if let Some(i) = update {
            let name = patch.presets[i].name.clone();
            patch.presets[i] = preset::capture(cat, patch, name);
            view.commit_meta(patch);
        }
        if let Some((i, name)) = renamed {
            if patch.presets[i].name != name {
                patch.presets[i].name = name;
                view.commit_meta(patch);
            }
        }
        if let Some(i) = delete {
            patch.presets.remove(i);
            view.commit_meta(patch);
            self.renaming = None;
        }
    }

    fn morph(&mut self, ui: &mut egui::Ui, patch: &mut PatchFile, view: &mut View) {
        let n = patch.presets.len();
        if self.a >= n {
            self.a = 0;
        }
        if self.b >= n || (self.b == self.a && n > 1) {
            self.b = if self.a == 0 { 1 } else { 0 };
        }
        ui.label(RichText::new("Morph").strong());
        let pick = |ui: &mut egui::Ui, id: &str, at: &mut usize| {
            egui::ComboBox::from_id_salt(id)
                .width(110.0)
                .selected_text(patch.presets[*at].name.as_str())
                .show_ui(ui, |ui| {
                    for (i, p) in patch.presets.iter().enumerate() {
                        ui.selectable_value(at, i, p.name.as_str());
                    }
                });
        };
        ui.horizontal(|ui| {
            pick(ui, "floret-morph-a", &mut self.a);
            ui.label(RichText::new("→").color(*theme::FG_2));
            pick(ui, "floret-morph-b", &mut self.b);
        });
        ui.spacing_mut().slider_width = (ui.available_width() - 8.0).max(80.0);
        let r = ui
            .add(egui::Slider::new(&mut self.t, 0.0..=1.0).show_value(false))
            .on_hover_text(
                "Blend from A to B: numbers, points and colours glide; on/off values, \
                 text, modes and the seed switch halfway",
            );
        if r.changed() {
            let (a, b) = (patch.presets[self.a].clone(), patch.presets[self.b].clone());
            self.go_to(patch, view, &a, &b, f64::from(self.t));
        }
        // One undo step per drag (or click on the track).
        if r.drag_stopped() || (r.changed() && !r.dragged()) {
            view.commit(patch);
        }
    }

    /// Write the blend of `a` and `b` at `t` into the patch and the running
    /// engine (a changed seed rebuilds it).
    fn go_to(
        &self,
        patch: &mut PatchFile,
        view: &mut View,
        a: &floret_engine::patch::Preset,
        b: &floret_engine::patch::Preset,
        t: f64,
    ) {
        let (fields, seed) = preset::morph(Catalogue::builtin(), patch, a, b, t);
        if preset::apply(patch, &fields) {
            view.push_live_edits(live_edits(&fields));
        }
        if seed != patch.seed {
            patch.seed = seed;
            view.request_recompile();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{node, patch};

    #[test]
    fn names_count_up_past_taken_ones() {
        let mut p = patch(vec![node("a", "value/number")], vec![]);
        assert_eq!(fresh_name(&p), "Preset 1");
        p.presets.push(floret_engine::patch::Preset {
            name: "Preset 1".into(),
            ..Default::default()
        });
        assert_eq!(fresh_name(&p), "Preset 2");
    }

    #[test]
    fn new_variation_rerolls_the_seed() {
        let mut p = patch(vec![node("a", "value/number")], vec![]);
        let mut view = View::default();
        view.on_load(&mut p);
        new_variation(&mut p, &mut view);
        assert_ne!(p.seed, 0);
        assert!(view.take_recompile());
        let first = p.seed;
        new_variation(&mut p, &mut view);
        assert_ne!(p.seed, first);
    }

    #[test]
    fn live_edits_mirror_the_fields() {
        let fields = vec![
            Field {
                node: "n".into(),
                key: Key::Param("value".into()),
                value: 1.into(),
                integer: false,
            },
            Field {
                node: "n".into(),
                key: Key::Port("x".into()),
                value: 2.into(),
                integer: false,
            },
        ];
        let edits = live_edits(&fields);
        assert!(matches!(&edits[0], LiveEdit::Param { key, .. } if key == "value"));
        assert!(matches!(&edits[1], LiveEdit::Port { port, .. } if port == "x"));
    }
}
