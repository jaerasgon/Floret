//! The app shell's side dock (`src/editor/dock/*`): panels as tabs that
//! split, stack, float as windows and hide — one Preview per renderer,
//! Inspector, Help, Library and Shader. The node canvas stays pinned in
//! the centre. A closed panel comes back from the tab bar's `+` menu; the
//! layout is saved to the app data dir.

use crate::canvas::View;
use crate::library::{self, Library};
use crate::node_help::HelpPanel;
use crate::preview::PreviewPanel;
use crate::shader_panel::ShaderPanel;
use crate::{crash, theme, Sample};
use eframe::egui::{self, RichText};
use eframe::egui_wgpu;
use egui_dock::{DockArea, DockState, NodeIndex, NodePath, TabViewer};
use floret_engine::patch::PatchFile;
use floret_engine::spec::Catalogue;
use floret_engine::tick::Outputs;
use floret_engine::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    /// The n-th renderer's preview (0 is always present).
    Preview(usize),
    Inspector,
    Help,
    Library,
    Shader,
}

/// Panels that can be closed and reopened.
const PANELS: [Tab; 4] = [Tab::Inspector, Tab::Help, Tab::Library, Tab::Shader];

fn title(tab: &Tab) -> &'static str {
    match tab {
        Tab::Preview(_) => "Preview",
        Tab::Inspector => "Inspector",
        Tab::Help => "Help",
        Tab::Library => "Library",
        Tab::Shader => "Shader",
    }
}

/// The default layout: previews on top, the other panels tabbed below.
pub fn default_dock() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Preview(0)]);
    dock.main_surface_mut().split_below(
        NodeIndex::root(),
        0.56,
        vec![Tab::Inspector, Tab::Help, Tab::Library, Tab::Shader],
    );
    dock
}

/// Make the dock hold exactly `count` preview tabs (at least one); new
/// ones join the first preview's leaf.
pub fn sync_previews(dock: &mut DockState<Tab>, count: usize) {
    let count = count.max(1);
    dock.retain_tabs(|t| !matches!(t, Tab::Preview(i) if *i >= count));
    if dock.find_tab(&Tab::Preview(0)).is_none() {
        dock.push_to_first_leaf(Tab::Preview(0));
    }
    let home = dock
        .find_tab(&Tab::Preview(0))
        .map(|p| p.node_path())
        .expect("just ensured");
    for i in 1..count {
        if dock.find_tab(&Tab::Preview(i)).is_none() {
            if let Ok(leaf) = dock.leaf_mut(home) {
                // Keep whichever preview was showing.
                let active = leaf.active;
                leaf.append_tab(Tab::Preview(i));
                leaf.active = active;
            }
        }
    }
}

/// Bring `tab` forward, opening it (next to the Inspector, else in the
/// first leaf) if it was closed.
pub fn reveal(dock: &mut DockState<Tab>, tab: Tab) {
    if dock.find_tab(&tab).is_none() {
        let near = PANELS
            .iter()
            .find_map(|t| dock.find_tab(t))
            .map(|p| p.node_path());
        match near.and_then(|n| dock.leaf_mut(n).ok()) {
            Some(leaf) => leaf.append_tab(tab.clone()),
            None => dock.push_to_first_leaf(tab.clone()),
        }
    }
    if let Some(p) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(p);
    }
}

fn layout_path() -> Option<PathBuf> {
    crate::editor::favorites::config_dir().map(|d| d.join("dock.json"))
}

/// What the app should do after the dock ran.
pub enum ShellAction {
    LoadSample(usize),
    LoadPatch(PatchFile),
}

/// Everything the panels read and edit this frame.
pub struct Shell<'a> {
    pub patch: &'a mut PatchFile,
    pub view: &'a mut View,
    pub preview: &'a mut PreviewPanel,
    pub shader: &'a mut ShaderPanel,
    pub crash: &'a mut crash::Guard,
    pub engine: Option<&'a Engine>,
    pub outputs: &'a Outputs,
    pub rs: Option<&'a egui_wgpu::RenderState>,
    pub samples: &'a [Sample],
    pub sample: Option<usize>,
}

