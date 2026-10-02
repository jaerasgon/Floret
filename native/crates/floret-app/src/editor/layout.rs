//! Auto-arrange: a left-to-right layered (Sugiyama) layout. Every scope
//! (the root, each region, each subpatch) is laid out on its own, innermost
//! first, so a region is sized to its contents before its parent places it:
//!
//! 1. Columns: longest path from the sources; a source is then pulled right
//!    to sit just before its first consumer.
//! 2. A wire spanning several columns gets a lane through each one it
//!    crosses, so no node sits on it.
//! 3. Order within a column: barycentre sweeps over pin positions plus
//!    adjacent swaps, keeping the order with the fewest crossings.
//! 4. Heights: every node moves toward the pins it's wired to (a
//!    least-squares fit that keeps the order and the gaps), so wires run
//!    as straight as they can. A frame's port pills move with its contents.
//! 5. Columns sit further apart where wires climb further.
//!
//! Comments go beside the graph: above it, or left of it when that suits a
//! laptop screen better.

use super::regions;
use floret_engine::patch::{PatchFile, PatchNode, PortDirection, Position};
use std::collections::HashMap;

/// Gap between columns, before the room steep wires get.
pub const H_GAP: f64 = 96.0;
/// Gap between nodes stacked in a column.
pub const V_GAP: f64 = 48.0;
/// Gap between the comments and the graph, and between comments.
pub const COMMENT_GAP: f64 = 64.0;
/// Inside a region frame: clear of the input pills and loop-variable pins
/// on the left and the output pills on the right, with room for wires.
pub const PAD_LEFT: f64 = 128.0;
pub const PAD_RIGHT: f64 = 112.0;
pub const PAD_TOP: f64 = 40.0;
pub const PAD_BOTTOM: f64 = 40.0;
/// The lane a long wire keeps through a column.
const LANE_H: f64 = 16.0;
const LANE_GAP: f64 = 16.0;
const FALLBACK: (f64, f64) = (200.0, 100.0);
/// The canvas shape comments are placed to suit (a laptop window, docks
/// open).
const SCREEN_ASPECT: f64 = 1.3;

/// What the layout needs to know about how nodes are drawn.
pub trait Measure {
    /// Size as drawn in the node's own scope (a region: its frame, without
    /// the label strip above it). Zero → a 200×100 fallback.
    fn size(&self, n: &PatchNode) -> (f64, f64);
    /// How far below the node's top a pin sits, by handle (`in-x`).
    fn pin_y(&self, n: &PatchNode, handle: &str) -> Option<f64>;
    /// The same for a region or subpatch frame's pins, seen from inside.
    fn frame_pin_y(&self, n: &PatchNode, handle: &str) -> Option<f64>;
    /// The highest a frame's input pills may sit (below its top-left
    /// stack).
    fn pill_floor(&self, n: &PatchNode) -> f64;
    /// Drawn as a frame its children sit in (ForEach / If / Cache).
    fn is_frame(&self, n: &PatchNode) -> bool;
    /// A wire into this input brings last frame's value back (a stateful
    /// node's `next`): feedback, which doesn't order the columns.
    fn is_feedback(&self, n: &PatchNode, handle: &str) -> bool;
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Node(usize),
    /// A long wire's way through a column.
    Lane,
    /// A frame pin that stays put (a loop variable, a static input).
    Pin,
    /// A frame port's pill (index into the frame's ports).
    Pill(usize),
}

#[derive(Clone, Copy)]
struct Vert {
    kind: Kind,
    w: f64,
    /// Height including a frame's label strip.
    h: f64,
    /// The label strip above a frame's position.
    strip: f64,
    /// Initial ordering key; a fixed pin's top.
    seed: f64,
    /// Pins and pills: on the frame's left (inputs) side.
    left: bool,
}

impl Vert {
    fn node(i: usize, w: f64, h: f64, strip: f64, seed: f64) -> Vert {
        Vert {
            kind: Kind::Node(i),
            w,
            h: h + strip,
            strip,
            seed,
            left: false,
        }
    }

    fn marker(kind: Kind, y: f64, left: bool) -> Vert {
        Vert {
            kind,
            w: 0.0,
            h: if kind == Kind::Lane { LANE_H } else { 0.0 },
            strip: 0.0,
            seed: y,
            left,
        }
    }

    fn is_node(&self) -> bool {
        matches!(self.kind, Kind::Node(_))
    }

    fn fixed(&self) -> bool {
        self.kind == Kind::Pin
    }
}

/// A wire between neighbouring columns: pins `uy` / `vy` below the tops.
#[derive(Clone, Copy)]
struct Seg {
    u: usize,
    uy: f64,
    v: usize,
    vy: f64,
}

struct Ctx<'a> {
    patch: &'a PatchFile,
    m: &'a dyn Measure,
    children: HashMap<Option<usize>, Vec<usize>>,
    positions: HashMap<usize, Position>,
    region_size: HashMap<usize, (f64, f64)>,
    /// (frame, port id) → its pill's new height.
    pills: HashMap<(usize, String), f64>,
}

fn is_comment(n: &PatchNode) -> bool {
    n.def_type == "meta/comment"
}

