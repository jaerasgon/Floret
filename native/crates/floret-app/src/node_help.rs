//! Help panel (`HelpPanel.tsx`): live docs for the selected node — label,
//! type, category, description, inputs (type, kind, default) and outputs —
//! from the node catalogue, plus the hand-written notes in `NODES.md`.
//! Its own search pins another node's docs; the link toggle decides
//! whether a canvas selection replaces them (`autoJumpHelp`).

use crate::editor::ports::type_name;
use crate::theme;
use eframe::egui::{self, RichText};
use floret_engine::spec::{Catalogue, NodeSpec, PortKind, PortSpec, RegionKind};
use floret_engine::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

const NODES_MD: &str = include_str!("../../../../NODES.md");
const MAX_RESULTS: usize = 64;

/// `NODES.md` rows by node type: (ports summary, description).
pub fn parse_nodes_md(text: &str) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    for line in text.lines().filter(|l| l.starts_with("| **")) {
        // `\|` is a literal bar inside a cell.
        let cells: Vec<String> = line
            .replace("\\|", "\u{1}")
            .split('|')
            .map(|c| c.trim().replace('\u{1}', "|"))
            .collect();
        let [_, head, ports, desc, ..] = cells.as_slice() else {
            continue;
        };
        let Some(ty) = head.split('`').nth(1) else {
            continue;
        };
        // First row wins (a few types are listed twice).
        out.entry(ty.to_owned())
            .or_insert_with(|| (ports.clone(), desc.clone()));
    }
    out
}

fn nodes_md() -> &'static HashMap<String, (String, String)> {
    static MD: OnceLock<HashMap<String, (String, String)>> = OnceLock::new();
    MD.get_or_init(|| parse_nodes_md(NODES_MD))
}

/// One port line.
#[derive(Clone, Debug, PartialEq)]
pub struct PortDoc {
    pub name: String,
    pub ty: &'static str,
    pub kind: Option<&'static str>,
    pub default: Option<String>,
}

/// Everything the panel shows for a node type.
#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    pub label: String,
    /// For an op family node: the op it's set to ("Multiply").
    pub op: Option<String>,
    pub ty: String,
    /// The browser group (Source, Draw …).
    pub group: String,
    /// For a hidden node an op family replaced: what to use instead.
    pub legacy: Option<String>,
    pub description: Option<String>,
    /// The `NODES.md` description, when it says more than the catalogue.
    pub notes: Option<String>,
    pub inputs: Vec<PortDoc>,
    pub outputs: Vec<PortDoc>,
    pub region: Option<&'static str>,
}

/// A pin's line (`input`: an input, shown with its default). Layer pins
/// read "layer", as on the canvas.
fn port_doc(p: &PortSpec, input: bool) -> PortDoc {
    PortDoc {
        name: p.label.clone().unwrap_or_else(|| {
            crate::editor::values::layer_label(p, input)
                .map_or_else(|| p.name.clone(), str::to_owned)
        }),
        ty: type_name(p.ty),
        kind: match p.kind {
            PortKind::Single => None,
            PortKind::Spread => Some("spread"),
            PortKind::Multi => Some("multi"),
        },
        default: p.default.as_ref().filter(|_| input).map(format_default),
    }
}

/// The docs a node shows in the panel: its type's, or for an op family
/// node (Math, Vector Math, … Lookup, Sublist, Reorder, Combine) those of
/// the op it's set to — keyed like a search entry (`math/math#multiply`).
pub fn key_for(n: &floret_engine::patch::PatchNode) -> String {
    match floret_engine::nodes::family::family(&n.def_type) {
        Some(f) => format!("{}#{}", n.def_type, f.op(&n.params).id),
        None => n.def_type.clone(),
    }
}

