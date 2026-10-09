//! Welcome screen and sample browser (#75; the web app's
//! `WelcomeModal.tsx` and Samples menu).
//!
//! Shown over an empty canvas on launch (unless turned off), from
//! Help → Welcome, and as the sample browser from File → Browse Samples.

use crate::files;
use crate::theme::{BG_0, BG_2, BG_3, BORDER, FG_0, FG_1, FG_2, FG_3, GOLD};
use crate::thumbs::Thumbnails;
use crate::{Sample, SAMPLES};
use eframe::egui::{self, vec2, Align, Layout, Rect, RichText, Sense, UiBuilder};
use eframe::egui_wgpu;
use std::path::PathBuf;

/// Tutorials featured on the home page (missing ids are skipped).
const FEATURED: [&str; 4] = [
    "tut-01-hello-rect",
    "tut-03-animate",
    "tut-07-grid",
    "lissajous",
];

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    New,
    Open,
    OpenRecent(PathBuf),
    /// Load `SAMPLES[i]`.
    Sample(usize),
    /// Show the Library panel (all samples and saved patches).
    Library,
}

#[derive(Default)]
pub struct Welcome {
    pub open: bool,
    pub thumbs: Thumbnails,
}

impl Welcome {
    pub fn show_home(&mut self) {
        self.open = true;
    }

    /// Draw (when open). `show_on_start` is the preference checkbox.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        rs: Option<&egui_wgpu::RenderState>,
        recent: &[PathBuf],
        show_on_start: &mut bool,
    ) -> Option<Action> {
        if !self.open {
            return None;
        }
        self.thumbs.update(ctx, rs);
        let screen = ctx.content_rect();
        let size = vec2(
            920.0_f32.min(screen.width() - 48.0),
            620.0_f32.min(screen.height() - 48.0),
        );
        let mut action = None;
        let modal = egui::Modal::new(egui::Id::new("floret-welcome"))
            .frame(
                egui::Frame::popup(&ctx.global_style())
                    .inner_margin(0)
                    .fill(BG_0.get()),
            )
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                action = self.home(ui, rect, recent, show_on_start);
            });
        if modal.should_close() || action.is_some() {
            self.open = false;
        }
        action
    }

    fn home(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        recent: &[PathBuf],
        show_on_start: &mut bool,
    ) -> Option<Action> {
        let mut action = None;
        let rail = Rect::from_min_size(rect.min, vec2(260.0, rect.height()));
        let main = Rect::from_min_max(egui::pos2(rail.max.x, rect.min.y), rect.max);
        ui.painter().rect_filled(
            rail,
            egui::CornerRadius {
                nw: 9,
                sw: 9,
                ne: 0,
                se: 0,
            },
            BG_2,
        );

        // ── Left rail: start actions and recent files.
        let mut ui_rail = ui.new_child(UiBuilder::new().max_rect(rail.shrink(20.0)));
        let ui = &mut ui_rail;
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        // The sunflower mark (as in the toolbar) beside the name.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (logo, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            crate::theme::paint_logo(ui.painter(), logo.center(), 24.0, *GOLD);
            ui.label(
                RichText::new("Floret")
                    .color(GOLD)
                    .font(crate::theme::medium(22.0)),
            );
        });
        ui.label(RichText::new("Wire shapes, motion, sound and data together.").color(FG_1));
        ui.add_space(14.0);
        let wide =
            |t: &str| egui::Button::new(RichText::new(t).size(14.0)).min_size(vec2(220.0, 30.0));
        if ui.add(wide("New patch")).clicked() {
            action = Some(Action::New);
        }
        if ui.add(wide("Open…")).clicked() {
            action = Some(Action::Open);
        }
        if ui.add(wide("Browse samples")).clicked() {
            action = Some(Action::Library);
        }
        ui.add_space(16.0);
        ui.label(RichText::new("RECENT").color(FG_2).small());
        if recent.is_empty() {
            ui.label(
                RichText::new("Files you open and save show up here.")
                    .color(FG_2)
                    .small(),
            );
        }
        for p in recent.iter().take(7) {
            let dir = p
                .parent()
                .map(|d| d.display().to_string())
                .unwrap_or_default();
            let r = ui
                .add(
                    egui::Button::new(RichText::new(files::file_stem(p)).color(FG_0))
                        .frame_when_inactive(false)
                        .min_size(vec2(220.0, 20.0)),
                )
                .on_hover_text(p.display().to_string());
            ui.label(RichText::new(short(&dir, 36)).color(FG_2).small());
            if r.clicked() {
                action = Some(Action::OpenRecent(p.clone()));
            }
        }

        // ── Main: tutorials and tips.
        let footer = Rect::from_min_max(egui::pos2(main.min.x, main.max.y - 52.0), main.max);
        let body = Rect::from_min_max(main.min, egui::pos2(main.max.x, footer.min.y));
        let mut ui_body = ui_rail.new_child(UiBuilder::new().max_rect(body.shrink(24.0)));
        let ui = &mut ui_body;
        ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
        let lessons = SAMPLES.iter().filter(|s| s.category == "Tutorial").count();
        ui.label(
            RichText::new("Start here")
                .strong()
                .font(crate::theme::medium(16.0)),
        );
        ui.label(
            RichText::new(format!(
                "{lessons} tutorial lessons build up from a single rectangle to \
                 audio-reactive compositions. Pick one to open it."
            ))
            .color(FG_1),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
            for id in FEATURED {
                if let Some(i) = SAMPLES.iter().position(|s| s.id == id) {
                    if card(ui, &self.thumbs, &SAMPLES[i], 136.0) {
                        action = Some(Action::Sample(i));
                    }
                }
            }
        });
        ui.add_space(14.0);
        tip(
            ui,
            "Editor basics",
            "Drag from an output pin (right) to an input pin (left). Double-click \
             empty canvas or press Tab to search for a node.",
        );
        tip(
            ui,
            "See what's happening",
            "Hover a pin for its live value; select a node to inspect it. \
             Help › Keyboard Shortcuts lists every key.",
        );

        let mut ui_foot = ui_body.new_child(
            UiBuilder::new()
                .max_rect(footer.shrink2(vec2(24.0, 12.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        let ui = &mut ui_foot;
        ui.checkbox(
            show_on_start,
            RichText::new("Show this on an empty canvas").color(FG_1),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Close").clicked() {
                self.open = false;
            }
            if ui.button("All samples").clicked() {
                action = Some(Action::Library);
            }
        });
        action
    }
}

