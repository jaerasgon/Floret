//! The app shell's side dock (`src/editor/dock/*`): panels as tabs that
//! split, stack, float as windows and hide — one Preview pane per renderer,
//! Inspector, Help, Library, Shader, Profiler, Presets and Subpatches. The
//! node canvas stays pinned in the centre. A closed panel comes back from
//! View → Panels or the tab bar's `+` menu; the layout is saved to the app
//! data dir.

use crate::canvas::View;
use crate::library::{self, Library};
use crate::node_help::HelpPanel;
use crate::presets_panel::PresetsPanel;
use crate::preview::PreviewPanel;
use crate::shader_panel::ShaderPanel;
use crate::{crash, subpatches_panel, theme, Sample};
use eframe::egui::{self, pos2, vec2, Pos2, Rect, RichText, Vec2};
use eframe::egui_wgpu;
use egui_dock::{
    DockArea, DockState, LeafHighlighting, Node, NodeIndex, NodePath, OverlayType, Surface,
    SurfaceIndex, TabPath, TabViewer, Tree,
};
use egui_phosphor::regular as icons;
use floret_engine::patch::PatchFile;
use floret_engine::spec::Catalogue;
use floret_engine::tick::Outputs;
use floret_engine::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    /// A renderer's preview, keyed by the Renderer node's id, so a pane
    /// stays with its renderer when others come and go. `""` is the
    /// placeholder shown while the patch has no renderer.
    Preview(String),
    Inspector,
    Help,
    Library,
    Shader,
    Profiler,
    Presets,
    Subpatches,
}

/// Panels that can be closed and reopened, in their default order (the
/// View → Panels menu lists them in this order too).
/// Presets comes right after Inspector so the whole tab is on screen at the
/// default dock width, where the last tabs are cut off.
pub const PANELS: [Tab; 7] = [
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

/// Whether a tab can be closed: a preview goes away with its renderer
/// node, never by hand.
fn closeable(tab: &Tab) -> bool {
    !matches!(tab, Tab::Preview(_))
}

/// Whether a panel is in the default layout. Subpatches starts closed (the
/// tab bar is full at the default width); it opens from the `+` menu.
fn open_by_default(tab: &Tab) -> bool {
    *tab != Tab::Subpatches
}

/// The default layout: previews on top, the other panels tabbed below.
pub fn default_dock() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Preview(String::new())]);
    let panels = PANELS.iter().filter(|t| open_by_default(t)).cloned();
    dock.main_surface_mut()
        .split_below(NodeIndex::root(), 0.56, panels.collect());
    dock
}

