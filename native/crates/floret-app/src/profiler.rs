//! The Profiler panel: tick time, memo hits and the slowest nodes. The
//! engine times nodes only while this panel is showing.

use crate::canvas::View;
use crate::theme;
use eframe::egui::{self, RichText};
use floret_engine::patch::PatchFile;
use floret_engine::spec::Catalogue;
use floret_engine::Engine;

/// Nodes listed, slowest first.
const TOP: usize = 15;
/// One frame at 60 fps.
const BUDGET_MS: f64 = 1000.0 / 60.0;

pub fn ui(
    ui: &mut egui::Ui,
    engine: Option<&Engine>,
    patch: &PatchFile,
    view: &mut View,
    render_ms: f32,
) {
    let Some(engine) = engine else {
        theme::empty_state(
            ui,
            "No patch running",
            "Node timings show here while a patch runs",
        );
        return;
    };
    let p = engine.profile();
    let (evals, hits) = p.totals();
    let frame_ms = p.tick_avg_ms + f64::from(render_ms);
    let over = frame_ms > BUDGET_MS;
    ui.horizontal_wrapped(|ui| {
        let stat = |ui: &mut egui::Ui, label: &str, value: String| {
            ui.label(RichText::new(label).color(*theme::FG_2).small());
            ui.label(RichText::new(value).monospace());
            ui.add_space(8.0);
        };
        stat(ui, "Tick", format!("{:.2} ms", p.tick_avg_ms));
        stat(ui, "Render", format!("{render_ms:.2} ms"));
        let budget = RichText::new(format!("{:.0}%", frame_ms / BUDGET_MS * 100.0)).monospace();
        ui.label(RichText::new("of 60 fps").color(*theme::FG_2).small());
        ui.label(if over {
            budget.color(*theme::GOLD)
        } else {
            budget
        });
    });
    ui.label(
        RichText::new(format!(
            "{evals} computed · {hits} reused (unchanged inputs)"
        ))
        .color(*theme::FG_2)
        .small(),
    );
    ui.add_space(6.0);
    if !p.enabled || p.ticks == 0 {
        ui.label(RichText::new("Timing starts with the next frame.").color(*theme::FG_2));
        return;
    }

    let cat = Catalogue::builtin();
    let label = |ty: &str, id: &str| {
        let custom = patch
            .nodes
            .iter()
            .find(|n| n.id == id)
            .and_then(|n| n.params.get("label"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        custom.map(str::to_owned).unwrap_or_else(|| {
            cat.get(ty)
                .map_or_else(|| ty.to_owned(), |s| s.label.clone())
        })
    };
    let selected = view.primary().cloned();
    let mut pick = None;
    egui::Grid::new("profiler-top")
        .num_columns(4)
        .striped(true)
        .spacing([10.0, 3.0])
        .show(ui, |ui| {
            for h in ["Node", "ms", "runs", "reused"] {
                ui.label(RichText::new(h).color(*theme::FG_2).small());
            }
            ui.end_row();
            for (i, np) in p.top(TOP) {
                if np.avg_ms < 0.0005 && np.evals == 0 && np.hits == 0 {
                    continue;
                }
                let node = &engine.graph.nodes[i];
                let text = RichText::new(label(&node.ty, &node.id));
                let is_sel = selected.as_deref() == Some(node.id.as_str());
                if ui
                    .selectable_label(is_sel, text)
                    .on_hover_text(format!("{} ({})", node.id, node.ty))
                    .clicked()
                {
                    pick = Some(node.id.clone());
                }
                ui.label(RichText::new(format!("{:.3}", np.avg_ms)).monospace());
                ui.label(RichText::new(np.evals.to_string()).monospace());
                ui.label(RichText::new(np.hits.to_string()).monospace());
                ui.end_row();
            }
        });
    if let Some(id) = pick {
        // Nodes inside a region show up by their own id; synthetic splicers
        // aren't in the patch, so there's nothing to select for them.
        if patch.nodes.iter().any(|n| n.id == id) {
            view.select_only(&id);
        }
    }
}
