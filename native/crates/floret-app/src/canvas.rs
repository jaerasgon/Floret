//! The node canvas: pan / zoom, nodes, pins, wires and live output values,
//! plus editing — wiring, selection, clipboard, undo / redo, context menus,
//! node search and favorites, region frames and subpatches, inline value
//! editing and auto-arrange. The edits themselves are plain functions in
//! `editor::*`; this file maps pointer and keyboard input onto them.
//! Layout and hit-testing live in `canvas/geom.rs`, painting in
//! `canvas/draw.rs`, engine read-outs in `canvas/live.rs`.

mod custom;
mod draw;
mod geom;
mod live;

pub use geom::node_size;
pub use live::Live;

use crate::editor::connect::{self, Conn};
use crate::editor::favorites::Favorites;
use crate::editor::history::History;
use crate::editor::popups::{
    self, ColorState, FavoritesState, MenuItem, Outcome, PortEditorState, Purpose, SearchState,
};
use crate::editor::ports::{self, type_name, Side};
use crate::editor::search::{self, Fit};
use crate::editor::values::{self, EditKey, FieldKind, LiveEdit};
use crate::editor::widgets::{self, Pad, SliderState};
use crate::editor::{clipboard, layout, ops, regions, subpatch};
use crate::theme::{self, BG_0, FG_2, FG_3, GOLD};
use custom::WidgetKind;
use draw::PinLook;
use eframe::egui::{
    self, pos2, vec2, CursorIcon, Key, Modifiers, PointerButton, Pos2, Rect, Sense, Shape, Stroke,
    Vec2,
};
use floret_engine::patch::{PatchFile, Position};
use floret_engine::spec::Catalogue;
use geom::{bezier, wire_points, Field, Hits, Look, NodeBox, PinRef, NODE_W, PIN_DROP, PIN_HIT};
use indexmap::IndexMap;
use serde_json::Value as Json;
use std::collections::{HashMap, HashSet};

/// A palette node type being dragged onto the canvas.
pub struct PaletteDrag(pub String);

enum Gesture {
    None,
    /// Dragging nodes; `start` holds each node's absolute position at
    /// press time.
    Move {
        ids: Vec<String>,
        start: Vec<Position>,
        origin: Pos2,
        /// Alt-drag: the moved nodes are fresh duplicates.
        duplicated: bool,
        moved: bool,
        /// Space was held at some point: drop re-parents into the region
        /// under the pointer (or out of all regions).
        reparent: bool,
    },
    /// Dragging a new wire out of a pin; `alt` = Alt held while dragging
    /// (drop → probe).
    Wire {
        from: PinRef,
        alt: bool,
    },
    /// Box-select from `origin`; `base` is the selection kept (shift).
    Marquee {
        origin: Pos2,
        base: Vec<String>,
    },
    Pan,
    /// Dragging a region frame's border / corner.
    Resize {
        id: String,
        dir: (i8, i8),
        start: Position,
        size: (f64, f64),
        origin: Pos2,
    },
    /// Dragging a frame-port pill along the border.
    PortDrag {
        region: String,
        port: String,
        start_y: f64,
        range: (f64, f64),
        origin: Pos2,
    },
    /// Scrubbing a number field.
    Scrub {
        field: Field,
        start: f64,
        origin_x: f32,
    },
    /// Drawing a new region with the armed tool.
    DrawRegion {
        origin: Pos2,
    },
    /// Pressing / dragging a custom-body control.
    Widget {
        node: String,
        rect: Rect,
        drag: WidgetDrag,
        origin: Pos2,
    },
}

/// What a control drag edits, with its value at press time.
#[derive(Clone, Debug)]
enum WidgetDrag {
    Slider(SliderState),
    Pad(Pad),
    /// A momentary button, held.
    Hold,
    /// A comment's corner, from its size at press time.
    Grip((f64, f64)),
}

#[derive(Clone, Debug)]
enum MenuTarget {
    Pane,
    Node(String),
    Edge(String),
    /// A dropdown field's options.
    Select {
        node: String,
        key: EditKey,
        options: Vec<String>,
        current: String,
    },
}

#[derive(Clone, Debug)]
enum Action {
    OpenSearch {
        screen: Pos2,
        at: Position,
    },
    OpenFavorites {
        screen: Pos2,
        at: Position,
    },
    PickSearch(String),
    PickFavorite(String),
    /// Palette drop / click: place a node, or arm a region.
    Place {
        ty: String,
        at: Position,
    },
    AddComment(Position),
    AddRegion {
        ty: String,
        min: Position,
        size: (f64, f64),
    },
    Copy,
    Cut,
    Paste(Position),
    PasteText(String, Position),
    Delete,
    Duplicate,
    SelectAll,
    DeleteEdge(String),
    ReorderEdge(String, i64),
    Connect(Conn),
    /// A wire dropped on a frame border: new port plus the wire.
    ConnectNewPort {
        from: PinRef,
        hit: regions::EdgeHit,
    },
    Probe {
        from: PinRef,
        at: Position,
    },
    /// Re-home a node: into a region, or out to the current scope.
    Reparent(String, Option<String>),
    Group,
    Enter(String),
    Exit,
    ViewDepth(usize),
    Focus(String),
    /// Edit a region's / subpatch's label in place.
    Rename(String),
    SetValue {
        node: String,
        key: EditKey,
        value: Json,
    },
    /// Record an edit already written live (colour picker).
    CommitValue {
        node: String,
        key: EditKey,
    },
    RenamePort {
        region: String,
        port: String,
        name: String,
    },
    /// Several params of one node in one undo step (file pickers).
    SetParams {
        node: String,
        values: Vec<(String, Json)>,
    },
    /// Open the OS file picker for an `io/file` / `io/svg` node.
    PickFile(String),
    /// Fire a bang (live param only: nothing to save or undo).
    Bang(String),
    /// Edit a comment's text in place.
    EditComment(String),
    Undo,
    Redo,
    Arrange,
    Fit,
}

#[derive(Clone)]
enum EditTarget {
    Field(Field),
    PortName { region: String, port: String },
}

/// A text field open over the canvas; `rect` is in canvas units so it
/// follows pan / zoom.
struct TextEditing {
    target: EditTarget,
    text: String,
    rect: Rect,
    first: bool,
}

/// Canvas camera, selection and editing state.
pub struct View {
    pub pan: Vec2,
    pub zoom: f32,
    /// Selected nodes, most recently selected last.
    selection: Vec<String>,
    selected_edge: Option<String>,
    /// Selected frame-port pills: (region, port).
    selected_ports: Vec<(String, String)>,
    /// Nodes raised above patch order (most recent last): selecting or
    /// dragging a node brings it to the front.
    raised: Vec<String>,
    gesture: Gesture,
    /// Snap dragged / placed nodes to the 24 px grid.
    pub snap: bool,
    /// Marching-ants flow animation on wires.
    pub wire_flow: bool,
    /// Grow a region while a child is dragged against its edge.
    pub auto_grow: bool,
    /// Entered subpatches, outermost first.
    stack: Vec<String>,
    /// Inputs whose inline editor is open: (node, port).
    expanded: HashSet<(String, String)>,
    pub favorites: Favorites,
    /// A region type armed by the palette / search: the next drag on the
    /// canvas draws it.
    armed: Option<String>,
    history: History,
    /// Last copied nodes (also mirrored to the system clipboard as JSON).
    clip: Option<PatchFile>,
    search: Option<SearchState>,
    favorites_menu: Option<FavoritesState>,
    menu: Option<(Pos2, Position, MenuTarget)>,
    editing: Option<TextEditing>,
    color: Option<ColorState>,
    port_editor: Option<PortEditorState>,
    /// Last pointer position over the canvas (paste / Tab anchor).
    last_pointer: Option<Pos2>,
    area: Rect,
    /// Text to put on the system clipboard at the end of the frame.
    copy_out: Option<String>,
    recompile: bool,
    live_edits: Vec<LiveEdit>,
    /// Actions raised by click handlers, applied with the frame's actions.
    queued: Vec<Action>,
    fit_pending: bool,
    /// Re-fit when the canvas resizes, until the camera is touched (the
    /// window settles over the first frames after a load).
    keep_fitted: bool,
    fitted_area: Rect,
    /// Bangs fired recently (node → time), for the flash.
    flashes: HashMap<String, f64>,
    /// Probe list scroll offsets (canvas units).
    probe_scroll: HashMap<String, f32>,
    /// Last frame's scrollable probe bodies: (node, rect, max scroll).
    probe_areas: Vec<(String, Rect, f32)>,
    /// Probe pictures by node id, set by the app each frame.
    pub thumbs: HashMap<String, egui::TextureId>,
    /// `ui.input().time` this frame.
    time: f64,
}

impl Default for View {
    fn default() -> Self {
        View {
            pan: vec2(40.0, 40.0),
            zoom: 1.0,
            selection: Vec::new(),
            selected_edge: None,
            selected_ports: Vec::new(),
            raised: Vec::new(),
            gesture: Gesture::None,
            snap: false,
            wire_flow: true,
            auto_grow: true,
            stack: Vec::new(),
            expanded: HashSet::new(),
            favorites: Favorites::load(),
            armed: None,
            history: History::default(),
            clip: None,
            search: None,
            favorites_menu: None,
            menu: None,
            editing: None,
            color: None,
            port_editor: None,
            last_pointer: None,
            area: Rect::ZERO,
            copy_out: None,
            recompile: false,
            live_edits: Vec::new(),
            queued: Vec::new(),
            fit_pending: true,
            keep_fitted: true,
            fitted_area: Rect::NOTHING,
            flashes: HashMap::new(),
            probe_scroll: HashMap::new(),
            probe_areas: Vec::new(),
            thumbs: HashMap::new(),
            time: 0.0,
        }
    }
}

impl View {
    /// The node the inspector shows: the most recently selected one.
    pub fn primary(&self) -> Option<&String> {
        self.selection.last()
    }

    pub fn select_only(&mut self, id: &str) {
        self.selection = vec![id.to_owned()];
        self.selected_edge = None;
        self.selected_ports.clear();
        self.raise(id);
    }

    /// The region type armed for drawing, if any.
    pub fn armed(&self) -> Option<&str> {
        self.armed.as_deref()
    }

    /// A new patch was loaded: settle it (orphan wires, `valueType`
    /// latch), seed undo with it, drop selection / overlays, and fit it.
    pub fn on_load(&mut self, patch: &mut PatchFile) {
        crate::editor::settle(patch, Catalogue::builtin());
        self.history.reset(patch);
        self.selection.clear();
        self.selected_edge = None;
        self.selected_ports.clear();
        self.raised.clear();
        self.stack.clear();
        self.expanded.clear();
        self.gesture = Gesture::None;
        self.discard_overlays();
        self.armed = None;
        self.fit_pending = true;
        self.keep_fitted = true;
    }

    /// Close every popup. An open text or colour edit is committed, as if
    /// it lost focus.
    fn close_overlays(&mut self) {
        if let Some(a) = self.editing.take().and_then(finish_edit) {
            self.queued.push(a);
        }
        if let Some(c) = self.color.take() {
            if values::as_rgba(&c.before) != popups::rgba_of(c.color) {
                self.queued.push(Action::CommitValue {
                    node: c.node,
                    key: c.key,
                });
            }
        }
        self.search = None;
        self.favorites_menu = None;
        self.menu = None;
        self.port_editor = None;
    }

    /// Drop every popup and pending edit (a new patch replaced the old).
    fn discard_overlays(&mut self) {
        self.editing = None;
        self.color = None;
        self.queued.clear();
        self.close_overlays();
    }

    /// Fit the visible patch on the next frame.
    pub fn request_fit(&mut self) {
        self.fit_pending = true;
    }

    /// Record a structural edit: settle, push an undo step, and ask the
    /// app to recompile the engine.
    pub fn commit(&mut self, patch: &mut PatchFile) {
        crate::editor::settle(patch, Catalogue::builtin());
        self.history.push(patch, None);
        self.recompile = true;
    }

    /// Whether the patch changed structurally since the last call.
    pub fn take_recompile(&mut self) -> bool {
        std::mem::take(&mut self.recompile)
    }

    /// Value edits to push into the running engine (no recompile).
    pub fn take_live_edits(&mut self) -> Vec<LiveEdit> {
        std::mem::take(&mut self.live_edits)
    }

    fn scope(&self) -> Option<&str> {
        self.stack.last().map(String::as_str)
    }