/// Weighted least-squares fit of `want` under `y[i+1] ≥ y[i] + step[i]`
/// (pool-adjacent-violators on the offsets from the tightest packing).
fn pack(want: &[f64], weight: &[f64], step: &[f64]) -> Vec<f64> {
    let mut base = vec![0.0; want.len()];
    for i in 1..want.len() {
        base[i] = base[i - 1] + step[i - 1];
    }
    // Blocks of (mean, weight, count).
    let mut blocks: Vec<(f64, f64, usize)> = Vec::new();
    for i in 0..want.len() {
        blocks.push((want[i] - base[i], weight[i], 1));
        while blocks.len() > 1 {
            let (m1, w1, c1) = blocks[blocks.len() - 1];
            let (m0, w0, c0) = blocks[blocks.len() - 2];
            if m0 <= m1 {
                break;
            }
            blocks.pop();
            let w = w0 + w1;
            *blocks.last_mut().expect("two blocks") = ((m0 * w0 + m1 * w1) / w, w, c0 + c1);
        }
    }
    let mut out = Vec::with_capacity(want.len());
    for (m, _, c) in blocks {
        for _ in 0..c {
            out.push(base[out.len()] + m);
        }
    }
    out
}

impl Ctx<'_> {
    fn kids(&self, parent: Option<usize>) -> Vec<usize> {
        self.children.get(&parent).cloned().unwrap_or_default()
    }

    /// Size in the node's scope: a region's frame as just fitted to its
    /// contents; a subpatch stays a collapsed node however big its inside.
    fn size(&self, i: usize) -> (f64, f64) {
        let n = &self.patch.nodes[i];
        if let Some(&s) = self.region_size.get(&i).filter(|_| self.m.is_frame(n)) {
            return s;
        }
        let (w, h) = self.m.size(n);
        (
            if w > 0.0 { w } else { FALLBACK.0 },
            if h > 0.0 { h } else { FALLBACK.1 },
        )
    }

    /// A pin's height below a node's top; frames use their moved pills.
    fn pin_y(&self, i: usize, handle: &str) -> Option<f64> {
        let n = &self.patch.nodes[i];
        let port = handle
            .strip_prefix("in-")
            .or_else(|| handle.strip_prefix("out-"));
        port.filter(|_| self.m.is_frame(n))
            .and_then(|p| self.pills.get(&(i, p.to_owned())).copied())
            .or_else(|| self.m.pin_y(n, handle))
    }

    /// Lay out one scope; returns the bottom-right of what it placed.
    fn layout_scope(&mut self, parent: Option<usize>) -> (f64, f64) {
        let patch = self.patch;
        let members = self.kids(parent);
        let (comments, graph): (Vec<usize>, Vec<usize>) =
            members.iter().partition(|&&i| is_comment(&patch.nodes[i]));
        let index: HashMap<&str, usize> = patch
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();

        let mut verts: Vec<Vert> = Vec::new();
        let mut vert_of: HashMap<usize, usize> = HashMap::new();
        for &i in &graph {
            let (w, h) = self.size(i);
            let strip = if self.m.is_frame(&patch.nodes[i]) {
                regions::HEADER_H
            } else {
                0.0
            };
            vert_of.insert(i, verts.len());
            verts.push(Vert::node(i, w, h, strip, patch.nodes[i].position.y));
        }
        let n_real = verts.len();

        // The frame's port pills, by the handle of their inner pin.
        let frame = parent.map(|r| &patch.nodes[r]);
        let mut inner: HashMap<String, usize> = HashMap::new();
        for (k, p) in frame
            .iter()
            .flat_map(|f| f.ports.iter().flatten().enumerate())
        {
            let input = p.direction == PortDirection::Input;
            let handle = format!("{}-{}", if input { "out" } else { "in" }, p.id);
            let y = frame
                .and_then(|f| self.m.frame_pin_y(f, &handle))
                .unwrap_or(0.0);
            inner.insert(handle, verts.len());
            verts.push(Vert::marker(Kind::Pill(k), y, input));
        }

        // Wires: (from, from pin, to, to pin).
        let pin = |verts: &[Vert], v: usize, handle: Option<&str>| -> f64 {
            let vt = verts[v];
            let off = match (vt.kind, handle) {
                (Kind::Node(i), Some(h)) => self.pin_y(i, h),
                _ => None,
            };
            vt.strip + off.unwrap_or((vt.h - vt.strip) / 2.0)
        };
        // A wire into or out of a nested region counts for the region; one
        // reaching in from outside this scope keeps its node on the left.
        let member = |mut i: usize| -> Option<usize> {
            for _ in 0..patch.nodes.len() {
                if vert_of.contains_key(&i) {
                    return Some(i);
                }
                i = *index.get(patch.nodes[i].parent_id.as_deref()?)?;
            }
            None
        };
        let mut wires: Vec<(usize, f64, usize, f64)> = Vec::new();
        let mut feedback = Vec::new();
        let mut left_fed = vec![false; n_real];
        for e in &patch.edges {
            let (Some(&s0), Some(&t0)) =
                (index.get(e.source.as_str()), index.get(e.target.as_str()))
            else {
                continue;
            };
            let (s, t) = (member(s0).unwrap_or(s0), member(t0).unwrap_or(t0));
            let sh = e.source_handle.as_deref().filter(|_| s == s0);
            let th = e.target_handle.as_deref().filter(|_| t == t0);
            if let (None, Some(&b)) = (vert_of.get(&s), vert_of.get(&t)) {
                if Some(s) != parent {
                    left_fed[b] = true;
                    continue;
                }
            }
            let mut frame_pin = |verts: &mut Vec<Vert>, h: &str, left: bool| {
                *inner.entry(h.to_owned()).or_insert_with(|| {
                    let y = frame.and_then(|f| self.m.frame_pin_y(f, h)).unwrap_or(0.0);
                    verts.push(Vert::marker(Kind::Pin, y, left));
                    verts.len() - 1
                })
            };
            let back = e
                .target_handle
                .as_deref()
                .is_some_and(|h| self.m.is_feedback(&patch.nodes[t0], h));
            match (vert_of.get(&s), vert_of.get(&t)) {
                (Some(&a), Some(&b)) if a != b => {
                    let w = (a, pin(&verts, a, sh), b, pin(&verts, b, th));
                    if back {
                        feedback.push(w);
                    } else {
                        wires.push(w);
                    }
                }
                _ if back => {}
                (None, Some(&b)) if Some(s) == parent => {
                    let Some(h) = sh else { continue };
                    let a = frame_pin(&mut verts, h, true);
                    left_fed[b] = true;
                    wires.push((a, 0.0, b, pin(&verts, b, th)));
                }
                (Some(&a), None) if Some(t) == parent => {
                    let Some(h) = th else { continue };
                    let b = frame_pin(&mut verts, h, false);
                    wires.push((a, pin(&verts, a, sh), b, 0.0));
                }
                _ => {}
            }
        }

        // 1. Columns; the first and last hold the frame's pins.
        let mut ins: Vec<Vec<usize>> = vec![Vec::new(); n_real];
        let mut outs: Vec<Vec<usize>> = vec![Vec::new(); n_real];
        for &(a, _, b, _) in &wires {
            if a < n_real && b < n_real {
                outs[a].push(b);
                ins[b].push(a);
            }
        }
        // Feedback that closes a loop doesn't order the columns (the loop
        // reads left to right, the wire back is the exception); feedback
        // into a node downstream of nothing it feeds is a plain wire.
        for w in feedback {
            let (a, _, b, _) = w;
            let mut seen = vec![false; n_real];
            let mut stack = vec![b];
            let mut loops = false;
            while let Some(v) = stack.pop() {
                if v == a {
                    loops = true;
                    break;
                }
                if !std::mem::replace(&mut seen[v], true) {
                    stack.extend(&outs[v]);
                }
            }
            if !loops {
                outs[a].push(b);
                ins[b].push(a);
                wires.push(w);
            }
        }
        let mut depth = vec![0usize; n_real];
        let mut indeg: Vec<usize> = ins.iter().map(Vec::len).collect();
        let mut done = vec![false; n_real];
        let mut order = Vec::with_capacity(n_real);
        loop {
            let mut queue: Vec<usize> =
                (0..n_real).filter(|&v| !done[v] && indeg[v] == 0).collect();
            if queue.is_empty() {
                // A cycle: break it at its first node.
                match (0..n_real).find(|&v| !done[v]) {
                    Some(v) => queue.push(v),
                    None => break,
                }
            }
            while let Some(v) = queue.pop() {
                if done[v] {
                    continue;
                }
                done[v] = true;
                order.push(v);
                for &t in &outs[v] {
                    if !done[t] {
                        depth[t] = depth[t].max(depth[v] + 1);
                        indeg[t] -= 1;
                        if indeg[t] == 0 {
                            queue.push(t);
                        }
                    }
                }
            }
        }
        // Pull a node right, next to its first consumer, when it feeds at
        // least as many wires as it takes (a source, a link in a chain):
        // the wires it shortens outnumber the ones it stretches.
        for &v in order.iter().rev() {
            if !left_fed[v] && outs[v].len() >= ins[v].len() {
                if let Some(m) = outs[v].iter().map(|&t| depth[t]).filter(|&d| d > 0).min() {
                    depth[v] = depth[v].max(m - 1);
                }
            }
        }
        let last = depth.iter().copied().max().unwrap_or(0) + 2;
        let mut layer: Vec<usize> = (0..verts.len())
            .map(|v| match verts[v].kind {
                Kind::Node(_) => depth[v] + 1,
                _ if verts[v].left => 0,
                _ => last,
            })
            .collect();

        // 2. Lanes for wires that skip columns.
        let mut segs: Vec<Seg> = Vec::new();
        for &(a, ay, b, by) in &wires {
            let (la, lb) = (layer[a], layer[b]);
            if lb <= la {
                continue; // a feedback wire
            }
            let mut prev = (a, ay);
            for l in la + 1..lb {
                let f = (l - la) as f64 / (lb - la) as f64;
                let seed = verts[a].seed + (verts[b].seed - verts[a].seed) * f;
                verts.push(Vert::marker(Kind::Lane, seed, false));
                layer.push(l);
                let lane = verts.len() - 1;
                segs.push(Seg {
                    u: prev.0,
                    uy: prev.1,
                    v: lane,
                    vy: LANE_H / 2.0,
                });
                prev = (lane, LANE_H / 2.0);
            }
            segs.push(Seg {
                u: prev.0,
                uy: prev.1,
                v: b,
                vy: by,
            });
        }

        // 3. Order within columns.
        let mut cols: Vec<Vec<usize>> = vec![Vec::new(); last + 1];
        for (v, &l) in layer.iter().enumerate() {
            cols[l].push(v);
        }
        for c in &mut cols {
            c.sort_by(|&a, &b| verts[a].seed.total_cmp(&verts[b].seed).then(a.cmp(&b)));
        }
        let cols = order_columns(&verts, &segs, &layer, cols);

        // 4. Heights.
        let floors = frame.map_or((0.0, 0.0), |f| {
            (self.m.pill_floor(f), regions::PORT_TOP_OFFSET)
        });
        let tops = place_heights(&verts, &segs, &cols, parent.is_some(), floors);

        // 5. Columns left to right.
        let col_w: Vec<f64> = cols
            .iter()
            .map(|c| c.iter().map(|&v| verts[v].w).fold(0.0, f64::max))
            .collect();
        let climb = |l: usize| -> f64 {
            segs.iter()
                .filter(|s| layer[s.u] == l)
                .map(|s| ((tops[s.u] + s.uy) - (tops[s.v] + s.vy)).abs())
                .fold(0.0, f64::max)
        };
        let room = |l: usize| (climb(l) * 0.15).min(120.0);
        let mut col_x = vec![0.0f64; last + 1];
        let mut x = if parent.is_some() {
            PAD_LEFT + room(0) * 0.5
        } else {
            0.0
        };
        for l in 1..last {
            col_x[l] = x;
            x += col_w[l] + H_GAP + room(l);
        }

        let (mut left, mut top) = (f64::INFINITY, f64::INFINITY);
        let (mut right, mut bottom) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (v, vt) in verts.iter().enumerate() {
            match vt.kind {
                Kind::Node(i) => {
                    let l = layer[v];
                    let px = (col_x[l] + (col_w[l] - vt.w) / 2.0).round();
                    let py = (tops[v] + vt.strip).round();
                    self.positions.insert(i, Position { x: px, y: py });
                    left = left.min(px);
                    top = top.min(py - vt.strip);
                    right = right.max(px + vt.w);
                    bottom = bottom.max(py - vt.strip + vt.h);
                }
                Kind::Pill(k) => {
                    let (Some(r), Some(f)) = (parent, frame) else {
                        continue;
                    };
                    let id = f.ports.as_ref().expect("pills come from ports")[k]
                        .id
                        .clone();
                    let y = tops[v].round();
                    bottom = bottom.max(y + regions::PORT_TAB_H);
                    self.pills.insert((r, id), y);
                }
                _ => {}
            }
        }
        if n_real > 0 && parent.is_some() {
            right += room(last - 1) * 0.5;
        }
        if n_real == 0 {
            let o = if parent.is_some() {
                (PAD_LEFT, PAD_TOP)
            } else {
                (0.0, 0.0)
            };
            (left, top, right) = (o.0, o.1, o.0);
            bottom = bottom.max(o.1 - COMMENT_GAP);
        }
        let (r, b) = self.place_comments(&comments, parent.is_some(), (left, top, right, bottom));
        (right.max(r), bottom.max(b))
    }

    /// Put a scope's comments beside its graph (`l, t, r, b`); returns the
    /// bottom-right of the comments.
    fn place_comments(
        &mut self,
        comments: &[usize],
        inside: bool,
        g: (f64, f64, f64, f64),
    ) -> (f64, f64) {
        if comments.is_empty() {
            return (f64::NEG_INFINITY, f64::NEG_INFINITY);
        }
        let patch = self.patch;
        let mut cs = comments.to_vec();
        cs.sort_by(|&a, &b| {
            let (pa, pb) = (patch.nodes[a].position, patch.nodes[b].position);
            pa.y.total_cmp(&pb.y).then(pa.x.total_cmp(&pb.x))
        });
        let sizes: Vec<(f64, f64)> = cs.iter().map(|&c| self.size(c)).collect();
        let (gl, gt, gr, gb) = g;
        let (gw, gh) = ((gr - gl).max(0.0), (gb - gt).max(0.0));
        // Rows no wider than the graph (or the widest comment).
        let wrap = sizes.iter().map(|s| s.0).fold(gw, f64::max);
        let mut rows: Vec<(Vec<usize>, f64, f64)> = Vec::new(); // (items, width, height)
        for (k, &(w, h)) in sizes.iter().enumerate() {
            match rows.last_mut() {
                Some((items, rw, rh)) if *rw + COMMENT_GAP + w <= wrap => {
                    items.push(k);
                    *rw += COMMENT_GAP + w;
                    *rh = rh.max(h);
                }
                _ => rows.push((vec![k], w, h)),
            }
        }
        let gaps = |n: usize| COMMENT_GAP * n.saturating_sub(1) as f64;
        let block_h = rows.iter().map(|r| r.2).sum::<f64>() + gaps(rows.len());
        let col_w = sizes.iter().map(|s| s.0).fold(0.0, f64::max);
        let col_h = sizes.iter().map(|s| s.1).sum::<f64>() + gaps(sizes.len());
        // Above (rows) or to the left (a column): whichever a screen shows
        // larger. Inside a region, below the contents.
        let scale = |w: f64, h: f64| (SCREEN_ASPECT / w.max(1.0)).min(1.0 / h.max(1.0));
        let above = scale(gw.max(wrap), gh + COMMENT_GAP + block_h);
        let beside = scale(gw + COMMENT_GAP + col_w, gh.max(col_h));
        let mut placed = Vec::new();
        if !inside && beside > above * 1.05 {
            let mut y = gt;
            for (k, &(w, h)) in sizes.iter().enumerate() {
                placed.push((cs[k], gl - COMMENT_GAP - w, y));
                y += h + COMMENT_GAP;
            }
        } else {
            let mut y = if inside {
                gb + COMMENT_GAP
            } else {
                gt - COMMENT_GAP - block_h
            };
            for (items, _, rh) in &rows {
                let mut x = gl;
                for &k in items {
                    placed.push((cs[k], x, y));
                    x += sizes[k].0 + COMMENT_GAP;
                }
                y += rh + COMMENT_GAP;
            }
        }
        let (mut r, mut b) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (c, x, y) in placed {
            let (w, h) = self.size(c);
            let p = Position {
                x: x.round(),
                y: y.round(),
            };
            r = r.max(p.x + w);
            b = b.max(p.y + h);
            self.positions.insert(c, p);
        }
        (r, b)
    }

    /// Post-order: inner scopes are laid out and sized before their parent.
    fn process(&mut self, parent: Option<usize>) {
        for c in self.kids(parent) {
            if self.children.contains_key(&Some(c)) {
                self.process(Some(c));
            }
        }
        let (right, bottom) = self.layout_scope(parent);
        if let Some(r) = parent {
            let w = (right + PAD_RIGHT).max(regions::MIN_W).round();
            let h = (bottom + PAD_BOTTOM).max(regions::MIN_H).round();
            self.region_size.insert(r, (w, h));
        }
    }
}

