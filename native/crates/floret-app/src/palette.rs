//! The left node palette (`NodePalette.tsx`): search, starred favorites
//! pinned on top, then the nodes in role-based groups (Source, Draw, Text
//! … in roughly the order a patch is built, see `spec::GROUPS`) with
//! accents and description tooltips. Drag an item onto the canvas to place
//! it there; a click adds it mid-canvas, and regions arm the draw tool
//! either way. A query also lists the op-family ops (typing "mul" offers
//! Multiply, a Math node set to multiply).

use crate::canvas::{PaletteDrag, View};
use crate::editor::regions;
use crate::editor::search::{self, Entry, Fit};
use crate::theme::{BG_3, FG_0, FG_1, FG_2, GOLD, GOLD_TEXT};
use eframe::egui::{self, vec2, Align2, CursorIcon, FontId, RichText, Sense};
use floret_engine::patch::PatchFile;
use floret_engine::spec::{Catalogue, GROUPS};
use std::collections::HashMap;

const ROW_H: f32 = 22.0;

/// Matching entries by group, in [`GROUPS`] order (a group the catalogue
/// names but the list doesn't, last), each sorted by label.
pub fn grouped<'a>(cat: &'a Catalogue, query: &str) -> Vec<(String, Vec<Entry<'a>>)> {
    let mut by_group: Vec<(String, Vec<Entry<'a>>)> = Vec::new();
    for e in search::matching(cat, query, &Fit::Any) {
        match by_group.iter_mut().find(|(g, _)| *g == e.spec.group) {
            Some((_, v)) => v.push(e),
            None => by_group.push((e.spec.group.clone(), vec![e])),
        }
    }
    let rank = |g: &str| {
        GROUPS
            .iter()
            .position(|x| x.name == g)
            .unwrap_or(GROUPS.len())
    };
    by_group.sort_by(|(a, _), (b, _)| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
    for (_, v) in &mut by_group {
        v.sort_by_cached_key(|e| (e.label().to_lowercase(), e.key()));
    }
    by_group
}

/// A muted qualifier for entries whose label repeats within one list
/// (the three AccReads, one per value type): the type id's last segment for a
/// three-part id (`region/accumulator-read/number` → number), else its
/// first (`collider/from-layer` → collider).
pub fn qualifiers(entries: &[Entry]) -> HashMap<String, String> {
    let mut count: HashMap<&str, usize> = HashMap::new();
    for e in entries {
        *count.entry(e.label()).or_default() += 1;
    }
    entries
        .iter()
        .filter(|e| count[e.label()] > 1)
        .map(|e| {
            let parts: Vec<&str> = e.spec.ty.split('/').collect();
            let q = if parts.len() >= 3 {
                parts[parts.len() - 1]
            } else {
                parts[0]
            };
            (e.key(), q.to_owned())
        })
        .collect()
}

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
            let favs: Vec<Entry> = view
                .favorites
                .types()
                .iter()
                .filter_map(|t| search::favorite(cat, t))
                .collect();
            if q.is_empty() && !favs.is_empty() {
                ui.label(RichText::new("★ Favorites").color(GOLD_TEXT));
                let quals = qualifiers(&favs);
                for e in &favs {
                    item(ui, view, cat, e, quals.get(&e.key()), &mut pick, &mut star);
                }
                ui.add_space(6.0);
            }
            let groups = grouped(cat, &q);
            if groups.is_empty() {
                ui.label(RichText::new("No nodes match").color(FG_2));
            }
            for (g, entries) in groups {
                let header = RichText::new(format!("{g}  ·  {}", entries.len())).color(*FG_1);
                let quals = qualifiers(&entries);
                let resp = egui::CollapsingHeader::new(header)
                    .id_salt(("palette-group", g.as_str()))
                    .open((!q.is_empty()).then_some(true))
                    .show(ui, |ui| {
                        for e in &entries {
                            item(ui, view, cat, e, quals.get(&e.key()), &mut pick, &mut star);
                        }
                    });
                if let Some(blurb) = GROUPS.iter().find(|x| x.name == g).map(|x| x.blurb) {
                    resp.header_response.on_hover_text(blurb);
                }
            }
        });
    if let Some(key) = star {
        view.favorites.toggle(&key);
    }
    if let Some(key) = pick {
        view.palette_pick(patch, cat, &key);
    }
}