    /// A palette click: arm a region, or add the node mid-canvas.
    pub fn palette_pick(&mut self, patch: &mut PatchFile, cat: &Catalogue, ty: &str) {
        let c = self.to_canvas(self.area.center());
        let at = Position {
            x: c.x - f64::from(NODE_W) / 2.0,
            y: c.y - 30.0,
        };
        apply(
            Action::Place {
                ty: ty.to_owned(),
                at,
            },
            patch,
            cat,
            self,
        );
    }

    fn raise(&mut self, id: &str) {
        self.raised.retain(|r| r != id);
        self.raised.push(id.to_owned());
    }

    fn is_selected(&self, id: &str) -> bool {
        self.selection.iter().any(|s| s == id)
    }

    /// Non-frame nodes back-to-front: patch order, raised nodes last.
    fn draw_order(&self, boxes: &IndexMap<String, NodeBox>) -> Vec<String> {
        let flat = |id: &String| boxes.get(id).is_some_and(|b| !b.look.is_frame());
        let mut order: Vec<String> = boxes
            .keys()
            .filter(|id| flat(id) && !self.raised.contains(id))
            .cloned()
            .collect();
        order.extend(self.raised.iter().filter(|r| flat(r)).cloned());
        order
    }

    fn to_canvas(&self, p: Pos2) -> Position {
        let c = (p - self.area.min) / self.zoom - self.pan;
        Position {
            x: f64::from(c.x),
            y: f64::from(c.y),
        }
    }

    fn snapped(&self, p: Position) -> Position {
        if self.snap {
            Position {
                x: ops::snap(p.x),
                y: ops::snap(p.y),
            }
        } else {
            p
        }
    }

    /// Fit the visible patch into `area`.
    pub fn fit(&mut self, patch: &PatchFile, area: Rect) {
        let cat = Catalogue::builtin();
        let visible = subpatch::visible(patch, cat, &self.stack);
        self.fit_nodes(patch, area, |id| visible.contains(id));
    }

    fn fit_nodes(&mut self, patch: &PatchFile, area: Rect, keep: impl Fn(&str) -> bool) {
        let cat = Catalogue::builtin();
        let scope = self.scope().map(str::to_owned);
        let mut bb = Rect::NOTHING;
        for n in patch.nodes.iter().filter(|n| keep(&n.id)) {
            let p = ops::absolute_pos(patch, &n.id);
            let look = geom::look_of(n, cat, scope.as_deref());
            let (w, h) = geom::box_size(n, cat, look);
            let top = if look.is_frame() {
                regions::HEADER_H
            } else {
                0.0
            };
            bb = bb.union(Rect::from_min_max(
                pos2(p.x as f32, (p.y - top) as f32),
                pos2((p.x + w) as f32, (p.y + h) as f32),
            ));
        }
        if !bb.is_finite() || bb.width() <= 0.0 {
            self.pan = vec2(40.0, 40.0);
            self.zoom = 1.0;
            return;
        }
        let margin = 48.0 + 0.04 * area.width().min(area.height());
        self.zoom = ((area.width() - 2.0 * margin) / bb.width())
            .min((area.height() - 2.0 * margin) / bb.height())
            .clamp(0.1, 1.25);
        let content = bb.size() * self.zoom;
        let offset = (area.size() - content) * 0.5;
        self.pan = offset / self.zoom - bb.min.to_vec2();
    }

    /// Selected ids without those whose region is also selected (a region
    /// moves / copies its contents already).
    fn top_level_selection(&self, patch: &PatchFile) -> Vec<String> {
        self.selection
            .iter()
            .filter(|id| {
                let mut cur = ports::find_node(patch, id).and_then(|n| n.parent_id.clone());
                while let Some(p) = cur {
                    if self.is_selected(&p) {
                        return false;
                    }
                    cur = ports::find_node(patch, &p).and_then(|n| n.parent_id.clone());
                }
                true
            })
            .cloned()
            .collect()
    }

    fn prune_selection(&mut self, patch: &PatchFile) {
        let ids: HashSet<&str> = patch.nodes.iter().map(|n| n.id.as_str()).collect();
        self.selection.retain(|s| ids.contains(s.as_str()));
        self.raised.retain(|s| ids.contains(s.as_str()));
        self.selected_ports.retain(|(r, p)| {
            ports::find_node(patch, r).is_some_and(|n| n.ports.iter().flatten().any(|x| &x.id == p))
        });
        if let Some(e) = &self.selected_edge {
            if !patch.edges.iter().any(|x| &x.id == e) {
                self.selected_edge = None;
            }
        }
        let stack = subpatch::valid_stack(patch, Catalogue::builtin(), &self.stack);
        if stack != self.stack {
            self.stack = stack;
            self.fit_pending = true;
        }
    }

    /// Record a value edit: params coalesce in undo and stay live (unless
    /// they reshape the node's inputs); port values recompile.
    fn commit_value(&mut self, patch: &mut PatchFile, cat: &Catalogue, node: &str, key: &EditKey) {
        // Dynamic inputs reshape the node; constant pins read their value
        // at compile. Either way the engine needs a rebuild.
        let constant = |p: &floret_engine::spec::PortSpec| {
            p.constant && matches!(key, EditKey::Param(k) if p.param.as_ref() == Some(k))
        };
        let reshapes = ports::find_node(patch, node).is_some_and(|n| {
            cat.get(&n.def_type).is_some_and(|s| {
                s.has_dynamic_inputs || cat.resolved_inputs(s, &n.params).iter().any(constant)
            })
        });
        match key {
            EditKey::Param(k) if !reshapes => {
                self.history.push(patch, Some(&format!("{node}.{k}")));
            }
            EditKey::Param(k) => {
                values::prune_dead_inputs(patch, cat, node);
                crate::editor::settle(patch, cat);
                self.history.push(patch, Some(&format!("{node}.{k}")));
                self.recompile = true;
            }
            EditKey::Port(p) => {
                crate::editor::settle(patch, cat);
                self.history.push(patch, Some(&format!("{node}.port.{p}")));
                self.recompile = true;
            }
        }
    }

    fn live(&mut self, node: &str, key: &EditKey, value: Json) {
        self.live_edits.push(match key {
            EditKey::Param(k) => LiveEdit::Param {
                node: node.to_owned(),
                key: k.clone(),
                value,
            },
            EditKey::Port(p) => LiveEdit::Port {
                node: node.to_owned(),
                port: p.clone(),
                value,
            },
        });
    }

    fn canvas_rect(&self, r: Rect) -> Rect {
        let min = self.to_canvas(r.min);
        Rect::from_min_size(pos2(min.x as f32, min.y as f32), r.size() / self.zoom)
    }

    fn overlay_open(&self) -> bool {
        self.search.is_some()
            || self.favorites_menu.is_some()
            || self.menu.is_some()
            || self.editing.is_some()
            || self.color.is_some()
            || self.port_editor.is_some()
    }
}

fn conn_between(a: &PinRef, b: &PinRef) -> Option<Conn> {
    Conn::between((&a.node, a.side, &a.port), (&b.node, b.side, &b.port))
}

/// Visible drawable regions (ForEach / If / Cache), the reparent targets.
fn drawable_ids(boxes: &IndexMap<String, NodeBox>) -> Vec<&str> {
    boxes
        .iter()
        .filter(|(_, b)| b.look == Look::Region)
        .map(|(id, _)| id.as_str())
        .collect()
}