/// The docs for `key`: a node type rendered against default params
/// (dynamic ports show their default arity, as in TS), or an op family
/// node set to an op (`math/math#multiply`): that op's own pins and
/// description.
pub fn doc(cat: &Catalogue, key: &str) -> Option<Doc> {
    let (ty, op) = crate::editor::search::split_key(key);
    if op.is_some() {
        return op_doc(cat, key);
    }
    let spec = cat.get(ty)?;
    let notes = nodes_md()
        .get(ty)
        .map(|(_, d)| d.clone())
        // Skip rows that only restate the catalogue line (plus TS-era
        // implementation remarks).
        .filter(|d| {
            !d.is_empty()
                && spec
                    .description
                    .as_ref()
                    .is_none_or(|c| !d.starts_with(c.trim_end_matches('.')))
        });
    Some(Doc {
        label: spec.label.clone(),
        op: None,
        ty: spec.ty.clone(),
        group: spec.group.clone(),
        legacy: floret_engine::nodes::family::superseded_by(ty).map(|(f, o)| {
            let node = cat.get(f.ty).map_or(f.ty, |s| s.label.as_str());
            format!(
                "Legacy node, hidden from the node browser: {node} set to {} does the same.",
                o.id
            )
        }),
        description: spec.description.clone(),
        notes,
        inputs: cat
            .resolved_inputs(spec, &spec.default_params)
            .iter()
            .map(|p| port_doc(p, true))
            .collect(),
        outputs: spec.outputs.iter().map(|p| port_doc(p, false)).collect(),
        region: spec.region_kind.map(|k| match k {
            RegionKind::Foreach => "foreach",
            RegionKind::If => "if",
            RegionKind::Cache => "cache",
            RegionKind::Subpatch => "subpatch",
            RegionKind::Section => "section",
        }),
    })
}

/// [`doc`] for a family node set to an op: the op's pins (its node's
/// inputs for these params, the family's `result`) and description.
fn op_doc(cat: &Catalogue, key: &str) -> Option<Doc> {
    let e = crate::editor::search::entry(cat, key)?;
    let (_, op) = e.op?;
    let spec = e.spec;
    let params = e.params();
    let notes = op
        .legacy()
        .and_then(|t| nodes_md().get(t))
        .map(|(_, d)| d.clone())
        .filter(|d| {
            !d.is_empty()
                && e.description(cat)
                    .is_none_or(|c| !d.starts_with(c.trim_end_matches('.')))
        });
    Some(Doc {
        label: spec.label.clone(),
        op: Some(op.label.to_owned()),
        ty: spec.ty.clone(),
        group: spec.group.clone(),
        legacy: None,
        description: e.description(cat).map(str::to_owned),
        notes,
        inputs: cat
            .resolved_inputs(spec, &params)
            .iter()
            .map(|p| port_doc(p, true))
            .collect(),
        outputs: cat
            .resolved_outputs(spec, &params)
            .iter()
            .map(|p| port_doc(p, false))
            .collect(),
        region: None,
    })
}

/// `formatDefault`.
pub fn format_default(v: &Value) -> String {
    match v {
        Value::Undefined | Value::Null => "null".into(),
        Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{n:.0}"),
        Value::Num(n) => {
            let s = format!("{n:.3}");
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        }
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => format!("\"{s}\""),
        other => {
            let s = crate::editor::values::value_to_json(other).to_string();
            if s.chars().count() > 24 {
                format!("{}…", s.chars().take(22).collect::<String>())
            } else {
                s
            }
        }
    }
}

/// The search box's matches: label / type / category with the query at a
/// word start (as the node search), label-prefix matches first, then
/// alphabetical.
pub fn search<'a>(cat: &'a Catalogue, query: &str) -> Vec<&'a NodeSpec> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<&NodeSpec> = cat
        .iter()
        .filter(|s| !s.hidden)
        .filter(|s| {
            let hay = format!(
                "{} {} {} {} {}",
                s.ty,
                s.label,
                s.label.replace(' ', ""),
                s.category,
                s.group
            );
            crate::editor::search::word_match(&hay, &q)
        })
        .collect();
    out.sort_by(|a, b| {
        let p = |s: &NodeSpec| !s.label.to_lowercase().starts_with(&q);
        p(a).cmp(&p(b)).then(a.label.cmp(&b.label))
    });
    out.truncate(MAX_RESULTS);
    out
}

/// Panel state.
pub struct HelpPanel {
    /// Follow the canvas selection.
    pub follow: bool,
    /// The type shown (from the selection or the search).
    shown: Option<String>,
    query: String,
}

impl Default for HelpPanel {
    fn default() -> Self {
        HelpPanel {
            follow: true,
            shown: None,
            query: String::new(),
        }
    }
}

