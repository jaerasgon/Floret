//! `geometry/tiles` (Tiles): a grid of tiles, each turned or chosen so
//! the pattern reads as one, by Truchet's rule or by wave function
//! collapse.
//!
//! - `truchet`: every cell takes one of a tile set's turns, at random or
//!   (`noise` > 0) following Perlin noise at that scale per cell, so the
//!   turns clump into regions. Sets: `arcs` (Smith's two quarter circles:
//!   meandering loops), `diagonals` (the "10 PRINT" maze) and `triangles`
//!   (half-square triangles, four ways round).
//! - `wfc`: wave function collapse over edge-matched tiles (the "simple
//!   tiled model"). `sockets` lists the tiles, each four characters for
//!   what its north, east, south and west edges carry (`0` = nothing):
//!   two neighbours must carry the same thing where they meet. Every tile
//!   comes in its four turns (repeats dropped); `*weight` after a tile
//!   makes it likelier (`1100*3`). `closed` makes the outer edges carry
//!   nothing, so no path runs off the grid. A tile is drawn as pipes from its
//!   carrying edges: straight across, a quarter circle round a corner, to
//!   the middle (with a dot for a dead end) otherwise. A contradiction
//!   restarts the collapse (at most [`ATTEMPTS`] times).
//!
//! Both are deterministic for a `seed`. Outputs: `shape` (every tile),
//! `layers` (one per cell), `centers`, `tiles` (which tile of the set) and
//! `angles` (its turn, radians clockwise), so a ForEach can place its own
//! artwork instead.

use super::{
    as_line, flat, int_pin, live_mode, num_pin, param_str, pin_style, rng, style_pin, vec2_pin,
    Pins,
};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::{ResolvedStyle, SceneNode};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Spread, Value, Vec2};
use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::sync::Arc;

const MODE: &str = "truchet";
/// Most cells a side.
pub const MAX_SIDE: usize = 64;
/// Fresh starts the collapse makes after a contradiction.
pub const ATTEMPTS: usize = 12;
/// Most tiles (turns included) in a `wfc` set.
pub const MAX_VARIANTS: usize = 64;
/// The Truchet sets, in dropdown order.
pub const SETS: [&str; 3] = ["arcs", "diagonals", "triangles"];
/// `wfc`'s tiles when `sockets` is empty: blanks, straights, corners, a
/// few tees, crossings and dead ends.
pub const DEFAULT_SOCKETS: &str = "0000*4 1010 1100*3 1110*0.6 1111*0.3 1000*0.3";

pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = vec![
        int_pin("cols", 12.0),
        int_pin("rows", 12.0),
        num_pin("size", 40.0),
        vec2_pin("center", 0.0, 0.0),
    ];
    if param_str(params, "mode", MODE) == "wfc" {
        v.push(PortSpec::new("sockets", PortType::String).default(Value::str(DEFAULT_SOCKETS)));
        v.push(PortSpec::new("closed", PortType::Boolean).default(Value::Bool(true)));
    } else {
        let mut set = PortSpec::new("tiles", PortType::String).default(Value::str("arcs"));
        set.param = Some("tiles".into());
        v.push(set);
        v.push(num_pin("noise", 0.0));
    }
    v.push(int_pin("seed", 1.0));
    v.push(PortSpec::new("colors", PortType::Color).kind(PortKind::Spread));
    v.push(style_pin());
    v
}

/// One placed tile: its cell, which tile of the set, and its quarter
/// turns clockwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Placed {
    pub col: usize,
    pub row: usize,
    pub tile: usize,
    pub turns: usize,
}

// ─── Truchet ─────────────────────────────────────────────────────────────────

/// Turns each tile of a set comes in.
fn set_turns(set: &str) -> usize {
    if set == "triangles" {
        4
    } else {
        2
    }
}