/// Column order with each vertex's rank, and the wires grouped by the
/// column they leave from.
struct Order<'a> {
    verts: &'a [Vert],
    segs: &'a [Seg],
    cols: Vec<Vec<usize>>,
    rank: Vec<f64>,
    /// Wires leaving each column.
    from: Vec<Vec<usize>>,
    /// Per vertex: wires to the column before / after it.
    before: Vec<Vec<usize>>,
    after: Vec<Vec<usize>>,
}

impl<'a> Order<'a> {
    fn new(verts: &'a [Vert], segs: &'a [Seg], layer: &[usize], cols: Vec<Vec<usize>>) -> Self {
        let mut from = vec![Vec::new(); cols.len()];
        let mut before = vec![Vec::new(); verts.len()];
        let mut after = vec![Vec::new(); verts.len()];
        for (k, s) in segs.iter().enumerate() {
            from[layer[s.u]].push(k);
            after[s.u].push(k);
            before[s.v].push(k);
        }
        let mut o = Order {
            verts,
            segs,
            cols,
            rank: vec![0.0; verts.len()],
            from,
            before,
            after,
        };
        for l in 0..o.cols.len() {
            o.rerank(l);
        }
        o
    }

    fn rerank(&mut self, l: usize) {
        for (i, &v) in self.cols[l].iter().enumerate() {
            self.rank[v] = i as f64;
        }
    }