pub struct Workspace {
    dock: DockState<Tab>,
    help: HelpPanel,
    library: Library,
    /// The layout as last saved.
    saved: String,
    /// Selected node last frame (a newly selected shader node brings the
    /// Shader tab forward).
    last_selected: Option<String>,
}

impl Workspace {
    /// The saved layout, else the default.
    pub fn load() -> Workspace {
        let saved = layout_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let dock =
            serde_json::from_str::<DockState<Tab>>(&saved).unwrap_or_else(|_| default_dock());
        Workspace {
            dock,
            help: HelpPanel::default(),
            library: Library::default(),
            saved,
            last_selected: None,
        }
    }

    /// Bring the Library (samples and saved patches) forward.
    pub fn show_library(&mut self) {
        reveal(&mut self.dock, Tab::Library);
    }

    /// Put every panel back where it started.
    pub fn reset(&mut self) {
        self.dock = default_dock();
    }

    pub fn show(&mut self, ui: &mut egui::Ui, shell: &mut Shell) -> Option<ShellAction> {
        let renderers = shell.preview.renderers();
        sync_previews(&mut self.dock, renderers.len());
        let selected = shell.view.primary().cloned();
        if selected != self.last_selected {
            if ShaderPanel::is_shader(shell.patch, selected.as_deref()) {
                reveal(&mut self.dock, Tab::Shader);
            }
            self.last_selected = selected.clone();
        }
        if shell
            .shader
            .tick(ui.ctx(), shell.patch, selected.as_deref())
        {
            shell.view.commit(shell.patch);
        }
        let mut viewer = Viewer {
            shell,
            help: &mut self.help,
            library: &mut self.library,
            renderers,
            selected,
            action: None,
            add: None,
            open: PANELS
                .iter()
                .filter(|t| self.dock.find_tab(t).is_some())
                .cloned()
                .collect(),
            reset: false,
        };
        DockArea::new(&mut self.dock)
            .id(egui::Id::new("floret-dock"))
            .style(style(ui.style()))
            .show_add_buttons(true)
            .show_add_popup(true)
            .show_leaf_collapse_buttons(false)
            .show_leaf_close_all_buttons(false)
            // Tabs dragged out float anywhere over the window, as cards.
            .window_bounds(ui.ctx().content_rect())
            .show_inside(ui, &mut viewer);
        let (action, add, reset) = (viewer.action.take(), viewer.add.take(), viewer.reset);
        shell
            .preview
            .output_window(ui.ctx(), shell.rs, shell.engine, shell.outputs);
        if let Some((path, tab)) = add {
            match self.dock.leaf_mut(path) {
                Ok(leaf) => leaf.append_tab(tab.clone()),
                Err(_) => self.dock.push_to_first_leaf(tab.clone()),
            }
            if let Some(p) = self.dock.find_tab(&tab) {
                let _ = self.dock.set_active_tab(p);
            }
        }
        if reset {
            self.reset();
        }
        self.save_layout(ui.ctx());
        action
    }

    /// Write the layout when it changed and the pointer is up.
    fn save_layout(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let Some(json) = layout_json(&self.dock) else {
            return;
        };
        if json == self.saved {
            return;
        }
        if let Some(path) = layout_path() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, &json);
        }
        self.saved = json;
    }
}

/// The layout as JSON. Rects egui_dock hasn't laid out yet are infinite
/// (written as `null`, which won't load back); they're zeroed, since the
/// dock recomputes every rect when it's next shown.
pub fn layout_json(dock: &DockState<Tab>) -> Option<String> {
    fn fix(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(m) if m.contains_key("x") && m.contains_key("y") => {
                for c in m.values_mut().filter(|c| c.is_null()) {
                    *c = 0.0.into();
                }
            }
            serde_json::Value::Object(m) => m.values_mut().for_each(fix),
            serde_json::Value::Array(a) => a.iter_mut().for_each(fix),
            _ => {}
        }
    }
    let mut v = serde_json::to_value(dock).ok()?;
    fix(&mut v);
    serde_json::to_string(&v).ok()
}

