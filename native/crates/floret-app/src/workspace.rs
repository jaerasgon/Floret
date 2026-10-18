//! The app shell's dock (`src/editor/dock/*`): every panel is a tab that
//! docks anywhere in the window, splits, stacks, floats as a window and
//! closes — the node Canvas, the Nodes palette, one Preview pane per
//! renderer, Inspector, Help, Library, Shader, Profiler, Presets and
//! Subpatches. Dropping a dragged tab on a highlighted zone docks it;
//! holding ⌥ Option / Alt while letting go floats it at the pointer. Each
//! tab has a faint × at its right end. A closed panel comes back from
//! View → Panels or the tab bar's `+` menu; the layout is saved to the app
//! data dir.

use crate::canvas::{self, View};
use crate::library::{self, Library};
use crate::node_help::HelpPanel;
use crate::presets_panel::PresetsPanel;
use crate::preview::PreviewPanel;
use crate::shader_panel::ShaderPanel;
use crate::{crash, palette, subpatches_panel, theme, Sample};
use eframe::egui::{self, pos2, vec2, Pos2, Rect, RichText, Vec2};
use eframe::egui_wgpu;
use egui_dock::tab_viewer::OnCloseResponse;
use egui_dock::{
    DockArea, DockState, LeafHighlighting, Node, NodeIndex, NodePath, OverlayType, Surface,
    SurfaceIndex, TabPath, TabStyle, TabViewer, Tree,
};
use egui_phosphor::regular as icons;
use floret_engine::patch::PatchFile;
use floret_engine::spec::Catalogue;
use floret_engine::tick::{Outputs, RegionStats};
use floret_engine::{CompileWarning, Engine};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    /// A renderer's preview, keyed by the Renderer node's id, so a pane
    /// stays with its renderer when others come and go. `""` is the
    /// placeholder shown while the patch has no renderer. In View → Panels
    /// `Preview("")` stands for every preview.
    Preview(String),
    Inspector,
    Help,
    Library,
    Shader,
    Profiler,
    Presets,
    Subpatches,
    /// The node editor.
    Canvas,
    /// The node palette.
    Nodes,
}

/// Panels that can be closed and reopened, in the View → Panels order.
/// After the Canvas and the Nodes palette come the panels that share the
/// side column's tab stack, in their default order there.
/// Presets comes right after Inspector so the whole tab is on screen at the
/// default dock width, where the last tabs are cut off.
pub const PANELS: [Tab; 9] = [
    Tab::Canvas,
    Tab::Nodes,
    Tab::Inspector,
    Tab::Presets,
    Tab::Help,
    Tab::Library,
    Tab::Shader,
    Tab::Profiler,
    Tab::Subpatches,
];

/// A panel's name (tab title, menu entry).
pub fn title(tab: &Tab) -> &'static str {
    match tab {
        Tab::Preview(_) => "Preview",
        Tab::Inspector => "Inspector",
        Tab::Help => "Help",
        Tab::Library => "Library",
        Tab::Shader => "Shader",
        Tab::Profiler => "Profiler",
        Tab::Presets => "Presets",
        Tab::Subpatches => "Subpatches",
        Tab::Canvas => "Canvas",
        Tab::Nodes => "Nodes",
    }
}

/// A tab's title: small spaced-out monospace capitals. Its colour is left
/// to the dock style (active / inactive), so it isn't set here.
fn tab_label(text: &str) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::monospace(10.5),
            extra_letter_spacing: 0.8,
            color: egui::Color32::PLACEHOLDER,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.into()
}

/// A panel of the side column's tab stack (not the Canvas, the Nodes
/// palette or a preview).
fn in_stack(tab: &Tab) -> bool {
    !matches!(tab, Tab::Canvas | Tab::Nodes | Tab::Preview(_))
}

/// Whether a stack panel is in the default layout. Subpatches starts
/// closed (the tab bar is full at the default width); it opens from the
/// `+` menu.
fn open_by_default(tab: &Tab) -> bool {
    *tab != Tab::Subpatches
}

/// The stack panels the default layout opens, in order.
fn default_stack() -> Vec<Tab> {
    PANELS
        .iter()
        .filter(|t| in_stack(t) && open_by_default(t))
        .cloned()
        .collect()
}

/// The canvas's share of the window's width in the default layout.
const CANVAS_SHARE: f32 = 0.72;
/// The previews' share of the side column's height.
const PREVIEW_SHARE: f32 = 0.56;
/// The Nodes column's share of the canvas's width.
const NODES_SHARE: f32 = 0.2;

/// The default layout: the Canvas fills the left; the side column has the
/// previews on top and the other panels tabbed below. With `nodes` the
/// Nodes palette is a narrow column at the far left.
pub fn default_dock(nodes: bool) -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Canvas]);
    let tree = dock.main_surface_mut();
    let [canvas, side] = tree.split_right(
        NodeIndex::root(),
        CANVAS_SHARE,
        vec![Tab::Preview(String::new())],
    );
    tree.split_below(side, PREVIEW_SHARE, default_stack());
    if nodes {
        tree.split_left(canvas, NODES_SHARE, vec![Tab::Nodes]);
    }
    dock
}

/// The saved layout's format: `{"layout": 2, "dock": …}`. Version 1 was
/// the bare dock, from before the Canvas was a tab.
const LAYOUT_VERSION: u64 = 2;

/// A saved layout, else the default (with the Nodes column when `nodes`).
/// Layouts from before the Canvas was a tab (a bare dock, never holding
/// the Canvas), and anything that doesn't parse, give the default.
pub fn parse_layout(json: &str, nodes: bool) -> DockState<Tab> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .filter(|v| v.get("layout").and_then(|l| l.as_u64()) == Some(LAYOUT_VERSION))
        .and_then(|mut v| serde_json::from_value::<DockState<Tab>>(v["dock"].take()).ok())
        .unwrap_or_else(|| default_dock(nodes))
}

/// The preview tabs' renderer ids, in dock order.
fn preview_ids(dock: &DockState<Tab>) -> Vec<String> {
    dock.iter_all_tabs()
        .filter_map(|(_, t)| match t {
            Tab::Preview(id) => Some(id.clone()),
            _ => None,
        })
        .collect()
}

/// The previews the patch wants (`ids`, or the placeholder when there are
/// none), less the ones closed by hand.
fn wanted_previews(ids: &[String], closed: &HashSet<String>) -> Vec<String> {
    let all = if ids.is_empty() {
        vec![String::new()]
    } else {
        ids.to_vec()
    };
    all.into_iter().filter(|id| !closed.contains(id)).collect()
}

/// Make the dock hold exactly one preview tab per renderer id (`ids`, in
/// patch order), or the placeholder when there are none, leaving out the
/// previews closed by hand (`closed`). A deleted renderer's pane closes; a
/// new renderer's preview opens as its own pane under the other previews.
/// When renderers are swapped wholesale (a new patch loaded) the old panes
/// are reused in order, keeping the layout.
fn sync_previews_except(dock: &mut DockState<Tab>, ids: &[String], closed: &HashSet<String>) {
    let want = wanted_previews(ids, closed);
    let have = preview_ids(dock);
    let mut stale: Vec<String> = have.iter().filter(|h| !want.contains(h)).cloned().collect();
    let mut missing: Vec<String> = want.iter().filter(|w| !have.contains(w)).cloned().collect();
    if stale.is_empty() && missing.is_empty() {
        return;
    }
    let reused = stale.len().min(missing.len());
    for (old, new) in stale.drain(..reused).zip(missing.drain(..reused)) {
        for (_, t) in dock.iter_all_tabs_mut() {
            if *t == Tab::Preview(old.clone()) {
                *t = Tab::Preview(new.clone());
            }
        }
    }
    if stale.is_empty() && missing.is_empty() {
        // Only renamed: the panes keep the sizes they were given.
        return;
    }
    for old in stale {
        // `remove_tab` (not `retain_tabs`, which leaves split subtrees
        // misplaced when it collapses a parent).
        while let Some(p) = dock.find_tab(&Tab::Preview(old.clone())) {
            dock.remove_tab(p);
        }
    }
    for new in missing {
        add_preview(dock, Tab::Preview(new));
    }
    balance_previews(dock);
}

/// A node of a tree by index, if there is one.
fn node_at<T>(tree: &Tree<T>, i: usize) -> Option<&Node<T>> {
    (i < tree.len()).then(|| &tree[NodeIndex(i)])
}

/// The deepest node holding both `a` and `b` (heap-ordered indices).
fn common_ancestor(mut a: usize, mut b: usize) -> usize {
    while a != b {
        if a > b {
            a = (a - 1) / 2;
        } else {
            b = (b - 1) / 2;
        }
    }
    a
}

/// Leaves under node `i`.
fn leaf_count(tree: &Tree<Tab>, i: usize) -> usize {
    match node_at(tree, i) {
        Some(Node::Leaf(_)) => 1,
        Some(Node::Vertical(_) | Node::Horizontal(_)) => {
            leaf_count(tree, 2 * i + 1) + leaf_count(tree, 2 * i + 2)
        }
        _ => 0,
    }
}

/// Node `i` is a column of preview panes only: leaves of previews under
/// top / bottom splits.
fn preview_column(tree: &Tree<Tab>, i: usize) -> bool {
    match node_at(tree, i) {
        Some(Node::Leaf(l)) => l.tabs().iter().all(|t| matches!(t, Tab::Preview(_))),
        Some(Node::Vertical(_)) => {
            preview_column(tree, 2 * i + 1) && preview_column(tree, 2 * i + 2)
        }
        _ => false,
    }
}

/// The main surface's leaves (index, tabs) whose tabs pass `keep`, in
/// tree order. The Canvas's pane is never one of them: nothing is put in
/// it or split off it on the user's behalf.
fn main_leaves(dock: &DockState<Tab>, keep: impl Fn(&[Tab]) -> bool) -> Vec<usize> {
    let tree = dock.main_surface();
    (0..tree.len())
        .filter(|&i| match node_at(tree, i) {
            Some(Node::Leaf(l)) => !l.tabs().contains(&Tab::Canvas) && keep(l.tabs()),
            _ => false,
        })
        .collect()
}

fn has_preview(tabs: &[Tab]) -> bool {
    tabs.iter().any(|t| matches!(t, Tab::Preview(_)))
}

/// The main surface's preview panes, as one node: the node holding every
/// leaf with a preview in it, and whether it's a column of previews only.
fn preview_block(dock: &DockState<Tab>) -> Option<(usize, bool)> {
    let tree = dock.main_surface();
    let leaves: Vec<usize> = (0..tree.len())
        .filter(|&i| match node_at(tree, i) {
            Some(Node::Leaf(l)) => has_preview(l.tabs()),
            _ => false,
        })
        .collect();
    let first = *leaves.first()?;
    let block = leaves.iter().fold(first, |a, &b| common_ancestor(a, b));
    Some((block, preview_column(tree, block)))
}