/// A Truchet grid: each cell's turn at random, or by noise at `noise`
/// (cells per noise unit⁻¹) when it's above 0.
pub(super) fn truchet(cols: usize, rows: usize, set: &str, noise: f64, seed: f64) -> Vec<Placed> {
    let n = set_turns(set);
    let mut r = rng(seed);
    let nseed = crate::seed::mix(seed);
    let mut out = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            let u = r.next_f64();
            let t = if noise > 0.0 && noise.is_finite() {
                let v = crate::nodes::noise::perlin_2d(
                    col as f64 * noise + 0.5,
                    row as f64 * noise + 0.5,
                    nseed,
                );
                // Perlin sits mostly in 0.25..0.75: stretch that to 0..1.
                ((v - 0.5) * 2.2 + 0.5).clamp(0.0, 0.999_999)
            } else {
                u
            };
            out.push(Placed {
                col,
                row,
                tile: 0,
                turns: ((t * n as f64) as usize).min(n - 1),
            });
        }
    }
    out
}

/// A Truchet tile's outlines in a cell of half-side `h` centred on the
/// origin: `(points, closed)`.
fn truchet_shapes(set: &str, turns: usize, h: f64) -> Vec<(Vec<Vec2>, bool)> {
    let p = |x: f64, y: f64| Vec2 { x, y };
    match set {
        "diagonals" => {
            if turns.is_multiple_of(2) {
                vec![(vec![p(-h, -h), p(h, h)], false)]
            } else {
                vec![(vec![p(h, -h), p(-h, h)], false)]
            }
        }
        "triangles" => {
            // The corner the triangle fills, clockwise from top left.
            let c = [p(-h, -h), p(h, -h), p(h, h), p(-h, h)];
            let k = turns % 4;
            vec![(vec![c[k], c[(k + 1) % 4], c[(k + 3) % 4]], true)]
        }
        _ => {
            // Two quarter circles round opposite corners.
            let (a, b) = if turns.is_multiple_of(2) {
                ((p(-h, -h), 0.0), (p(h, h), PI))
            } else {
                ((p(h, -h), FRAC_PI_2), (p(-h, h), 1.5 * PI))
            };
            vec![(arc(a.0, h, a.1), false), (arc(b.0, h, b.1), false)]
        }
    }
}

/// A quarter circle of radius `r` round `c`, from angle `start` on
/// (clockwise on screen).
fn arc(c: Vec2, r: f64, start: f64) -> Vec<Vec2> {
    const STEPS: usize = 12;
    (0..=STEPS)
        .map(|i| {
            let a = start + FRAC_PI_2 * i as f64 / STEPS as f64;
            Vec2 {
                x: c.x + r * a.cos(),
                y: c.y + r * a.sin(),
            }
        })
        .collect()
}

// ─── Wave function collapse ──────────────────────────────────────────────────

/// A tile set for `wfc`: every distinct turn of every tile.
#[derive(Clone, Debug)]
pub(super) struct TileSet {
    /// Edge sockets N, E, S, W.
    pub edges: Vec<[char; 4]>,
    /// Which listed tile each variant is, and its quarter turns.
    pub source: Vec<(usize, usize)>,
    pub weight: Vec<f64>,
}

/// Parse `sockets` ("1100*2 1010 …"), and the pieces that aren't tiles.
pub(super) fn parse_sockets(text: &str) -> (TileSet, Vec<String>) {
    let mut set = TileSet {
        edges: Vec::new(),
        source: Vec::new(),
        weight: Vec::new(),
    };
    let mut bad = Vec::new();
    let tokens = text
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|t| !t.is_empty());
    for (index, tok) in tokens.enumerate() {
        let (spec, w) = match tok.split_once('*') {
            Some((s, w)) => (s, w.trim().parse::<f64>().ok()),
            None => (tok, Some(1.0)),
        };
        let chars: Vec<char> = spec.chars().collect();
        let Some(w) = w.filter(|w| w.is_finite() && *w > 0.0) else {
            bad.push(tok.to_owned());
            continue;
        };
        if chars.len() != 4 {
            bad.push(tok.to_owned());
            continue;
        }
        let base = [chars[0], chars[1], chars[2], chars[3]];
        // Each distinct turn, the tile's weight shared between them.
        let mut turns: Vec<(usize, [char; 4])> = Vec::new();
        for k in 0..4 {
            let e = rotate(base, k);
            if !turns.iter().any(|(_, x)| *x == e) {
                turns.push((k, e));
            }
        }
        let share = w / turns.len() as f64;
        for (k, e) in turns {
            if set.edges.contains(&e) || set.edges.len() >= MAX_VARIANTS {
                continue;
            }
            set.edges.push(e);
            set.source.push((index, k));
            set.weight.push(share);
        }
    }
    (set, bad)
}