    /// A pin's place in column order: its vertex's rank plus how far down
    /// the vertex it sits.
    fn pos(&self, v: usize, y: f64) -> f64 {
        let h = self.verts[v].h;
        let frac = if h > 0.0 {
            (y / h).clamp(0.0, 1.0) * 0.98
        } else {
            0.5
        };
        self.rank[v] + frac
    }

    /// Crossings between column `l` and the next.
    fn crossings_at(&self, l: usize) -> usize {
        let ends: Vec<(f64, f64)> = self.from[l]
            .iter()
            .map(|&k| {
                let s = self.segs[k];
                (self.pos(s.u, s.uy), self.pos(s.v, s.vy))
            })
            .collect();
        let mut n = 0;
        for (i, a) in ends.iter().enumerate() {
            for b in &ends[i + 1..] {
                if (a.0 - b.0) * (a.1 - b.1) < 0.0 {
                    n += 1;
                }
            }
        }
        n
    }

    fn crossings(&self) -> usize {
        (0..self.cols.len()).map(|l| self.crossings_at(l)).sum()
    }

    /// Crossings on either side of column `l`.
    fn crossings_around(&self, l: usize) -> usize {
        let before = if l > 0 { self.crossings_at(l - 1) } else { 0 };
        before + self.crossings_at(l)
    }