/// A new pane in a new side column at the right of everything (the main
/// surface only has the Canvas's pane, or nothing else to go next to).
fn new_side_column(dock: &mut DockState<Tab>, tab: Tab) {
    let tree = dock.main_surface_mut();
    if tree.is_empty() {
        tree.push_to_first_leaf(tab);
    } else {
        tree.split_right(NodeIndex::root(), CANVAS_SHARE, vec![tab]);
    }
}

/// Open a new preview as its own pane: under the other previews (sharing
/// their column evenly), else above the panels, else in a new side column.
/// Never in or beside the Canvas's pane.
fn add_preview(dock: &mut DockState<Tab>, tab: Tab) {
    if dock.main_surface().is_empty() {
        dock.main_surface_mut().push_to_first_leaf(tab);
        return;
    }
    if let Some((block, true)) = preview_block(dock) {
        dock.main_surface_mut()
            .split_below(NodeIndex(block), 0.5, vec![tab]);
        return;
    }
    // Previews share panes with other panels: split the last one.
    if let Some(&last) = main_leaves(dock, has_preview).last() {
        dock.main_surface_mut()
            .split_below(NodeIndex(last), 0.5, vec![tab]);
        return;
    }
    let stack = |tabs: &[Tab]| tabs.iter().any(in_stack);
    if let Some(&panels) = main_leaves(dock, stack).first() {
        dock.main_surface_mut()
            .split_above(NodeIndex(panels), 1.0 - PREVIEW_SHARE, vec![tab]);
        return;
    }
    new_side_column(dock, tab);
}

/// Give the preview panes in the main column equal heights.
fn balance_previews(dock: &mut DockState<Tab>) {
    fn even(tree: &mut Tree<Tab>, i: usize) {
        let (l, r) = (leaf_count(tree, 2 * i + 1), leaf_count(tree, 2 * i + 2));
        if let Node::Vertical(split) = &mut tree[NodeIndex(i)] {
            split.fraction = l as f32 / (l + r).max(1) as f32;
            even(tree, 2 * i + 1);
            even(tree, 2 * i + 2);
        }
    }
    if let Some((block, true)) = preview_block(dock) {
        even(dock.main_surface_mut(), block);
    }
}

/// Put a closed tab back: the Canvas at the left of everything, the Nodes
/// palette just left of the Canvas, a preview with the other previews and
/// any other panel in the panel stack (next to the other panels, else
/// under the previews, else in a new side column). Nothing goes into the
/// Canvas's pane.
fn place(dock: &mut DockState<Tab>, tab: Tab) {
    if dock.main_surface().is_empty() {
        dock.main_surface_mut().push_to_first_leaf(tab);
        return;
    }
    match tab {
        Tab::Canvas => {
            dock.main_surface_mut()
                .split_left(NodeIndex::root(), CANVAS_SHARE, vec![tab]);
        }
        Tab::Nodes => {
            let (at, share) = match dock.find_main_surface_tab(&Tab::Canvas) {
                Some((canvas, _)) => (canvas, NODES_SHARE),
                None => (NodeIndex::root(), NODES_SHARE * CANVAS_SHARE),
            };
            dock.main_surface_mut().split_left(at, share, vec![tab]);
        }
        Tab::Preview(_) => add_preview(dock, tab),
        _ => {
            // The panel stack: the first pane (main surface first) holding
            // another stack panel, unless it's the Canvas's.
            let near = dock
                .iter_leaves()
                .find(|(_, l)| !l.tabs().contains(&Tab::Canvas) && l.tabs().iter().any(in_stack))
                .map(|(p, _)| p);
            if let Some(leaf) = near.and_then(|p| dock.leaf_mut(p).ok()) {
                leaf.append_tab(tab);
            } else if let Some(&last) = main_leaves(dock, has_preview).last() {
                dock.main_surface_mut()
                    .split_below(NodeIndex(last), PREVIEW_SHARE, vec![tab]);
            } else {
                new_side_column(dock, tab);
            }
        }
    }
}

/// Bring `tab` forward, opening it (see [`place`]) if it was closed.
pub fn reveal(dock: &mut DockState<Tab>, tab: Tab) {
    if dock.find_tab(&tab).is_none() {
        place(dock, tab.clone());
    }
    if let Some(p) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(p);
    }
}

/// Open a closed panel (brought forward) or close an open one: the
/// View → Panels checkboxes. (Previews are the workspace's: see
/// [`Workspace::toggle_panel`].)
pub fn toggle_panel(dock: &mut DockState<Tab>, tab: Tab) {
    match dock.find_tab(&tab) {
        Some(p) => {
            dock.remove_tab(p);
        }
        None => reveal(dock, tab),
    }
}

/// The dock's own words: a tab's "Eject" is "Float" here. (Translations
/// aren't saved with the layout, so this runs on every load.)
fn named(mut dock: DockState<Tab>) -> DockState<Tab> {
    dock.translations.tab_context_menu.eject_button = "Float".into();
    dock
}

fn layout_path() -> Option<PathBuf> {
    crate::editor::favorites::config_dir().map(|d| d.join("dock.json"))
}

/// What the app should do after the dock ran.
pub enum ShellAction {
    LoadSample(usize),
    LoadPatch(Box<PatchFile>),
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
    pub region_stats: &'a HashMap<usize, RegionStats>,
    pub warnings: &'a [CompileWarning],
    /// Node compute errors from the last tick.
    pub errors: &'a [(usize, String)],
    pub rs: Option<&'a egui_wgpu::RenderState>,
    pub samples: &'a [Sample],
    pub sample: Option<usize>,
    /// The Welcome window is open (the empty canvas shows no hint then).
    pub welcome_open: bool,
}

pub struct Workspace {
    dock: DockState<Tab>,
    help: HelpPanel,
    pub(crate) library: Library,
    presets: PresetsPanel,
    /// The Nodes palette's search text.
    query: String,
    /// The layout as last saved.
    saved: String,
    /// Selected node last frame (a newly selected shader node brings the
    /// Shader tab forward).
    last_selected: Option<String>,
    /// The docked tab being dragged (once the drag is decided), and where
    /// it was.
    dragging: Option<(Tab, TabPath)>,
    /// The Profiler was on screen last frame.
    profiling: bool,
    /// The renderers' ids last frame, in patch order.
    ids: Vec<String>,
    /// Previews closed by hand (renderer ids): they stay closed until
    /// reopened from View → Panels or another patch is loaded.
    closed_previews: HashSet<String>,
    /// The Canvas was drawn this frame.
    canvas_shown: bool,
}

impl Workspace {
    /// The saved layout, else the default. A layout from before the Canvas
    /// was a tab gives the default, with the Nodes column if `nodes` (the
    /// Nodes panel was showing).
    pub fn load(nodes: bool) -> Workspace {
        let saved = layout_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        Workspace::with_dock(parse_layout(&saved, nodes), saved)
    }

    fn with_dock(dock: DockState<Tab>, saved: String) -> Workspace {
        Workspace {
            dock: named(dock),
            help: HelpPanel::default(),
            library: Library::default(),
            presets: PresetsPanel::default(),
            query: String::new(),
            saved,
            last_selected: None,
            dragging: None,
            profiling: false,
            ids: Vec::new(),
            closed_previews: HashSet::new(),
            canvas_shown: false,
        }
    }

    /// Whether the engine should time its nodes (the Profiler is showing).
    pub fn profiling(&self) -> bool {
        self.profiling
    }

    /// Whether the Canvas was drawn this frame (it can be behind another
    /// tab, in a minimised window or closed).
    pub fn canvas_shown(&self) -> bool {
        self.canvas_shown
    }

    /// Bring the Library (samples and saved patches) forward.
    pub fn show_library(&mut self) {
        reveal(&mut self.dock, Tab::Library);
    }

    /// Bring the Presets panel forward (a patch with presets was loaded).
    pub fn show_presets(&mut self) {
        reveal(&mut self.dock, Tab::Presets);
    }

    /// Another patch was loaded: previews closed by hand come back.
    pub fn patch_loaded(&mut self) {
        self.closed_previews.clear();
    }

    /// Whether a panel is open (anywhere in the dock or a window).
    pub fn is_open(&self, tab: &Tab) -> bool {
        match tab {
            Tab::Preview(_) => self.closed_previews.is_empty(),
            t => self.dock.find_tab(t).is_some(),
        }
    }

    /// Open or close a panel.
    pub fn set_open(&mut self, tab: Tab, open: bool) {
        if self.is_open(&tab) != open {
            self.toggle_panel(tab);
        }
    }

    /// Every panel and whether it's open (View → Panels): the Canvas, the
    /// Nodes palette, the previews (one entry for all of them), then the
    /// panel stack.
    pub fn panels(&self) -> Vec<(Tab, bool)> {
        let mut out: Vec<(Tab, bool)> = PANELS
            .iter()
            .map(|t| (t.clone(), self.is_open(t)))
            .collect();
        let preview = Tab::Preview(String::new());
        let open = self.is_open(&preview);
        out.insert(2, (preview, open));
        out
    }

    /// Open or close a panel (View → Panels). The Preview entry closes
    /// every preview, or reopens the ones closed.
    pub fn toggle_panel(&mut self, tab: Tab) {
        if !matches!(tab, Tab::Preview(_)) {
            toggle_panel(&mut self.dock, tab);
            return;
        }
        if self.closed_previews.is_empty() {
            self.closed_previews = wanted_previews(&self.ids, &HashSet::new())
                .into_iter()
                .chain(preview_ids(&self.dock))
                .collect();
        } else {
            self.closed_previews.clear();
        }
        sync_previews_except(&mut self.dock, &self.ids, &self.closed_previews);
    }

    /// Put every panel back where it started. The Nodes palette stays open
    /// if it is.
    pub fn reset(&mut self) {
        let nodes = self.dock.find_tab(&Tab::Nodes).is_some();
        self.dock = named(default_dock(nodes));
        self.closed_previews.clear();
        sync_previews_except(&mut self.dock, &self.ids, &self.closed_previews);
    }