/// Edges after `k` quarter turns clockwise: what was west faces north.
fn rotate(e: [char; 4], k: usize) -> [char; 4] {
    let mut out = e;
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = e[(i + 4 - k % 4) % 4];
    }
    out
}

fn empty(c: char) -> bool {
    c == '0' || c == '.'
}

/// Collapse a `cols × rows` grid over `set`: each cell's variant, and
/// whether every attempt hit a contradiction (the last one's cells that
/// never settled take their likeliest option, unmatched).
pub(super) fn collapse(
    set: &TileSet,
    cols: usize,
    rows: usize,
    closed: bool,
    seed: f64,
) -> (Vec<usize>, bool) {
    let n = set.edges.len();
    if n == 0 || cols == 0 || rows == 0 {
        return (Vec::new(), false);
    }
    let all: u64 = if n == 64 { u64::MAX } else { (1u64 << n) - 1 };
    // compat[d][a]: the variants that may sit in direction d of a
    // (0 N, 1 E, 2 S, 3 W).
    let mut compat = [[0u64; MAX_VARIANTS]; 4];
    for (a, ea) in set.edges.iter().enumerate() {
        for (b, eb) in set.edges.iter().enumerate() {
            for (d, row) in compat.iter_mut().enumerate() {
                if ea[d] == eb[(d + 2) % 4] {
                    row[a] |= 1 << b;
                }
            }
        }
    }
    let cells = cols * rows;
    let mut r = rng(seed);
    let mut last = Vec::new();
    for _ in 0..ATTEMPTS {
        let mut wave = vec![all; cells];
        if closed {
            for (k, w) in wave.iter_mut().enumerate() {
                let (i, j) = (k % cols, k / cols);
                for v in 0..n {
                    let e = set.edges[v];
                    let out = (j == 0 && !empty(e[0]))
                        || (i == cols - 1 && !empty(e[1]))
                        || (j == rows - 1 && !empty(e[2]))
                        || (i == 0 && !empty(e[3]));
                    if out {
                        *w &= !(1 << v);
                    }
                }
            }
        }
        let mut ok = propagate(&mut wave, (0..cells).collect(), &compat, n, cols, rows);
        while ok {
            // The undecided cell with the fewest options (by weight
            // entropy), ties broken at random.
            let mut best: Option<(f64, usize)> = None;
            for (k, &w) in wave.iter().enumerate() {
                if w.count_ones() <= 1 {
                    continue;
                }
                let (mut sum, mut sum_log) = (0.0, 0.0);
                for v in 0..n {
                    if w & (1 << v) != 0 {
                        let x = set.weight[v];
                        sum += x;
                        sum_log += x * x.ln();
                    }
                }
                let entropy = sum.ln() - sum_log / sum + r.next_f64() * 1e-6;
                if best.is_none_or(|(e, _)| entropy < e) {
                    best = Some((entropy, k));
                }
            }
            let Some((_, k)) = best else { break };
            // Pick one of its options by weight.
            let w = wave[k];
            let total: f64 = (0..n)
                .filter(|v| w & (1 << v) != 0)
                .map(|v| set.weight[v])
                .sum();
            let mut x = r.next_f64() * total;
            let mut pick = w.trailing_zeros() as usize;
            for v in 0..n {
                if w & (1 << v) != 0 {
                    pick = v;
                    x -= set.weight[v];
                    if x < 0.0 {
                        break;
                    }
                }
            }
            wave[k] = 1 << pick;
            ok = propagate(&mut wave, vec![k], &compat, n, cols, rows);
        }
        let chosen: Vec<usize> = wave
            .iter()
            .map(|&w| {
                // A contradiction (no option left) falls back to the
                // heaviest tile.
                if w == 0 {
                    (0..n)
                        .max_by(|a, b| set.weight[*a].total_cmp(&set.weight[*b]))
                        .unwrap_or(0)
                } else {
                    w.trailing_zeros() as usize
                }
            })
            .collect();
        if ok {
            return (chosen, false);
        }
        last = chosen;
    }
    (last, true)
}