pub fn show(
    ui: &mut egui::Ui,
    patch: &mut PatchFile,
    live: &Live,
    cat: &Catalogue,
    view: &mut View,
) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let area = resp.rect;
    view.area = area;
    view.time = ui.input(|i| i.time);
    let origin = area.min;
    let ctx = ui.ctx().clone();
    painter.rect_filled(area, 0.0, BG_0);

    let visible = subpatch::visible(patch, cat, &view.stack);
    if view.fit_pending || (view.keep_fitted && area != view.fitted_area) {
        view.fit_nodes(patch, area, |id| visible.contains(id));
        view.fit_pending = false;
        view.fitted_area = area;
    }

    // ── Camera: two-finger scroll pans, pinch / ⌘-scroll zooms around the pointer.
    if resp.hovered() {
        let (scroll, zoom_delta, pointer) =
            ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
        if zoom_delta != 1.0 {
            if let Some(p) = pointer {
                let before = (p - origin) / view.zoom - view.pan;
                view.zoom = (view.zoom * zoom_delta).clamp(0.1, 3.0);
                view.pan = (p - origin) / view.zoom - before;
                view.keep_fitted = false;
            }
        } else if scroll != Vec2::ZERO {
            let probe = pointer.and_then(|p| {
                view.probe_areas
                    .iter()
                    .find(|(_, r, _)| r.contains(p))
                    .cloned()
            });
            match probe {
                // Over a scrollable probe list: scroll it, not the canvas.
                Some((id, _, max)) => {
                    let off = view.probe_scroll.entry(id).or_insert(0.0);
                    *off = (*off - scroll.y / view.zoom).clamp(0.0, max);
                }
                None => {
                    view.pan += scroll / view.zoom;
                    view.keep_fitted = false;
                }
            }
        }
    }

    // ── Dot grid (24 px — the snap grid).
    let step = 24.0 * view.zoom;
    if step > 6.0 {
        let start = origin
            + vec2(
                (view.pan.x * view.zoom).rem_euclid(step),
                (view.pan.y * view.zoom).rem_euclid(step),
            );
        // Subtle, and fading out as the grid gets dense when zoomed out.
        let fade = ((step - 6.0) / 10.0).clamp(0.0, 1.0);
        let dot = FG_3.gamma_multiply(if view.snap { 0.5 } else { 0.28 } * fade);
        let radius = 0.8 * view.zoom.clamp(0.6, 1.2);
        let mut y = start.y;
        while y < area.bottom() {
            let mut x = start.x;
            while x < area.right() {
                painter.circle_filled(pos2(x, y), radius, dot);
                x += step;
            }
            y += step;
        }
    }

    let z = view.zoom;
    let scope = view.scope().map(str::to_owned);
    let decor = live.decor(patch, cat, &visible);
    let boxes = {
        let gx = geom::Ctx {
            patch,
            cat,
            painter: &painter,
            origin,
            pan: view.pan,
            zoom: z,
            scope: scope.as_deref(),
            expanded: &view.expanded,
            decor: &decor,
        };
        let unimplemented = |id: &str| {
            live.engine
                .graph
                .idx(id)
                .is_some_and(|i| live.engine.graph.nodes[i].unimplemented)
        };
        geom::layout(&gx, &visible, &unimplemented)
    };
    let order = view.draw_order(&boxes);
    let wire_list = geom::wires(patch, &boxes, z);
    let hits = Hits {
        boxes: &boxes,
        order: &order,
        wires: &wire_list,
    };
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let mods = ui.input(|i| i.modifiers);
    let typing = ctx.text_edit_focused();
    let space = !typing && ui.input(|i| i.key_down(Key::Space));
    if let Some(p) = pointer.filter(|p| area.contains(*p)) {
        view.last_pointer = Some(p);
    }
    let mut actions: Vec<Action> = Vec::new();

    // ── Palette drag-to-add.
    let palette_hover = resp.dnd_hover_payload::<PaletteDrag>();
    if let Some(drop) = resp.dnd_release_payload::<PaletteDrag>() {
        if let Some(p) = pointer {
            actions.push(Action::Place {
                ty: drop.0.clone(),
                at: view.to_canvas(p),
            });
        }
    }

    // ── Custom-body controls: they act on press, not after a drag threshold.
    let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
    if primary_pressed
        && resp.hovered()
        && view.armed.is_none()
        && matches!(view.gesture, Gesture::None)
    {
        if let Some(w) = pointer
            .and_then(|p| hits.widget(p))
            .filter(|w| w.kind.drags())
        {
            let drag = match &w.kind {
                WidgetKind::Slider(st) => Some(WidgetDrag::Slider(*st)),
                WidgetKind::Pad(pad) => Some(WidgetDrag::Pad(*pad)),
                WidgetKind::Hold => Some(WidgetDrag::Hold),
                WidgetKind::Grip => ports::find_node(patch, &w.node).map(|n| {
                    let size = geom::box_size(n, cat, Look::Comment);
                    WidgetDrag::Grip(size)
                }),
                _ => None,
            };
            if let (Some(drag), Some(p)) = (drag, pointer) {
                if matches!(drag, WidgetDrag::Hold) {
                    view.live(&w.node, &EditKey::Param("value".into()), Json::Bool(true));
                }
                view.gesture = Gesture::Widget {
                    node: w.node.clone(),
                    rect: w.rect,
                    drag,
                    origin: p,
                };
            }
        }
    }
    if let Gesture::Widget {
        node,
        rect,
        drag,
        origin,
    } = &view.gesture
    {
        let (node, rect, drag, origin) = (node.clone(), *rect, drag.clone(), *origin);
        if ui.input(|i| i.pointer.primary_down()) {
            if let Some(p) = pointer {
                widget_drag(view, patch, &node, rect, &drag, origin, p, mods.shift);
            }
        } else {
            view.gesture = Gesture::None;
            match drag {
                WidgetDrag::Slider(_) => {
                    view.commit_value(patch, cat, &node, &EditKey::Param("value".into()));
                }
                WidgetDrag::Pad(_) => {
                    view.commit_value(patch, cat, &node, &EditKey::Param("x".into()));
                }
                WidgetDrag::Hold => {
                    view.live(&node, &EditKey::Param("value".into()), Json::Bool(false));
                }
                WidgetDrag::Grip(_) => view.history.push(patch, None),
            }
        }
    }

    // ── Gestures.
    if resp.drag_started_by(PointerButton::Primary)
        && !matches!(view.gesture, Gesture::Widget { .. })
    {
        let start = ui.input(|i| i.pointer.press_origin()).or(pointer);
        if let Some(start) = start {
            view.selected_edge = None;
            if view.armed.is_some()
                && hits.pin(start, PIN_HIT, None).is_none()
                && hits.body(start).is_none()
            {
                view.gesture = Gesture::DrawRegion { origin: start };
            } else if let Some(pin) = hits.pin(start, PIN_HIT, None) {
                view.gesture = Gesture::Wire {
                    from: pin,
                    alt: mods.alt,
                };
            } else if let Some(f) = hits.field(start) {
                if matches!(f.kind, FieldKind::Number { .. } | FieldKind::Vec2) {
                    let v = match f.axis {
                        Some(a) => {
                            let (x, y) = values::as_vec2(&f.value);
                            [x, y][a.min(1)]
                        }
                        None => values::as_f64(&f.value),
                    };
                    view.gesture = Gesture::Scrub {
                        field: f.clone(),
                        start: v,
                        origin_x: start.x,
                    };
                }
            } else if let Some((region, port)) = hits.pill(start) {
                if let Some(pl) = boxes[region.as_str()].pills.iter().find(|p| p.port == port) {
                    let y = f64::from((pl.rect.center().y - boxes[region.as_str()].rect.top()) / z);
                    view.gesture = Gesture::PortDrag {
                        region: region.clone(),
                        port,
                        start_y: y,
                        range: (pl.min_y, pl.max_y),
                        origin: start,
                    };
                }
            } else if let Some(id) = hits.node(start) {
                if !view.is_selected(&id) {
                    if mods.shift {
                        view.selection.push(id.clone());
                    } else {
                        view.selection = vec![id.clone()];
                    }
                }
                view.selected_ports.clear();
                view.raise(&id);
                let mut ids = view.top_level_selection(patch);
                let duplicated = mods.alt;
                if duplicated {
                    // Alt-drag: duplicates follow the pointer, originals stay.
                    ids = ops::duplicate(patch, &ids, 0.0, 0.0);
                    view.selection = ids.clone();
                    for i in &ids {
                        view.raise(i);
                    }
                }
                let start_pos = ids.iter().map(|i| ops::absolute_pos(patch, i)).collect();
                view.gesture = Gesture::Move {
                    ids,
                    start: start_pos,
                    origin: start,
                    duplicated,
                    moved: false,
                    reparent: space,
                };
            } else if let Some((id, dx, dy)) = hits.resize(start) {
                if let Some(n) = ports::find_node(patch, &id) {
                    view.gesture = Gesture::Resize {
                        size: ops::region_size(n),
                        start: ops::absolute_pos(patch, &id),
                        id,
                        dir: (dx, dy),
                        origin: start,
                    };
                }
            } else if mods.alt {
                view.gesture = Gesture::Pan;
            } else {
                let base = if mods.shift {
                    view.selection.clone()
                } else {
                    Vec::new()
                };
                view.gesture = Gesture::Marquee {
                    origin: start,
                    base,
                };
            }
        }
    } else if resp.drag_started_by(PointerButton::Middle) {
        view.gesture = Gesture::Pan;
    }

    let mut hover_region: Option<String> = None;
    if resp.dragged() {
        let (snap, auto_grow) = (view.snap, view.auto_grow);
        match &mut view.gesture {
            Gesture::Pan => {
                view.pan += resp.drag_delta() / z;
                view.keep_fitted = false;
            }
            Gesture::Move {
                ids,
                start,
                origin: o,
                moved,
                reparent,
                ..
            } => {
                *reparent |= space;
                if let Some(p) = pointer {
                    let d = (p - *o) / z;
                    *moved = true;
                    for (id, s) in ids.iter().zip(start.iter()) {
                        let mut t = Position {
                            x: s.x + f64::from(d.x),
                            y: s.y + f64::from(d.y),
                        };
                        if snap {
                            t = Position {
                                x: ops::snap(t.x),
                                y: ops::snap(t.y),
                            };
                        }
                        let parent = ports::find_node(patch, id).and_then(|n| n.parent_id.clone());
                        let base = parent
                            .map(|p| ops::absolute_pos(patch, &p))
                            .unwrap_or_default();
                        if let Some(n) = ports::find_node_mut(patch, id) {
                            n.position = Position {
                                x: t.x - base.x,
                                y: t.y - base.y,
                            };
                        }
                    }
                    if *reparent {
                        hover_region = regions::containing(
                            patch,
                            drawable_ids(&boxes),
                            view_to_canvas(area, view.pan, z, p),
                            ids,
                        );
                    } else if auto_grow {
                        for id in ids.iter() {
                            // Regions, and the entered subpatch's frame.
                            let in_frame = ports::find_node(patch, id)
                                .and_then(|n| n.parent_id.as_deref())
                                .is_some_and(|p| {
                                    regions::is_drawable_node(patch, cat, p)
                                        || scope.as_deref() == Some(p)
                                });
                            if !in_frame {
                                continue;
                            }
                            let size = ports::find_node(patch, id)
                                .map(|n| {
                                    geom::box_size(n, cat, geom::look_of(n, cat, scope.as_deref()))
                                })
                                .unwrap_or_default();
                            regions::auto_grow(patch, id, size, ids);
                        }
                    }
                }
            }
            Gesture::Marquee { origin: o, base } => {
                if let Some(p) = pointer {
                    let r = Rect::from_two_pos(*o, p);
                    let mut sel = base.clone();
                    for (id, b) in &boxes {
                        if b.look != Look::Frame && r.contains_rect(b.rect) && !sel.contains(id) {
                            sel.push(id.clone());
                        }
                    }
                    view.selection = sel;
                }
            }
            Gesture::Wire { alt, .. } => *alt = mods.alt,
            Gesture::Resize {
                id,
                dir,
                start,
                size,
                origin: o,
            } => {
                if let Some(p) = pointer {
                    let d = (p - *o) / z;
                    let (dx, dy) = (f64::from(d.x), f64::from(d.y));
                    let edge = |dir: i8, pos: f64, len: f64, delta: f64, min: f64| match dir {
                        -1 => {
                            let dd = delta.min(len - min);
                            (pos + dd, len - dd)
                        }
                        1 => (pos, (len + delta).max(min)),
                        _ => (pos, len),
                    };
                    let (x, w) = edge(dir.0, start.x, size.0, dx, regions::MIN_W);
                    let (y, h) = edge(dir.1, start.y, size.1, dy, regions::MIN_H);
                    regions::resize(patch, id, Position { x, y }, w, h);
                }
            }
            Gesture::PortDrag {
                region,
                port,
                start_y,
                range,
                origin: o,
            } => {
                if let Some(p) = pointer {
                    let y = (*start_y + f64::from((p.y - o.y) / z)).clamp(range.0, range.1);
                    regions::set_port_y(patch, region, port, y);
                }
            }
            Gesture::Scrub {
                field,
                start,
                origin_x,
            } => {
                if let Some(p) = pointer {
                    let integer = matches!(field.kind, FieldKind::Number { integer: true });
                    let v = if field.step == 1.0 {
                        values::scrub(
                            *start,
                            f64::from(p.x - *origin_x),
                            mods.shift,
                            mods.alt,
                            integer,
                        )
                    } else {
                        let d = values::scrub(
                            0.0,
                            f64::from(p.x - *origin_x),
                            mods.shift,
                            mods.alt,
                            false,
                        );
                        ((*start + d * field.step) * 1e6).round() / 1e6
                    };
                    let value = match field.axis {
                        Some(a) => {
                            let (x, y) = values::as_vec2(&field.value);
                            let (x, y) = if a == 0 { (v, y) } else { (x, v) };
                            serde_json::json!({ "x": x, "y": y })
                        }
                        None => serde_json::json!(v),
                    };
                    let (node, key) = (field.node.clone(), field.key.clone());
                    if values::set(patch, &node, &key, value.clone()) {
                        view.live(&node, &key, value);
                    }
                }
            }
            Gesture::DrawRegion { .. } | Gesture::Widget { .. } | Gesture::None => {}
        }
    }

    // Wire-drag affinity: which pins would accept the dragged wire.
    let affinity: Option<(PinRef, Vec<PinMatch>)> = match &view.gesture {
        Gesture::Wire { from, .. } => Some((
            from.clone(),
            hits.all_pins()
                .into_iter()
                .map(|(r, pos)| {
                    let ok = conn_between(from, &r)
                        .is_some_and(|c| connect::check_connection(patch, cat, &c, true));
                    (r, pos, ok)
                })
                .collect(),
        )),
        _ => None,
    };
    let drop_target = |p: Pos2| -> Option<PinRef> {
        let (_, pins) = affinity.as_ref()?;
        pins.iter()
            .filter(|(_, _, ok)| *ok)
            .map(|(r, pos, _)| (r, pos.distance(p)))
            .filter(|(_, d)| *d <= PIN_DROP)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(r, _)| r.clone())
    };
    let frames: Vec<&str> = boxes
        .iter()
        .filter(|(_, b)| b.look.is_frame())
        .map(|(id, _)| id.as_str())
        .collect();
    // A wire hovering a frame border adds a port there on release.
    let edge_drop = |from: &PinRef, p: Pos2| -> Option<regions::EdgeHit> {
        regions::edge_at(
            patch,
            frames.iter().copied(),
            view_to_canvas(area, view.pan, z, p),
        )
        .filter(|h| h.region != from.node)
    };
    let pending_edge = match (&affinity, pointer) {
        (Some((from, _)), Some(p)) if drop_target(p).is_none() => edge_drop(from, p),
        _ => None,
    };

    if resp.drag_stopped() {
        match std::mem::replace(&mut view.gesture, Gesture::None) {
            Gesture::Move {
                ids,
                duplicated,
                moved,
                reparent,
                ..
            } => {
                if reparent && moved {
                    let target = pointer.and_then(|p| {
                        regions::containing(patch, drawable_ids(&boxes), view.to_canvas(p), &ids)
                    });
                    let target = target.or_else(|| scope.clone());
                    let mut changed = false;
                    let movable: Vec<&String> = ids
                        .iter()
                        .filter(|id| !regions::is_drawable_node(patch, cat, id))
                        .collect();
                    for id in movable {
                        changed |= regions::set_parent(patch, id, target.as_deref());
                    }
                    if changed || duplicated {
                        view.commit(patch);
                    } else {
                        view.history.push(patch, None);
                    }
                } else if duplicated {
                    view.commit(patch);
                } else if moved {
                    view.history.push(patch, None);
                }
            }
            Gesture::Wire { from, alt } => {
                if let Some(p) = pointer {
                    if let Some(to) = drop_target(p) {
                        if let Some(c) = conn_between(&from, &to) {
                            actions.push(Action::Connect(c));
                        }
                    } else if let Some(hit) = edge_drop(&from, p) {
                        actions.push(Action::ConnectNewPort { from, hit });
                    } else if hits.node(p).is_none() && hits.pin(p, PIN_HIT, None).is_none() {
                        let at = view.to_canvas(p);
                        if alt || mods.alt {
                            actions.push(Action::Probe { from, at });
                        } else {
                            view.search = Some(SearchState::new(
                                p,
                                at,
                                Fit::Pin {
                                    side: from.side,
                                    ty: from.ty,
                                },
                                Purpose::Wire {
                                    node: from.node.clone(),
                                    side: from.side,
                                    port: from.port.clone(),
                                },
                            ));
                        }
                    }
                }
            }
            Gesture::Resize { .. } | Gesture::PortDrag { .. } => view.history.push(patch, None),
            Gesture::Scrub { field, .. } => {
                clamp_slider(view, patch, &field.node);
                view.commit_value(patch, cat, &field.node, &field.key);
            }
            Gesture::DrawRegion { origin: o } => {
                if let (Some(ty), Some(p)) = (view.armed.take(), pointer) {
                    let (a, b) = (view.to_canvas(o), view.to_canvas(p));
                    actions.push(Action::AddRegion {
                        ty,
                        min: Position {
                            x: a.x.min(b.x),
                            y: a.y.min(b.y),
                        },
                        size: ((a.x - b.x).abs().max(60.0), (a.y - b.y).abs().max(60.0)),
                    });
                }
            }
            Gesture::Marquee { .. } | Gesture::Pan | Gesture::Widget { .. } | Gesture::None => {}
        }
    }

    // ── Clicks.
    if resp.clicked() {
        if let Some(p) = pointer {
            if view.armed.is_some() {
                // The draw tool needs a drag.
            } else if let Some(w) = hits.widget(p) {
                if let Some(a) = widget_click(patch, &w.node, &w.kind) {
                    actions.push(a);
                }
            } else if let Some((id, _)) = hits.chip(p) {
                actions.push(Action::Focus(id.clone()));
            } else if let Some(f) = hits.field(p) {
                click_field(view, f.clone(), p);
            } else if let Some((node, port)) = hits.toggle(p) {
                let key = (node, port);
                if !view.expanded.remove(&key) {
                    view.expanded.insert(key);
                }
            } else if let Some(pp) = hits.pill(p) {
                if mods.shift || mods.command {
                    if let Some(i) = view.selected_ports.iter().position(|x| *x == pp) {
                        view.selected_ports.remove(i);
                    } else {
                        view.selected_ports.push(pp);
                    }
                } else {
                    view.selected_ports = vec![pp];
                    view.selection.clear();
                }
                view.selected_edge = None;
            } else if hits.pin(p, PIN_HIT, None).is_some() {
                // Pins only start wires.
            } else if let Some(id) = hits.node(p) {
                if mods.shift || mods.command {
                    if view.is_selected(&id) {
                        view.selection.retain(|s| s != &id);
                    } else {
                        view.selection.push(id.clone());
                    }
                } else {
                    view.selection = vec![id.clone()];
                }
                view.selected_edge = None;
                view.selected_ports.clear();
                view.raise(&id);
            } else if let Some(w) = hits.wire(p) {
                view.selected_edge = Some(w.id.clone());
                view.selection.clear();
                view.selected_ports.clear();
            } else if !mods.shift {
                view.selection.clear();
                view.selected_edge = None;
                view.selected_ports.clear();
            }
        }
    }
    if resp.double_clicked() && view.armed.is_none() {
        if let Some(p) = pointer {
            let at = view.to_canvas(p);
            if let Some(w) = hits.widget(p) {
                // Double-click a slider / pad to reset it.
                if let Some(a) = widget_reset(patch, cat, &w.node, &w.kind) {
                    actions.push(a);
                }
            } else if let Some((region, port)) = hits.pill(p) {
                let name = ports::find_node(patch, &region)
                    .and_then(|n| n.ports.iter().flatten().find(|x| x.id == port).cloned())
                    .map(|x| x.name)
                    .unwrap_or_default();
                if let Some(pl) = boxes[region.as_str()].pills.iter().find(|x| x.port == port) {
                    view.editing = Some(TextEditing {
                        target: EditTarget::PortName { region, port },
                        text: name,
                        rect: view.canvas_rect(pl.rect),
                        first: true,
                    });
                }
            } else if let Some(id) = hits.node(p) {
                match boxes[id.as_str()].look {
                    Look::Collapsed => actions.push(Action::Enter(id)),
                    Look::Region => actions.push(Action::Rename(id)),
                    Look::Comment => actions.push(Action::EditComment(id)),
                    _ => {}
                }
            } else if let Some((id, _)) = boxes
                .iter()
                .find(|(_, b)| b.look == Look::Frame && b.header.contains(p))
            {
                actions.push(Action::Rename(id.clone()));
            } else if let Some(w) = hits.wire(p) {
                if let Some(s) = insertion_search(patch, cat, &w.id, p, at) {
                    view.search = Some(s);
                }
            } else if hits.field(p).is_none() && hits.pin(p, PIN_HIT, None).is_none() {
                actions.push(Action::OpenSearch { screen: p, at });
            }
        }
    }
    if resp.secondary_clicked() {
        if view.armed.take().is_some() {
            // Right-click cancels the draw tool.
        } else if let Some(p) = pointer {
            let at = view.to_canvas(p);
            let pin = hits.pin(p, PIN_HIT, Some(Side::In));
            let pin = pin.filter(|r| boxes.get(&r.node).is_some_and(|b| b.look == Look::Node));
            if let Some(pin) = pin {
                open_port_editor(view, patch, cat, &boxes, &pin, p);
            } else if hits.pill(p).is_some() {
                // Pills delete with Del once selected, never by right-click.
            } else {
                let target = if let Some(id) = hits.node(p) {
                    if !view.is_selected(&id) {
                        view.select_only(&id);
                    }
                    MenuTarget::Node(id)
                } else if let Some(w) = hits.wire(p) {
                    view.selection.clear();
                    view.selected_edge = Some(w.id.clone());
                    MenuTarget::Edge(w.id.clone())
                } else {
                    MenuTarget::Pane
                };
                view.close_overlays();
                view.menu = Some((p, at, target));
            }
        }
    }

    // ── Keyboard (not while typing in a text field or a popup is open).
    if !typing && !view.overlay_open() {
        let anchor = view.to_canvas(view.last_pointer.unwrap_or(area.center()));
        let cursor = view.last_pointer.unwrap_or(area.center());
        let selected_edge = view.selected_edge.clone();
        let one_subpatch = match view.selection.as_slice() {
            [id] if boxes.get(id).is_some_and(|b| b.look == Look::Collapsed) => Some(id.clone()),
            _ => None,
        };
        let in_subpatch = !view.stack.is_empty();
        let mut escape = false;
        ui.input_mut(|i| {
            let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
            if i.consume_key(cmd_shift, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y) {
                actions.push(Action::Redo);
            } else if i.consume_key(Modifiers::COMMAND, Key::Z) {
                actions.push(Action::Undo);
            }
            if i.consume_key(Modifiers::COMMAND, Key::D) {
                actions.push(Action::Duplicate);
            }
            if i.consume_key(Modifiers::COMMAND, Key::A) {
                actions.push(Action::SelectAll);
            }
            if i.consume_key(Modifiers::COMMAND, Key::G) {
                actions.push(Action::Group);
            }
            if i.consume_key(Modifiers::NONE, Key::Delete)
                || i.consume_key(Modifiers::NONE, Key::Backspace)
            {
                actions.push(Action::Delete);
            }
            if let Some(e) = &selected_edge {
                if i.consume_key(Modifiers::NONE, Key::OpenBracket) {
                    actions.push(Action::ReorderEdge(e.clone(), -1));
                }
                if i.consume_key(Modifiers::NONE, Key::CloseBracket) {
                    actions.push(Action::ReorderEdge(e.clone(), 1));
                }
            }
            if i.consume_key(Modifiers::NONE, Key::F) {
                actions.push(Action::Fit);
            }
            if i.consume_key(Modifiers::NONE, Key::Tab) {
                actions.push(Action::OpenSearch {
                    screen: cursor,
                    at: anchor,
                });
            }
            if i.consume_key(Modifiers::NONE, Key::Q) {
                actions.push(Action::OpenFavorites {
                    screen: cursor,
                    at: anchor,
                });
            }
            if let Some(id) = &one_subpatch {
                if i.consume_key(Modifiers::NONE, Key::I) {
                    actions.push(Action::Enter(id.clone()));
                }
            }
            if in_subpatch && i.consume_key(Modifiers::NONE, Key::O) {
                actions.push(Action::Exit);
            }
            for ev in &i.events {
                match ev {
                    egui::Event::Copy => actions.push(Action::Copy),
                    egui::Event::Cut => actions.push(Action::Cut),
                    egui::Event::Paste(text) => {
                        actions.push(Action::PasteText(text.clone(), anchor));
                    }
                    _ => {}
                }
            }
            escape = i.consume_key(Modifiers::NONE, Key::Escape);
        });
        // Esc disarms the draw tool first, else clears the selection.
        if escape && view.armed.take().is_none() {
            view.gesture = Gesture::None;
            view.selection.clear();
            view.selected_edge = None;
            view.selected_ports.clear();
        }
    }

    // ── Cursor.
    if resp.hovered() {
        if let Some(p) = pointer {
            let icon = if view.armed.is_some() {
                Some(CursorIcon::Crosshair)
            } else if let Gesture::Widget { drag, .. } = &view.gesture {
                Some(match drag {
                    WidgetDrag::Slider(_) => CursorIcon::ResizeHorizontal,
                    WidgetDrag::Pad(_) => CursorIcon::Crosshair,
                    WidgetDrag::Grip(_) => CursorIcon::ResizeNwSe,
                    WidgetDrag::Hold => CursorIcon::PointingHand,
                })
            } else if matches!(view.gesture, Gesture::None) {
                if hits.pin(p, PIN_HIT, None).is_some() {
                    None
                } else if let Some(w) = hits.widget(p) {
                    Some(match w.kind {
                        WidgetKind::Slider(_) => CursorIcon::ResizeHorizontal,
                        WidgetKind::Pad(_) => CursorIcon::Crosshair,
                        WidgetKind::Grip => CursorIcon::ResizeNwSe,
                        _ => CursorIcon::PointingHand,
                    })
                } else if hits
                    .field(p)
                    .is_some_and(|f| matches!(f.kind, FieldKind::Number { .. } | FieldKind::Vec2))
                {
                    Some(CursorIcon::ResizeHorizontal)
                } else if hits.pill(p).is_some() {
                    Some(CursorIcon::ResizeVertical)
                } else if hits.node(p).is_none() {
                    hits.resize(p).map(|(_, dx, dy)| match (dx, dy) {
                        (0, _) => CursorIcon::ResizeVertical,
                        (_, 0) => CursorIcon::ResizeHorizontal,
                        (a, b) if a == b => CursorIcon::ResizeNwSe,
                        _ => CursorIcon::ResizeNeSw,
                    })
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(icon) = icon {
                ctx.set_cursor_icon(icon);
            }
        }
    }

    // ── Frames (behind everything): the entered subpatch, then regions
    // outermost first.
    let flow_t = ui.input(|i| i.time) as f32;
    let mut frame_ids: Vec<&String> = boxes
        .iter()
        .filter(|(_, b)| b.look.is_frame())
        .map(|(id, _)| id)
        .collect();
    frame_ids.sort_by(|a, b| {
        let (ba, bb) = (&boxes[a.as_str()], &boxes[b.as_str()]);
        (ba.look != Look::Frame)
            .cmp(&(bb.look != Look::Frame))
            .then(bb.rect.area().total_cmp(&ba.rect.area()))
    });
    let reparenting = matches!(view.gesture, Gesture::Move { reparent: true, .. });
    for id in &frame_ids {
        let b = &boxes[id.as_str()];
        let n = ports::find_node(patch, id);
        let def_type = n.map_or("", |n| n.def_type.as_str());
        let sel_ports: Vec<String> = view
            .selected_ports
            .iter()
            .filter(|(r, _)| r == *id)
            .map(|(_, p)| p.clone())
            .collect();
        let edge_hit = pending_edge.as_ref().is_some_and(|h| &h.region == *id);
        draw::frame(
            &painter,
            b,
            &draw::FrameState {
                kind: cat.get(def_type).and_then(|s| s.region_kind),
                def_type,
                selected: view.is_selected(id),
                drop_target: (reparenting && hover_region.as_ref() == Some(*id)) || edge_hit,
                selected_ports: &sel_ports,
            },
            z,
        );
        if let Some(h) = pending_edge.as_ref().filter(|h| &h.region == *id) {
            let x = match h.direction {
                floret_engine::patch::PortDirection::Input => b.rect.left(),
                floret_engine::patch::PortDirection::Output => b.rect.right(),
            };
            let y = b.rect.top() + h.y as f32 * z;
            painter.line_segment(
                [pos2(x - 12.0 * z, y), pos2(x + 12.0 * z, y)],
                Stroke::new(3.0, GOLD),
            );
        }
    }

    // ── Wires, coloured by the source pin's type.
    let hovered_wire = match (&view.gesture, pointer) {
        (Gesture::None, Some(p))
            if resp.hovered() && hits.node(p).is_none() && hits.pin(p, PIN_HIT, None).is_none() =>
        {
            hits.wire(p).map(|w| w.id.clone())
        }
        _ => None,
    };
    let width = (1.8 * z).clamp(1.0, 3.0);
    for w in &wire_list {
        let base = theme::port_color(type_name(w.ty));
        let selected = view.selected_edge.as_ref() == Some(&w.id);
        let hovered = hovered_wire.as_ref() == Some(&w.id);
        if selected {
            painter.add(bezier(
                w.pts,
                Stroke::new(width + 5.0, GOLD.gamma_multiply(0.18)),
            ));
        }
        let (color, wdt) = if selected {
            (GOLD, width + 0.6)
        } else if hovered {
            (base, width + 0.8)
        } else {
            (draw::mix_wire(base), width)
        };
        painter.add(bezier(w.pts, Stroke::new(wdt, color)));
        if view.wire_flow && z > 0.3 {
            // Marching ants: small bright dashes flowing source → target.
            let ants = Shape::dashed_line_with_offset(
                &w.flat,
                Stroke::new(wdt, if selected { GOLD } else { base }),
                &[2.0 * z],
                &[9.0 * z],
                (flow_t * 20.0 * z).rem_euclid(11.0 * z),
            );
            painter.extend(ants);
        }
    }

    // ── In-progress wire.
    if let (Some((from, _)), Some(p)) = (&affinity, pointer) {
        if let Some(a) = boxes
            .get(&from.node)
            .and_then(|b| b.pin(from.side, &from.port))
            .map(|g| g.pos)
        {
            let target = drop_target(p);
            let end = target
                .as_ref()
                .and_then(|t| boxes.get(&t.node).and_then(|b| b.pin(t.side, &t.port)))
                .map_or(p, |g| g.pos);
            let pts = match from.side {
                Side::Out => wire_points(a, end, z),
                Side::In => wire_points(end, a, z),
            };
            let flat = bezier(pts, Stroke::NONE).flatten(Some(0.5));
            let color = if target.is_some() || pending_edge.is_some() {
                GOLD
            } else {
                GOLD.gamma_multiply(0.85)
            };
            painter.add(bezier(
                pts,
                Stroke::new(width + 4.0, GOLD.gamma_multiply(0.12)),
            ));
            painter.extend(Shape::dashed_line(&flat, Stroke::new(1.6, color), 5.0, 4.0));
        }
    }

    // ── Pins and nodes.
    let pin_look = |node: &str, side: Side, port: &str| -> PinLook {
        match &affinity {
            None => PinLook::Normal,
            Some((from, pins)) => {
                if from.node == node && from.side == side && from.port == port {
                    PinLook::Source
                } else if pins
                    .iter()
                    .any(|(r, _, ok)| *ok && r.node == node && r.side == side && r.port == port)
                {
                    PinLook::Match
                } else {
                    PinLook::Dim
                }
            }
        }
    };
    let pulse = 0.5 + 0.5 * (flow_t * std::f32::consts::TAU / 1.2).sin();
    for id in &frame_ids {
        for g in &boxes[id.as_str()].pins {
            draw::pin(&painter, g, pin_look(id, g.side, &g.port), pulse, z);
        }
    }
    let editing_field = match &view.editing {
        Some(TextEditing {
            target: EditTarget::Field(f),
            ..
        }) => Some(f.clone()),
        _ => None,
    };
    let now = ui.input(|i| i.time);
    view.flashes.retain(|_, t| now - *t < 0.15);
    let dropping = ui.input(|i| !i.raw.hovered_files.is_empty());
    let drop_node = dropping
        .then(|| {
            pointer
                .or(view.last_pointer)
                .and_then(|p| hits.body(p).cloned())
        })
        .flatten()
        .filter(|id| is_picker(patch, id));
    view.probe_areas.clear();
    for id in &order {
        let b = &boxes[id.as_str()];
        if b.look == Look::Probe {
            let max = custom::probe_max_scroll(b, z);
            let off = view.probe_scroll.entry(id.clone()).or_insert(0.0);
            *off = off.clamp(0.0, max);
            if max > 0.0 {
                view.probe_areas
                    .push((id.clone(), custom::probe_body(b, z), max));
            }
        }
        let out_text = |port: &str| live.output(patch, cat, id, port).map(|s| format_spread(&s));
        let subtitle = (b.look == Look::Collapsed).then_some("subpatch");
        let active = view.flashes.contains_key(id)
            || matches!(&view.gesture, Gesture::Widget { node, .. } if node == id);
        let editing_comment = matches!(
            &view.editing,
            Some(TextEditing { target: EditTarget::Field(f), .. })
                if &f.node == id && b.look == Look::Comment
        );
        draw::node(
            &painter,
            b,
            &draw::NodeState {
                selected: view.is_selected(id),
                subtitle,
                out_text: &out_text,
                editing: editing_field.as_ref().filter(|f| &f.node == id),
                body: custom::BodyState {
                    hover: pointer.filter(|_| resp.hovered()),
                    selected: view.is_selected(id),
                    active,
                    scroll: view.probe_scroll.get(id).copied().unwrap_or(0.0),
                    editing: editing_comment,
                    drop: drop_node.as_ref() == Some(id),
                    thumb: view.thumbs.get(id).copied(),
                },
            },
            z,
        );
        for g in &b.pins {
            draw::pin(&painter, g, pin_look(id, g.side, &g.port), pulse, z);
        }
    }

    // ── Marquee, region draw, palette drop ghost.
    if let (Gesture::Marquee { origin: o, .. }, Some(p)) = (&view.gesture, pointer) {
        let r = Rect::from_two_pos(*o, p);
        painter.rect(
            r,
            2.0,
            GOLD.gamma_multiply(0.06),
            Stroke::new(1.0, GOLD.gamma_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
    }
    if let (Gesture::DrawRegion { origin: o }, Some(p), Some(ty)) =
        (&view.gesture, pointer, view.armed.as_ref())
    {
        let spec = cat.get(ty);
        draw::region_preview(
            &painter,
            Rect::from_two_pos(*o, p),
            spec.and_then(|s| s.region_kind),
            spec.map_or(ty.as_str(), |s| s.label.as_str()),
        );
    }
    if let (Some(drag), Some(p)) = (&palette_hover, pointer) {
        if let Some(spec) = cat.get(&drag.0) {
            draw::drop_ghost(&painter, p, &spec.label, &spec.category, z);
        }
    }

    // ── Hover tooltips: warning chips, region badges, pin and wire values.
    if matches!(view.gesture, Gesture::None) && resp.hovered() && !view.overlay_open() {
        if let Some(p) = pointer {
            if let Some((_, text)) = hits.chip(p) {
                draw::tooltip(&ctx, p, "Warning", "click to focus", text);
            } else if let Some(hint) = hits.badge(p).filter(|h| !h.is_empty()) {
                draw::tooltip(&ctx, p, "Iterations", "", hint);
            } else if let Some(pin) = hits.pin(p, PIN_HIT, None) {
                pin_tooltip(&ctx, patch, cat, live, &boxes, &pin, p);
            } else if hits.node(p).is_none() {
                if let Some(w) = hits.wire(p) {
                    pin_tooltip(&ctx, patch, cat, live, &boxes, &w.source, p);
                }
            }
        }
    }

    // ── Files dropped from the OS onto a File / SVG node.
    let dropped = ui.input(|i| i.raw.dropped_files.clone());
    if let Some(f) = dropped.first() {
        let target = pointer
            .or(view.last_pointer)
            .and_then(|p| hits.body(p).cloned())
            .filter(|id| is_picker(patch, id));
        if let Some(id) = target {
            let bytes = f.bytes().ok();
            if let Some(a) = ingest_file(patch, &id, Some(f.path()), bytes.as_deref(), "") {
                actions.push(a);
            }
        }
    }

    // ── Corner toolbar: breadcrumb, then canvas toggles.
    toolbar(ui, patch, view, area, &mut actions);

    // ── Overlays.
    overlays(&ctx, patch, cat, view, area, &mut actions);

    for a in actions {
        apply(a, patch, cat, view);
    }
    if let Some(text) = view.copy_out.take() {
        ctx.copy_text(text);
    }
}

fn view_to_canvas(area: Rect, pan: Vec2, zoom: f32, p: Pos2) -> Position {
    let c = (p - area.min) / zoom - pan;
    Position {
        x: f64::from(c.x),
        y: f64::from(c.y),
    }
}

fn pin_tooltip(
    ctx: &egui::Context,
    patch: &PatchFile,
    cat: &Catalogue,
    live: &Live,
    boxes: &IndexMap<String, NodeBox>,
    pin: &PinRef,
    at: Pos2,
) {
    let Some(g) = boxes
        .get(&pin.node)
        .and_then(|b| b.pin(pin.side, &pin.port))
    else {
        return;
    };
    let kind = if g.spread {
        format!("{} · spread", type_name(g.ty))
    } else {
        type_name(g.ty).to_owned()
    };
    let body = live
        .pin_value(patch, cat, pin)
        .map_or_else(|| "—".to_owned(), |v| v.text());
    draw::tooltip(ctx, at, &g.label, &kind, &body);
}

/// A click on an inline field: toggle, open a dropdown / colour picker,
/// or start typing.
fn click_field(view: &mut View, f: Field, at: Pos2) {
    match &f.kind {
        FieldKind::Bool => {
            let value = Json::Bool(!values::as_bool(&f.value));
            view.gesture = Gesture::None;
            view.menu = None;
            let a = Action::SetValue {
                node: f.node.clone(),
                key: f.key.clone(),
                value,
            };
            // Applied with the rest of this frame's actions.
            view.pending_action(a);
        }
        FieldKind::Select(options) => {
            let current = values::as_text(&f.value);
            let screen = f.rect.left_bottom() + vec2(0.0, 2.0);
            view.close_overlays();
            view.menu = Some((
                screen,
                view.to_canvas(screen),
                MenuTarget::Select {
                    node: f.node.clone(),
                    key: f.key.clone(),
                    options: options.clone(),
                    current,
                },
            ));
        }
        FieldKind::Color => {
            let rgba = values::as_rgba(&f.value);
            view.close_overlays();
            view.color = Some(ColorState {
                node: f.node.clone(),
                key: f.key.clone(),
                color: popups::color32(rgba),
                hex: values::hex(rgba),
                screen: at + vec2(8.0, 8.0),
                before: f.value.clone(),
            });
        }
        FieldKind::Number { .. } | FieldKind::Text | FieldKind::Vec2 => {
            let text = match (f.axis, &f.kind) {
                (Some(a), _) => {
                    let (x, y) = values::as_vec2(&f.value);
                    values::num([x, y][a.min(1)], 6)
                }
                (None, FieldKind::Number { .. }) => values::num(values::as_f64(&f.value), 6),
                _ => values::as_text(&f.value),
            };
            view.close_overlays();
            view.editing = Some(TextEditing {
                rect: view.canvas_rect(f.rect),
                target: EditTarget::Field(f),
                text,
                first: true,
            });
        }
    }
}

impl View {
    /// Queue an action raised outside `show`'s action list.
    fn pending_action(&mut self, a: Action) {
        self.queued.push(a);
    }
}

fn open_port_editor(
    view: &mut View,
    patch: &PatchFile,
    cat: &Catalogue,
    boxes: &IndexMap<String, NodeBox>,
    pin: &PinRef,
    at: Pos2,
) {
    let Some(n) = ports::find_node(patch, &pin.node) else {
        return;
    };
    let Some(spec) = cat.get(&n.def_type) else {
        return;
    };
    let Some(port) = cat
        .resolved_inputs(spec, &n.params)
        .into_iter()
        .find(|p| p.name == pin.port)
    else {
        return;
    };
    let current = n
        .port_values
        .as_ref()
        .and_then(|pv| pv.get(&port.name))
        .cloned();
    let label = boxes
        .get(&pin.node)
        .and_then(|b| b.pin(Side::In, &pin.port))
        .map_or_else(|| port.name.clone(), |g| g.label.clone());
    view.close_overlays();
    view.port_editor = Some(PortEditorState {
        node: pin.node.clone(),
        port: port.name.clone(),
        label,
        ty: port.ty,
        text: current
            .as_ref()
            .map(values::port_editor_text)
            .unwrap_or_default(),
        placeholder: port
            .default
            .as_ref()
            .map(|d| {
                format!(
                    "default: {}",
                    values::port_editor_text(&values::value_to_json(d))
                )
            })
            .unwrap_or_default(),
        screen: at,
        first: true,
    });
}

fn toolbar(
    ui: &mut egui::Ui,
    patch: &PatchFile,
    view: &mut View,
    area: Rect,
    actions: &mut Vec<Action>,
) {
    let bar = Rect::from_min_size(area.min + vec2(10.0, 10.0), vec2(area.width() - 20.0, 26.0));
    let mut tb = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    tb.spacing_mut().item_spacing.x = 6.0;
    if !view.stack.is_empty() {
        let depth = view.stack.len();
        if popups::chip(&mut tb, "Project", false, "Back to the top level").clicked() {
            actions.push(Action::ViewDepth(0));
        }
        for (i, id) in view.stack.iter().enumerate() {
            tb.label(egui::RichText::new("/").color(FG_2));
            let label = ports::find_node(patch, id)
                .and_then(|n| n.params.get("label").and_then(|v| v.as_str()))
                .unwrap_or("Subpatch");
            let current = i + 1 == depth;
            if popups::chip(
                &mut tb,
                label,
                current,
                if current {
                    "Current view"
                } else {
                    "Go to this level"
                },
            )
            .clicked()
                && !current
            {
                actions.push(Action::ViewDepth(i + 1));
            }
        }
        if popups::chip(&mut tb, "⬆ O", false, "Exit one level (O)").clicked() {
            actions.push(Action::Exit);
        }
        tb.add_space(8.0);
    }
    if popups::chip(
        &mut tb,
        "Arrange",
        false,
        "Auto-arrange the patch left → right",
    )
    .clicked()
    {
        actions.push(Action::Arrange);
    }
    if popups::chip(&mut tb, "Snap", view.snap, "Snap nodes to the 24 px grid").clicked() {
        view.snap = !view.snap;
    }
    if popups::chip(
        &mut tb,
        "Flow",
        view.wire_flow,
        "Animate data flow along wires",
    )
    .clicked()
    {
        view.wire_flow = !view.wire_flow;
    }
    if popups::chip(
        &mut tb,
        "Grow",
        view.auto_grow,
        "Grow a region while a child is dragged against its edge",
    )
    .clicked()
    {
        view.auto_grow = !view.auto_grow;
    }
    tb.add_space(4.0);
    let hint = match view.armed.as_deref().and_then(|t| Catalogue::builtin().get(t)) {
        Some(s) => egui::RichText::new(format!(
            "Drag on the canvas to draw a {} region · Esc to cancel",
            s.label
        ))
        .color(GOLD),
        None => egui::RichText::new(
            "Dbl-click / Tab: add node · Q: favorites · drag pins to wire · Space-drag: in / out of regions · right-click for more",
        )
        .color(FG_3),
    };
    tb.label(hint.small());
}

fn overlays(
    ctx: &egui::Context,
    patch: &mut PatchFile,
    cat: &Catalogue,
    view: &mut View,
    area: Rect,
    actions: &mut Vec<Action>,
) {
    if let Some((at_screen, at, target)) = view.menu.clone() {
        let items = menu_items(patch, cat, view, &target, at);
        match popups::menu_popup(ctx, area, at_screen, &items) {
            Outcome::Open => {}
            Outcome::Close => view.menu = None,
            Outcome::Pick(a) => {
                view.menu = None;
                actions.push(a);
            }
        }
    }
    if let Some(st) = view.search.as_mut() {
        match popups::search_popup(ctx, area, cat, &mut view.favorites, st) {
            Outcome::Open => {}
            Outcome::Close => view.search = None,
            Outcome::Pick(ty) => actions.push(Action::PickSearch(ty)),
        }
    }
    if let Some(st) = view.favorites_menu.as_mut() {
        match popups::favorites_popup(ctx, area, cat, &view.favorites, st) {
            Outcome::Open => {}
            Outcome::Close => view.favorites_menu = None,
            Outcome::Pick(ty) => actions.push(Action::PickFavorite(ty)),
        }
    }
    if let Some(ed) = view.editing.as_mut() {
        let rect = Rect::from_min_size(
            area.min + (ed.rect.min.to_vec2() + view.pan) * view.zoom,
            ed.rect.size() * view.zoom,
        );
        let first = std::mem::take(&mut ed.first);
        let multiline = matches!(&ed.target, EditTarget::Field(f) if f.multiline);
        let outcome = if multiline {
            popups::multiline_text(
                ctx,
                rect,
                &mut ed.text,
                first,
                (11.0 * view.zoom).clamp(9.0, 16.0),
            )
        } else {
            popups::inline_text(ctx, rect, &mut ed.text, first)
        };
        match outcome {
            Outcome::Open => {}
            Outcome::Close => view.editing = None,
            Outcome::Pick(()) => {
                if let Some(a) = view.editing.take().and_then(finish_edit) {
                    actions.push(a);
                }
            }
        }
    }
    if let Some(st) = view.color.as_mut() {
        let (changed, out) = popups::color_popup(ctx, area, st);
        let (node, key) = (st.node.clone(), st.key.clone());
        // Palette stops keep their colours as hex strings.
        let value = if st.before.is_string() {
            Json::String(values::hex(popups::rgba_of(st.color)))
        } else {
            values::rgba_json(popups::rgba_of(st.color))
        };
        if changed && values::set(patch, &node, &key, value.clone()) {
            view.live(&node, &key, value);
        }
        match out {
            Outcome::Open => {}
            Outcome::Close => {
                let st = view.color.take().expect("open");
                values::set(patch, &node, &key, st.before.clone());
                view.live(&node, &key, st.before);
            }
            Outcome::Pick(()) => {
                let st = view.color.take().expect("open");
                if values::as_rgba(&st.before) != popups::rgba_of(st.color) {
                    view.commit_value(patch, cat, &node, &key);
                }
            }
        }
    }
    if let Some(st) = view.port_editor.as_mut() {
        match popups::port_editor(ctx, area, st) {
            Outcome::Open => {}
            Outcome::Close => view.port_editor = None,
            Outcome::Pick(()) => {
                let st = view.port_editor.take().expect("open");
                match values::port_editor_value(st.ty, &st.text) {
                    Some(Some(value)) => actions.push(Action::SetValue {
                        node: st.node,
                        key: EditKey::Port(st.port),
                        value,
                    }),
                    Some(None) if values::clear_port_value(patch, &st.node, &st.port) => {
                        view.commit_value(patch, cat, &st.node, &EditKey::Port(st.port));
                    }
                    _ => {}
                }
            }
        }
    }
    actions.append(&mut view.queued);
}

/// The action that commits typed text.
fn finish_edit(ed: TextEditing) -> Option<Action> {
    match ed.target {
        EditTarget::Field(f) => {
            let value = match f.axis {
                Some(a) => {
                    let v: f64 = ed
                        .text
                        .trim()
                        .parse()
                        .ok()
                        .filter(|v: &f64| v.is_finite())?;
                    let (x, y) = values::as_vec2(&f.value);
                    let (x, y) = if a == 0 { (v, y) } else { (x, v) };
                    serde_json::json!({ "x": x, "y": y })
                }
                None if f.kind == FieldKind::Color && f.value.is_string() => {
                    let rgba = values::parse_hex(&ed.text)?;
                    Json::String(values::hex(rgba))
                }
                None => values::parse_field(&f.kind, &ed.text)?,
            };
            Some(Action::SetValue {
                node: f.node,
                key: f.key,
                value,
            })
        }
        EditTarget::PortName { region, port } => Some(Action::RenamePort {
            region,
            port,
            name: ed.text,
        }),
    }
}

/// A pin, its screen position, and whether the dragged wire may land on it.
type PinMatch = (PinRef, Pos2, bool);

/// The search for splitting wire `eid` with a new node.
fn insertion_search(
    patch: &PatchFile,
    cat: &Catalogue,
    eid: &str,
    screen: Pos2,
    at: Position,
) -> Option<SearchState> {
    let e = patch.edges.iter().find(|e| e.id == eid)?.clone();
    let (_, sname) = ports::split_handle(e.source_handle.as_deref())?;
    let (_, tname) = ports::split_handle(e.target_handle.as_deref())?;
    let src = ports::resolve_port(patch, cat, &e.source, Side::Out, sname)?.ty;
    let dst = ports::resolve_port(patch, cat, &e.target, Side::In, tname)?.ty;
    Some(SearchState::new(
        screen,
        at,
        Fit::Insert { src, dst },
        Purpose::Insert { edge: e },
    ))
}

fn menu_items(
    patch: &PatchFile,
    cat: &Catalogue,
    view: &View,
    target: &MenuTarget,
    at: Position,
) -> Vec<MenuItem<Action>> {
    let has_clip = view.clip.is_some();
    match target {
        MenuTarget::Pane => {
            let mut items = vec![
                MenuItem::new(
                    "Add node…",
                    Action::OpenSearch {
                        screen: view.menu.as_ref().map_or(Pos2::ZERO, |m| m.0),
                        at,
                    },
                )
                .key("Dbl-click"),
                MenuItem::new(
                    "Favorites…",
                    Action::OpenFavorites {
                        screen: view.menu.as_ref().map_or(Pos2::ZERO, |m| m.0),
                        at,
                    },
                )
                .key("Q"),
                MenuItem::new("Add comment", Action::AddComment(at)),
                MenuItem::separator(),
                MenuItem::new("Paste", Action::Paste(at))
                    .key(shortcut("V"))
                    .enabled(has_clip),
                MenuItem::new("Select all", Action::SelectAll).key(shortcut("A")),
                MenuItem::separator(),
                MenuItem::new("Auto-arrange", Action::Arrange),
                MenuItem::new("Fit view", Action::Fit).key("F"),
            ];
            if !view.stack.is_empty() {
                items.push(MenuItem::new("Exit subpatch", Action::Exit).key("O"));
            }
            items.extend([
                MenuItem::separator(),
                MenuItem::new("Undo", Action::Undo)
                    .key(shortcut("Z"))
                    .enabled(view.history.can_undo()),
                MenuItem::new("Redo", Action::Redo)
                    .key(shortcut("⇧Z"))
                    .enabled(view.history.can_redo()),
            ]);
            items
        }
        MenuTarget::Node(id) => {
            let n = view.selection.len();
            let mut items = vec![
                MenuItem::new(
                    if n > 1 {
                        format!("Duplicate {n} nodes")
                    } else {
                        "Duplicate".into()
                    },
                    Action::Duplicate,
                )
                .key(shortcut("D")),
                MenuItem::new("Copy", Action::Copy).key(shortcut("C")),
                MenuItem::new("Cut", Action::Cut).key(shortcut("X")),
                MenuItem::new(
                    "Paste",
                    Action::Paste(Position {
                        x: at.x + 24.0,
                        y: at.y + 24.0,
                    }),
                )
                .key(shortcut("V"))
                .enabled(has_clip),
            ];
            let node = ports::find_node(patch, id);
            if node.is_some_and(|n| subpatch::is_subpatch(cat, n))
                && view.scope() != Some(id.as_str())
            {
                items.push(MenuItem::separator());
                items.push(MenuItem::new("Enter subpatch", Action::Enter(id.clone())).key("I"));
            }
            if node
                .and_then(|n| cat.get(&n.def_type))
                .is_some_and(|s| s.region_kind.is_some())
            {
                items.push(MenuItem::new("Rename…", Action::Rename(id.clone())).key("Dbl-click"));
            }
            if !regions::is_drawable_node(patch, cat, id) {
                let parent = node.and_then(|n| n.parent_id.clone());
                let scope = view.scope().map(str::to_owned);
                let mut region_items = Vec::new();
                if parent.is_some() && parent != scope {
                    region_items.push(
                        MenuItem::new(
                            "Move out of region",
                            Action::Reparent(id.clone(), scope.clone()),
                        )
                        .key("Space+drag"),
                    );
                }
                let visible = subpatch::visible(patch, cat, &view.stack);
                for r in patch.nodes.iter().filter(|r| {
                    visible.contains(&r.id)
                        && Some(&r.id) != parent.as_ref()
                        && &r.id != id
                        && regions::is_drawable_node(patch, cat, &r.id)
                }) {
                    let label = r
                        .params
                        .get("label")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Region");
                    region_items.push(MenuItem::new(
                        format!("Move into {label}"),
                        Action::Reparent(id.clone(), Some(r.id.clone())),
                    ));
                }
                if !region_items.is_empty() {
                    items.push(MenuItem::separator());
                    items.extend(region_items);
                }
            }
            items.push(MenuItem::separator());
            items.push(
                MenuItem::new(
                    if n > 1 {
                        format!("Group {n} into subpatch")
                    } else {
                        "Wrap in subpatch".into()
                    },
                    Action::Group,
                )
                .key(shortcut("G")),
            );
            items.push(MenuItem::separator());
            items.push(
                MenuItem::new(
                    if n > 1 {
                        format!("Delete {n} nodes")
                    } else {
                        "Delete".into()
                    },
                    Action::Delete,
                )
                .key("Del")
                .danger(),
            );
            items
        }
        MenuTarget::Edge(eid) => {
            let mut items = Vec::new();
            let multi = patch
                .edges
                .iter()
                .find(|e| &e.id == eid)
                .and_then(|e| e.target_handle.as_deref().map(|h| (e, h)))
                .is_some_and(|(e, h)| connect::is_multi_target(patch, cat, &e.target, h));
            if let (true, Some((idx, count))) = (multi, connect::edge_slot(patch, eid)) {
                let c = count as i64;
                items.extend([
                    MenuItem::new(
                        format!("Move up · ({}/{count})", idx + 1),
                        Action::ReorderEdge(eid.clone(), -1),
                    )
                    .key("[")
                    .enabled(idx > 0),
                    MenuItem::new("Move down", Action::ReorderEdge(eid.clone(), 1))
                        .key("]")
                        .enabled(idx + 1 < count),
                    MenuItem::new("Move to top", Action::ReorderEdge(eid.clone(), -c))
                        .enabled(idx > 0),
                    MenuItem::new("Move to bottom", Action::ReorderEdge(eid.clone(), c))
                        .enabled(idx + 1 < count),
                    MenuItem::separator(),
                ]);
            }
            items.push(
                MenuItem::new("Delete wire", Action::DeleteEdge(eid.clone()))
                    .key("Del")
                    .danger(),
            );
            items
        }
        MenuTarget::Select {
            node,
            key,
            options,
            current,
        } => options
            .iter()
            .map(|o| {
                let label = if o == current {
                    format!("•  {o}")
                } else {
                    format!("    {o}")
                };
                // Keep numbers numeric (e.g. FFT bins) when the stored value is.
                let value = match ports::find_node(patch, node).and_then(|n| match key {
                    EditKey::Param(k) => n.params.get(k),
                    EditKey::Port(p) => n.port_values.as_ref().and_then(|pv| pv.get(p)),
                }) {
                    Some(Json::Number(_)) => o
                        .parse::<f64>()
                        .map_or_else(|_| Json::String(o.clone()), |v| serde_json::json!(v)),
                    _ => Json::String(o.clone()),
                };
                MenuItem::new(
                    label,
                    Action::SetValue {
                        node: node.clone(),
                        key: key.clone(),
                        value,
                    },
                )
            })
            .collect(),
    }
}

/// "⌘V" on macOS, "Ctrl+V" elsewhere.
fn shortcut(k: &str) -> &'static str {
    let mac = cfg!(target_os = "macos");
    match (mac, k) {
        (true, "V") => "⌘V",
        (true, "A") => "⌘A",
        (true, "Z") => "⌘Z",
        (true, "⇧Z") => "⇧⌘Z",
        (true, "D") => "⌘D",
        (true, "C") => "⌘C",
        (true, "X") => "⌘X",
        (true, "G") => "⌘G",
        (false, "V") => "Ctrl+V",
        (false, "A") => "Ctrl+A",
        (false, "Z") => "Ctrl+Z",
        (false, "⇧Z") => "Ctrl+Shift+Z",
        (false, "D") => "Ctrl+D",
        (false, "C") => "Ctrl+C",
        (false, "X") => "Ctrl+X",
        (false, "G") => "Ctrl+G",
        _ => "",
    }
}

fn size_of(cat: &Catalogue) -> impl Fn(&floret_engine::patch::PatchNode) -> (f64, f64) + '_ {
    move |n| node_size(n, cat)
}

fn apply(action: Action, patch: &mut PatchFile, cat: &Catalogue, view: &mut View) {
    let scope = view.scope().map(str::to_owned);
    match action {
        Action::OpenSearch { screen, at } => {
            view.close_overlays();
            view.search = Some(SearchState::new(screen, at, Fit::Any, Purpose::Plain));
        }
        Action::OpenFavorites { screen, at } => {
            view.close_overlays();
            view.favorites_menu = Some(FavoritesState {
                screen,
                at,
                highlight: 0,
            });
        }
        Action::PickSearch(ty) => {
            let Some(st) = view.search.take() else {
                return;
            };
            place_from_search(patch, cat, view, st, &ty);
        }
        Action::PickFavorite(ty) => {
            let Some(st) = view.favorites_menu.take() else {
                return;
            };
            let at = Position {
                x: st.at.x - f64::from(NODE_W) / 2.0,
                y: st.at.y - 20.0,
            };
            apply(Action::Place { ty, at }, patch, cat, view);
        }
        Action::Place { ty, at } => {
            if cat
                .get(&ty)
                .is_some_and(|s| regions::is_drawable(s.region_kind))
            {
                view.armed = Some(ty);
                return;
            }
            if let Some(id) = ops::place_node(patch, cat, &ty, view.snapped(at), scope.as_deref()) {
                view.select_only(&id);
                view.commit(patch);
            }
        }
        Action::AddComment(at) => {
            if let Some(id) = ops::place_node(
                patch,
                cat,
                "meta/comment",
                view.snapped(at),
                scope.as_deref(),
            ) {
                view.select_only(&id);
                view.commit(patch);
            }
        }
        Action::AddRegion { ty, min, size } => {
            let size_fn = size_of(cat);
            if let Some(id) = regions::add_region(
                patch,
                cat,
                &ty,
                view.snapped(min),
                size,
                scope.as_deref(),
                &size_fn,
            ) {
                view.select_only(&id);
                view.commit(patch);
            }
        }
        Action::Copy => copy(patch, view),
        Action::Cut => {
            copy(patch, view);
            apply(Action::Delete, patch, cat, view);
        }
        Action::Paste(at) => {
            if let Some(clip) = view.clip.clone() {
                paste(patch, view, &clip, at);
            }
        }
        Action::PasteText(text, at) => match clipboard::parse(&text) {
            Some(clip) => {
                paste(patch, view, &clip, at);
                view.clip = Some(clip);
            }
            None => apply(Action::Paste(at), patch, cat, view),
        },
        Action::Delete => {
            let mut changed = false;
            for (r, p) in std::mem::take(&mut view.selected_ports) {
                changed |= regions::remove_port(patch, &r, &p);
            }
            if !view.selection.is_empty() {
                let ids: Vec<String> = std::mem::take(&mut view.selection)
                    .into_iter()
                    .filter(|id| Some(id.as_str()) != scope.as_deref())
                    .collect();
                changed |= ops::delete_nodes(patch, &ids) > 0;
            } else if let Some(e) = view.selected_edge.take() {
                changed |= connect::remove_edge(patch, &e);
            }
            if changed {
                view.commit(patch);
            }
            view.prune_selection(patch);
        }
        Action::Duplicate => {
            let ids = view.top_level_selection(patch);
            if ids.is_empty() {
                return;
            }
            let new = ops::duplicate(patch, &ids, 24.0, 24.0);
            let tops = new.len().min(ids.len());
            view.selection = new[..tops].to_vec();
            for i in &new[..tops] {
                view.raise(i);
            }
            view.commit(patch);
        }
        Action::SelectAll => {
            let visible = subpatch::visible(patch, cat, &view.stack);
            view.selection = patch
                .nodes
                .iter()
                .filter(|n| visible.contains(&n.id) && Some(n.id.as_str()) != scope.as_deref())
                .map(|n| n.id.clone())
                .collect();
            view.selected_edge = None;
        }
        Action::DeleteEdge(e) => {
            if connect::remove_edge(patch, &e) {
                view.commit(patch);
            }
            view.prune_selection(patch);
        }
        Action::ReorderEdge(e, d) => {
            if connect::reorder_multi_edge(patch, cat, &e, d) {
                view.commit(patch);
            }
        }
        Action::Connect(c) => {
            if connect::connect(patch, cat, &c).is_some() {
                view.selected_edge = None;
                view.commit(patch);
            }
        }
        Action::ConnectNewPort { from, hit } => {
            let Some(pid) =
                regions::add_port(patch, &hit.region, from.ty, hit.direction, Some(hit.y))
            else {
                return;
            };
            let c = match from.side {
                Side::Out => Conn::new(
                    &from.node,
                    &Side::Out.handle(&from.port),
                    &hit.region,
                    &Side::In.handle(&pid),
                ),
                Side::In => Conn::new(
                    &hit.region,
                    &Side::Out.handle(&pid),
                    &from.node,
                    &Side::In.handle(&from.port),
                ),
            };
            if connect::connect(patch, cat, &c).is_none() {
                regions::remove_port(patch, &hit.region, &pid);
                return;
            }
            view.commit(patch);
        }
        Action::Probe { from, at } => {
            let at = view.snapped(match from.side {
                Side::Out => at,
                Side::In => Position {
                    x: at.x - f64::from(NODE_W),
                    y: at.y,
                },
            });
            let Some(id) = ops::place_node(patch, cat, "meta/probe", at, scope.as_deref()) else {
                return;
            };
            let c = match from.side {
                Side::Out => Conn::new(&from.node, &Side::Out.handle(&from.port), &id, "in-in"),
                Side::In => Conn::new(&id, "out-out", &from.node, &Side::In.handle(&from.port)),
            };
            connect::connect(patch, cat, &c);
            view.select_only(&id);
            view.commit(patch);
        }
        Action::Reparent(id, parent) => {
            if regions::set_parent(patch, &id, parent.as_deref()) {
                view.commit(patch);
            }
        }
        Action::Group => {
            let ids: Vec<String> = view
                .top_level_selection(patch)
                .into_iter()
                .filter(|id| Some(id.as_str()) != scope.as_deref())
                .collect();
            let size_fn = size_of(cat);
            if let Some(id) = subpatch::group(patch, cat, &ids, &size_fn) {
                view.select_only(&id);
                view.commit(patch);
            }
        }
        Action::Enter(id) => {
            if subpatch::is_subpatch_id(patch, cat, &id) && view.scope() != Some(id.as_str()) {
                view.stack.push(id);
                view.stack = subpatch::valid_stack(patch, cat, &view.stack);
                view.selection.clear();
                view.selected_ports.clear();
                view.selected_edge = None;
                view.close_overlays();
                view.fit_pending = true;
            }
        }
        Action::Exit => {
            if let Some(id) = view.stack.pop() {
                view.close_overlays();
                view.select_only(&id);
                view.fit_pending = true;
            }
        }
        Action::ViewDepth(d) => {
            if d < view.stack.len() {
                let exited = view.stack[d].clone();
                view.stack.truncate(d);
                view.close_overlays();
                view.select_only(&exited);
                view.fit_pending = true;
            }
        }
        Action::Rename(id) => {
            let Some(n) = ports::find_node(patch, &id) else {
                return;
            };
            let look = geom::look_of(n, cat, view.scope());
            let p = ops::absolute_pos(patch, &id);
            let (w, _) = geom::box_size(n, cat, look);
            let rect = if look.is_frame() {
                let h = regions::HEADER_H as f32;
                Rect::from_min_size(
                    pos2(p.x as f32 + 4.0, p.y as f32 - h + 2.0),
                    vec2((w as f32 * 0.5).clamp(120.0, 240.0), h - 4.0),
                )
            } else {
                Rect::from_min_size(
                    pos2(p.x as f32 + 18.0, p.y as f32 + 4.0),
                    vec2(NODE_W - 36.0, geom::HEADER_H - 8.0),
                )
            };
            let label = n
                .params
                .get("label")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .or_else(|| cat.get(&n.def_type).map(|s| s.label.clone()))
                .unwrap_or_default();
            view.close_overlays();
            view.editing = Some(TextEditing {
                target: EditTarget::Field(Field::new(
                    Rect::NOTHING,
                    &id,
                    EditKey::Param("label".into()),
                    FieldKind::Text,
                    Json::String(label.clone()),
                )),
                text: label,
                rect,
                first: true,
            });
        }
        Action::Focus(id) => {
            view.select_only(&id);
            let area = view.area;
            view.fit_nodes(patch, area, |n| n == id);
            view.zoom = view.zoom.min(1.0);
            view.keep_fitted = false;
        }
        Action::CommitValue { node, key } => view.commit_value(patch, cat, &node, &key),
        Action::RenamePort { region, port, name } => {
            if regions::rename_port(patch, &region, &port, &name) {
                view.history.push(patch, None);
            }
        }
        Action::SetValue { node, key, value } => {
            if values::set(patch, &node, &key, value.clone()) {
                view.live(&node, &key, value);
                clamp_slider(view, patch, &node);
                view.commit_value(patch, cat, &node, &key);
            }
        }
        Action::SetParams { node, values: list } => {
            let mut changed = None;
            for (k, v) in list {
                let key = EditKey::Param(k);
                if values::set(patch, &node, &key, v.clone()) {
                    view.live(&node, &key, v);
                    changed = Some(key);
                }
            }
            if let Some(key) = changed {
                view.commit_value(patch, cat, &node, &key);
            }
        }
        Action::PickFile(node) => {
            let svg = ports::find_node(patch, &node).is_some_and(|n| n.def_type == "io/svg");
            let mut dialog = rfd::FileDialog::new();
            if svg {
                dialog = dialog.add_filter("SVG", &["svg"]);
            }
            if let Some(path) = dialog.pick_file() {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if let Some(a) = ingest_file(patch, &node, Some(&path), None, &name) {
                    apply(a, patch, cat, view);
                }
            }
        }
        Action::Bang(node) => {
            view.live(
                &node,
                &EditKey::Param(floret_engine::nodes::io::BANG_PARAM.into()),
                Json::Bool(true),
            );
            view.flashes.insert(node, view.time);
        }
        Action::EditComment(id) => {
            let Some(n) = ports::find_node(patch, &id) else {
                return;
            };
            let p = ops::absolute_pos(patch, &id);
            let (w, h) = geom::box_size(n, cat, Look::Comment);
            let text = n
                .params
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            let rect = Rect::from_min_size(
                pos2(p.x as f32 + 6.0, p.y as f32 + 4.0),
                vec2(w as f32 - 12.0, h as f32 - 8.0),
            );
            view.close_overlays();
            view.select_only(&id);
            view.editing = Some(TextEditing {
                target: EditTarget::Field(Field {
                    multiline: true,
                    ..Field::new(
                        rect,
                        &id,
                        EditKey::Param("text".into()),
                        FieldKind::Text,
                        Json::String(text.clone()),
                    )
                }),
                text,
                rect,
                first: true,
            });
        }
        Action::Undo => {
            if view.history.undo(patch) {
                view.recompile = true;
                view.prune_selection(patch);
            }
        }
        Action::Redo => {
            if view.history.redo(patch) {
                view.recompile = true;
                view.prune_selection(patch);
            }
        }
        Action::Arrange => {
            layout::auto_layout(patch, &|n| node_size(n, cat));
            view.commit(patch);
            view.fit_pending = true;
        }
        Action::Fit => {
            let area = view.area;
            view.keep_fitted = false;
            if view.selection.is_empty() {
                view.fit(patch, area);
            } else {
                let sel = view.selection.clone();
                view.fit_nodes(patch, area, |id| sel.iter().any(|s| s == id));
            }
        }
    }
}

fn copy(patch: &PatchFile, view: &mut View) {
    let ids = view.top_level_selection(patch);
    if let Some(clip) = clipboard::copy(patch, &ids) {
        // Also onto the system clipboard as `.flora` JSON.
        view.copy_out = Some(clip.to_json());
        view.clip = Some(clip);
    }
}

fn paste(patch: &mut PatchFile, view: &mut View, clip: &PatchFile, at: Position) {
    let ids = clipboard::paste(patch, clip, view.snapped(at));
    if ids.is_empty() {
        return;
    }
    let tops = clip
        .nodes
        .iter()
        .filter(|n| n.parent_id.is_none())
        .count()
        .min(ids.len());
    // Inside a subpatch, pasted nodes belong to it.
    if let Some(scope) = view.scope().map(str::to_owned) {
        for id in &ids[..tops] {
            if let Some(n) = ports::find_node_mut(patch, id) {
                n.parent_id = None;
            }
            regions::set_parent(patch, id, Some(&scope));
        }
    }
    view.selection = ids[..tops].to_vec();
    view.selected_edge = None;
    for i in &ids[..tops] {
        view.raise(i);
    }
    view.commit(patch);
}

fn place_from_search(
    patch: &mut PatchFile,
    cat: &Catalogue,
    view: &mut View,
    st: SearchState,
    ty: &str,
) {
    let Some(spec) = cat.get(ty) else {
        return;
    };
    if regions::is_drawable(spec.region_kind) {
        // Drawable regions arm the crosshair: the user sizes them.
        view.armed = Some(ty.to_owned());
        return;
    }
    let at = match (&st.purpose, &st.fit) {
        // Dragged out of an input: put the node to the left of the drop.
        (Purpose::Wire { side: Side::In, .. }, _) => Position {
            x: st.at.x - f64::from(NODE_W),
            y: st.at.y - 30.0,
        },
        (Purpose::Wire { .. }, _) => Position {
            x: st.at.x,
            y: st.at.y - 30.0,
        },
        _ => Position {
            x: st.at.x - f64::from(NODE_W) / 2.0,
            y: st.at.y - 20.0,
        },
    };
    let scope = view.scope().map(str::to_owned);
    let Some(id) = ops::place_node(patch, cat, ty, view.snapped(at), scope.as_deref()) else {
        return;
    };
    match (&st.purpose, &st.fit) {
        (Purpose::Wire { node, side, port }, Fit::Pin { ty: wty, .. }) => {
            if let Some(p) = search::first_compatible(cat, spec, *side, *wty) {
                let c = match side {
                    Side::Out => {
                        Conn::new(node, &Side::Out.handle(port), &id, &Side::In.handle(&p))
                    }
                    Side::In => Conn::new(&id, &Side::Out.handle(&p), node, &Side::In.handle(port)),
                };
                connect::connect(patch, cat, &c);
            }
        }
        (Purpose::Insert { edge }, Fit::Insert { src, dst }) => {
            if let Some((i, o)) = search::insertion_ports(cat, spec, *src, *dst) {
                connect::remove_edge(patch, &edge.id);
                if let (Some(sh), Some(th)) = (&edge.source_handle, &edge.target_handle) {
                    connect::connect(
                        patch,
                        cat,
                        &Conn::new(&edge.source, sh, &id, &Side::In.handle(&i)),
                    );
                    connect::connect(
                        patch,
                        cat,
                        &Conn::new(&id, &Side::Out.handle(&o), &edge.target, th),
                    );
                }
            }
        }
        _ => {}
    }
    view.select_only(&id);
    view.commit(patch);
}

/// Short live-value text for an output pin.
fn format_spread(s: &floret_engine::Spread) -> String {
    match s.len() {
        0 => String::new(),
        1 => format_value(&s[0]),
        n => format!("[{n}]"),
    }
}

fn format_value(v: &floret_engine::Value) -> String {
    use floret_engine::Value;
    match v {
        Value::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 1e9 {
                format!("{n:.0}")
            } else {
                format!("{n:.3}")
            }
        }
        Value::Bool(b) => b.to_string(),
        // An empty string reads as nothing: the pin's label shows instead.
        Value::Str(s) if s.is_empty() => String::new(),
        Value::Str(s) => {
            let s: String = s.chars().take(14).collect();
            format!("\"{s}\"")
        }
        Value::Vec2(p) => format!("{:.1}, {:.1}", p.x, p.y),
        Value::Color(c) => values::hex([c.r, c.g, c.b, 1.0]),
        Value::List(items) => format!("[{}]", items.len()),
        Value::Undefined | Value::Null => String::new(),
        Value::Object(_) => "{…}".into(),
        Value::Scene(n) => n.kind().to_owned(),
        Value::Style(_) => "style".into(),
        Value::Handle(h) => h.type_name().to_owned(),
        Value::Image(i) => format!("{}×{}", i.width(), i.height()),
    }
}