fn item(
    ui: &mut egui::Ui,
    view: &View,
    cat: &Catalogue,
    e: &Entry,
    qualifier: Option<&String>,
    pick: &mut Option<String>,
    star: &mut Option<String>,
) {
    let key = e.key();
    let s = e.spec;
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
    let star_rect =
        egui::Rect::from_center_size(rect.right_center() - vec2(11.0, 0.0), vec2(18.0, 18.0));
    let star_resp = ui.interact(star_rect, resp.id.with("star"), Sense::click());
    let fav = view.favorites.contains(&key);
    let region = regions::is_drawable(s.region_kind);
    let armed = region && view.armed() == Some(s.ty.as_str());
    let p = ui.painter();
    if resp.hovered() || armed {
        p.rect_filled(rect, 4.0, BG_3);
    }
    let name = p.text(
        rect.left_center() + vec2(14.0, 0.0),
        Align2::LEFT_CENTER,
        e.label(),
        FontId::proportional(13.0),
        if armed { *GOLD_TEXT } else { *FG_0 },
    );
    // Which node an op places, or which of two same-named nodes this is.
    let aside = e.owner().map(str::to_owned).or_else(|| qualifier.cloned());
    if let (Some(aside), false) = (aside, resp.hovered() || armed) {
        p.text(
            name.right_center() + vec2(6.0, 0.5),
            Align2::LEFT_CENTER,
            aside,
            FontId::proportional(11.0),
            *FG_2,
        );
    }
    if resp.hovered() || armed {
        p.text(
            star_rect.left_center() - vec2(4.0, 0.0),
            Align2::RIGHT_CENTER,
            if armed { "draw on canvas…" } else { &key },
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
        *star = Some(key.clone());
    }
    resp.dnd_set_drag_payload(PaletteDrag(key.clone()));
    if resp.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    } else if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
    let tip = match (e.description(cat), region) {
        (Some(d), true) => {
            format!("{d}\n\nClick or drag out, then drag on the canvas to draw the region.")
        }
        (Some(d), false) => format!("{d}\n\nDrag onto the canvas to place."),
        (None, _) => key.clone(),
    };
    let resp = resp.on_hover_text(tip);
    if resp.clicked() {
        *pick = Some(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browsing_lists_every_group_in_order_and_every_node_once() {
        let cat = Catalogue::builtin();
        let groups = grouped(cat, "");
        let names: Vec<&str> = groups.iter().map(|(g, _)| g.as_str()).collect();
        let want: Vec<&str> = GROUPS.iter().map(|g| g.name).collect();
        assert_eq!(names, want);
        let listed: usize = groups.iter().map(|(_, v)| v.len()).sum();
        assert_eq!(listed, cat.iter().filter(|s| !s.hidden).count());
        // Browsing shows node types, not ops; each list is by label.
        for (_, v) in &groups {
            assert!(v.iter().all(|e| e.op.is_none()));
            let labels: Vec<String> = v.iter().map(|e| e.label().to_lowercase()).collect();
            let mut sorted = labels.clone();
            sorted.sort();
            assert_eq!(labels, sorted);
        }
        let math = &groups.iter().find(|(g, _)| g == "Math").unwrap().1;
        assert!(math.iter().any(|e| e.spec.ty == "math/math"));
        assert!(math.iter().all(|e| !e.spec.hidden));
    }

    #[test]
    fn a_query_lists_ops_under_their_node_s_group() {
        let cat = Catalogue::builtin();
        let groups = grouped(cat, "mul");
        let math = &groups.iter().find(|(g, _)| g == "Math").unwrap().1;
        assert!(math.iter().any(|e| e.key() == "math/math#multiply"));
        // Old category names still find their nodes.
        let spread = grouped(cat, "spread");
        assert!(spread
            .iter()
            .any(|(_, v)| v.iter().any(|e| e.spec.ty == "spread/ring")));
    }

    #[test]
    fn repeated_labels_get_a_qualifier() {
        let cat = Catalogue::builtin();
        let groups = grouped(cat, "");
        let structure = &groups.iter().find(|(g, _)| g == "Structure").unwrap().1;
        let q = qualifiers(structure);
        assert_eq!(q["region/accumulator-read/number"], "number");
        assert_eq!(q["region/accumulator-write/image"], "image");
        assert!(!q.contains_key("state/read"));
        assert!(!q.contains_key("region/foreach"));
        // No other group repeats a label.
        for (g, v) in &groups {
            if g != "Structure" {
                assert!(qualifiers(v).is_empty(), "{g}");
            }
        }
    }
}