    /// Re-sort column `l` by the mean position of its wired neighbours on
    /// one side. Fixed pins stay on top.
    fn barycentre(&mut self, l: usize, look_back: bool) {
        let mut keyed: Vec<(usize, f64)> = self.cols[l]
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                if self.verts[v].fixed() {
                    return (v, -1e9 + self.verts[v].seed);
                }
                let near: Vec<f64> = if look_back {
                    self.before[v]
                        .iter()
                        .map(|&k| self.pos(self.segs[k].u, self.segs[k].uy))
                        .collect()
                } else {
                    self.after[v]
                        .iter()
                        .map(|&k| self.pos(self.segs[k].v, self.segs[k].vy))
                        .collect()
                };
                let key = if near.is_empty() {
                    i as f64
                } else {
                    near.iter().sum::<f64>() / near.len() as f64
                };
                (v, key)
            })
            .collect();
        keyed.sort_by(|a, b| a.1.total_cmp(&b.1));
        self.cols[l] = keyed.into_iter().map(|(v, _)| v).collect();
        self.rerank(l);
    }

    /// Swap neighbours in a column while that removes crossings.
    fn transpose(&mut self) {
        for _ in 0..8 {
            let mut improved = false;
            for l in 0..self.cols.len() {
                for i in 0..self.cols[l].len().saturating_sub(1) {
                    let (a, b) = (self.cols[l][i], self.cols[l][i + 1]);
                    if self.verts[a].fixed() || self.verts[b].fixed() {
                        continue;
                    }
                    let before = self.crossings_around(l);
                    self.cols[l].swap(i, i + 1);
                    self.rerank(l);
                    if self.crossings_around(l) < before {
                        improved = true;
                    } else {
                        self.cols[l].swap(i, i + 1);
                        self.rerank(l);
                    }
                }
            }
            if !improved {
                break;
            }
        }
    }
}

/// Barycentre sweeps and adjacent swaps; the order with fewest crossings.
fn order_columns(
    verts: &[Vert],
    segs: &[Seg],
    layer: &[usize],
    cols: Vec<Vec<usize>>,
) -> Vec<Vec<usize>> {
    let mut o = Order::new(verts, segs, layer, cols);
    let n_cols = o.cols.len();
    let mut best = o.cols.clone();
    let mut best_n = o.crossings();
    for sweep in 0..16 {
        if best_n == 0 {
            break;
        }
        if sweep % 2 == 0 {
            for l in 0..n_cols {
                o.barycentre(l, true);
            }
        } else {
            for l in (0..n_cols).rev() {
                o.barycentre(l, false);
            }
        }
        o.transpose();
        let n = o.crossings();
        if n < best_n {
            best_n = n;
            best = o.cols.clone();
        }
    }
    best
}