/// One frame of a control drag at pointer `p`.
#[allow(clippy::too_many_arguments)]
fn widget_drag(
    view: &mut View,
    patch: &mut PatchFile,
    node: &str,
    rect: Rect,
    drag: &WidgetDrag,
    origin: Pos2,
    p: Pos2,
    fine: bool,
) {
    let z = view.zoom;
    let mut set = |view: &mut View, key: &str, v: Json| {
        let key = EditKey::Param(key.into());
        if values::set(patch, node, &key, v.clone()) {
            view.live(node, &key, v);
        }
    };
    match drag {
        WidgetDrag::Slider(s) => {
            // The thumb travels inside the track's end caps.
            let inner = rect.shrink2(vec2(7.0 * z, 0.0));
            let v = if fine {
                s.fine(s.value, f64::from((p.x - origin.x) / inner.width()))
            } else {
                s.at(f64::from((p.x - inner.left()) / inner.width()))
            };
            set(view, "value", serde_json::json!(v));
        }
        WidgetDrag::Pad(pad) => {
            let (u, v) = if fine {
                let (u0, v0) = pad.uv();
                (
                    u0 + 0.1 * f64::from((p.x - origin.x) / rect.width()),
                    v0 + 0.1 * f64::from((p.y - origin.y) / rect.height()),
                )
            } else {
                (
                    f64::from((p.x - rect.left()) / rect.width()),
                    f64::from((p.y - rect.top()) / rect.height()),
                )
            };
            let (x, y) = pad.at(u, v);
            set(view, "x", serde_json::json!(x));
            set(view, "y", serde_json::json!(y));
        }
        WidgetDrag::Grip(start) => {
            let d = (p - origin) / z;
            let (w, h) = widgets::comment_size(*start, f64::from(d.x), f64::from(d.y));
            set(view, "width", serde_json::json!(w));
            set(view, "height", serde_json::json!(h));
        }
        WidgetDrag::Hold => {}
    }
}