    pub fn show(&mut self, ui: &mut egui::Ui, shell: &mut Shell) -> Option<ShellAction> {
        shell.preview.sync(shell.engine);
        let renderers = shell.preview.renderers();
        let ids: Vec<String> = renderers.iter().map(|(id, _)| id.clone()).collect();
        // A closed preview whose renderer is gone is forgotten.
        let all = wanted_previews(&ids, &HashSet::new());
        self.closed_previews.retain(|id| all.contains(id));
        sync_previews_except(&mut self.dock, &ids, &self.closed_previews);
        self.ids = ids;
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
        // Dragged tabs: dropped on a highlighted zone they dock (egui_dock
        // does that); let go with ⌥ / Alt held they float at the pointer,
        // which egui_dock can't be told, so the layout from before its drop
        // is put back and the tab floated from there.
        let ctx = ui.ctx().clone();
        let held = self
            .dock
            .iter_all_tabs()
            .find(|(path, _)| ctx.is_being_dragged(tab_id(dock_id(), *path)))
            .map(|(path, tab)| (tab.clone(), path));
        let (released, pointer, moving, alt) = ctx.input(|i| {
            (
                i.pointer.primary_released(),
                i.pointer.latest_pos(),
                i.pointer.is_decidedly_dragging(),
                i.modifiers.alt,
            )
        });
        let dock_rect = ui.max_rect();
        let windows = window_rects(&self.dock, &ctx);
        let over_window = pointer.is_some_and(|p| windows.iter().any(|(_, r)| r.contains(p)));
        let dropped = if released {
            self.dragging.take()
        } else {
            if held.is_none() {
                self.dragging = None;
            } else if moving {
                self.dragging = held.clone();
            }
            None
        };
        let dragging = held.is_some() || self.dragging.is_some() || dropped.is_some();
        let float = dropped.filter(|_| alt);
        let before = float.as_ref().map(|_| self.dock.clone());
        // Where a tab dragged with ⌥ would float, as a card.
        if let (Some((_, from)), Some(p), true) = (&self.dragging, pointer, alt && moving) {
            paint_float_preview(&ctx, float_target(&self.dock, *from, &windows, p));
        }
        let active: Vec<Tab> = self
            .dock
            .iter_leaves()
            .filter_map(|(_, l)| l.tabs().get(l.active.0).cloned())
            .collect();
        let mut viewer = Viewer {
            shell,
            help: &mut self.help,
            library: &mut self.library,
            presets: &mut self.presets,
            query: &mut self.query,
            renderers,
            selected,
            action: None,
            add: None,
            open: PANELS
                .iter()
                .filter(|t| self.dock.find_tab(t).is_some())
                .cloned()
                .collect(),
            previews_closed: !self.closed_previews.is_empty(),
            reset: false,
            closed: Vec::new(),
            close_now: Vec::new(),
            profiling: false,
            reveal: None,
            // Over the docked panels a drop always docks: no stray
            // floating window when a tab is let go away from the drop
            // targets (⌥ floats it instead).
            dock_only: dragging && pointer.is_some_and(|p| dock_rect.contains(p)) && !over_window,
            active,
            under_controls: under_window_controls(&self.dock),
            canvas_shown: false,
        };
        DockArea::new(&mut self.dock)
            .id(dock_id())
            .style(style(ui.style(), alt && dragging))
            .show_add_buttons(true)
            .show_add_popup(true)
            .show_leaf_collapse_buttons(false)
            .show_leaf_close_all_buttons(false)
            .show_close_buttons(true)
            // Floating windows go anywhere over the app window.
            .window_bounds(ui.ctx().content_rect())
            .show_inside(ui, &mut viewer);
        let (action, add, reset) = (viewer.action.take(), viewer.add.take(), viewer.reset);
        // Requests for the canvas (Library insert, Subpatches, Arrange,
        // Wrap …) still happen when it isn't on screen.
        if !viewer.canvas_shown {
            canvas::drain(viewer.shell.patch, viewer.shell.view);
        }
        self.canvas_shown = viewer.canvas_shown;
        self.profiling = viewer.profiling;
        let (closed, close_now, revealed) = (
            std::mem::take(&mut viewer.closed),
            std::mem::take(&mut viewer.close_now),
            viewer.reveal.take(),
        );
        if let (Some(before), Some((tab, from)), Some(p)) = (before, float, pointer) {
            self.dock = before;
            if self.dock.find_tab(&tab) == Some(from) {
                let rect = float_target(&self.dock, from, &windows, p);
                float_at(&mut self.dock, from, rect);
            }
        }
        for t in &close_now {
            if let Some(p) = self.dock.find_tab(t) {
                self.dock.remove_tab(p);
            }
        }
        for t in closed.into_iter().chain(close_now) {
            if let Tab::Preview(id) = t {
                self.closed_previews.insert(id);
            }
        }
        if let Some(t) = revealed {
            reveal(&mut self.dock, t);
        }
        shell
            .preview
            .output_window(ui.ctx(), shell.rs, shell.engine, shell.outputs);
        match add {
            Some((_, Tab::Preview(_))) => {
                self.closed_previews.clear();
                sync_previews_except(&mut self.dock, &self.ids, &self.closed_previews);
            }
            Some((path, tab)) => {
                match self.dock.leaf_mut(path) {
                    Ok(leaf) => leaf.append_tab(tab.clone()),
                    Err(_) => place(&mut self.dock, tab.clone()),
                }
                if let Some(p) = self.dock.find_tab(&tab) {
                    let _ = self.dock.set_active_tab(p);
                }
            }
            None => {}
        }
        match window_controls(&ctx, &self.dock, &window_rects(&self.dock, &ctx)) {
            Some(WindowAction::Close(s)) => {
                // The panels in it close; its previews (which close only
                // from their own tab) dock again under the others.
                self.dock.remove_surface(s);
                sync_previews_except(&mut self.dock, &self.ids, &self.closed_previews);
            }
            Some(WindowAction::Minimize(s, on, h)) => set_minimized(&mut self.dock, s, on, h),
            None => {}
        }
        if reset {
            self.reset();
        }
        self.save_layout(ui.ctx());
        action
    }

    /// Write the layout when it changed and the pointer is up. Floating
    /// windows are saved where they are on screen and at their size.
    fn save_layout(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let mut dock = self.dock.clone();
        remember_windows(&mut dock, &window_rects(&self.dock, ctx));
        let Some(json) = layout_json(&dock) else {
            return;
        };
        if json == self.saved {
            return;
        }
        if let Some(path) = layout_path() {
            // Best effort: a read-only config dir just loses the layout.
            let _ = crate::files::write_atomic(&path, &json);
        }
        self.saved = json;
    }
}

fn dock_id() -> egui::Id {
    egui::Id::new("floret-dock")
}
/// A tab floated with ⌥ or from its menu.
const FLOAT_SIZE: Vec2 = Vec2::new(380.0, 320.0);

/// Where a tab floated at the pointer `p` goes.
fn float_rect(p: Pos2) -> Rect {
    Rect::from_min_size(p - vec2(24.0, 12.0), FLOAT_SIZE)
}

/// The card outline a tab dragged with ⌥ would float as.
fn paint_float_preview(ctx: &egui::Context, rect: Rect) {
    let layer = egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("floret-float-preview"),
    );
    let p = ctx.layer_painter(layer);
    p.rect(
        rect,
        4.0,
        theme::GOLD.gamma_multiply(0.10),
        egui::Stroke::new(1.5, theme::GOLD.gamma_multiply(0.8)),
        egui::StrokeKind::Inside,
    );
}

// ── egui_dock internals ─────────────────────────────────────────────────
//
// The ⌥ float, the floating-window controls, the saved window positions
// and the tab ×'s highlight rely on ids and sizes egui_dock 0.21.1 keeps
// private. They are not part of its API, so check them when upgrading
// egui_dock (the test `egui_dock_ids_are_what_we_expect` fails if they
// change):
// - a tab's title button: `DockArea::id().with((surface, "surface"))
//   .with((node, "node")).with((tab_index, "tab"))`
//   (`src/widgets/dock_area/show/leaf.rs`, `tabs`);
// - its close button: that id `.with("close-button")`, a square of
//   `Style::TAB_CLOSE_BUTTON_SIZE` (24, at most the bar's height) at the
//   tab's right end with a `TAB_CLOSE_X_SIZE` (9) × in it (`tab_title`);
// - a floating window's `egui::Window`: `format!("window {surface:?}")`
//   (`src/widgets/dock_area/show/window_surface.rs`).

/// egui's id for a tab's title button (as `DockArea` makes it).
fn tab_id(dock: egui::Id, path: TabPath) -> egui::Id {
    dock.with((path.surface, "surface"))
        .with((path.node, "node"))
        .with((path.tab.0, "tab"))
}

/// egui's id for a floating window (as `DockArea` makes it).
fn window_id(surface: SurfaceIndex) -> egui::Id {
    egui::Id::new(format!("window {surface:?}"))
}

/// egui_dock's close button in a tab of rect `tab`.
fn close_button_rect(tab: Rect) -> Rect {
    let size = 24.0_f32.min(TAB_BAR_HEIGHT);
    Rect::from_center_size(
        pos2(tab.right() - size / 2.0, tab.center().y),
        Vec2::splat(size),
    )
}

/// The × egui_dock draws in a close button.
fn close_x_rect(button: Rect) -> Rect {
    Rect::from_center_size(button.center(), Vec2::splat(9.0))
}

/// The tab × is faint (the dock style); it brightens while its tab is
/// hovered or active, painted over egui_dock's own. Over the × itself
/// egui_dock draws it at full strength.
fn brighten_close(r: &egui::Response, active: bool) {
    let ctx = &r.ctx;
    if ctx.dragged_id().is_some() {
        return;
    }
    let button = close_button_rect(r.rect);
    let on_x = ctx.pointer_hover_pos().is_some_and(|p| button.contains(p));
    if on_x || !(active || r.contains_pointer()) {
        return;
    }
    let x = close_x_rect(button);
    let painter = ctx
        .layer_painter(r.layer_id)
        .with_clip_rect(r.interact_rect);
    let stroke = egui::Stroke::new(1.0, *theme::FG_2);
    painter.line_segment([x.left_top(), x.right_bottom()], stroke);
    painter.line_segment([x.right_top(), x.left_bottom()], stroke);
}

/// Floating windows and where egui last drew them.
fn window_rects(dock: &DockState<Tab>, ctx: &egui::Context) -> Vec<(SurfaceIndex, Rect)> {
    dock.iter_surfaces_indexed()
        .filter(|(_, s)| matches!(s, Surface::Window(..)))
        .filter_map(|(i, _)| Some((i, ctx.memory(|m| m.area_rect(window_id(i)))?)))
        .collect()
}

/// The tab under each floating window's − × controls: the last tab of the
/// window's top-right pane. It has no × of its own (the window's take its
/// place); it closes from its menu or with a middle click.
fn under_window_controls(dock: &DockState<Tab>) -> Vec<Tab> {
    let mut out = Vec::new();
    for (s, surface) in dock.iter_surfaces_indexed() {
        let Surface::Window(tree, _) = surface else {
            continue;
        };
        if is_minimized(dock, s) {
            continue;
        }
        let leaves: Vec<(Rect, &[Tab])> = (0..tree.len())
            .filter_map(|i| match node_at(tree, i) {
                Some(Node::Leaf(l)) if !l.tabs().is_empty() => Some((l.rect(), l.tabs())),
                _ => None,
            })
            .collect();
        let corner = leaves
            .iter()
            .filter(|(r, _)| r.is_finite() && r.is_positive())
            .fold(None::<Pos2>, |c, (r, _)| {
                let p = r.right_top();
                Some(c.map_or(p, |c| pos2(c.x.max(p.x), c.y.min(p.y))))
            });
        let top_right = match corner {
            Some(c) => leaves
                .iter()
                .find(|(r, _)| r.contains(c + vec2(-2.0, 2.0)))
                .map(|(_, t)| *t),
            None => (leaves.len() == 1).then(|| leaves[0].1),
        };
        if let Some(last) = top_right.and_then(|t| t.last()) {
            out.push(last.clone());
        }
    }
    out
}