/// A saved layout, else the default. Layouts from before previews were
/// keyed by renderer id (`{"Preview": 0}`) don't parse, so they give the
/// default too.
pub fn parse_layout(json: &str) -> DockState<Tab> {
    serde_json::from_str::<DockState<Tab>>(json).unwrap_or_else(|_| default_dock())
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

/// Make the dock hold exactly one preview tab per renderer id (`ids`, in
/// patch order), or the placeholder when there are none. A deleted
/// renderer's pane closes; a new renderer's preview opens as its own pane
/// under the other previews. When renderers are swapped wholesale (a new
/// patch loaded) the old panes are reused in order, keeping the layout.
pub fn sync_previews(dock: &mut DockState<Tab>, ids: &[String]) {
    let want: Vec<String> = if ids.is_empty() {
        vec![String::new()]
    } else {
        ids.to_vec()
    };
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

/// The main surface's preview panes, as one node: the node holding every
/// leaf with a preview in it, and whether it's a column of previews only.
fn preview_block(dock: &DockState<Tab>) -> Option<(usize, bool)> {
    let tree = dock.main_surface();
    let leaves: Vec<usize> = (0..tree.len())
        .filter(|&i| match node_at(tree, i) {
            Some(Node::Leaf(l)) => l.tabs().iter().any(|t| matches!(t, Tab::Preview(_))),
            _ => false,
        })
        .collect();
    let first = *leaves.first()?;
    let block = leaves.iter().fold(first, |a, &b| common_ancestor(a, b));
    Some((block, preview_column(tree, block)))
}

/// Open a new preview as its own pane: under the other previews (sharing
/// their column evenly), else above the panels.
fn add_preview(dock: &mut DockState<Tab>, tab: Tab) {
    let tree = dock.main_surface_mut();
    if tree.is_empty() {
        tree.push_to_first_leaf(tab);
        return;
    }
    let target = match preview_block(dock) {
        Some((block, true)) => NodeIndex(block),
        Some(_) => {
            // Previews share panes with other panels: split the last one.
            let tree = dock.main_surface();
            let last = (0..tree.len())
                .rev()
                .find(|&i| match node_at(tree, i) {
                    Some(Node::Leaf(l)) => l.tabs().iter().any(|t| matches!(t, Tab::Preview(_))),
                    _ => false,
                })
                .expect("a block has a leaf");
            NodeIndex(last)
        }
        None => {
            dock.main_surface_mut()
                .split_above(NodeIndex::root(), 0.44, vec![tab]);
            return;
        }
    };
    dock.main_surface_mut().split_below(target, 0.5, vec![tab]);
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

/// Open a closed panel (brought forward) or close an open one: the
/// View → Panels checkboxes.
pub fn toggle_panel(dock: &mut DockState<Tab>, tab: Tab) {
    match dock.find_tab(&tab) {
        Some(p) if closeable(&tab) => {
            dock.remove_tab(p);
        }
        Some(_) => {}
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
    pub rs: Option<&'a egui_wgpu::RenderState>,
    pub samples: &'a [Sample],
    pub sample: Option<usize>,
}

pub struct Workspace {
    dock: DockState<Tab>,
    help: HelpPanel,
    pub(crate) library: Library,
    presets: PresetsPanel,
    /// The layout as last saved.
    saved: String,
    /// Selected node last frame (a newly selected shader node brings the
    /// Shader tab forward).
    last_selected: Option<String>,
    /// The docked tab being dragged, and where it was.
    dragging: Option<(Tab, TabPath)>,
    /// The Profiler was on screen last frame.
    profiling: bool,
}

impl Workspace {
    /// The saved layout, else the default.
    pub fn load() -> Workspace {
        let saved = layout_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        Workspace::with_dock(parse_layout(&saved), saved)
    }

    fn with_dock(dock: DockState<Tab>, saved: String) -> Workspace {
        Workspace {
            dock: named(dock),
            help: HelpPanel::default(),
            library: Library::default(),
            presets: PresetsPanel::default(),
            saved,
            last_selected: None,
            dragging: None,
            profiling: false,
        }
    }

    /// Whether the engine should time its nodes (the Profiler is showing).
    pub fn profiling(&self) -> bool {
        self.profiling
    }

    /// Bring the Library (samples and saved patches) forward.
    pub fn show_library(&mut self) {
        reveal(&mut self.dock, Tab::Library);
    }

    /// Bring the Presets panel forward (a patch with presets was loaded).
    pub fn show_presets(&mut self) {
        reveal(&mut self.dock, Tab::Presets);
    }

    /// Every closeable panel and whether it's open (View → Panels).
    pub fn panels(&self) -> Vec<(Tab, bool)> {
        PANELS
            .iter()
            .map(|t| (t.clone(), self.dock.find_tab(t).is_some()))
            .collect()
    }

    /// Open or close a panel (View → Panels).
    pub fn toggle_panel(&mut self, tab: Tab) {
        toggle_panel(&mut self.dock, tab);
    }

    /// Put every panel back where it started.
    pub fn reset(&mut self) {
        self.dock = named(default_dock());
    }

    pub fn show(&mut self, ui: &mut egui::Ui, shell: &mut Shell) -> Option<ShellAction> {
        shell.preview.sync(shell.engine);
        let renderers = shell.preview.renderers();
        let ids: Vec<String> = renderers.iter().map(|(id, _)| id.clone()).collect();
        sync_previews(&mut self.dock, &ids);
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
        // A tab dragged off the dock (onto the canvas) floats as a card
        // where it's dropped; egui_dock only handles drops on a dock.
        let ctx = ui.ctx().clone();
        let held = self
            .dock
            .iter_all_tabs()
            .find(|(path, _)| ctx.is_being_dragged(tab_id(dock_id(), *path)))
            .map(|(path, tab)| (tab.clone(), path));
        let (released, pointer, moving) = ctx.input(|i| {
            (
                i.pointer.primary_released(),
                i.pointer.latest_pos(),
                i.pointer.is_decidedly_dragging(),
            )
        });
        let dock_rect = ui.max_rect();
        let windows = window_rects(&self.dock, &ctx);
        let over_window = pointer.is_some_and(|p| windows.iter().any(|(_, r)| r.contains(p)));
        let dragging = held.is_some() || self.dragging.is_some();
        let dropped = if released {
            self.dragging.take().or(held.clone())
        } else {
            self.dragging = held.clone();
            None
        };
        // Where a tab dragged over the canvas would land, as a card.
        if let (Some((tab, from)), Some(p), true) = (&held, pointer, moving) {
            if float_drop(&self.dock, tab, *from, dock_rect, &windows, p) {
                paint_float_preview(&ctx, float_target(&self.dock, *from, &windows, p));
            }
        }
        let mut viewer = Viewer {
            shell,
            help: &mut self.help,
            library: &mut self.library,
            presets: &mut self.presets,
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
            close: None,
            profiling: false,
            reveal: None,
            // Inside the dock a drop always docks: no stray floating
            // window when a tab is let go away from the drop targets.
            dock_only: dragging && pointer.is_some_and(|p| dock_rect.contains(p)) && !over_window,
        };
        DockArea::new(&mut self.dock)
            .id(dock_id())
            .style(style(ui.style()))
            .show_add_buttons(true)
            .show_add_popup(true)
            .show_leaf_collapse_buttons(false)
            .show_leaf_close_all_buttons(false)
            .show_close_buttons(false)
            // Tabs dragged out float anywhere over the window, as cards.
            .window_bounds(ui.ctx().content_rect())
            .show_inside(ui, &mut viewer);
        let (action, add, reset) = (viewer.action.take(), viewer.add.take(), viewer.reset);
        if let Some(p) = viewer.close.take().and_then(|t| self.dock.find_tab(&t)) {
            self.dock.remove_tab(p);
        }
        if let (Some((tab, from)), Some(p)) = (dropped, pointer) {
            if float_drop(&self.dock, &tab, from, dock_rect, &windows, p) {
                let rect = float_target(&self.dock, from, &windows, p);
                float_at(&mut self.dock, from, rect);
            }
        }
        self.profiling = viewer.profiling;
        if let Some(t) = viewer.reveal.take() {
            reveal(&mut self.dock, t);
        }
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
        match window_controls(&ctx, &self.dock, &window_rects(&self.dock, &ctx)) {
            Some(WindowAction::Close(s)) => {
                // The panels in it close; its previews (which only go
                // with their renderer) dock again under the others.
                self.dock.remove_surface(s);
                sync_previews(&mut self.dock, &ids);
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
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, &json);
        }
        self.saved = json;
    }
}

fn dock_id() -> egui::Id {
    egui::Id::new("floret-dock")
}
/// A tab floated by dragging it off the dock or from its menu.
const FLOAT_SIZE: Vec2 = Vec2::new(380.0, 320.0);

/// Where a tab floated at the pointer `p` goes.
fn float_rect(p: Pos2) -> Rect {
    Rect::from_min_size(p - vec2(24.0, 12.0), FLOAT_SIZE)
}

/// The card outline a tab dragged over the canvas would float as.
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
// The off-dock float, the floating-window controls and the saved window
// positions rely on two ids egui_dock 0.21.1 makes privately. They are not
// part of its API, so check them when upgrading egui_dock (the test
// `egui_dock_ids_are_what_we_expect` fails if they change):
// - a tab's title button: `DockArea::id().with((surface, "surface"))
//   .with((node, "node")).with((tab_index, "tab"))`
//   (`src/widgets/dock_area/show/leaf.rs`, `tabs`);
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

/// Floating windows and where egui last drew them.
fn window_rects(dock: &DockState<Tab>, ctx: &egui::Context) -> Vec<(SurfaceIndex, Rect)> {
    dock.iter_surfaces_indexed()
        .filter(|(_, s)| matches!(s, Surface::Window(..)))
        .filter_map(|(i, _)| Some((i, ctx.memory(|m| m.area_rect(window_id(i)))?)))
        .collect()
}

/// Whether a tab released at `p` should float there: it was let go away
/// from the docked panels and the floating windows (egui_dock only handles
/// drops on those), and egui_dock didn't move it.
pub fn float_drop(
    dock: &DockState<Tab>,
    tab: &Tab,
    from: TabPath,
    dock_rect: Rect,
    windows: &[(SurfaceIndex, Rect)],
    p: Pos2,
) -> bool {
    !dock_rect.contains(p)
        && !windows.iter().any(|(_, r)| r.contains(p))
        && dock.find_tab(tab) == Some(from)
}

/// The only tab of a floating window: dragging it moves the window.
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

/// The layout as JSON (see [`zero_nulls`]).
pub fn layout_json(dock: &DockState<Tab>) -> Option<String> {
    let mut v = serde_json::to_value(dock).ok()?;
    zero_nulls(&mut v);
    serde_json::to_string(&v).ok()
}

const TAB_BAR_HEIGHT: f32 = 26.0;

/// Floret's theme on the dock.
fn style(base: &egui::Style) -> egui_dock::Style {
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
    s.overlay.button_color = *theme::FG_1;
    // Dropping inside a pane never floats it (float by dragging onto the
    // canvas, or with the tab's Float menu item).
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
    /// (renderer id, title), in patch order.
    renderers: Vec<(String, String)>,
    selected: Option<String>,
    action: Option<ShellAction>,
    /// A closed panel reopened from a `+` menu.
    add: Option<(NodePath, Tab)>,
    /// Panels currently in the dock.
    open: Vec<Tab>,
    reset: bool,
    /// A panel closed from its tab (context menu, middle click).
    close: Option<Tab>,
    profiling: bool,
    /// A panel to bring forward after the dock ran (the Library, when a
    /// subpatch is being saved to it).
    reveal: Option<Tab>,
    /// A tab is being dragged over the docked panels: it can only dock.
    dock_only: bool,
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

    fn is_closeable(&self, tab: &Tab) -> bool {
        closeable(tab)
    }

    /// Tabs have no × (the dock hides close buttons): a closeable panel
    /// closes from its right-click menu or with a middle click. The menu's
    /// Float (egui_dock's own item) follows.
    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Tab, _path: NodePath) {
        if closeable(tab) && ui.button("Close").clicked() {
            self.close = Some(tab.clone());
            ui.close();
        }
    }

    fn on_tab_button(&mut self, tab: &mut Tab, response: &egui::Response) {
        if closeable(tab) && response.middle_clicked() {
            self.close = Some(tab.clone());
        }
    }

    /// Tabs float from their menu or by dragging them onto the canvas;
    /// while one is dragged over the dock, wherever it's let go it docks.
    fn allowed_in_windows(&self, _tab: &mut Tab) -> bool {
        !self.dock_only
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
        // Lay the tree out in a 1000 px tall rect and compare heights.
        let mut d = d.clone();
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 1000.0));
        layout(d.main_surface_mut(), 0, rect);
        let tree = d.main_surface();
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

    #[test]
    fn each_renderer_gets_its_own_preview_pane() {
        let mut d = default_dock();
        sync_previews(&mut d, &ids(&["r1"]));
        assert_eq!(preview_ids(&d), ids(&["r1"]));
        // The placeholder became r1's pane, where the default put it.
        assert_eq!(leaf_of(&d, &preview("r1")), Some(NodeIndex::root().left()));
        sync_previews(&mut d, &ids(&["r1", "r2", "r3", "r4", "r5"]));
        assert_stacked(&d, &["r1", "r2", "r3", "r4", "r5"]);
        // The other panels are untouched, below the previews.
        let panels: Vec<Tab> = tabs(&d).into_iter().filter(closeable).collect();
        assert_eq!(panels, default_panels());
    }

    #[test]
    fn a_deleted_renderer_closes_its_pane_and_the_others_keep_theirs() {
        let mut d = default_dock();
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
        let panels: Vec<Tab> = tabs(&d).into_iter().filter(closeable).collect();
        assert_eq!(panels, default_panels());
    }

    #[test]
    fn a_new_patch_reuses_the_preview_panes() {
        let mut d = default_dock();
        sync_previews(&mut d, &ids(&["a", "b"]));
        let at = [leaf_of(&d, &preview("a")), leaf_of(&d, &preview("b"))];
        sync_previews(&mut d, &ids(&["x", "y"]));
        assert_eq!(preview_ids(&d), ids(&["x", "y"]));
        assert_eq!([leaf_of(&d, &preview("x")), leaf_of(&d, &preview("y"))], at);
    }

    #[test]
    fn a_floating_preview_stays_floating_and_new_ones_dock() {
        let mut d = default_dock();
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
        let mut d = default_dock();
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

    #[test]
    fn an_old_layout_loads_as_the_default() {
        // Before previews were keyed by renderer id they were numbered.
        #[derive(Clone, Serialize)]
        enum Old {
            Preview(usize),
            Inspector,
            Help,
        }
        let mut old = DockState::new(vec![Old::Preview(0), Old::Preview(1)]);
        old.main_surface_mut()
            .split_below(NodeIndex::root(), 0.5, vec![Old::Inspector, Old::Help]);
        let json = serde_json::to_string(&old).unwrap();
        assert!(json.contains("\"Preview\":0"));
        assert_eq!(tabs(&parse_layout(&json)), tabs(&default_dock()));
        // Garbage too.
        assert_eq!(tabs(&parse_layout("{")), tabs(&default_dock()));
        assert_eq!(tabs(&parse_layout("")), tabs(&default_dock()));
    }

    #[test]
    fn a_tab_dropped_off_the_dock_floats() {
        let mut d = default_dock();
        let from = d.find_tab(&Tab::Help).unwrap();
        let dock = Rect::from_min_size(Pos2::new(1000.0, 0.0), vec2(400.0, 800.0));
        let canvas = Pos2::new(300.0, 300.0);
        assert!(float_drop(&d, &Tab::Help, from, dock, &[], canvas));
        let in_dock = Pos2::new(1100.0, 300.0);
        assert!(!float_drop(&d, &Tab::Help, from, dock, &[], in_dock));
        let target = float_target(&d, from, &[], canvas);
        assert_eq!(target.size(), FLOAT_SIZE);
        float_at(&mut d, from, target);
        let now = d.find_tab(&Tab::Help).unwrap();
        assert!(!now.surface.is_main());
        // Moved by the dock itself: left alone.
        assert!(!float_drop(&d, &Tab::Help, from, dock, &[], canvas));
        // Let go over a floating window: the dock handles it.
        let window = [(now.surface, Rect::from_min_size(canvas, FLOAT_SIZE))];
        let over = canvas + vec2(10.0, 10.0);
        assert!(!float_drop(&d, &Tab::Help, now, dock, &window, over));
    }

    #[test]
    fn dragging_a_windows_only_tab_onto_the_canvas_moves_the_window() {
        let mut d = default_dock();
        let p = d.find_tab(&Tab::Help).unwrap();
        let s = d.detach_tab(p, Rect::from_min_size(pos2(50.0, 60.0), FLOAT_SIZE));
        let from = d.find_tab(&Tab::Help).unwrap();
        let windows = [(s, Rect::from_min_size(pos2(50.0, 60.0), vec2(300.0, 200.0)))];
        let dock = Rect::from_min_size(Pos2::new(1000.0, 0.0), vec2(400.0, 800.0));
        let to = pos2(500.0, 400.0);
        assert!(float_drop(&d, &Tab::Help, from, dock, &windows, to));
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

    fn workspace() -> Workspace {
        Workspace::with_dock(default_dock(), String::new())
    }

    /// The panels the default layout opens (Subpatches starts closed).
    fn default_panels() -> Vec<Tab> {
        PANELS
            .iter()
            .filter(|t| open_by_default(t))
            .cloned()
            .collect()
    }

    #[test]
    fn the_panels_menu_opens_and_closes_panels() {
        let mut w = workspace();
        let defaults: Vec<(Tab, bool)> = PANELS
            .iter()
            .map(|t| (t.clone(), open_by_default(t)))
            .collect();
        assert_eq!(w.panels(), defaults);
        // Open the ones that start closed, so every panel is open.
        for t in PANELS.iter().filter(|t| !open_by_default(t)) {
            w.toggle_panel(t.clone());
        }
        for t in PANELS {
            w.toggle_panel(t.clone());
            assert!(w.dock.find_tab(&t).is_none(), "{t:?} closed");
            assert!(w.panels().contains(&(t.clone(), false)));
        }
        // Reopened, each comes forward.
        for t in PANELS {
            w.toggle_panel(t.clone());
            let p = w.dock.find_tab(&t).expect("reopened");
            let leaf = w.dock.leaf(p.node_path()).unwrap();
            assert_eq!(leaf.tabs()[leaf.active.0], t);
        }
        assert!(w.panels().iter().all(|(_, open)| *open));
        // A preview isn't a panel: toggling it does nothing.
        sync_previews(&mut w.dock, &ids(&["r"]));
        w.toggle_panel(preview("r"));
        assert!(w.dock.find_tab(&preview("r")).is_some());
    }

    #[test]
    fn reset_layout_brings_everything_back() {
        let mut w = workspace();
        w.toggle_panel(Tab::Library);
        let p = w.dock.find_tab(&Tab::Help).unwrap();
        w.dock
            .detach_tab(p, Rect::from_min_size(pos2(0.0, 0.0), FLOAT_SIZE));
        w.reset();
        assert_eq!(tabs(&w.dock), tabs(&default_dock()));
        assert_eq!(w.dock.surfaces_count(), 1);
        assert_eq!(w.dock.translations.tab_context_menu.eject_button, "Float");
    }

    #[test]
    fn presets_comes_right_after_inspector_in_the_default_dock() {
        let t = tabs(&default_dock());
        let at = |tab: Tab| t.iter().position(|x| *x == tab).unwrap();
        assert_eq!(at(Tab::Presets), at(Tab::Inspector) + 1);
        assert!(at(Tab::Presets) < at(Tab::Profiler));
        // Every panel but Subpatches starts open, in `PANELS` order.
        let panels: Vec<Tab> = t.iter().filter(|x| PANELS.contains(x)).cloned().collect();
        let open: Vec<Tab> = PANELS
            .iter()
            .filter(|x| open_by_default(x))
            .cloned()
            .collect();
        assert_eq!(panels, open);
        assert!(!t.contains(&Tab::Subpatches));
    }

    #[test]
    fn subpatches_opens_from_the_add_menu() {
        // It is a closeable panel, so the `+` menu (which lists closed
        // `PANELS`) offers it, and reveal opens it next to the others.
        let mut d = default_dock();
        assert!(PANELS.contains(&Tab::Subpatches) && closeable(&Tab::Subpatches));
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
        let mut d = default_dock();
        sync_previews(&mut d, &ids(&["a", "b"]));
        let p = d.find_tab(&Tab::Help).unwrap();
        d.detach_tab(p, Rect::from_min_size(pos2(40.0, 50.0), FLOAT_SIZE));
        let json = layout_json(&d).unwrap();
        let back: DockState<Tab> = serde_json::from_str(&json).unwrap();
        assert_eq!(tabs(&back), tabs(&d));
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

    fn run_dock(ctx: &egui::Context, d: &mut DockState<Tab>) {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
            ..Default::default()
        };
        ctx.run_ui(raw, |ui| {
            DockArea::new(d)
                .id(dock_id())
                .style(style(ui.style()))
                .show_inside(ui, &mut Plain);
        })
        .textures_delta
        .clear();
    }

    #[test]
    fn egui_dock_ids_are_what_we_expect() {
        // `tab_id` and `window_id` copy egui_dock's private ids (see
        // "egui_dock internals"): an upgrade that changes them fails here.
        let ctx = egui::Context::default();
        let mut d = default_dock();
        let p = d.find_tab(&Tab::Help).unwrap();
        d.detach_tab(p, Rect::from_min_size(pos2(50.0, 60.0), FLOAT_SIZE));
        for _ in 0..3 {
            run_dock(&ctx, &mut d);
        }
        for (path, tab) in d.iter_all_tabs() {
            assert!(
                ctx.read_response(tab_id(dock_id(), path)).is_some(),
                "{tab:?}'s title button"
            );
        }
        let windows = window_rects(&d, &ctx);
        assert_eq!(windows.len(), 1);
        assert!(windows[0].1.width() > 100.0);
    }

    #[test]
    fn floating_windows_minimise_restore_and_remember_where_they_are() {
        let ctx = egui::Context::default();
        let mut d = default_dock();
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
        let mut again = parse_layout(&json);
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
}