/// Floret's theme on the dock.
fn style(base: &egui::Style) -> egui_dock::Style {
    let mut s = egui_dock::Style::from_egui(base);
    s.tab_bar.bg_fill = *theme::BG_1;
    s.tab_bar.hline_color = *theme::BORDER;
    s.tab_bar.height = 26.0;
    s.separator.color_idle = *theme::BORDER;
    s.separator.color_hovered = *theme::FG_3;
    s.separator.color_dragged = *theme::GOLD;
    s.tab.tab_body.bg_fill = *theme::BG_1;
    s.tab.tab_body.stroke = egui::Stroke::NONE;
    s.tab.tab_body.inner_margin = egui::Margin::same(8);
    s.tab.active.bg_fill = *theme::BG_2;
    s.tab.active.text_color = *theme::FG_0;
    s.tab.focused.bg_fill = *theme::BG_2;
    s.tab.focused.text_color = *theme::FG_0;
    s.tab.inactive.bg_fill = *theme::BG_1;
    s.tab.inactive.text_color = *theme::FG_2;
    s.tab.hovered.text_color = *theme::FG_1;
    s.tab.hline_below_active_tab_name = false;
    s.overlay.selection_color = theme::GOLD.gamma_multiply(0.35);
    s.overlay.button_color = *theme::FG_1;
    s.main_surface_border_stroke = egui::Stroke::NONE;
    s
}

struct Viewer<'s, 'a> {
    shell: &'s mut Shell<'a>,
    help: &'s mut HelpPanel,
    library: &'s mut Library,
    /// (renderer id, title), in patch order.
    renderers: Vec<(String, String)>,
    selected: Option<String>,
    action: Option<ShellAction>,
    /// A closed panel reopened from a `+` menu.
    add: Option<(NodePath, Tab)>,
    /// Panels currently in the dock.
    open: Vec<Tab>,
    reset: bool,
}

impl TabViewer for Viewer<'_, '_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(("floret-tab", tab.clone()))
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Preview(i) => self
                .renderers
                .get(*i)
                .map_or_else(|| "Preview".to_owned(), |(_, t)| t.clone())
                .into(),
            other => title(other).into(),
        }
    }

    fn is_closeable(&self, tab: &Tab) -> bool {
        // A preview goes away with its renderer node, never by hand.
        !matches!(tab, Tab::Preview(_))
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        match tab {
            Tab::Inspector | Tab::Shader => [false, true],
            _ => [false, false],
        }
    }

    fn add_popup(&mut self, ui: &mut egui::Ui, path: NodePath) {
        ui.set_min_width(140.0);
        let closed: Vec<&Tab> = PANELS.iter().filter(|t| !self.open.contains(t)).collect();
        if closed.is_empty() {
            ui.label(
                RichText::new("All panels are open")
                    .color(theme::FG_2)
                    .small(),
            );
        }
        for t in closed {
            if ui.button(title(t)).clicked() {
                self.add = Some((path, t.clone()));
            }
        }
        ui.separator();
        if ui.button("Reset layout").clicked() {
            self.reset = true;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        let cat = Catalogue::builtin();
        let sh = &mut *self.shell;
        match tab {
            Tab::Preview(i) => {
                if !sh.crash.render_ok() {
                    return;
                }
                let id = self.renderers.get(*i).map_or("", |(id, _)| id.as_str());
                let (preview, rs, engine, outputs) =
                    (&mut *sh.preview, sh.rs, sh.engine, sh.outputs);
                sh.crash.run(crash::Stage::Render, sh.patch, || {
                    preview.show_renderer(ui, rs, engine, outputs, id);
                });
            }
            Tab::Inspector => inspector(ui, sh),
            Tab::Help => {
                let ty = self
                    .selected
                    .as_deref()
                    .and_then(|id| sh.patch.nodes.iter().find(|n| n.id == id))
                    .map(|n| n.def_type.clone());
                self.help.ui(ui, cat, ty.as_deref());
            }
            Tab::Library => match self.library.ui(ui, sh.samples, sh.sample, sh.patch, sh.rs) {
                Some(library::Pick::Sample(i)) => self.action = Some(ShellAction::LoadSample(i)),
                Some(library::Pick::Patch(p)) => self.action = Some(ShellAction::LoadPatch(p)),
                None => {}
            },
            Tab::Shader => {
                let runtime = sh.preview.shader_errors();
                if sh
                    .shader
                    .show_in(ui, sh.patch, self.selected.as_deref(), runtime)
                {
                    sh.view.commit(sh.patch);
                }
            }
        }
    }
}