/// Whether the tab at `from` is the only tab of a floating window:
/// floating it moves the window.
fn alone_in_window(dock: &DockState<Tab>, from: TabPath) -> bool {
    !from.surface.is_main()
        && dock
            .get_surface(from.surface)
            .and_then(|s| s.node_tree())
            .is_some_and(|t| t.num_tabs() == 1)
}

/// Where a tab dropped at `p` floats: a new card, or its own window moved
/// there (at its size) when it's alone in one.
fn float_target(
    dock: &DockState<Tab>,
    from: TabPath,
    windows: &[(SurfaceIndex, Rect)],
    p: Pos2,
) -> Rect {
    let size = windows
        .iter()
        .find(|(s, _)| *s == from.surface && alone_in_window(dock, from))
        .map_or(FLOAT_SIZE, |(_, r)| r.size());
    Rect::from_min_size(float_rect(p).min, size)
}

/// Float the tab at `from` into `rect`: its window moves there if it's
/// the window's only tab, else it gets a window of its own.
fn float_at(dock: &mut DockState<Tab>, from: TabPath, rect: Rect) {
    if alone_in_window(dock, from) {
        if let Some(w) = dock.get_window_state_mut(from.surface) {
            w.set_position(rect.min);
        }
        return;
    }
    // `detach_tab` shrinks a docked tab's rect to 80%.
    let scale = if from.surface.is_main() { 0.8 } else { 1.0 };
    let r = Rect::from_min_size(rect.min, FLOAT_SIZE / scale);
    dock.detach_tab(from, r);
}

// ── Floating windows ────────────────────────────────────────────────────

/// A window control clicked.
enum WindowAction {
    Close(SurfaceIndex),
    /// Minimise (`true`) or restore; the window's height on screen.
    Minimize(SurfaceIndex, bool, f32),
}

/// A floating window's state as JSON. Its fields are private to
/// egui_dock, but it's saved with the layout, so serde reaches them.
fn window_json(dock: &DockState<Tab>, s: SurfaceIndex) -> Option<serde_json::Value> {
    match dock.get_surface(s)? {
        Surface::Window(_, w) => serde_json::to_value(w).ok(),
        _ => None,
    }
}

fn is_minimized(dock: &DockState<Tab>, s: SurfaceIndex) -> bool {
    window_json(dock, s)
        .and_then(|v| v.get("minimized").and_then(|m| m.as_bool()))
        .unwrap_or(false)
}

/// Minimise a floating window to its tab bar, or restore it: what
/// egui_dock's own Shift+collapse does (its expand arrow restores too).
/// `height` is the window's height on screen, which it restores to.
fn set_minimized(dock: &mut DockState<Tab>, s: SurfaceIndex, on: bool, height: f32) {
    let Some(mut v) = window_json(dock, s) else {
        return;
    };
    zero_nulls(&mut v);
    v["minimized"] = on.into();
    if on {
        if height.is_finite() && height > 0.0 {
            v["expanded_height"] = height.into();
        }
    } else {
        v["new"] = true.into();
    }
    if let (Ok(w), Some(slot)) = (serde_json::from_value(v), dock.get_window_state_mut(s)) {
        *slot = w;
    }
}

/// Store each floating window's place and size in its state, so a saved
/// layout reopens them where they were.
fn remember_windows(dock: &mut DockState<Tab>, windows: &[(SurfaceIndex, Rect)]) {
    for &(s, rect) in windows {
        // A minimised window keeps the height it restores to instead.
        let minimized = is_minimized(dock, s);
        if let Some(w) = dock.get_window_state_mut(s) {
            w.set_position(rect.min);
            if !minimized {
                w.set_size(rect.size());
            }
        }
    }
}

/// Minimise and close buttons at the right of each floating window's top
/// tab bar (left of its `+`). egui_dock's own leaf buttons would also show
/// on every docked pane, so these are drawn into the window's layer.
fn window_controls(
    ctx: &egui::Context,
    dock: &DockState<Tab>,
    windows: &[(SurfaceIndex, Rect)],
) -> Option<WindowAction> {
    const BUTTON: Vec2 = Vec2::new(22.0, 22.0);
    const ADD_BUTTON: f32 = 24.0;
    let margin = ctx.global_style().spacing.window_margin;
    let bar_h = TAB_BAR_HEIGHT;
    let mut action = None;
    for &(s, rect) in windows {
        let minimized = is_minimized(dock, s);
        let inner_right = rect.right() - f32::from(margin.right) - 1.0;
        let right = if minimized {
            inner_right
        } else {
            inner_right - ADD_BUTTON
        };
        let top = rect.top() + f32::from(margin.top) + 1.0;
        let area = Rect::from_min_max(
            pos2(right - 2.0 * BUTTON.x - 2.0, top),
            pos2(right, top + bar_h),
        );
        let layer = egui::LayerId::new(egui::Order::Middle, window_id(s));
        let mut ui = egui::Ui::new(
            ctx.clone(),
            window_id(s).with("floret-controls"),
            egui::UiBuilder::new()
                .layer_id(layer)
                .max_rect(area)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        ui.spacing_mut().item_spacing.x = 2.0;
        let button = |ui: &mut egui::Ui, glyph: &str, tip: &str| {
            ui.add(
                egui::Button::new(theme::icon(glyph, 13.0, *theme::FG_1))
                    .min_size(BUTTON)
                    .frame_when_inactive(false),
            )
            .on_hover_text(tip)
        };
        let has_preview = dock
            .get_surface(s)
            .and_then(|surface| surface.node_tree())
            .is_some_and(|t| t.tabs().any(|t| matches!(t, Tab::Preview(_))));
        let close_tip = if has_preview {
            "Close window (previews go back to the dock)"
        } else {
            "Close window"
        };
        if button(&mut ui, icons::X, close_tip).clicked() {
            action = Some(WindowAction::Close(s));
        }
        let (glyph, tip) = if minimized {
            (icons::CORNERS_OUT, "Restore window")
        } else {
            (icons::MINUS, "Minimise window")
        };
        if button(&mut ui, glyph, tip).clicked() {
            action = Some(WindowAction::Minimize(s, !minimized, rect.height()));
        }
    }
    action
}

/// Rects egui_dock hasn't laid out yet are infinite (written as `null`,
/// which won't load back); they're zeroed, since the dock recomputes every
/// rect when it's next shown.
fn zero_nulls(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(m) if m.contains_key("x") && m.contains_key("y") => {
            for c in m.values_mut().filter(|c| c.is_null()) {
                *c = 0.0.into();
            }
        }
        serde_json::Value::Object(m) => m.values_mut().for_each(zero_nulls),
        serde_json::Value::Array(a) => a.iter_mut().for_each(zero_nulls),
        _ => {}
    }
}

/// The layout as JSON, in the saved format (see [`parse_layout`],
/// [`zero_nulls`]).
pub fn layout_json(dock: &DockState<Tab>) -> Option<String> {
    let mut v = serde_json::to_value(dock).ok()?;
    zero_nulls(&mut v);
    serde_json::to_string(&serde_json::json!({ "layout": LAYOUT_VERSION, "dock": v })).ok()
}

const TAB_BAR_HEIGHT: f32 = 26.0;

/// Floret's theme on the dock. While a tab is dragged with ⌥ (`floating`)
/// the dock's drop highlights are hidden: the float card shows instead.
fn style(base: &egui::Style, floating: bool) -> egui_dock::Style {
    let mut s = egui_dock::Style::from_egui(base);
    s.tab_bar.bg_fill = *theme::BG_1;
    s.tab_bar.hline_color = *theme::BORDER;
    s.tab_bar.height = TAB_BAR_HEIGHT;
    // Tabs share the bar's width; when they don't fit they keep their
    // full titles and the bar scrolls (wheel over it, or its scroll bar).
    s.tab_bar.fill_tab_bar = true;
    s.tab_bar.show_scroll_bar_on_overflow = true;
    // Wide canvas-coloured gutters, so the panels read as separate cards.
    s.separator.width = 3.0;
    // A wider grab, and panes can get down to 80 px (175 by default, which
    // locked side-by-side splits in the narrow dock).
    s.separator.extra_interact_width = 4.0;
    s.separator.extra = 80.0;
    s.separator.color_idle = *theme::BG_0;
    s.separator.color_hovered = *theme::FG_3;
    s.separator.color_dragged = *theme::GOLD;
    s.tab.tab_body.bg_fill = *theme::BG_1;
    s.tab.tab_body.stroke = egui::Stroke::NONE;
    s.tab.tab_body.inner_margin = egui::Margin::same(8);
    s.tab.active.bg_fill = *theme::BG_2;
    s.tab.active.text_color = *theme::FG_1;
    s.tab.focused.bg_fill = *theme::BG_2;
    s.tab.focused.text_color = *theme::FG_0;
    s.tab.inactive.bg_fill = *theme::BG_1;
    s.tab.inactive.text_color = *theme::FG_2;
    s.tab.hovered.text_color = *theme::FG_1;
    s.tab.hline_below_active_tab_name = false;
    // The tab ×: faint (see `brighten_close`), full strength under the
    // pointer.
    s.buttons.close_tab_color = *theme::FG_3;
    s.buttons.close_tab_active_color = *theme::FG_0;
    s.buttons.close_tab_bg_fill = *theme::BG_3;
    // Dragging a tab lights up the whole region it would land in (a half
    // to split, the middle to join as a tab), and outlines the pane.
    s.overlay.overlay_type = OverlayType::HighlightedAreas;
    s.overlay.selection_color = theme::GOLD.gamma_multiply(0.35);
    s.overlay.hovered_leaf_highlight = LeafHighlighting {
        color: theme::GOLD.gamma_multiply(0.06),
        corner_radius: egui::CornerRadius::ZERO,
        stroke: egui::Stroke::new(1.5, theme::GOLD.gamma_multiply(0.8)),
        expansion: 0.0,
    };
    if floating {
        s.overlay.selection_color = egui::Color32::TRANSPARENT;
        s.overlay.hovered_leaf_highlight.color = egui::Color32::TRANSPARENT;
        s.overlay.hovered_leaf_highlight.stroke = egui::Stroke::NONE;
    }
    s.overlay.button_color = *theme::FG_1;
    // Dropping inside a pane never floats it (float by holding ⌥ while
    // letting go, or with the tab's Float menu item).
    s.overlay.feel.window_drop_coverage = 0.0;
    s.overlay.feel.center_drop_coverage = 0.3;
    s.main_surface_border_stroke = egui::Stroke::NONE;
    s
}