/// Tops for every vertex: wired pins pulled level, order and gaps kept.
/// `floors`: the highest the input / output pills may sit.
fn place_heights(
    verts: &[Vert],
    segs: &[Seg],
    cols: &[Vec<usize>],
    inside: bool,
    floors: (f64, f64),
) -> Vec<f64> {
    let gap = |a: usize, b: usize| match (verts[a].kind, verts[b].kind) {
        (Kind::Node(_), Kind::Node(_)) => V_GAP,
        (Kind::Pill(_), Kind::Pill(_)) => regions::PORT_ROW_PITCH,
        _ => LANE_GAP,
    };
    let floor = |v: usize| match verts[v].kind {
        Kind::Pill(_) if verts[v].left => floors.0,
        Kind::Pill(_) => floors.1,
        _ => f64::NEG_INFINITY,
    };
    // Per vertex: (other end, own pin, other pin, weight). Long wires stay
    // straighter: their lanes pull harder.
    let mut wired: Vec<Vec<(usize, f64, f64, f64)>> = vec![Vec::new(); verts.len()];
    for s in segs {
        let w = if verts[s.u].kind == Kind::Lane || verts[s.v].kind == Kind::Lane {
            2.0
        } else {
            1.0
        };
        wired[s.u].push((s.v, s.uy, s.vy, w));
        wired[s.v].push((s.u, s.vy, s.uy, w));
    }
    let movable: Vec<Vec<usize>> = cols
        .iter()
        .map(|c| c.iter().copied().filter(|&v| !verts[v].fixed()).collect())
        .collect();
    let mut tops: Vec<f64> = verts.iter().map(|v| v.seed).collect();
    for c in &movable {
        let mut y = c.first().map_or(0.0, |&v| floor(v).max(0.0));
        for (i, &v) in c.iter().enumerate() {
            if i > 0 {
                y += verts[c[i - 1]].h + gap(c[i - 1], v);
            }
            tops[v] = y;
        }
    }
    for round in 0..40 {
        let range: Vec<usize> = if round % 2 == 0 {
            (0..cols.len()).collect()
        } else {
            (0..cols.len()).rev().collect()
        };
        for l in range {
            let c = &movable[l];
            if c.is_empty() {
                continue;
            }
            let mut want = Vec::with_capacity(c.len());
            let mut wt = Vec::with_capacity(c.len());
            for &v in c {
                let (mut sum, mut total) = (0.0, 0.0);
                for &(o, mine, theirs, w) in &wired[v] {
                    sum += (tops[o] + theirs - mine) * w;
                    total += w;
                }
                if total > 0.0 {
                    want.push(sum / total);
                    wt.push(total);
                } else {
                    want.push(tops[v]);
                    wt.push(0.01);
                }
            }
            let step: Vec<f64> = c
                .windows(2)
                .map(|p| verts[p[0]].h + gap(p[0], p[1]))
                .collect();
            let mut prev = f64::NEG_INFINITY;
            for (i, (&v, y)) in c.iter().zip(pack(&want, &wt, &step)).enumerate() {
                let min = if i == 0 { floor(v) } else { prev + step[i - 1] };
                tops[v] = y.max(min);
                prev = tops[v];
            }
        }
    }
    // The root starts at 0; a region's contents sit below its top padding,
    // with the pills under their floor. One shift for all keeps wires level.
    let min_of = |pick: &dyn Fn(&Vert) -> bool| {
        (0..verts.len())
            .filter(|&v| pick(&verts[v]))
            .map(|v| tops[v])
            .fold(f64::INFINITY, f64::min)
    };
    let nodes_top = min_of(&|v| v.is_node());
    if nodes_top.is_finite() {
        let mut shift = if inside { PAD_TOP } else { 0.0 } - nodes_top;
        for (left, f) in [(true, floors.0), (false, floors.1)] {
            let pills_top = min_of(&|v| matches!(v.kind, Kind::Pill(_)) && v.left == left);
            if pills_top.is_finite() {
                shift = shift.max(f - pills_top);
            }
        }
        for (v, t) in tops.iter_mut().enumerate() {
            if !verts[v].fixed() {
                *t += shift;
            }
        }
    }
    tops
}