impl HelpPanel {
    /// The panel body; `selected` is the selected node's type.
    pub fn ui(&mut self, ui: &mut egui::Ui, cat: &Catalogue, selected: Option<&str>) {
        if self.follow {
            if let Some(t) = selected {
                self.shown = Some(t.to_owned());
            }
        }
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("Search nodes…")
                    .desired_width(ui.available_width() - 60.0),
            );
            if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.query.clear();
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(first) = search(cat, &self.query).first() {
                    self.shown = Some(first.ty.clone());
                    self.follow = false;
                }
                self.query.clear();
            }
            let tip = if self.follow {
                "Following the selected node — click to pin these docs"
            } else {
                "Pinned — click to follow the selected node again"
            };
            if ui
                .add(egui::Button::new(RichText::new("Follow").small()).selected(self.follow))
                .on_hover_text(tip)
                .clicked()
            {
                self.follow = !self.follow;
            }
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if !self.query.trim().is_empty() {
                    self.results(ui, cat);
                } else {
                    match self.shown.as_deref().and_then(|t| doc(cat, t)) {
                        Some(d) => show_doc(ui, &d),
                        None => theme::empty_state(
                            ui,
                            "No node selected",
                            "Select a node, or search above, to read its docs",
                        ),
                    }
                }
            });
    }

    fn results(&mut self, ui: &mut egui::Ui, cat: &Catalogue) {
        let found = search(cat, &self.query);
        if found.is_empty() {
            ui.label(RichText::new("No matches.").color(theme::FG_2));
            return;
        }
        for s in found {
            let resp = ui
                .horizontal(|ui| {
                    let r = ui.selectable_label(false, RichText::new(&s.label).color(theme::FG_0));
                    ui.label(
                        RichText::new(&s.group)
                            .color(theme::category_color(&s.group))
                            .small(),
                    );
                    ui.label(RichText::new(&s.ty).color(theme::FG_2).small().monospace());
                    r
                })
                .inner;
            if resp.clicked() {
                self.shown = Some(s.ty.clone());
                self.follow = false;
                self.query.clear();
            }
        }
    }
}

fn show_doc(ui: &mut egui::Ui, d: &Doc) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(&d.label)
                .color(theme::FG_0)
                .strong()
                .size(15.0),
        );
        if let Some(op) = &d.op {
            ui.label(
                RichText::new(format!("· {op}"))
                    .color(theme::FG_1)
                    .size(15.0),
            );
        }
        ui.label(RichText::new(&d.ty).color(theme::FG_2).monospace().small());
    });
    ui.label(
        RichText::new(&d.group)
            .color(theme::category_color(&d.group))
            .small(),
    );
    if let Some(desc) = &d.description {
        ui.add_space(4.0);
        ui.label(RichText::new(desc).color(theme::FG_1));
    }
    if let Some(l) = &d.legacy {
        ui.add_space(2.0);
        ui.label(RichText::new(l).color(theme::FG_2).small().italics());
    }
    if let Some(n) = &d.notes {
        ui.add_space(2.0);
        ui.label(RichText::new(n).color(theme::FG_2).small());
    }
    section(
        ui,
        &format!("Inputs · {}", d.inputs.len()),
        &d.inputs,
        "No inputs.",
    );
    section(
        ui,
        &format!("Outputs · {}", d.outputs.len()),
        &d.outputs,
        "No outputs.",
    );
    if let Some(kind) = d.region {
        ui.add_space(8.0);
        let (title, text) = if kind == "section" {
            (
                "Section",
                "A backdrop for organising the patch: nodes inside move with it, \
                 but run exactly as if it weren't there, and wires pass straight \
                 through. Right-click it to change its colour or unwrap it."
                    .to_owned(),
            )
        } else {
            (
                "Region",
                format!(
                    "This is a {kind} region. Nodes placed inside its frame run in its scope. \
                     Drag wires onto the edge to add input / output ports."
                ),
            )
        };
        ui.label(RichText::new(title).color(theme::FG_2).small());
        ui.label(RichText::new(text).color(theme::FG_1).small());
    }
}