struct Viewer<'s, 'a> {
    shell: &'s mut Shell<'a>,
    help: &'s mut HelpPanel,
    library: &'s mut Library,
    presets: &'s mut PresetsPanel,
    query: &'s mut String,
    /// (renderer id, title), in patch order.
    renderers: Vec<(String, String)>,
    selected: Option<String>,
    action: Option<ShellAction>,
    /// A closed panel reopened from a `+` menu.
    add: Option<(NodePath, Tab)>,
    /// Panels currently in the dock.
    open: Vec<Tab>,
    /// Some previews are closed (the `+` menu offers Preview).
    previews_closed: bool,
    reset: bool,
    /// Tabs closed by the dock this frame (×, middle click, menu).
    closed: Vec<Tab>,
    /// Tabs under a window's controls closed from their menu or with a
    /// middle click: removed after the dock ran.
    close_now: Vec<Tab>,
    profiling: bool,
    /// A panel to bring forward after the dock ran (the Library, when a
    /// subpatch is being saved to it).
    reveal: Option<Tab>,
    /// A tab is being dragged over the docked panels: it can only dock.
    dock_only: bool,
    /// The tab each pane shows.
    active: Vec<Tab>,
    /// See [`under_window_controls`].
    under_controls: Vec<Tab>,
    /// The Canvas was drawn.
    canvas_shown: bool,
}