/// Narrow the neighbours of the cells in `stack` until nothing changes;
/// false on a contradiction.
fn propagate(
    wave: &mut [u64],
    mut stack: Vec<usize>,
    compat: &[[u64; MAX_VARIANTS]; 4],
    n: usize,
    cols: usize,
    rows: usize,
) -> bool {
    while let Some(k) = stack.pop() {
        let w = wave[k];
        if w == 0 {
            return false;
        }
        let (i, j) = (k % cols, k / cols);
        let nbrs = [
            (j > 0).then(|| k - cols),
            (i + 1 < cols).then(|| k + 1),
            (j + 1 < rows).then(|| k + cols),
            (i > 0).then(|| k - 1),
        ];
        for (d, nb) in nbrs.into_iter().enumerate() {
            let Some(nb) = nb else { continue };
            let mut allowed = 0u64;
            for (v, m) in compat[d].iter().enumerate().take(n) {
                if w & (1 << v) != 0 {
                    allowed |= m;
                }
            }
            let next = wave[nb] & allowed;
            if next != wave[nb] {
                wave[nb] = next;
                if next == 0 {
                    return false;
                }
                stack.push(nb);
            }
        }
    }
    true
}

/// A `wfc` tile drawn as pipes in a cell of half-side `h` centred on the
/// origin: `(points, closed)`.
fn pipe_shapes(edges: [char; 4], h: f64) -> Vec<(Vec<Vec2>, bool)> {
    let mid = |d: usize| match d {
        0 => Vec2 { x: 0.0, y: -h },
        1 => Vec2 { x: h, y: 0.0 },
        2 => Vec2 { x: 0.0, y: h },
        _ => Vec2 { x: -h, y: 0.0 },
    };
    let on: Vec<usize> = (0..4).filter(|d| !empty(edges[*d])).collect();
    let centre = Vec2::default();
    match on.as_slice() {
        [] => Vec::new(),
        [d] => {
            let dot = (0..16)
                .map(|i| {
                    let a = TAU * i as f64 / 16.0;
                    Vec2 {
                        x: h * 0.22 * a.cos(),
                        y: h * 0.22 * a.sin(),
                    }
                })
                .collect();
            vec![(vec![mid(*d), centre], false), (dot, true)]
        }
        [a, b] if b - a == 2 => vec![(vec![mid(*a), mid(*b)], false)],
        [a, b] => {
            // Round the corner the two edges share.
            let (x, y) = match (a, b) {
                (0, 1) => (h, -h),
                (1, 2) => (h, h),
                (2, 3) => (-h, h),
                _ => (-h, -h),
            };
            let c = Vec2 { x, y };
            let s = mid(*a);
            let start = (s.y - c.y).atan2(s.x - c.x);
            let e = mid(*b);
            let end = (e.y - c.y).atan2(e.x - c.x);
            // The short way round (a quarter turn).
            let mut sweep = end - start;
            while sweep > PI {
                sweep -= TAU;
            }
            while sweep < -PI {
                sweep += TAU;
            }
            let pts = (0..=12)
                .map(|i| {
                    let t = start + sweep * i as f64 / 12.0;
                    Vec2 {
                        x: c.x + h * t.cos(),
                        y: c.y + h * t.sin(),
                    }
                })
                .collect();
            vec![(pts, false)]
        }
        _ => on.iter().map(|d| (vec![mid(*d), centre], false)).collect(),
    }
}

// ─── The node ────────────────────────────────────────────────────────────────

fn styled(base: &Arc<ResolvedStyle>, color: Option<[f64; 4]>, closed: bool) -> Arc<ResolvedStyle> {
    let Some(c) = color else {
        return base.clone();
    };
    let mut st = (**base).clone();
    if closed && st.fill.is_some() {
        st.fill = Some(flat(c));
    } else if st.stroke.is_some() {
        st.stroke = Some(flat(c));
    } else {
        st.fill = Some(flat(c));
    }
    Arc::new(st)
}