fn section(ui: &mut egui::Ui, title: &str, ports: &[PortDoc], empty: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).color(theme::FG_2).small());
    ui.separator();
    if ports.is_empty() {
        ui.label(RichText::new(empty).color(theme::FG_2).small());
    }
    for p in ports {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 14.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(r.center(), 4.0, theme::port_color(p.ty));
            ui.label(RichText::new(&p.name).color(theme::FG_0));
            let kind = p.kind.map_or(String::new(), |k| format!(" · {k}"));
            ui.label(
                RichText::new(format!("{}{kind}", p.ty))
                    .color(theme::FG_2)
                    .small()
                    .monospace(),
            );
            if let Some(d) = &p.default {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(d).color(theme::GOLD_TEXT).small().monospace())
                        .on_hover_text("default when unwired");
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_md_rows_parse() {
        let md = "| Label · type | Inputs → Outputs | Description |\n\
                  |---|---|---|\n\
                  | **Or** · `logic/or` | `a, b: boolean` → `out: boolean` | a \\|\\| b |\n\
                  | **Group** · `transform/group` | `layers` → `out` | first |\n\
                  | **Group** · `transform/group` | `layers*` → `out` | second |\n";
        let m = parse_nodes_md(md);
        assert_eq!(m.len(), 2);
        assert_eq!(m["logic/or"].1, "a || b");
        assert_eq!(m["transform/group"].1, "first");
        // The real file covers most of the catalogue.
        assert!(nodes_md().len() > 150);
    }

    /// Visible catalogue nodes with no `NODES.md` row, and rows whose type
    /// isn't in the catalogue at all (hidden legacy nodes may keep theirs).
    fn nodes_md_gaps(
        cat: &Catalogue,
        rows: &HashMap<String, (String, String)>,
    ) -> (Vec<String>, Vec<String>) {
        let mut missing: Vec<String> = cat
            .iter()
            .filter(|s| !s.hidden && !rows.contains_key(&s.ty))
            .map(|s| s.ty.clone())
            .collect();
        let mut unknown: Vec<String> = rows
            .keys()
            .filter(|ty| cat.get(ty).is_none())
            .cloned()
            .collect();
        missing.sort();
        unknown.sort();
        (missing, unknown)
    }

    #[test]
    fn nodes_md_gap_check_finds_both_kinds() {
        let cat = Catalogue::from_json(
            r#"{"nodes":[
                {"type":"a/shown","category":"A","label":"Shown","inputs":[],"outputs":[],"eval":"raw"},
                {"type":"a/old","category":"A","label":"Old","hidden":true,"inputs":[],"outputs":[],"eval":"raw"},
                {"type":"a/new","category":"A","label":"New","inputs":[],"outputs":[],"eval":"raw"}
            ]}"#,
        )
        .expect("catalogue");
        let rows = parse_nodes_md(
            "| **Shown** · `a/shown` | — → — | x |\n\
             | **Old** · `a/old` | — → — | x |\n\
             | **Gone** · `a/gone` | — → — | x |\n",
        );
        assert_eq!(
            nodes_md_gaps(&cat, &rows),
            (vec!["a/new".to_owned()], vec!["a/gone".to_owned()])
        );
    }

    /// One side of a row's pins cell as `NODES.md` writes it (and
    /// `tools/nodes-doc-check.py` generates it): the names a user sees on
    /// the node, the saved name in brackets where it differs, names of one
    /// type grouped (`w, h: number`), `*` after a spread type, `—` for none.
    fn pins_side(ports: &[PortSpec], input: bool) -> String {
        if ports.is_empty() {
            return "—".into();
        }
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for p in ports {
            let shown = crate::editor::values::display_label(p, input);
            let name = if shown == p.name {
                shown
            } else {
                format!("{shown} ({})", p.name)
            };
            let star = if p.kind == PortKind::Spread { "*" } else { "" };
            let ty = format!("{}{star}", type_name(p.ty));
            match groups.last_mut() {
                Some((t, names)) if *t == ty => names.push(name),
                _ => groups.push((ty, vec![name])),
            }
        }
        let list: Vec<String> = groups
            .iter()
            .map(|(t, names)| format!("{}: {t}", names.join(", ")))
            .collect();
        format!("`{}`", list.join(", "))
    }

    /// Splits `inputs → outputs` at the first arrow outside backticks.
    fn split_arrow(cell: &str) -> Option<(&str, &str)> {
        let mut ticks = false;
        for (i, ch) in cell.char_indices() {
            match ch {
                '`' => ticks = !ticks,
                '→' if !ticks => {
                    return Some((cell[..i].trim(), cell[i + '→'.len_utf8()..].trim()))
                }
                _ => {}
            }
        }
        None
    }

    /// Whether `name` appears in `text` as a whole word.
    fn mentions(text: &str, name: &str) -> bool {
        let word = |c: char| c.is_alphanumeric() || c == '_';
        text.match_indices(name).any(|(i, m)| {
            !text[..i].chars().next_back().is_some_and(word)
                && !text[i + m.len()..].chars().next().is_some_and(word)
        })
    }

    /// Rows of `md` whose title or pins differ from the catalogue: the
    /// title must be `**Label** · `type``; a side with fixed pins must be
    /// exactly what [`pins_side`] writes; a side whose pins change with a
    /// setting is hand-written and must still name each of its default
    /// pins. One message per offending row.
    fn nodes_md_mismatches(cat: &Catalogue, md: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in md.lines().filter(|l| l.starts_with("| **")) {
            let cells: Vec<String> = line
                .replace("\\|", "\u{1}")
                .split('|')
                .map(|c| c.trim().replace('\u{1}', "|"))
                .collect();
            let [_, head, ports, ..] = cells.as_slice() else {
                continue;
            };
            let Some(spec) = head.split('`').nth(1).and_then(|ty| cat.get(ty)) else {
                continue; // an unknown type is reported on its own
            };
            let title = format!("**{}** · `{}`", spec.label, spec.ty);
            if *head != title {
                out.push(format!("{}: title is {head}, should be {title}", spec.ty));
            }
            let Some((ins, outs)) = split_arrow(ports) else {
                out.push(format!("{}: pins cell has no `→`", spec.ty));
                continue;
            };
            for (word, input, pins, dynamic, got) in [
                ("inputs", true, &spec.inputs, spec.has_dynamic_inputs, ins),
                (
                    "outputs",
                    false,
                    &spec.outputs,
                    spec.has_dynamic_outputs,
                    outs,
                ),
            ] {
                if dynamic {
                    for p in pins {
                        let shown = crate::editor::values::display_label(p, input);
                        if !mentions(got, &p.name) && !mentions(got, &shown) {
                            out.push(format!("{}: {word} don't mention `{shown}`", spec.ty));
                        }
                    }
                } else if got != pins_side(pins, input) {
                    out.push(format!(
                        "{}: {word} are {got}, should be {}",
                        spec.ty,
                        pins_side(pins, input)
                    ));
                }
            }
        }
        out
    }

    #[test]
    fn nodes_md_checks_titles_and_pins() {
        let cat = Catalogue::from_json(
            r#"{"nodes":[
                {"type":"a/rect","category":"A","label":"Rect","inputs":[
                    {"name":"pos","type":"vec2","kind":"single"},
                    {"name":"w","type":"number","kind":"single"},
                    {"name":"h","type":"number","kind":"single"}],
                 "outputs":[{"name":"shape","type":"layer","kind":"single"}],"eval":"raw"},
                {"type":"a/dyn","category":"A","label":"Dyn","dynamicInputs":true,
                 "inputs":[{"name":"x","type":"number","kind":"single"}],
                 "outputs":[{"name":"result","type":"number","kind":"single"}],"eval":"raw"}
            ]}"#,
        )
        .expect("catalogue");
        let ok =
            "| **Rect** · `a/rect` | `pos: vec2, w, h: number` → `layer (shape): layer` | x |\n\
                  | **Dyn** · `a/dyn` | per setting: `x` → `value (result): number` | x |\n";
        assert_eq!(nodes_md_mismatches(&cat, ok), Vec::<String>::new());
        // The saved output name instead of the one on the node, a wrong
        // title, a missing pin, and a dynamic cell that forgets its pin.
        let bad = "| **Rectangle** · `a/rect` | `pos: vec2, w: number` → `shape: layer` | x |\n\
                   | **Dyn** · `a/dyn` | per setting → `value (result): number` | x |\n";
        let found = nodes_md_mismatches(&cat, bad);
        assert_eq!(found.len(), 4, "{found:?}");
        assert!(found[0].starts_with("a/rect: title is **Rectangle**"));
        assert!(found[1].contains("inputs are `pos: vec2, w: number`"));
        assert!(found[2].contains("outputs are `shape: layer`"));
        assert_eq!(found[3], "a/dyn: inputs don't mention `x`");
    }

    #[test]
    fn nodes_md_has_no_web_era_wording() {
        for word in ["Native-only", "TypeScript", "src/nodes"] {
            assert!(!NODES_MD.contains(word), "NODES.md still says {word:?}");
        }
    }

    #[test]
    fn nodes_md_documents_every_visible_node() {
        let (missing, unknown) = nodes_md_gaps(Catalogue::builtin(), nodes_md());
        assert!(
            missing.is_empty(),
            "NODES.md has no row for these {} visible nodes (add one per node, in the `| **Label** · `type` | ports | description |` format): {}",
            missing.len(),
            missing.join(", ")
        );
        assert!(
            unknown.is_empty(),
            "NODES.md has rows for node types that aren't in the catalogue (remove or fix them): {}",
            unknown.join(", ")
        );
        let differ = nodes_md_mismatches(Catalogue::builtin(), NODES_MD);
        assert!(
            differ.is_empty(),
            "{} NODES.md rows name a node or pin differently from the catalogue \
             (`python3 -I tools/nodes-doc-check.py --fix` rewrites titles and \
             fixed pins; hand-written cells need a look):\n{}",
            differ.len(),
            differ.join("\n")
        );
    }

    #[test]
    fn layer_pins_read_layer_as_on_the_canvas() {
        let cat = Catalogue::builtin();
        let r = doc(cat, "render/renderer").expect("renderer");
        assert_eq!(r.inputs[0].name, "layer");
        assert_eq!(r.inputs[0].kind, Some("spread"));
        assert_eq!(r.outputs[0].name, "layer");
        let g = doc(cat, "transform/group").expect("group");
        assert_eq!(g.outputs[0].name, "layer");
        assert_eq!(g.inputs[0].name, "1");
        let l = doc(cat, "input/landmarks").expect("landmarks");
        assert!(l.outputs.iter().any(|p| p.name == "guides"));
    }

    #[test]
    fn docs_come_from_the_catalogue() {
        let cat = Catalogue::builtin();
        let d = doc(cat, "io/slider").expect("slider");
        assert_eq!(d.label, "Slider");
        assert_eq!(d.group, "Source");
        assert_eq!(d.legacy, None);
        assert!(d.inputs.is_empty());
        assert_eq!(d.outputs.len(), 1);
        assert_eq!(d.outputs[0].ty, "number");
        // NODES.md only restates it (plus a TS remark): no notes.
        assert_eq!(d.notes, None);
        let add = doc(cat, "math/add").expect("add");
        assert_eq!(add.inputs.len(), 2);
        assert_eq!(
            add.legacy.as_deref(),
            Some("Legacy node, hidden from the node browser: Math set to add does the same.")
        );
        let math = doc(cat, "math/math").expect("math");
        assert_eq!((math.group.as_str(), math.legacy), ("Math", None));
        // A Math node set to an op shows that op's own pins and text.
        let mul = doc(cat, "math/math#multiply").expect("multiply");
        assert_eq!(mul.op.as_deref(), Some("Multiply"));
        assert_eq!(mul.label, "Math");
        assert_eq!(
            mul.description,
            cat.get("math/mul").unwrap().description.clone()
        );
        assert_eq!(mul.outputs.len(), 1);
        assert_eq!(mul.outputs[0].name, "result");
        let sin = doc(cat, "math/math#sin").expect("sin");
        assert_eq!(sin.inputs.len(), 1);
        assert_ne!(sin.description, mul.description);
        let cmp = doc(cat, "logic/compare#less").expect("less");
        assert_eq!(cmp.outputs[0].ty, "boolean");
        assert!(doc(cat, "math/math#nope").is_none());
        let mut n = crate::editor::test_util::node("m", "math/math");
        n.params.insert("op".into(), "sin".into());
        assert_eq!(key_for(&n), "math/math#sin");
        assert_eq!(
            key_for(&crate::editor::test_util::node("s", "io/slider")),
            "io/slider"
        );
        let foreach = doc(cat, "region/foreach").expect("foreach");
        assert_eq!(foreach.region, Some("foreach"));
        assert!(doc(cat, "nope/nope").is_none());
    }

    #[test]
    fn search_ranks_label_prefixes_first() {
        let cat = Catalogue::builtin();
        let r = search(cat, "slider");
        assert_eq!(r.first().map(|s| s.ty.as_str()), Some("io/slider"));
        assert!(search(cat, "  ").is_empty());
        assert!(search(cat, "a").len() <= MAX_RESULTS);
    }

    #[test]
    fn defaults_format_like_ts() {
        assert_eq!(format_default(&Value::Num(2.0)), "2");
        assert_eq!(format_default(&Value::Num(0.12345)), "0.123");
        assert_eq!(format_default(&Value::str("a")), "\"a\"");
        assert_eq!(format_default(&Value::Null), "null");
    }
}