/// Tidy every scope of the patch in place. Regions and subpatches with
/// contents are resized to fit them, their port pills moved level with
/// what they're wired to.
pub fn auto_layout(patch: &mut PatchFile, m: &dyn Measure) {
    let index: HashMap<&str, usize> = patch
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut children: HashMap<Option<usize>, Vec<usize>> = HashMap::new();
    for (i, n) in patch.nodes.iter().enumerate() {
        let parent = n.parent_id.as_deref().and_then(|p| index.get(p).copied());
        children.entry(parent).or_default().push(i);
    }
    let (positions, region_size, pills) = {
        let mut ctx = Ctx {
            patch,
            m,
            children,
            positions: HashMap::new(),
            region_size: HashMap::new(),
            pills: HashMap::new(),
        };
        ctx.process(None);
        (ctx.positions, ctx.region_size, ctx.pills)
    };
    for (i, n) in patch.nodes.iter_mut().enumerate() {
        if let Some(p) = positions.get(&i) {
            n.position = *p;
        }
        if let Some(&(w, h)) = region_size.get(&i) {
            n.width = Some(w);
            n.height = Some(h);
            for (k, v) in [("width", w), ("height", h)] {
                if n.params.contains_key(k) {
                    n.params.insert(k.into(), v.into());
                }
            }
        }
        for p in n.ports.iter_mut().flatten() {
            if let Some(&y) = pills.get(&(i, p.id.clone())) {
                p.y = Some(y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{edge, node, patch};
    use floret_engine::patch::RegionPort;

    /// Sizes from `width` / `height`, pins in the middle, frame pins 120
    /// down, pills below 76.
    struct Headless;

    impl Measure for Headless {
        fn size(&self, n: &PatchNode) -> (f64, f64) {
            match (n.width, n.height) {
                (Some(w), Some(h)) => (w, h),
                _ => (0.0, 0.0),
            }
        }
        fn pin_y(&self, _: &PatchNode, _: &str) -> Option<f64> {
            None
        }
        fn frame_pin_y(&self, _: &PatchNode, _: &str) -> Option<f64> {
            Some(120.0)
        }
        fn pill_floor(&self, _: &PatchNode) -> f64 {
            76.0
        }
        fn is_frame(&self, n: &PatchNode) -> bool {
            n.def_type.starts_with("region/")
        }
        fn is_feedback(&self, n: &PatchNode, handle: &str) -> bool {
            n.def_type == "state/value" && handle == "in-next"
        }
    }

    fn at(p: &PatchFile, id: &str) -> Position {
        p.nodes.iter().find(|n| n.id == id).unwrap().position
    }

    fn e(s: &str, t: &str) -> floret_engine::patch::PatchEdge {
        edge(s, "out-result", t, "in-a")
    }

    fn region(id: &str, ports: &[(&str, PortDirection)]) -> PatchNode {
        let mut r = node(id, "region/foreach");
        r.width = Some(100.0);
        r.height = Some(100.0);
        r.ports = Some(
            ports
                .iter()
                .map(|&(p, direction)| RegionPort {
                    id: p.into(),
                    name: p.into(),
                    ty: "number".into(),
                    direction,
                    y: None,
                })
                .collect(),
        );
        r
    }

    fn child(id: &str, parent: &str) -> PatchNode {
        let mut c = node(id, "math/add");
        c.parent_id = Some(parent.into());
        c
    }

    #[test]
    fn chain_left_to_right() {
        let mut p = patch(
            vec![
                node("a", "math/add"),
                node("b", "math/add"),
                node("c", "math/add"),
            ],
            vec![e("a", "b"), e("b", "c")],
        );
        auto_layout(&mut p, &Headless);
        assert!(at(&p, "a").x < at(&p, "b").x);
        assert!(at(&p, "b").x < at(&p, "c").x);
        assert_eq!(at(&p, "a"), Position { x: 0.0, y: 0.0 });
        assert_eq!(at(&p, "b").x, FALLBACK.0 + H_GAP);
        // Straight wires: one row.
        assert_eq!(at(&p, "b").y, 0.0);
        assert_eq!(at(&p, "c").y, 0.0);
    }

    #[test]
    fn same_layer_stacks_and_fan_in_centres() {
        let mut p = patch(
            vec![
                node("a", "math/add"),
                node("b", "math/add"),
                node("c", "math/add"),
            ],
            vec![e("a", "c"), e("b", "c")],
        );
        auto_layout(&mut p, &Headless);
        let (a, b, c) = (at(&p, "a"), at(&p, "b"), at(&p, "c"));
        assert!((a.x - b.x).abs() < 1.0);
        assert_eq!((a.y - b.y).abs(), FALLBACK.1 + V_GAP);
        assert_eq!(c.y, (a.y + b.y) / 2.0);
    }

    #[test]
    fn sources_sit_next_to_their_consumer() {
        // a → b → c → d, and k feeds d only: k sits in c's column.
        let mut p = patch(
            vec![
                node("a", "math/add"),
                node("b", "math/add"),
                node("c", "math/add"),
                node("d", "math/add"),
                node("k", "math/add"),
            ],
            vec![e("a", "b"), e("b", "c"), e("c", "d"), e("k", "d")],
        );
        auto_layout(&mut p, &Headless);
        assert_eq!(at(&p, "k").x, at(&p, "c").x);
    }

    #[test]
    fn feedback_doesnt_turn_the_flow_around() {
        // s → b → c, and c feeds s's `next`: still left to right. c also
        // feeds w's `next`, which closes no loop: w sits after c.
        let mut p = patch(
            vec![
                node("c", "math/add"),
                node("b", "math/add"),
                node("s", "state/value"),
                node("w", "state/value"),
            ],
            vec![
                edge("s", "out-current", "b", "in-a"),
                e("b", "c"),
                edge("c", "out-result", "s", "in-next"),
                edge("c", "out-result", "w", "in-next"),
            ],
        );
        auto_layout(&mut p, &Headless);
        assert!(at(&p, "s").x < at(&p, "b").x);
        assert!(at(&p, "b").x < at(&p, "c").x);
        assert!(at(&p, "c").x < at(&p, "w").x);
    }

    #[test]
    fn long_wires_keep_a_lane() {
        // a feeds d across the two columns b and c sit in.
        let mut p = patch(
            vec![
                node("a", "math/add"),
                node("b", "math/add"),
                node("c", "math/add"),
                node("d", "math/add"),
                node("s", "math/add"),
            ],
            vec![e("s", "b"), e("b", "c"), e("c", "d"), e("a", "d")],
        );
        auto_layout(&mut p, &Headless);
        let wire_y = at(&p, "a").y + FALLBACK.1 / 2.0;
        for n in [at(&p, "b"), at(&p, "c")] {
            assert!(
                wire_y < n.y - LANE_GAP || wire_y > n.y + FALLBACK.1 + LANE_GAP,
                "a node sits on the long wire"
            );
        }
    }

    #[test]
    fn fewer_crossings_than_the_seed() {
        // Seeded crossed: a1 above a2 but a1 feeds the lower target.
        let mut nodes: Vec<PatchNode> = ["a1", "a2", "b1", "b2"]
            .iter()
            .map(|id| node(id, "math/add"))
            .collect();
        nodes[1].position.y = 200.0;
        nodes[3].position.y = 200.0;
        let mut p = patch(nodes, vec![e("a1", "b2"), e("a2", "b1")]);
        auto_layout(&mut p, &Headless);
        let (a1, a2) = (at(&p, "a1"), at(&p, "a2"));
        let (b1, b2) = (at(&p, "b1"), at(&p, "b2"));
        assert_eq!(a1.y < a2.y, b2.y < b1.y);
    }

    #[test]
    fn region_grows_to_fit_children() {
        let mut p = patch(
            vec![region("r", &[]), child("c1", "r"), child("c2", "r")],
            vec![],
        );
        auto_layout(&mut p, &Headless);
        let r = p.nodes.iter().find(|n| n.id == "r").unwrap();
        assert!(r.width.unwrap() >= PAD_LEFT + FALLBACK.0 + PAD_RIGHT);
        assert!(r.height.unwrap() >= PAD_TOP + 2.0 * FALLBACK.1 + V_GAP + PAD_BOTTOM);
        assert_eq!(at(&p, "c1").x, PAD_LEFT);
        assert_eq!(at(&p, "c1").y, PAD_TOP);
    }

    #[test]
    fn pills_line_up_with_what_they_feed() {
        let mut p = patch(
            vec![
                region(
                    "r",
                    &[("i", PortDirection::Input), ("o", PortDirection::Output)],
                ),
                child("c", "r"),
            ],
            vec![
                edge("r", "out-i", "c", "in-a"),
                edge("c", "out-result", "r", "in-o"),
            ],
        );
        auto_layout(&mut p, &Headless);
        let r = p.nodes.iter().find(|n| n.id == "r").unwrap();
        let ports = r.ports.as_ref().unwrap();
        let pin = at(&p, "c").y + FALLBACK.1 / 2.0;
        assert_eq!(ports[0].y, Some(pin));
        assert_eq!(ports[1].y, Some(pin));
        assert!(ports[0].y.unwrap() >= 76.0, "below the top-left stack");
    }

    #[test]
    fn a_subpatch_keeps_its_node_size_outside() {
        let mut p = patch(
            vec![
                node("a", "math/add"),
                node("s", "meta/subpatch"),
                node("b", "math/add"),
                child("c1", "s"),
                child("c2", "s"),
            ],
            vec![e("a", "s"), e("s", "b"), e("c1", "c2")],
        );
        auto_layout(&mut p, &Headless);
        // Its inside is laid out and the frame sized to it…
        let s = p.nodes.iter().find(|n| n.id == "s").unwrap();
        assert!(s.width.unwrap() >= PAD_LEFT + 2.0 * FALLBACK.0 + H_GAP + PAD_RIGHT);
        // …but outside it's one node wide.
        assert_eq!(at(&p, "b").x - at(&p, "s").x, FALLBACK.0 + H_GAP);
    }

    #[test]
    fn comments_sit_beside_the_graph() {
        let mut cm = node("cm", "meta/comment");
        cm.width = Some(300.0);
        cm.height = Some(120.0);
        let mut p = patch(
            vec![node("a", "math/add"), node("b", "math/add"), cm],
            vec![e("a", "b")],
        );
        auto_layout(&mut p, &Headless);
        let c = at(&p, "cm");
        // A wide, short graph: the comment goes above, clear of it.
        assert_eq!(c.x, 0.0);
        assert_eq!(c.y + 120.0 + COMMENT_GAP, 0.0);
    }

    #[test]
    fn pack_keeps_order_and_gaps() {
        assert_eq!(pack(&[0.0, 0.0], &[1.0, 1.0], &[10.0]), vec![-5.0, 5.0]);
        assert_eq!(pack(&[0.0, 50.0], &[1.0, 1.0], &[10.0]), vec![0.0, 50.0]);
        assert_eq!(pack(&[0.0, 0.0], &[3.0, 1.0], &[8.0]), vec![-2.0, 6.0]);
    }

    #[test]
    fn deterministic_and_idempotent() {
        let mk = || {
            patch(
                (0..12)
                    .map(|i| {
                        let mut n = node(&format!("n{i}"), "math/add");
                        n.position = Position {
                            x: (i * 37 % 11) as f64 * 50.0,
                            y: (i * 53 % 7) as f64 * 40.0,
                        };
                        n
                    })
                    .collect(),
                vec![
                    e("n0", "n3"),
                    e("n1", "n3"),
                    e("n2", "n4"),
                    e("n3", "n5"),
                    e("n4", "n5"),
                    e("n5", "n6"),
                    e("n7", "n6"),
                    e("n8", "n9"),
                    e("n9", "n10"),
                    e("n2", "n10"),
                ],
            )
        };
        let (mut p1, mut p2) = (mk(), mk());
        auto_layout(&mut p1, &Headless);
        auto_layout(&mut p2, &Headless);
        let pos = |p: &PatchFile| p.nodes.iter().map(|n| n.position).collect::<Vec<_>>();
        assert_eq!(pos(&p1), pos(&p2));
        let once = pos(&p1);
        auto_layout(&mut p1, &Headless);
        assert_eq!(pos(&p1), once, "re-arranging a tidy patch changes nothing");
    }
}