/// The selected node: type, description and live output values.
fn inspector(ui: &mut egui::Ui, sh: &mut Shell) {
    let Some(id) = sh.view.primary() else {
        ui.label(RichText::new("Select a node to inspect it.").color(theme::FG_2));
        return;
    };
    let Some(n) = sh.patch.nodes.iter().find(|n| &n.id == id) else {
        return;
    };
    let spec = Catalogue::builtin().get(&n.def_type);
    ui.label(RichText::new(spec.map_or(n.def_type.as_str(), |s| s.label.as_str())).strong());
    ui.label(RichText::new(&n.def_type).color(theme::FG_2).monospace());
    if let Some(d) = spec.and_then(|s| s.description.as_ref()) {
        ui.label(RichText::new(d).color(theme::FG_1));
    }
    if let Some(s) = crate::devices::status(&n.def_type) {
        ui.label(RichText::new(s).color(*theme::FG_2));
    }
    if let (Some(engine), Some(spec)) = (sh.engine, spec) {
        if let Some(out) = engine
            .graph
            .idx(id)
            .and_then(|i| sh.outputs.get(i))
            .and_then(|o| o.as_ref())
        {
            ui.add_space(6.0);
            for (p, v) in spec.outputs.iter().zip(out.iter()) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(p.label.as_deref().unwrap_or(&p.name)).color(theme::FG_1),
                    );
                    let text = format!("{:?}", v.iter().take(8).collect::<Vec<_>>());
                    ui.label(RichText::new(text).monospace().size(11.0));
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs(d: &DockState<Tab>) -> Vec<Tab> {
        d.iter_all_tabs().map(|(_, t)| t.clone()).collect()
    }

    #[test]
    fn previews_follow_the_renderer_count() {
        let mut d = default_dock();
        sync_previews(&mut d, 3);
        let t = tabs(&d);
        for i in 0..3 {
            assert!(t.contains(&Tab::Preview(i)));
        }
        // Extra previews share the first preview's leaf, behind it.
        let leaf = |t: Tab| d.find_tab(&t).map(|p| p.node_path());
        assert_eq!(leaf(Tab::Preview(2)), leaf(Tab::Preview(0)));
        let home = d.leaf(leaf(Tab::Preview(0)).unwrap()).unwrap();
        assert_eq!(home.tabs()[home.active.0], Tab::Preview(0));
        sync_previews(&mut d, 1);
        assert!(!tabs(&d).contains(&Tab::Preview(1)));
        // Preview 0 stays even with no renderer.
        sync_previews(&mut d, 0);
        assert!(tabs(&d).contains(&Tab::Preview(0)));
    }

    #[test]
    fn reveal_reopens_a_closed_panel() {
        let mut d = default_dock();
        let p = d.find_tab(&Tab::Help).unwrap();
        d.remove_tab(p);
        assert!(d.find_tab(&Tab::Help).is_none());
        reveal(&mut d, Tab::Help);
        let p = d.find_tab(&Tab::Help).expect("reopened");
        let leaf = d.leaf(p.node_path()).unwrap();
        assert_eq!(leaf.tabs()[leaf.active.0], Tab::Help);
    }

    #[test]
    fn layout_round_trips_through_json() {
        // A fresh tree's rects are infinite; it still saves and loads.
        let mut d = default_dock();
        d.push_to_first_leaf(Tab::Preview(1));
        let json = layout_json(&d).unwrap();
        let back: DockState<Tab> = serde_json::from_str(&json).unwrap();
        assert_eq!(tabs(&back), tabs(&d));
    }
}