/// A sample card: thumbnail, name and description. True when clicked.
fn card(ui: &mut egui::Ui, thumbs: &Thumbnails, s: &Sample, width: f32) -> bool {
    let thumb_h = width * 0.75;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, thumb_h + 58.0), Sense::click());
    let hovered = resp.hovered();
    let p = ui.painter();
    p.rect(
        rect,
        8,
        if hovered { BG_3.get() } else { BG_2.get() },
        egui::Stroke::new(1.0, if hovered { GOLD.get() } else { BORDER.get() }),
        egui::StrokeKind::Inside,
    );
    let img_area =
        egui::Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(width - 12.0, thumb_h - 6.0));
    p.rect_filled(img_area, 5, BG_0);
    match thumbs.get(s.id) {
        Some(tex) => {
            let [w, h] = tex.size();
            let scale = (img_area.width() / w as f32).min(img_area.height() / h as f32);
            let r =
                egui::Rect::from_center_size(img_area.center(), vec2(w as f32, h as f32) * scale);
            p.image(
                tex.id(),
                r,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        None => {
            let text = if thumbs.pending(s.id) { "…" } else { "✿" };
            p.text(
                img_area.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(18.0),
                *FG_3,
            );
        }
    }
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + 8.0, img_area.max.y + 6.0),
        rect.max - vec2(8.0, 4.0),
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::top_down(Align::LEFT)),
    );
    child.add(egui::Label::new(RichText::new(s.name).color(FG_0).strong()).truncate());
    child.add(
        egui::Label::new(RichText::new(s.description).color(FG_2).small())
            .wrap_mode(egui::TextWrapMode::Truncate),
    );
    let resp = resp.on_hover_text(s.description);
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn tip(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(title).color(FG_0).strong());
        ui.label(RichText::new(body).color(FG_1));
    });
    ui.add_space(4.0);
}

/// `…/tail` of a long path.
fn short(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_owned()
    } else {
        let tail: String = s.chars().skip(n - (max - 1)).collect();
        format!("…{tail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn featured_samples_exist() {
        for id in FEATURED {
            assert!(SAMPLES.iter().any(|s| s.id == id), "featured {id} exists");
        }
        assert_eq!(short("abcdef", 4), "…def");
    }
}