impl TabViewer for Viewer<'_, '_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(("floret-tab", tab.clone()))
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Preview(id) => tab_label(
                self.renderers
                    .iter()
                    .find(|(r, _)| r == id)
                    .map_or("Preview", |(_, t)| t.as_str()),
            ),
            other => tab_label(title(other)),
        }
    }

    /// Every tab closes (its ×, a middle click, its menu's Close), except
    /// that the one under a window's controls has no × (see
    /// [`under_window_controls`]).
    fn is_closeable(&self, tab: &Tab) -> bool {
        !self.under_controls.contains(tab)
    }

    fn on_close(&mut self, tab: &mut Tab) -> OnCloseResponse {
        // Called twice for a menu Close.
        if !self.closed.contains(tab) {
            self.closed.push(tab.clone());
        }
        OnCloseResponse::Close
    }

    /// The tab under a window's controls gets the Close the dock leaves
    /// out for it; the menu's Float (egui_dock's own item) follows.
    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Tab, _path: NodePath) {
        if self.under_controls.contains(tab) && ui.button("Close").clicked() {
            self.close_now.push(tab.clone());
            ui.close();
        }
    }

    fn on_tab_button(&mut self, tab: &mut Tab, response: &egui::Response) {
        if self.under_controls.contains(tab) {
            if response.middle_clicked() {
                self.close_now.push(tab.clone());
            }
            return;
        }
        brighten_close(response, self.active.contains(tab));
    }

    /// Tabs float from their menu or with ⌥ held as they're let go; while
    /// one is dragged over the docked panels, wherever it's let go it docks.
    fn allowed_in_windows(&self, _tab: &mut Tab) -> bool {
        !self.dock_only
    }

    /// The Canvas fills its pane edge to edge, on the canvas colour.
    fn tab_style_override(&self, tab: &Tab, global: &TabStyle) -> Option<TabStyle> {
        (*tab == Tab::Canvas).then(|| {
            let mut s = global.clone();
            s.tab_body.inner_margin = egui::Margin::ZERO;
            s.tab_body.bg_fill = *theme::BG_0;
            s
        })
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        match tab {
            Tab::Inspector | Tab::Shader | Tab::Profiler | Tab::Presets | Tab::Subpatches => {
                [false, true]
            }
            _ => [false, false],
        }
    }

    fn add_popup(&mut self, ui: &mut egui::Ui, path: NodePath) {
        ui.set_min_width(140.0);
        let mut closed: Vec<Tab> = PANELS
            .iter()
            .filter(|t| !self.open.contains(t))
            .cloned()
            .collect();
        if self.previews_closed {
            closed.insert(
                closed.iter().filter(|t| !in_stack(t)).count(),
                Tab::Preview(String::new()),
            );
        }
        if closed.is_empty() {
            ui.label(
                RichText::new("All panels are open")
                    .color(theme::FG_2)
                    .small(),
            );
        }
        for t in closed {
            if ui.button(title(&t)).clicked() {
                self.add = Some((path, t));
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
            Tab::Preview(id) => {
                if !sh.crash.render_ok() {
                    return;
                }
                let (preview, rs, engine, outputs) =
                    (&mut *sh.preview, sh.rs, sh.engine, sh.outputs);
                sh.crash.run(crash::Stage::Render, sh.patch, || {
                    preview.show_renderer(ui, rs, engine, outputs, id);
                });
            }
            Tab::Canvas => {
                sh.view.thumbs =
                    sh.preview
                        .probe_thumbs(ui.ctx(), sh.rs, sh.engine, sh.outputs, sh.patch);
                if let Some(engine) = sh.engine {
                    let live = canvas::Live {
                        engine,
                        outputs: sh.outputs,
                        region_stats: sh.region_stats,
                        warnings: sh.warnings,
                        errors: sh.errors,
                    };
                    let (patch, view) = (&mut *sh.patch, &mut *sh.view);
                    // A panic in the editor shows the crash screen: the
                    // half-done edit is rolled back and the app keeps going.
                    match crash::catch(crash::Stage::Editor, || {
                        canvas::show(ui, patch, &live, cat, view);
                    }) {
                        Ok(()) => self.canvas_shown = true,
                        Err(caught) => {
                            sh.view.recover(sh.patch);
                            sh.crash.report(caught, sh.patch);
                        }
                    }
                }
                if sh.patch.nodes.is_empty() && !sh.welcome_open {
                    empty_hint(ui);
                }
            }
            Tab::Nodes => palette::show(ui, self.query, sh.view, sh.patch),
            Tab::Inspector => inspector(ui, sh),
            Tab::Help => {
                let ty = self
                    .selected
                    .as_deref()
                    .and_then(|id| sh.patch.nodes.iter().find(|n| n.id == id))
                    .map(crate::node_help::key_for);
                self.help.ui(ui, cat, ty.as_deref());
            }
            Tab::Library => match self.library.ui(ui, sh.samples, sh.sample, sh.patch, sh.rs) {
                Some(library::Pick::Sample(i)) => self.action = Some(ShellAction::LoadSample(i)),
                Some(library::Pick::Patch(p)) => self.action = Some(ShellAction::LoadPatch(p)),
                Some(library::Pick::Insert(p)) => sh.view.request_insert(*p),
                None => {}
            },
            Tab::Subpatches => {
                if let Some((snippet, name)) = subpatches_panel::ui(ui, sh.patch, sh.view) {
                    self.library.open_save_form(snippet, &name);
                    self.reveal = Some(Tab::Library);
                }
            }
            Tab::Profiler => {
                self.profiling = true;
                let render_ms = sh.preview.render_ms();
                crate::profiler::ui(ui, sh.engine, sh.patch, sh.view, render_ms);
            }
            Tab::Presets => self.presets.ui(ui, sh.patch, sh.view),
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

/// Centred hint on an empty canvas.
fn empty_hint(ui: &mut egui::Ui) {
    theme::paint_empty_state(
        ui,
        ui.max_rect(),
        "Empty patch",
        "Double-click or press Tab to add a node · ? for shortcuts · Library for samples",
    );
}

/// The selected node: type, description and live output values.
fn inspector(ui: &mut egui::Ui, sh: &mut Shell) {
    let Some(id) = sh.view.primary() else {
        theme::empty_state(
            ui,
            "No node selected",
            "Click a node, or press Tab to add one",
        );
        return;
    };
    let Some(n) = sh.patch.nodes.iter().find(|n| &n.id == id) else {
        return;
    };
    let spec = Catalogue::builtin().get(&n.def_type);
    ui.label(
        RichText::new(spec.map_or(n.def_type.as_str(), |s| s.label.as_str()))
            .strong()
            .font(theme::medium(14.0)),
    );
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
                    let name = p.label.as_deref().unwrap_or_else(|| {
                        crate::editor::values::layer_label(p, false).unwrap_or(&p.name)
                    });
                    ui.label(RichText::new(name).color(theme::FG_1));
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

    fn sync_previews(dock: &mut DockState<Tab>, ids: &[String]) {
        sync_previews_except(dock, ids, &HashSet::new());
    }

    fn tabs(d: &DockState<Tab>) -> Vec<Tab> {
        d.iter_all_tabs().map(|(_, t)| t.clone()).collect()
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn preview(id: &str) -> Tab {
        Tab::Preview(id.to_owned())
    }

    /// The main-surface leaf holding `t`.
    fn leaf_of(d: &DockState<Tab>, t: &Tab) -> Option<NodeIndex> {
        d.find_tab(t)
            .filter(|p| p.surface.is_main())
            .map(|p| p.node)
    }

    /// The main surface laid out in a 1400 × 900 window (no separators).
    fn rects(d: &DockState<Tab>) -> DockState<Tab> {
        let mut d = d.clone();
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0));
        layout(d.main_surface_mut(), 0, rect);
        d
    }

    /// Where `t`'s pane is on screen (see [`rects`]).
    fn rect_of(d: &DockState<Tab>, t: &Tab) -> Rect {
        let laid = rects(d);
        let n = leaf_of(&laid, t).unwrap_or_else(|| panic!("{t:?} docked"));
        laid.main_surface()[n].rect().unwrap()
    }

    /// Every preview is the only tab of its own main-surface pane, and the
    /// panes stack top to bottom in `order`, with equal heights.
    fn assert_stacked(d: &DockState<Tab>, order: &[&str]) {
        let tree = d.main_surface();
        let mut leaves = Vec::new();
        for id in order {
            let n = leaf_of(d, &preview(id)).unwrap_or_else(|| panic!("{id} docked"));
            assert_eq!(tree[n].tabs_count(), 1, "{id} has a pane of its own");
            leaves.push(n);
        }
        let mut uniq = leaves.clone();
        uniq.dedup();
        assert_eq!(uniq.len(), order.len());
        let laid = rects(d);
        let tree = laid.main_surface();
        let rects: Vec<Rect> = leaves.iter().map(|&n| tree[n].rect().unwrap()).collect();
        for w in rects.windows(2) {
            assert!(w[0].center().y < w[1].center().y, "panes stack in order");
            assert!(
                (w[0].height() - w[1].height()).abs() < 1.0,
                "even: {rects:?}"
            );
        }
    }

    /// egui_dock's split geometry, without the separators.
    fn layout(tree: &mut Tree<Tab>, i: usize, rect: Rect) {
        if i >= tree.len() {
            return;
        }
        tree[NodeIndex(i)].set_rect(rect);
        let (a, b) = match &tree[NodeIndex(i)] {
            Node::Vertical(s) => {
                let y = rect.top() + rect.height() * s.fraction;
                (
                    Rect::from_min_max(rect.min, pos2(rect.right(), y)),
                    Rect::from_min_max(pos2(rect.left(), y), rect.max),
                )
            }
            Node::Horizontal(s) => {
                let x = rect.left() + rect.width() * s.fraction;
                (
                    Rect::from_min_max(rect.min, pos2(x, rect.bottom())),
                    Rect::from_min_max(pos2(x, rect.top()), rect.max),
                )
            }
            _ => return,
        };
        layout(tree, 2 * i + 1, a);
        layout(tree, 2 * i + 2, b);
    }

    /// The stack panels in the dock, in order.
    fn stack(d: &DockState<Tab>) -> Vec<Tab> {
        tabs(d).into_iter().filter(in_stack).collect()
    }

    /// The Canvas's pane holds the Canvas only.
    fn canvas_alone(d: &DockState<Tab>) -> bool {
        leaf_of(d, &Tab::Canvas).is_some_and(|n| d.main_surface()[n].tabs_count() == 1)
    }

    #[test]
    fn the_default_layout_has_the_canvas_left_and_the_side_column_right() {
        let d = default_dock(false);
        assert!(canvas_alone(&d));
        assert!(d.find_tab(&Tab::Nodes).is_none());
        let canvas = rect_of(&d, &Tab::Canvas);
        let previews = rect_of(&d, &preview(""));
        let panels = rect_of(&d, &Tab::Inspector);
        // The canvas fills the left ~72%, full height.
        assert!((canvas.width() / 1400.0 - CANVAS_SHARE).abs() < 0.01);
        assert_eq!(canvas.height(), 900.0);
        assert_eq!(canvas.left(), 0.0);
        // The side column: the preview(s) on top, the panels tabbed below.
        assert_eq!(previews.left(), canvas.right());
        assert_eq!(panels.left(), canvas.right());
        assert!(previews.bottom() <= panels.top());
        assert_eq!(stack(&d), default_stack());
        let p = d.find_tab(&Tab::Inspector).unwrap();
        assert_eq!(d.leaf(p.node_path()).unwrap().tabs(), default_stack());
        assert_eq!(d.surfaces_count(), 1);
    }

    #[test]
    fn the_default_layout_has_a_nodes_column_when_asked() {
        let d = default_dock(true);
        assert!(canvas_alone(&d));
        let nodes = rect_of(&d, &Tab::Nodes);
        let canvas = rect_of(&d, &Tab::Canvas);
        assert_eq!(nodes.left(), 0.0);
        assert_eq!(nodes.right(), canvas.left());
        assert!(nodes.width() < 250.0, "narrow: {nodes:?}");
        assert!((rect_of(&d, &preview("")).left() / 1400.0 - CANVAS_SHARE).abs() < 0.01);
        assert_eq!(stack(&d), default_stack());
    }

    #[test]
    fn each_renderer_gets_its_own_preview_pane() {
        let mut d = default_dock(false);
        let home = leaf_of(&d, &preview(""));
        sync_previews(&mut d, &ids(&["r1"]));
        assert_eq!(preview_ids(&d), ids(&["r1"]));
        // The placeholder became r1's pane, where the default put it.
        assert_eq!(leaf_of(&d, &preview("r1")), home);
        sync_previews(&mut d, &ids(&["r1", "r2", "r3", "r4", "r5"]));
        assert_stacked(&d, &["r1", "r2", "r3", "r4", "r5"]);
        // The other panels are untouched, below the previews; the canvas
        // keeps its pane to itself.
        assert_eq!(stack(&d), default_stack());
        assert!(canvas_alone(&d));
        let canvas = rect_of(&d, &Tab::Canvas);
        assert!(rect_of(&d, &preview("r5")).left() >= canvas.right());
    }

    #[test]
    fn a_deleted_renderer_closes_its_pane_and_the_others_keep_theirs() {
        let mut d = default_dock(false);
        sync_previews(&mut d, &ids(&["a", "b", "c"]));
        assert_stacked(&d, &["a", "b", "c"]);
        // Delete the middle one.
        sync_previews(&mut d, &ids(&["a", "c"]));
        assert_eq!(preview_ids(&d).len(), 2);
        assert!(d.find_tab(&preview("b")).is_none());
        assert_stacked(&d, &["a", "c"]);
        // A new one opens under them.
        sync_previews(&mut d, &ids(&["a", "c", "d"]));
        assert_stacked(&d, &["a", "c", "d"]);
        // Down to none: one placeholder pane stays.
        sync_previews(&mut d, &[]);
        assert_eq!(preview_ids(&d), ids(&[""]));
        assert_eq!(stack(&d), default_stack());
        assert!(canvas_alone(&d));
    }

    #[test]
    fn a_new_patch_reuses_the_preview_panes() {
        let mut d = default_dock(false);
        sync_previews(&mut d, &ids(&["a", "b"]));
        let at = [leaf_of(&d, &preview("a")), leaf_of(&d, &preview("b"))];
        sync_previews(&mut d, &ids(&["x", "y"]));
        assert_eq!(preview_ids(&d), ids(&["x", "y"]));
        assert_eq!([leaf_of(&d, &preview("x")), leaf_of(&d, &preview("y"))], at);
    }

    #[test]
    fn a_floating_preview_stays_floating_and_new_ones_dock() {
        let mut d = default_dock(false);
        sync_previews(&mut d, &ids(&["a", "b"]));
        let p = d.find_tab(&preview("a")).unwrap();
        d.detach_tab(p, Rect::from_min_size(pos2(10.0, 10.0), FLOAT_SIZE));
        sync_previews(&mut d, &ids(&["a", "b", "c"]));
        assert!(!d.find_tab(&preview("a")).unwrap().surface.is_main());
        assert_stacked(&d, &["b", "c"]);
        // A preview whose window was closed docks again under the others.
        let s = d.find_tab(&preview("a")).unwrap().surface;
        d.remove_surface(s);
        sync_previews(&mut d, &ids(&["a", "b", "c"]));
        assert_stacked(&d, &["b", "c", "a"]);
    }

    #[test]
    fn previews_mixed_with_panels_still_get_their_own_pane() {
        let mut d = default_dock(false);
        sync_previews(&mut d, &ids(&["a"]));
        // The user drops the Help tab into the preview's pane.
        let help = d.find_tab(&Tab::Help).unwrap();
        let home = d.find_tab(&preview("a")).unwrap().node_path();
        d.move_tab(
            help,
            egui_dock::TabDestination::Node(home, egui_dock::TabInsert::Append),
        );
        sync_previews(&mut d, &ids(&["a", "b"]));
        let b = leaf_of(&d, &preview("b")).unwrap();
        assert_eq!(d.main_surface()[b].tabs_count(), 1);
        assert_ne!(leaf_of(&d, &preview("a")), Some(b));
    }

    /// Move `t` into the Canvas's pane (the user stacks it there).
    fn stack_with_canvas(d: &mut DockState<Tab>, t: &Tab) {
        let from = d.find_tab(t).unwrap();
        let canvas = d.find_tab(&Tab::Canvas).unwrap().node_path();
        d.move_tab(
            from,
            egui_dock::TabDestination::Node(canvas, egui_dock::TabInsert::Append),
        );
    }

    #[test]
    fn a_new_preview_never_goes_into_or_beside_the_canvas() {
        // The only preview is stacked with the canvas.
        let mut d = default_dock(false);
        sync_previews(&mut d, &ids(&["a"]));
        stack_with_canvas(&mut d, &preview("a"));
        let canvas = leaf_of(&d, &Tab::Canvas).unwrap();
        sync_previews(&mut d, &ids(&["a", "b"]));
        let b = leaf_of(&d, &preview("b")).unwrap();
        assert_ne!(b, canvas);
        assert_eq!(d.main_surface()[b].tabs_count(), 1);
        // It went above the panel stack, right of the canvas.
        let laid_b = rect_of(&d, &preview("b"));
        assert!(laid_b.left() >= rect_of(&d, &Tab::Canvas).right());
        assert!(laid_b.bottom() <= rect_of(&d, &Tab::Inspector).top());
        // Only the canvas and the previews it holds: a new side column.
        let mut d = DockState::new(vec![Tab::Canvas]);
        sync_previews(&mut d, &ids(&["a"]));
        assert!(canvas_alone(&d));
        assert!(rect_of(&d, &preview("a")).left() >= rect_of(&d, &Tab::Canvas).right());
        // Nothing docked at all: it's the first pane.
        let mut d = DockState::new(vec![Tab::Canvas]);
        let p = d.find_tab(&Tab::Canvas).unwrap();
        d.detach_tab(p, Rect::from_min_size(Pos2::ZERO, FLOAT_SIZE));
        sync_previews(&mut d, &ids(&["a"]));
        assert!(d.find_tab(&preview("a")).unwrap().surface.is_main());
    }

    #[test]
    fn reveal_reopens_a_closed_panel() {
        let mut d = default_dock(false);
        let p = d.find_tab(&Tab::Help).unwrap();
        d.remove_tab(p);
        assert!(d.find_tab(&Tab::Help).is_none());
        reveal(&mut d, Tab::Help);
        let p = d.find_tab(&Tab::Help).expect("reopened");
        let leaf = d.leaf(p.node_path()).unwrap();
        assert_eq!(leaf.tabs()[leaf.active.0], Tab::Help);
        assert_eq!(
            Some(p.node_path()),
            d.find_tab(&Tab::Inspector).map(|p| p.node_path())
        );
    }

    #[test]
    fn a_revealed_panel_never_goes_into_the_canvas() {
        // Inspector stacked with the canvas, the rest of the stack closed:
        // Help doesn't join the canvas's pane.
        let mut d = default_dock(false);
        stack_with_canvas(&mut d, &Tab::Inspector);
        for t in default_stack().iter().filter(|t| **t != Tab::Inspector) {
            let p = d.find_tab(t).unwrap();
            d.remove_tab(p);
        }
        let canvas = leaf_of(&d, &Tab::Canvas).unwrap();
        reveal(&mut d, Tab::Help);
        assert_ne!(leaf_of(&d, &Tab::Help), Some(canvas));
        // It went under the previews, in the side column.
        let help = rect_of(&d, &Tab::Help);
        assert!(help.left() >= rect_of(&d, &Tab::Canvas).right());
        assert!(help.top() >= rect_of(&d, &preview("")).bottom());
        // A dock with the canvas only: a new side column.
        let mut d = DockState::new(vec![Tab::Canvas]);
        reveal(&mut d, Tab::Library);
        assert!(canvas_alone(&d));
        assert!(rect_of(&d, &Tab::Library).left() >= rect_of(&d, &Tab::Canvas).right());
        // Everything closed: the first pane.
        let mut d = DockState::new(vec![Tab::Canvas]);
        let p = d.find_tab(&Tab::Canvas).unwrap();
        d.remove_tab(p);
        reveal(&mut d, Tab::Library);
        assert!(leaf_of(&d, &Tab::Library).is_some());
    }

    #[test]
    fn the_canvas_and_nodes_come_back_where_they_belong() {
        let mut d = default_dock(false);
        let p = d.find_tab(&Tab::Canvas).unwrap();
        d.remove_tab(p);
        assert!(d.find_tab(&Tab::Canvas).is_none());
        reveal(&mut d, Tab::Canvas);
        assert!(canvas_alone(&d));
        let canvas = rect_of(&d, &Tab::Canvas);
        assert_eq!(canvas.left(), 0.0);
        assert!((canvas.width() / 1400.0 - CANVAS_SHARE).abs() < 0.01);
        reveal(&mut d, Tab::Nodes);
        let nodes = rect_of(&d, &Tab::Nodes);
        assert_eq!(nodes.left(), 0.0);
        assert_eq!(nodes.right(), rect_of(&d, &Tab::Canvas).left());
        assert!(canvas_alone(&d));
        // Nodes with the canvas floating: a narrow column at the left.
        let mut d = default_dock(false);
        let p = d.find_tab(&Tab::Canvas).unwrap();
        d.detach_tab(p, Rect::from_min_size(Pos2::ZERO, FLOAT_SIZE));
        reveal(&mut d, Tab::Nodes);
        let nodes = rect_of(&d, &Tab::Nodes);
        assert_eq!(nodes.left(), 0.0);
        assert!(nodes.width() < 250.0, "{nodes:?}");
    }

    #[test]
    fn an_old_layout_loads_as_the_default() {
        // The layout as saved before the canvas was a tab: a bare dock.
        let mut old = DockState::new(vec![preview("r")]);
        old.main_surface_mut()
            .split_below(NodeIndex::root(), 0.5, vec![Tab::Help, Tab::Inspector]);
        let json = serde_json::to_string(&old).unwrap();
        assert_eq!(
            tabs(&parse_layout(&json, false)),
            tabs(&default_dock(false))
        );
        // With the Nodes panel showing, the default has its column.
        let with = parse_layout(&json, true);
        assert_eq!(tabs(&with), tabs(&default_dock(true)));
        assert!(with.find_tab(&Tab::Nodes).is_some());
        // Before previews were keyed by renderer id they were numbered.
        #[derive(Clone, Serialize)]
        enum Older {
            Preview(usize),
            Inspector,
        }
        let mut older = DockState::new(vec![Older::Preview(0), Older::Preview(1)]);
        older
            .main_surface_mut()
            .split_below(NodeIndex::root(), 0.5, vec![Older::Inspector]);
        let json = serde_json::to_string(&older).unwrap();
        assert_eq!(
            tabs(&parse_layout(&json, false)),
            tabs(&default_dock(false))
        );
        // Garbage too.
        assert_eq!(tabs(&parse_layout("{", false)), tabs(&default_dock(false)));
        assert_eq!(tabs(&parse_layout("", false)), tabs(&default_dock(false)));
        let unknown = r#"{"layout": 99, "dock": {}}"#;
        assert_eq!(
            tabs(&parse_layout(unknown, false)),
            tabs(&default_dock(false))
        );
    }

    #[test]
    fn a_layout_with_the_canvas_closed_stays_that_way() {
        let mut d = default_dock(false);
        let p = d.find_tab(&Tab::Canvas).unwrap();
        d.remove_tab(p);
        let back = parse_layout(&layout_json(&d).unwrap(), true);
        assert_eq!(tabs(&back), tabs(&d));
        assert!(back.find_tab(&Tab::Canvas).is_none());
        assert!(back.find_tab(&Tab::Nodes).is_none());
    }

    #[test]
    fn a_tab_floats_at_the_pointer_as_a_card() {
        let mut d = default_dock(false);
        let from = d.find_tab(&Tab::Help).unwrap();
        let at = Pos2::new(300.0, 300.0);
        let target = float_target(&d, from, &[], at);
        assert_eq!(target.size(), FLOAT_SIZE);
        assert!(target.contains(at));
        float_at(&mut d, from, target);
        let now = d.find_tab(&Tab::Help).unwrap();
        assert!(!now.surface.is_main());
        // The canvas floats like any panel.
        let from = d.find_tab(&Tab::Canvas).unwrap();
        let target = float_target(&d, from, &[], at);
        float_at(&mut d, from, target);
        assert!(!d.find_tab(&Tab::Canvas).unwrap().surface.is_main());
    }

    #[test]
    fn floating_a_windows_only_tab_moves_the_window() {
        let mut d = default_dock(false);
        let p = d.find_tab(&Tab::Help).unwrap();
        let s = d.detach_tab(p, Rect::from_min_size(pos2(50.0, 60.0), FLOAT_SIZE));
        let from = d.find_tab(&Tab::Help).unwrap();
        let windows = [(s, Rect::from_min_size(pos2(50.0, 60.0), vec2(300.0, 200.0)))];
        let to = pos2(500.0, 400.0);
        let target = float_target(&d, from, &windows, to);
        // The window keeps its size.
        assert_eq!(target.size(), vec2(300.0, 200.0));
        float_at(&mut d, from, target);
        assert_eq!(d.find_tab(&Tab::Help), Some(from), "same window");
        let v = window_json(&d, s).unwrap();
        assert_eq!(v["next_position"]["x"], f64::from(target.min.x));
        // A tab sharing a window gets a window of its own instead.
        let lib = d.find_tab(&Tab::Library).unwrap();
        d.remove_tab(lib);
        let home = d.find_tab(&Tab::Help).unwrap().node_path();
        d.leaf_mut(home).unwrap().append_tab(Tab::Library);
        let from = d.find_tab(&Tab::Library).unwrap();
        let target = float_target(&d, from, &windows, to);
        assert_eq!(target.size(), FLOAT_SIZE);
        float_at(&mut d, from, target);
        let now = d.find_tab(&Tab::Library).unwrap();
        assert_ne!(now.surface, from.surface);
        assert_eq!(d.find_tab(&Tab::Help).unwrap().surface, from.surface);
    }

    fn workspace() -> Workspace {
        Workspace::with_dock(default_dock(false), String::new())
    }

    #[test]
    fn every_panel_closes_and_reopens_from_the_panels_menu() {
        let mut w = workspace();
        let defaults: Vec<(Tab, bool)> = w.panels();
        assert_eq!(defaults[0], (Tab::Canvas, true));
        assert_eq!(defaults[1], (Tab::Nodes, false));
        assert_eq!(defaults[2], (preview(""), true));
        for (t, open) in &defaults[3..] {
            assert_eq!(*open, open_by_default(t), "{t:?}");
        }
        // Open the ones that start closed, so every panel is open.
        let closed: Vec<Tab> = PANELS.iter().filter(|t| !w.is_open(t)).cloned().collect();
        for t in closed {
            w.toggle_panel(t);
        }
        for t in PANELS {
            w.toggle_panel(t.clone());
            assert!(w.dock.find_tab(&t).is_none(), "{t:?} closed");
            assert!(w.panels().contains(&(t.clone(), false)));
        }
        // Reopened, each comes forward; the canvas has its pane to itself.
        for t in PANELS {
            w.toggle_panel(t.clone());
            let p = w.dock.find_tab(&t).expect("reopened");
            let leaf = w.dock.leaf(p.node_path()).unwrap();
            assert_eq!(leaf.tabs()[leaf.active.0], t);
        }
        assert!(w.panels().iter().all(|(_, open)| *open));
        assert!(canvas_alone(&w.dock));
        assert!(w.dock.find_tab(&Tab::Canvas).unwrap().surface.is_main());
    }

    #[test]
    fn a_closed_preview_stays_closed_until_reopened_or_a_new_patch() {
        let mut w = workspace();
        w.ids = ids(&["a", "b"]);
        sync_previews_except(&mut w.dock, &w.ids, &w.closed_previews);
        // Closed from its tab (the dock removes it, `on_close` records it).
        let p = w.dock.find_tab(&preview("a")).unwrap();
        w.dock.remove_tab(p);
        w.closed_previews.insert("a".into());
        sync_previews_except(&mut w.dock, &w.ids, &w.closed_previews);
        assert!(w.dock.find_tab(&preview("a")).is_none(), "not reopened");
        assert!(w.dock.find_tab(&preview("b")).is_some());
        assert!(w.panels().contains(&(preview(""), false)));
        // View → Panels → Preview brings it back.
        w.toggle_panel(preview(""));
        assert!(w.dock.find_tab(&preview("a")).is_some());
        assert!(w.panels().contains(&(preview(""), true)));
        // Unticked, every preview closes; a new patch brings them back.
        w.toggle_panel(preview(""));
        assert!(preview_ids(&w.dock).is_empty());
        assert!(canvas_alone(&w.dock));
        w.patch_loaded();
        sync_previews_except(&mut w.dock, &w.ids, &w.closed_previews);
        assert_eq!(preview_ids(&w.dock).len(), 2);
    }

    #[test]
    fn reset_layout_brings_everything_back() {
        let mut w = workspace();
        w.toggle_panel(Tab::Library);
        w.toggle_panel(Tab::Canvas);
        let p = w.dock.find_tab(&Tab::Help).unwrap();
        w.dock
            .detach_tab(p, Rect::from_min_size(pos2(0.0, 0.0), FLOAT_SIZE));
        w.reset();
        assert_eq!(tabs(&w.dock), tabs(&default_dock(false)));
        assert_eq!(w.dock.surfaces_count(), 1);
        assert_eq!(w.dock.translations.tab_context_menu.eject_button, "Float");
        // The Nodes palette stays open if it is.
        w.toggle_panel(Tab::Nodes);
        w.reset();
        assert_eq!(tabs(&w.dock), tabs(&default_dock(true)));
    }

    #[test]
    fn presets_comes_right_after_inspector_in_the_default_dock() {
        let t = tabs(&default_dock(false));
        let at = |tab: Tab| t.iter().position(|x| *x == tab).unwrap();
        assert_eq!(at(Tab::Presets), at(Tab::Inspector) + 1);
        assert!(at(Tab::Presets) < at(Tab::Profiler));
        // Every stack panel but Subpatches starts open, in `PANELS` order.
        let open: Vec<Tab> = PANELS
            .iter()
            .filter(|x| in_stack(x) && open_by_default(x))
            .cloned()
            .collect();
        assert_eq!(stack(&default_dock(false)), open);
        assert!(!t.contains(&Tab::Subpatches));
    }

    #[test]
    fn subpatches_opens_from_the_add_menu() {
        // It is a closeable panel, so the `+` menu (which lists closed
        // `PANELS`) offers it, and reveal opens it next to the others.
        let mut d = default_dock(false);
        assert!(PANELS.contains(&Tab::Subpatches));
        reveal(&mut d, Tab::Subpatches);
        let p = d.find_tab(&Tab::Subpatches).expect("opened");
        assert_eq!(
            Some(p.node_path()),
            d.find_tab(&Tab::Inspector).map(|p| p.node_path())
        );
        let leaf = d.leaf(p.node_path()).unwrap();
        assert_eq!(leaf.tabs()[leaf.active.0], Tab::Subpatches);
        assert_eq!(title(&Tab::Subpatches), "Subpatches");
    }

    #[test]
    fn show_presets_brings_its_tab_forward() {
        let mut w = workspace();
        w.show_presets();
        let p = w.dock.find_tab(&Tab::Presets).unwrap();
        let leaf = w.dock.leaf(p.node_path()).unwrap();
        assert_eq!(leaf.tabs()[leaf.active.0], Tab::Presets);
    }

    #[test]
    fn layout_round_trips_through_json() {
        // A fresh tree's rects are infinite; it still saves and loads.
        let mut d = default_dock(true);
        sync_previews(&mut d, &ids(&["a", "b"]));
        let p = d.find_tab(&Tab::Help).unwrap();
        d.detach_tab(p, Rect::from_min_size(pos2(40.0, 50.0), FLOAT_SIZE));
        let json = layout_json(&d).unwrap();
        assert!(json.starts_with(r#"{"dock":"#) || json.contains(r#""layout":2"#));
        let back = parse_layout(&json, false);
        assert_eq!(tabs(&back), tabs(&d));
    }

    /// The layout the user arranged is in `dock.json` (written by the
    /// workspace's own save) and is what the next launch opens with: the
    /// panels they closed stay closed, a floated panel is where they left it.
    #[test]
    fn the_layout_survives_a_restart() {
        let cfg = crate::test_env::config_dir("dock");
        let ctx = egui::Context::default();
        // First launch: the default layout, written once it is shown.
        let mut w = Workspace::load(false);
        assert_eq!(tabs(&w.dock), tabs(&default_dock(false)));
        w.save_layout(&ctx);
        assert!(cfg.file("dock.json").is_file());
        // The user floats Help and closes Profiler, then quits.
        let help = w.dock.find_tab(&Tab::Help).unwrap();
        w.dock
            .detach_tab(help, Rect::from_min_size(pos2(40.0, 50.0), FLOAT_SIZE));
        if let Some(p) = w.dock.find_tab(&Tab::Profiler) {
            w.dock.remove_tab(p);
        }
        w.save_layout(&ctx);
        let saved = tabs(&w.dock);
        assert!(!saved.contains(&Tab::Profiler));
        // The next launch.
        let next = Workspace::load(false);
        assert_eq!(tabs(&next.dock), saved);
        let p = next.dock.find_tab(&Tab::Help).unwrap();
        assert!(!p.surface.is_main(), "Help is still a floating window");
        assert!(next.dock.find_tab(&Tab::Profiler).is_none());
        assert_eq!(layout_json(&next.dock), layout_json(&w.dock));
        assert!(!cfg.file("dock.json.tmp").exists());
        // A damaged file opens the default layout, not a broken one.
        for bad in ["", "{", "{\"layout\":2,\"dock\":{}}", "[1]"] {
            std::fs::write(cfg.file("dock.json"), bad).unwrap();
            let w = Workspace::load(false);
            assert_eq!(tabs(&w.dock), tabs(&default_dock(false)), "{bad:?}");
        }
    }

    /// The dock with plain tab bodies, for running egui_dock headless.
    struct Plain;
    impl TabViewer for Plain {
        type Tab = Tab;
        fn id(&mut self, tab: &mut Tab) -> egui::Id {
            egui::Id::new(tab.clone())
        }
        fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
            title(tab).into()
        }
        fn ui(&mut self, ui: &mut egui::Ui, _tab: &mut Tab) {
            ui.label("body");
        }
    }

    fn raw() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
            ..Default::default()
        }
    }

    fn run_dock(ctx: &egui::Context, d: &mut DockState<Tab>) {
        ctx.run_ui(raw(), |ui| {
            DockArea::new(d)
                .id(dock_id())
                .style(style(ui.style(), false))
                .show_close_buttons(true)
                .show_inside(ui, &mut Plain);
        })
        .textures_delta
        .clear();
    }

    #[test]
    fn egui_dock_ids_are_what_we_expect() {
        // `tab_id`, `window_id` and `close_button_rect` copy egui_dock's
        // private ids and sizes (see "egui_dock internals"): an upgrade
        // that changes them fails here.
        let ctx = egui::Context::default();
        let mut d = default_dock(false);
        let p = d.find_tab(&Tab::Help).unwrap();
        d.detach_tab(p, Rect::from_min_size(pos2(50.0, 60.0), FLOAT_SIZE));
        for _ in 0..3 {
            run_dock(&ctx, &mut d);
        }
        for (path, tab) in d.iter_all_tabs() {
            let id = tab_id(dock_id(), path);
            let Some(title) = ctx.read_response(id) else {
                panic!("{tab:?}'s title button");
            };
            // Hidden tabs scroll out of the bar; the shown ones have their
            // close button where we paint its highlight.
            if let Some(close) = ctx.read_response(id.with("close-button")) {
                assert!(
                    (close.rect.min - close_button_rect(title.rect).min).length() < 0.5
                        && (close.rect.size() - close_button_rect(title.rect).size()).length()
                            < 0.5,
                    "{tab:?}: {:?} vs {:?}",
                    close.rect,
                    close_button_rect(title.rect)
                );
            } else {
                panic!("{tab:?}'s close button");
            }
        }
        let windows = window_rects(&d, &ctx);
        assert_eq!(windows.len(), 1);
        assert!(windows[0].1.width() > 100.0);
    }

    #[test]
    fn the_tab_under_a_windows_controls_is_its_top_right_panes_last() {
        let ctx = egui::Context::default();
        let mut d = default_dock(false);
        assert!(under_window_controls(&d).is_empty());
        let p = d.find_tab(&Tab::Help).unwrap();
        let s = d.detach_tab(p, Rect::from_min_size(pos2(50.0, 60.0), FLOAT_SIZE));
        // Before it's laid out: its only pane.
        assert_eq!(under_window_controls(&d), [Tab::Help]);
        // Split the window: Library on the right, Shader below Help.
        let home = d.find_tab(&Tab::Help).unwrap().node_path();
        let lib = d.find_tab(&Tab::Library).unwrap();
        d.move_tab(
            lib,
            egui_dock::TabDestination::Node(
                home,
                egui_dock::TabInsert::Split(egui_dock::Split::Right),
            ),
        );
        for _ in 0..3 {
            run_dock(&ctx, &mut d);
        }
        assert_eq!(d.find_tab(&Tab::Library).unwrap().surface, s);
        assert_eq!(under_window_controls(&d), [Tab::Library]);
        // A minimised window shows no tabs.
        set_minimized(&mut d, s, true, 300.0);
        assert!(under_window_controls(&d).is_empty());
    }

    #[test]
    fn floating_windows_minimise_restore_and_remember_where_they_are() {
        let ctx = egui::Context::default();
        let mut d = default_dock(false);
        let p = d.find_tab(&Tab::Help).unwrap();
        let s = d.detach_tab(p, Rect::from_min_size(pos2(50.0, 60.0), FLOAT_SIZE));
        for _ in 0..3 {
            run_dock(&ctx, &mut d);
        }
        let open = window_rects(&d, &ctx)[0].1;
        set_minimized(&mut d, s, true, open.height());
        assert!(is_minimized(&d, s));
        for _ in 0..3 {
            run_dock(&ctx, &mut d);
        }
        let small = window_rects(&d, &ctx)[0].1;
        assert!(
            small.height() < open.height() / 2.0,
            "{small:?} vs {open:?}"
        );
        set_minimized(&mut d, s, false, small.height());
        assert!(!is_minimized(&d, s));
        for _ in 0..3 {
            run_dock(&ctx, &mut d);
        }
        let back = window_rects(&d, &ctx)[0].1;
        assert!(
            (back.height() - open.height()).abs() < 2.0,
            "{back:?} vs {open:?}"
        );
        // Saved with its place and size, and reopened there.
        let mut saved = d.clone();
        remember_windows(&mut saved, &window_rects(&d, &ctx));
        let json = layout_json(&saved).unwrap();
        let mut again = parse_layout(&json, false);
        let fresh = egui::Context::default();
        for _ in 0..3 {
            run_dock(&fresh, &mut again);
        }
        let reopened = window_rects(&again, &fresh)[0].1;
        assert!(
            (reopened.min - back.min).length() < 2.0,
            "{reopened:?} vs {back:?}"
        );
        assert!(
            (reopened.size() - back.size()).length() < 4.0,
            "{reopened:?} vs {back:?}"
        );
    }

    /// What the app hands the workspace, for running it headless.
    struct Rig {
        patch: PatchFile,
        view: View,
        preview: PreviewPanel,
        shader: ShaderPanel,
        crash: crash::Guard,
        outputs: Outputs,
        stats: HashMap<usize, RegionStats>,
        engine: Option<Engine>,
    }

    impl Rig {
        fn new(patch: PatchFile) -> Rig {
            let mut rig = Rig {
                patch,
                view: View::default(),
                preview: PreviewPanel::new(None),
                shader: ShaderPanel::default(),
                crash: crash::Guard::default(),
                outputs: Vec::new(),
                stats: HashMap::new(),
                engine: None,
            };
            rig.view.on_load(&mut rig.patch);
            rig
        }

        /// One frame of the app's dock.
        fn frame(&mut self, ctx: &egui::Context, w: &mut Workspace) {
            ctx.run_ui(raw(), |ui| {
                w.show(
                    ui,
                    &mut Shell {
                        patch: &mut self.patch,
                        view: &mut self.view,
                        preview: &mut self.preview,
                        shader: &mut self.shader,
                        crash: &mut self.crash,
                        engine: self.engine.as_ref(),
                        outputs: &self.outputs,
                        region_stats: &self.stats,
                        warnings: &[],
                        errors: &[],
                        rs: None,
                        samples: &[],
                        sample: None,
                        welcome_open: false,
                    },
                );
            })
            .textures_delta
            .clear();
        }
    }

    fn headless() -> egui::Context {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        ctx
    }

    /// A saved snippet: one node.
    fn clip() -> PatchFile {
        use crate::editor::test_util::{node, patch};
        patch(vec![node("x", "math/add")], Vec::new())
    }

    #[test]
    fn library_inserts_and_focus_reach_a_hidden_canvas() {
        let ctx = headless();
        // The canvas stacked behind the Inspector.
        let mut w = workspace();
        stack_with_canvas(&mut w.dock, &Tab::Inspector);
        let mut rig = Rig::new(crate::app::empty_patch());
        rig.frame(&ctx, &mut w);
        assert!(!w.canvas_shown());
        rig.view.request_insert(clip());
        rig.frame(&ctx, &mut w);
        assert_eq!(rig.patch.nodes.len(), 1, "inserted while hidden");
        assert!(rig.view.take_recompile());
        let id = rig.patch.nodes[0].id.clone();
        rig.view.select_only("nothing");
        rig.view.request_focus(&id);
        rig.frame(&ctx, &mut w);
        assert_eq!(rig.view.primary(), Some(&id), "focused while hidden");
        // Closed altogether: still applied.
        w.toggle_panel(Tab::Canvas);
        rig.view.request_insert(clip());
        rig.frame(&ctx, &mut w);
        assert_eq!(rig.patch.nodes.len(), 2);
        assert!(!w.canvas_shown());
    }

    #[test]
    fn a_shown_canvas_runs_its_requests_once() {
        let ctx = headless();
        let mut w = workspace();
        let mut rig = Rig::new(crate::app::empty_patch());
        rig.engine = Some(Engine::from_patch(&rig.patch).unwrap().0);
        rig.frame(&ctx, &mut w);
        assert!(w.canvas_shown());
        rig.view.request_insert(clip());
        rig.frame(&ctx, &mut w);
        assert_eq!(rig.patch.nodes.len(), 1);
        rig.frame(&ctx, &mut w);
        assert_eq!(rig.patch.nodes.len(), 1);
        // It drew where the dock put it: the left of the window.
        assert!(rig.view.area().left() < 10.0 && rig.view.area().width() > 600.0);
    }
}