/// The action a click on a (non-drag) control raises.
fn widget_click(patch: &PatchFile, node: &str, kind: &WidgetKind) -> Option<Action> {
    let n = ports::find_node(patch, node)?;
    let set = |key: &str, value: Json| Action::SetValue {
        node: node.to_owned(),
        key: EditKey::Param(key.into()),
        value,
    };
    match kind {
        WidgetKind::Toggle(on) => Some(set("value", Json::Bool(!on))),
        WidgetKind::Bang => Some(Action::Bang(node.to_owned())),
        WidgetKind::Step(d) => Some(set("value", serde_json::json!(widgets::nudge(n, *d)))),
        WidgetKind::Slots { param, to, .. } => to.map(|v| set(param, serde_json::json!(v))),
        WidgetKind::Pick { .. } => Some(Action::PickFile(node.to_owned())),
        WidgetKind::Clear => Some(Action::SetParams {
            node: node.to_owned(),
            values: widgets::clear_file_params(&n.def_type),
        }),
        WidgetKind::Slider(_) | WidgetKind::Pad(_) | WidgetKind::Hold | WidgetKind::Grip => None,
    }
}

/// Double-click: a slider / pad back to its default value.
fn widget_reset(
    patch: &PatchFile,
    cat: &Catalogue,
    node: &str,
    kind: &WidgetKind,
) -> Option<Action> {
    let n = ports::find_node(patch, node)?;
    let spec = cat.get(&n.def_type)?;
    let keys: &[&str] = match kind {
        WidgetKind::Slider(_) => &["value"],
        WidgetKind::Pad(_) => &["x", "y"],
        _ => return None,
    };
    let values = keys
        .iter()
        .filter_map(|k| Some(((*k).to_owned(), spec.default_params.get(*k)?.clone())))
        .collect();
    Some(Action::SetParams {
        node: node.to_owned(),
        values,
    })
}

