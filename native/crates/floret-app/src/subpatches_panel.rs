//! The Subpatches panel: the subpatches at the level the canvas shows
//! (the root, or the entered subpatch), with their node counts. A click
//! frames one on the canvas, a double-click enters it, "Up" goes back a
//! level, and the right-click menu can save one to the Library, where it
//! inserts into any patch.

use crate::canvas::View;
use crate::editor::{clipboard, ops, ports, subpatch};
use crate::theme;
use eframe::egui::{self, vec2, RichText};
use floret_engine::patch::{PatchFile, PatchNode};
use floret_engine::spec::Catalogue;

/// One subpatch in the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: String,
    pub label: String,
    /// Everything inside it, nested levels included.
    pub nodes: usize,
}

/// A subpatch's name: its label, else "Subpatch".
fn label_of(n: &PatchNode) -> String {
    n.params
        .get("label")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Subpatch")
        .to_owned()
}

/// The subpatches at the level of `stack` (the entered subpatches,
/// outermost first): those whose nearest subpatch is the entered one, or
/// none at the root. These are the ones the canvas shows, so subpatches
/// inside a ForEach / If / Cache region at this level are listed too.
pub fn at_level(patch: &PatchFile, cat: &Catalogue, stack: &[String]) -> Vec<Row> {
    let scope = stack.last().map(String::as_str);
    patch
        .nodes
        .iter()
        .filter(|n| {
            subpatch::is_subpatch(cat, n)
                && Some(n.id.as_str()) != scope
                && subpatch::nearest_subpatch(patch, cat, &n.id).as_deref() == scope
        })
        .map(|n| Row {
            id: n.id.clone(),
            label: label_of(n),
            nodes: ops::descendants(patch, &n.id).len(),
        })
        .collect()
}

/// The name of the level `stack` shows: "Project" at the root, else the
/// entered subpatch's name.
pub fn level_name(patch: &PatchFile, stack: &[String]) -> String {
    match stack.last().and_then(|id| ports::find_node(patch, id)) {
        Some(n) => label_of(n),
        None => "Project".to_owned(),
    }
}

/// Subpatch `id` as a standalone patch for the Library: the frame, all
/// it holds and the wires inside, named after its label. Returns the
/// snippet and that name.
pub fn snippet(patch: &PatchFile, id: &str) -> Option<(PatchFile, String)> {
    let name = label_of(ports::find_node(patch, id)?);
    let mut clip = clipboard::copy(patch, &[id.to_owned()])?;
    let mut meta = clip.meta.take().unwrap_or_default();
    meta.name = Some(name.clone());
    clip.meta = Some(meta);
    Some((clip, name))
}

/// What a row asked for.
enum Ask {
    Focus(String),
    Enter(String),
    Save(String),
}

/// Show the panel. Returns a subpatch to save to the Library (its snippet
/// and name) when "Save to library…" was picked.
pub fn ui(ui: &mut egui::Ui, patch: &PatchFile, view: &mut View) -> Option<(PatchFile, String)> {
    let cat = Catalogue::builtin();
    let inside = !view.stack().is_empty();
    ui.horizontal(|ui| {
        ui.label(RichText::new(level_name(patch, view.stack())).strong());
        if inside {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Up")
                    .on_hover_text("Leave this subpatch, back up one level")
                    .clicked()
                {
                    view.request_exit();
                }
            });
        }
    });
    let rows = at_level(patch, cat, view.stack());
    if rows.is_empty() {
        // ⌘G on a Mac, Ctrl+G elsewhere.
        let group = crate::menu::Command::Group
            .shortcut()
            .map(|s| crate::menu::shortcut_text(&s))
            .unwrap_or_default();
        theme::empty_state(
            ui,
            "No subpatches here",
            &format!("Select nodes and press {group}"),
        );
        return None;
    }
    let selected = view.primary().cloned();
    let mut ask = None;
    for r in &rows {
        let resp = row(ui, r, selected.as_deref() == Some(r.id.as_str()))
            .on_hover_text("Click to show it on the canvas, double-click to enter");
        if resp.double_clicked() {
            ask = Some(Ask::Enter(r.id.clone()));
        } else if resp.clicked() {
            ask = Some(Ask::Focus(r.id.clone()));
        }
        resp.context_menu(|ui| {
            if ui.button("Enter").clicked() {
                ask = Some(Ask::Enter(r.id.clone()));
                ui.close();
            }
            if ui.button("Show on canvas").clicked() {
                ask = Some(Ask::Focus(r.id.clone()));
                ui.close();
            }
            if ui.button("Save to library…").clicked() {
                ask = Some(Ask::Save(r.id.clone()));
                ui.close();
            }
        });
    }
    match ask? {
        Ask::Focus(id) => view.request_focus(&id),
        Ask::Enter(id) => view.request_enter(&id),
        Ask::Save(id) => return snippet(patch, &id),
    }
    None
}