pub(super) fn tiles(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    raw(move |inputs, cx| {
        let mode = live_mode(cx, MODE);
        let side = |name: &str| {
            let v = pins.num(inputs, name, 12.0).max(0.0) as usize;
            if v > MAX_SIDE {
                cx.warn(format!("At most {MAX_SIDE} {name}."));
            }
            v.min(MAX_SIDE)
        };
        let (cols, rows) = (side("cols"), side("rows"));
        let size = pins.num(inputs, "size", 40.0).abs();
        let half = size / 2.0;
        let center = pins.vec2(inputs, "center", Vec2::default());
        let seed = pins.num(inputs, "seed", 1.0);

        let cells: Vec<Cell> = if mode == "wfc" {
            let text = match pins.value(inputs, "sockets") {
                Value::Str(s) if !s.trim().is_empty() => s.to_string(),
                _ => DEFAULT_SOCKETS.to_owned(),
            };
            let (set, bad) = parse_sockets(&text);
            if !bad.is_empty() {
                cx.warn(format!(
                    "Not a tile (four edge characters, then *weight): {}",
                    bad.join(", ")
                ));
            }
            let closed = pins.bool(inputs, "closed", true);
            let (wave, failed) = collapse(&set, cols, rows, closed, seed);
            if failed {
                cx.warn(format!(
                    "No arrangement found in {ATTEMPTS} tries: some tiles don't match."
                ));
            }
            wave.iter()
                .enumerate()
                .map(|(k, &v)| {
                    let (tile, turns) = set.source[v];
                    Cell {
                        col: k % cols,
                        row: k / cols,
                        tile,
                        turns,
                        shapes: pipe_shapes(set.edges[v], half),
                    }
                })
                .collect()
        } else {
            let set = match pins.value(inputs, "tiles") {
                Value::Str(s) if SETS.contains(&&*s) => s.to_string(),
                _ => "arcs".to_owned(),
            };
            let noise = pins.num(inputs, "noise", 0.0);
            truchet(cols, rows, &set, noise, seed)
                .into_iter()
                .map(|p| Cell {
                    col: p.col,
                    row: p.row,
                    tile: 0,
                    turns: p.turns,
                    shapes: truchet_shapes(&set, p.turns, half),
                })
                .collect()
        };

        let colors = pins.colors(inputs, "colors");
        let style_v = pins.value(inputs, "style");
        let (line_style, fill_style) = (
            as_line(&pin_style(&style_v, false)),
            pin_style(&style_v, true),
        );
        let (w, h) = (cols as f64 * size, rows as f64 * size);
        let mut layers = Vec::with_capacity(cells.len());
        let mut centers = Vec::with_capacity(cells.len());
        let mut tiles = Vec::with_capacity(cells.len());
        let mut angles = Vec::with_capacity(cells.len());
        for cell in cells {
            let c = Vec2 {
                x: center.x - w / 2.0 + (cell.col as f64 + 0.5) * size,
                y: center.y - h / 2.0 + (cell.row as f64 + 0.5) * size,
            };
            // Colour k for the k-th tile of the set (its turn in truchet).
            let key = if mode == "wfc" { cell.tile } else { cell.turns };
            let color = (!colors.is_empty()).then(|| colors[key % colors.len()]);
            let parts: Vec<Arc<SceneNode>> = cell
                .shapes
                .into_iter()
                .map(|(pts, closed)| {
                    let base = if closed { &fill_style } else { &line_style };
                    Arc::new(SceneNode::Polygon {
                        points: pts
                            .into_iter()
                            .map(|q| Vec2 {
                                x: q.x + c.x,
                                y: q.y + c.y,
                            })
                            .collect(),
                        closed,
                        style: styled(base, color, closed),
                        tension: None,
                    })
                })
                .collect();
            layers.push(Arc::new(SceneNode::group(parts)));
            centers.push(Value::Vec2(c));
            tiles.push(Value::Num(cell.tile as f64));
            angles.push(Value::Num(cell.turns as f64 * FRAC_PI_2));
        }
        Ok(vec![
            crate::value::one(Value::Scene(Arc::new(SceneNode::group(layers.clone())))),
            layers.into_iter().map(Value::Scene).collect(),
            Spread::from(centers),
            Spread::from(tiles),
            Spread::from(angles),
        ])
    })
}

/// One cell of the grid: which tile of the set, its quarter turns, and
/// its outlines round the cell's centre (`(points, closed)`).
struct Cell {
    col: usize,
    row: usize,
    tile: usize,
    turns: usize,
    shapes: Vec<(Vec<Vec2>, bool)>,
}