/// After a slider's min / max changed, pull its value back into range.
fn clamp_slider(view: &mut View, patch: &mut PatchFile, node: &str) {
    let Some(n) = ports::find_node(patch, node).filter(|n| n.def_type == "io/slider") else {
        return;
    };
    if let Some(v) = widgets::slider(n).clamped() {
        let key = EditKey::Param("value".into());
        values::set(patch, node, &key, serde_json::json!(v));
        view.live(node, &key, serde_json::json!(v));
    }
}

/// Nodes that take a picked / dropped file.
fn is_picker(patch: &PatchFile, id: &str) -> bool {
    ports::find_node(patch, id).is_some_and(|n| {
        n.def_type == "io/file"
            || (n.def_type == "io/svg"
                && n.params
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("file")
                    == "file")
    })
}

/// Read a picked / dropped file into a node's params.
fn ingest_file(
    patch: &PatchFile,
    node: &str,
    path: Option<&std::path::Path>,
    bytes: Option<&[u8]>,
    name: &str,
) -> Option<Action> {
    let n = ports::find_node(patch, node)?;
    let data = match (bytes, path) {
        (Some(b), _) => b.to_vec(),
        (None, Some(p)) => std::fs::read(p).ok()?,
        (None, None) => return None,
    };
    let name = if name.is_empty() {
        path.and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        name.to_owned()
    };
    let values = widgets::file_params(&n.def_type, &name, &data)?;
    Some(Action::SetParams {
        node: node.to_owned(),
        values,
    })
}