/// A full-width row: the name on the left, the node count on the right.
fn row(ui: &mut egui::Ui, r: &Row, on: bool) -> egui::Response {
    let h = 22.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), egui::Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 3.0, ui.visuals().selection.bg_fill);
    } else if resp.hovered() {
        p.rect_filled(rect, 3.0, *theme::BG_3);
    }
    let pad = 6.0;
    let count = if r.nodes == 1 {
        "1 node".to_owned()
    } else {
        format!("{} nodes", r.nodes)
    };
    let small = egui::TextStyle::Small.resolve(ui.style());
    let body = egui::TextStyle::Body.resolve(ui.style());
    let count = p.layout_no_wrap(count, small, *theme::FG_2);
    let name_w = (rect.width() - count.size().x - 3.0 * pad).max(0.0);
    let name = p.layout(r.label.clone(), body, *theme::FG_0, name_w);
    let y = |g: &egui::Galley| rect.center().y - g.size().y / 2.0;
    p.galley(
        egui::pos2(rect.right() - pad - count.size().x, y(&count)),
        count,
        *theme::FG_2,
    );
    p.with_clip_rect(rect.shrink2(vec2(pad, 0.0))).galley(
        egui::pos2(rect.left() + pad, y(&name)),
        name,
        *theme::FG_0,
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{edge, node, patch};
    use subpatch::SUBPATCH;

    fn inside(mut n: PatchNode, parent: &str) -> PatchNode {
        n.parent_id = Some(parent.into());
        n
    }

    fn labelled(mut n: PatchNode, label: &str) -> PatchNode {
        n.params.insert("label".into(), label.into());
        n
    }

    /// Root: `top` (holding `a`, `b`, nested `inner` with `deep`) and a
    /// ForEach `r` holding subpatch `in_r`.
    fn sample() -> PatchFile {
        patch(
            vec![
                node("x", "math/add"),
                labelled(node("top", SUBPATCH), "Wobble"),
                inside(node("a", "math/add"), "top"),
                inside(node("b", "math/add"), "top"),
                inside(labelled(node("inner", SUBPATCH), "  "), "top"),
                inside(node("deep", "math/add"), "inner"),
                node("r", "region/foreach"),
                inside(node("in_r", SUBPATCH), "r"),
                inside(node("in_r_child", "math/add"), "in_r"),
            ],
            vec![
                edge("a", "out-result", "b", "in-a"),
                edge("x", "out-result", "b", "in-b"),
            ],
        )
    }

    fn ids(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn lists_only_this_levels_subpatches() {
        let cat = Catalogue::builtin();
        let p = sample();
        // Root: the top-level one and the one inside the ForEach.
        let root = at_level(&p, cat, &[]);
        assert_eq!(ids(&root), ["top", "in_r"]);
        assert_eq!(root[0].label, "Wobble");
        assert_eq!(root[0].nodes, 4, "a, b, inner and deep");
        assert_eq!(root[1].label, "Subpatch");
        assert_eq!(level_name(&p, &[]), "Project");
        // Inside `top`: only `inner` (not `top` itself, not `deep`).
        let stack = ["top".to_owned()];
        let rows = at_level(&p, cat, &stack);
        assert_eq!(ids(&rows), ["inner"]);
        assert_eq!(rows[0].label, "Subpatch", "a blank label");
        assert_eq!(rows[0].nodes, 1);
        assert_eq!(level_name(&p, &stack), "Wobble");
        // Inside `inner`: nothing.
        let stack = ["top".to_owned(), "inner".to_owned()];
        assert!(at_level(&p, cat, &stack).is_empty());
        // Matches what the canvas shows at each level.
        for stack in [vec![], vec!["top".to_owned()]] {
            let shown = subpatch::visible(&p, cat, &stack);
            for r in at_level(&p, cat, &stack) {
                assert!(shown.contains(&r.id));
            }
        }
    }

    #[test]
    fn the_snippet_holds_exactly_the_subpatch() {
        let p = sample();
        let (clip, name) = snippet(&p, "top").unwrap();
        assert_eq!(name, "Wobble");
        assert_eq!(
            clip.meta.as_ref().and_then(|m| m.name.as_deref()),
            Some("Wobble")
        );
        let mut got: Vec<&str> = clip.nodes.iter().map(|n| n.id.as_str()).collect();
        got.sort_unstable();
        assert_eq!(got, ["a", "b", "deep", "inner", "top"]);
        // Only the wire inside; the one from `x` stays behind.
        assert_eq!(clip.edges.len(), 1);
        assert_eq!(
            (clip.edges[0].source.as_str(), clip.edges[0].target.as_str()),
            ("a", "b")
        );
        // The frame is top-level in the snippet; its contents keep their
        // parents.
        let parent = |id: &str| {
            clip.nodes
                .iter()
                .find(|n| n.id == id)
                .and_then(|n| n.parent_id.clone())
        };
        assert_eq!(parent("top"), None);
        assert_eq!(parent("deep").as_deref(), Some("inner"));
        // A subpatch inside a region lifts out of it.
        let (clip, _) = snippet(&p, "in_r").unwrap();
        assert_eq!(clip.nodes.len(), 2);
        assert!(clip
            .nodes
            .iter()
            .all(|n| n.parent_id.as_deref() != Some("r")));
        assert!(snippet(&p, "nope").is_none());
    }
}
