//! The left node palette (`NodePalette.tsx`): search, starred favorites
//! pinned on top, the category tree with accents and description
//! tooltips. Drag an item onto the canvas to place it there; a click adds
//! it mid-canvas, and regions arm the draw tool either way.

use crate::canvas::{PaletteDrag, View};
use crate::editor::regions;
use crate::theme::{self, BG_3, FG_0, FG_1, FG_2, GOLD, GOLD_TEXT};
use eframe::egui::{self, vec2, Align2, CursorIcon, FontId, RichText, Sense};
use floret_engine::patch::PatchFile;
use floret_engine::spec::{Catalogue, NodeSpec};

const ROW_H: f32 = 22.0;

pub fn show(ui: &mut egui::Ui, query: &mut String, view: &mut View, patch: &mut PatchFile) {
    let cat = Catalogue::builtin();
    ui.add_space(6.0);
    ui.add(
        egui::TextEdit::singleline(query)
            .hint_text("Search nodes…")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(4.0);
    let q = query.trim().to_lowercase();
    let mut pick: Option<String> = None;
    let mut star: Option<String> = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let favs: Vec<&NodeSpec> = view
                .favorites
                .types()
                .iter()
                .filter_map(|t| cat.get(t))
                .filter(|s| !s.hidden)
                .collect();
            if q.is_empty() && !favs.is_empty() {
                ui.label(RichText::new("★ Favorites").color(GOLD_TEXT));
                for s in favs {
                    item(ui, view, s, &mut pick, &mut star);
                }
                ui.add_space(6.0);
            }
            let mut by_cat: Vec<(&str, Vec<&NodeSpec>)> = Vec::new();
            for s in cat.iter().filter(|s| !s.hidden) {
                let hit = q.is_empty()
                    || s.label.to_lowercase().contains(&q)
                    || s.ty.to_lowercase().contains(&q)
                    || s.category.to_lowercase().contains(&q)
                    || s.description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&q));
                if !hit {
                    continue;
                }
                match by_cat.iter_mut().find(|(c, _)| *c == s.category) {
                    Some((_, v)) => v.push(s),
                    None => by_cat.push((&s.category, vec![s])),
                }
            }
            by_cat.sort_by_key(|(c, _)| *c);
            if by_cat.is_empty() {
                ui.label(RichText::new("No nodes match").color(FG_2));
            }
            for (c, mut specs) in by_cat {
                specs.sort_by_key(|s| s.label.to_lowercase());
                let header = RichText::new(format!("{c}  ·  {}", specs.len()))
                    .color(theme::category_color(c));
                egui::CollapsingHeader::new(header)
                    .id_salt(c)
                    .open((!q.is_empty()).then_some(true))
                    .show(ui, |ui| {
                        for s in specs {
                            item(ui, view, s, &mut pick, &mut star);
                        }
                    });
            }
        });
    if let Some(ty) = star {
        view.favorites.toggle(&ty);
    }
    if let Some(ty) = pick {
        view.palette_pick(patch, cat, &ty);
    }
}

fn item(
    ui: &mut egui::Ui,
    view: &View,
    s: &NodeSpec,
    pick: &mut Option<String>,
    star: &mut Option<String>,
) {
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
    let star_rect =
        egui::Rect::from_center_size(rect.right_center() - vec2(11.0, 0.0), vec2(18.0, 18.0));
    let star_resp = ui.interact(star_rect, resp.id.with("star"), Sense::click());
    let fav = view.favorites.contains(&s.ty);
    let region = regions::is_drawable(s.region_kind);
    let armed = region && view.armed() == Some(s.ty.as_str());
    let p = ui.painter();
    if resp.hovered() || armed {
        p.rect_filled(rect, 4.0, BG_3);
    }
    p.circle_filled(
        rect.left_center() + vec2(6.0, 0.0),
        2.5,
        theme::category_color(&s.category),
    );
    p.text(
        rect.left_center() + vec2(14.0, 0.0),
        Align2::LEFT_CENTER,
        &s.label,
        FontId::proportional(13.0),
        if armed { *GOLD_TEXT } else { *FG_0 },
    );
    if resp.hovered() || armed {
        p.text(
            star_rect.left_center() - vec2(4.0, 0.0),
            Align2::RIGHT_CENTER,
            if armed { "draw on canvas…" } else { &s.ty },
            FontId::monospace(10.0),
            *FG_2,
        );
    }
    if fav || star_resp.hovered() || resp.hovered() {
        p.text(
            star_rect.center(),
            Align2::CENTER_CENTER,
            if fav { "★" } else { "☆" },
            FontId::proportional(13.0),
            if fav { *GOLD } else { *FG_1 },
        );
    }
    if star_resp.clicked() {
        *star = Some(s.ty.clone());
    }
    resp.dnd_set_drag_payload(PaletteDrag(s.ty.clone()));
    if resp.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    } else if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
    let tip = match (&s.description, region) {
        (Some(d), true) => {
            format!("{d}\n\nClick or drag out, then drag on the canvas to draw the region.")
        }
        (Some(d), false) => format!("{d}\n\nDrag onto the canvas to place."),
        (None, _) => s.ty.clone(),
    };
    let resp = resp.on_hover_text(tip);
    if resp.clicked() {
        *pick = Some(s.ty.clone());
    }
}
