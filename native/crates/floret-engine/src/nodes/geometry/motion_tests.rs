//! Tests for Trim Path, Connect Points and Shape Morph.

use super::connect::{nearest, within, Grid, Link};
use super::measure::Measured;
use super::morph::{align, blend, blend_style, prepare};
use super::relax::lloyd;
use super::scatter::{random, Area};
use super::trim::{trim_each, trim_sequence, Window};
use super::*;
use crate::nodes::value::harness::node;
use crate::scene::{Paint, Style, StyleOp};
use serde_json::json;

fn v(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

fn square() -> Vec<Vec2> {
    vec![v(0.0, 0.0), v(100.0, 0.0), v(100.0, 100.0), v(0.0, 100.0)]
}

fn near(a: &[Vec2], b: &[Vec2]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| (p.x - q.x).abs() < 1e-6 && (p.y - q.y).abs() < 1e-6)
}

fn vecs(out: &[Value]) -> Vec<Vec2> {
    out.iter()
        .map(|p| match p {
            Value::Vec2(p) => *p,
            other => panic!("not a vec2: {other:?}"),
        })
        .collect()
}

fn contour(pts: Vec<Vec2>, closed: bool) -> Contour {
    Contour {
        pts,
        closed,
        style: line_default_style().clone(),
        source: 0,
    }
}

fn poly_layer_of(pts: Vec<Vec2>, style: Arc<ResolvedStyle>) -> Value {
    Value::Scene(Arc::new(SceneNode::Polygon {
        points: Arc::from(pts),
        closed: true,
        style,
        tension: None,
    }))
}

fn filled(css: &str) -> Arc<ResolvedStyle> {
    Arc::new(ResolvedStyle {
        fill: Some(Paint::css(css)),
        ..(**default_style()).clone()
    })
}

// ─── Trim Path ───────────────────────────────────────────────────────────────

fn trim_points(
    pts: &[Vec2],
    closed: bool,
    w: Window,
    sequence: bool,
) -> (Vec<Vec<Vec2>>, Vec<(Vec2, f64)>) {
    let c = contour(pts.to_vec(), closed);
    let paths: Vec<(Measured, &Contour)> = Measured::new(pts, closed)
        .map(|m| (m, &c))
        .into_iter()
        .collect();
    let t = if sequence {
        trim_sequence(&paths, w)
    } else {
        trim_each(&paths, w)
    };
    (t.polys.into_iter().flat_map(|p| p.rings).collect(), t.heads)
}

#[test]
fn trim_window_clamps_swaps_and_wraps() {
    let w = Window::new(0.2, 0.7, 0.0);
    assert!((w.from - 0.2).abs() < 1e-12 && (w.span - 0.5).abs() < 1e-12);
    // Reversed ends swap; out-of-range ends clamp; NaN falls back.
    assert_eq!(Window::new(0.7, 0.2, 0.0), Window::new(0.2, 0.7, 0.0));
    assert_eq!(Window::new(-1.0, 3.0, 0.0), Window::new(0.0, 1.0, 0.0));
    assert_eq!(
        Window::new(f64::NAN, f64::NAN, f64::NAN),
        Window::new(0.0, 1.0, 0.0)
    );
    // The offset wraps either way.
    let w = Window::new(0.0, 0.25, -0.25);
    assert!((w.from - 0.75).abs() < 1e-12 && (w.span - 0.25).abs() < 1e-12);
    assert!((Window::new(0.0, 0.25, 2.5).from - 0.5).abs() < 1e-12);
}

#[test]
fn trim_half_a_closed_square_with_its_head() {
    let (rings, heads) = trim_points(&square(), true, Window::new(0.0, 0.5, 0.0), false);
    assert_eq!(rings.len(), 1);
    assert!(near(
        &rings[0],
        &[v(0.0, 0.0), v(100.0, 0.0), v(100.0, 100.0)]
    ));
    assert_eq!(heads.len(), 1);
    assert!(near(&[heads[0].0], &[v(100.0, 100.0)]));
    assert!((heads[0].1 - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
}

#[test]
fn trim_closed_paths_carry_on_round_their_start() {
    // From 350 to 550 of 400: round the first corner, in one piece.
    let (rings, heads) = trim_points(&square(), true, Window::new(0.0, 0.5, 0.875), false);
    assert_eq!(rings.len(), 1);
    assert!(near(
        &rings[0],
        &[v(0.0, 50.0), v(0.0, 0.0), v(100.0, 0.0), v(100.0, 50.0)]
    ));
    assert!(near(&[heads[0].0], &[v(100.0, 50.0)]));
    // All of it: the closed ring itself, starting at the offset.
    let (rings, _) = trim_points(&square(), true, Window::new(0.0, 1.0, 0.125), false);
    assert_eq!(rings.len(), 1);
    assert!(near(
        &rings[0],
        &[
            v(50.0, 0.0),
            v(100.0, 0.0),
            v(100.0, 100.0),
            v(0.0, 100.0),
            v(0.0, 0.0)
        ]
    ));
}

#[test]
fn trim_open_paths_split_where_they_wrap() {
    let line = [v(0.0, 0.0), v(100.0, 0.0)];
    let (rings, heads) = trim_points(&line, false, Window::new(0.0, 0.3, 0.8), false);
    assert_eq!(rings.len(), 2);
    assert!(near(&rings[0], &[v(80.0, 0.0), v(100.0, 0.0)]));
    assert!(near(&rings[1], &[v(0.0, 0.0), v(10.0, 0.0)]));
    assert!(near(&[heads[0].0], &[v(10.0, 0.0)]));
    // Nothing visible when start == end.
    let (rings, heads) = trim_points(&line, false, Window::new(0.4, 0.4, 0.0), false);
    assert!(rings.is_empty() && heads.is_empty());
}

#[test]
fn trim_degenerate_paths_draw_nothing() {
    for pts in [
        vec![],
        vec![v(5.0, 5.0)],
        vec![v(5.0, 5.0), v(5.0, 5.0)],
        vec![v(f64::NAN, 0.0), v(1.0, f64::INFINITY)],
    ] {
        for seq in [false, true] {
            let (rings, heads) = trim_points(&pts, true, Window::new(0.0, 1.0, 0.0), seq);
            assert!(rings.is_empty() && heads.is_empty(), "{pts:?}");
        }
    }
    // NaN points are skipped, the rest still trims.
    let pts = [v(0.0, 0.0), v(f64::NAN, 1.0), v(10.0, 0.0)];
    let (rings, _) = trim_points(&pts, false, Window::new(0.0, 0.5, 0.0), false);
    assert!(near(&rings[0], &[v(0.0, 0.0), v(5.0, 0.0)]));
}

fn line_layer(y: f64) -> Value {
    Value::Scene(Arc::new(SceneNode::Line {
        x1: 0.0,
        y1: y,
        x2: 100.0,
        y2: y,
        style: line_default_style().clone(),
    }))
}

/// Inputs: points, closed, path, start, end, offset, style.
fn run_trim(mode: &str, path: Vec<Value>, start: f64, end: f64, offset: f64) -> Vec<Vec<Value>> {
    let mut n = node("geometry/trim", json!({ "mode": mode }));
    n.eval(vec![
        vec![],
        vec![],
        path,
        vec![Value::Num(start)],
        vec![Value::Num(end)],
        vec![Value::Num(offset)],
        vec![],
    ])
    .unwrap()
}

#[test]
fn trim_sequence_draws_paths_one_after_another() {
    let lines = vec![line_layer(0.0), line_layer(10.0)];
    let out = run_trim("sequence", lines.clone(), 0.0, 0.75, 0.0);
    // The first line whole, half of the second; one head.
    assert_eq!(out[1].len(), 2);
    assert_eq!(vecs(&out[2]).len(), 4);
    assert_eq!(out[5].len(), 1);
    assert!(near(&vecs(&out[4]), &[v(50.0, 10.0)]));
    // `each` trims both at once: two heads.
    let out = run_trim("each", lines.clone(), 0.0, 0.75, 0.0);
    assert!(near(&vecs(&out[4]), &[v(75.0, 0.0), v(75.0, 10.0)]));
    // Wrapping round the run: the end of the second, the start of the first.
    let out = run_trim("sequence", lines, 0.0, 0.5, 0.75);
    assert_eq!(out[1].len(), 2);
    assert!(near(
        &vecs(&out[2]),
        &[v(0.0, 0.0), v(50.0, 0.0), v(50.0, 10.0), v(100.0, 10.0)]
    ));
}

#[test]
fn trim_filled_shapes_become_lines_in_their_fill_colour() {
    let out = run_trim(
        "each",
        vec![poly_layer_of(square(), filled("#ff0000"))],
        0.0,
        0.5,
        0.0,
    );
    let Value::Scene(l) = &out[1][0] else {
        panic!()
    };
    let SceneNode::Polygon { style, closed, .. } = &**l else {
        panic!()
    };
    assert!(!closed);
    assert!(style.fill.is_none());
    assert_eq!(style.stroke.as_ref(), Some(&Paint::css("#ff0000")));
    // Nothing wired: nothing out, no panic.
    let out = run_trim("each", vec![], 0.0, 1.0, 0.0);
    assert!(out[1].is_empty() && out[4].is_empty());
}

// ─── Connect Points ──────────────────────────────────────────────────────────

fn brute_within(pts: &[Vec2], reach: f64) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for i in 0..pts.len() {
        for j in i + 1..pts.len() {
            if dist(pts[i], pts[j]) < reach {
                out.push((i, j));
            }
        }
    }
    out
}

fn pairs(links: &[Link]) -> Vec<(usize, usize)> {
    let mut p: Vec<(usize, usize)> = links.iter().map(|l| (l.a, l.b)).collect();
    p.sort_unstable();
    p
}

fn cloud(n: usize, seed: f64) -> Vec<Vec2> {
    let mut r = rng(seed);
    (0..n)
        .map(|_| v(r.next_f64() * 600.0 - 300.0, r.next_f64() * 400.0 - 200.0))
        .collect()
}

#[test]
fn connect_within_matches_brute_force() {
    // A 5 × 5 grid 10 apart: only the 40 neighbours along rows / columns.
    let grid: Vec<Vec2> = (0..25)
        .map(|i| v((i % 5) as f64 * 10.0, (i / 5) as f64 * 10.0))
        .collect();
    assert_eq!(within(&grid, 10.5, 0).len(), 40);
    for (n, reach) in [(400, 30.0), (300, 500.0), (500, 2.0)] {
        let pts = cloud(n, n as f64);
        let links = within(&pts, reach, 0);
        assert_eq!(pairs(&links), brute_within(&pts, reach), "{n} {reach}");
        for l in &links {
            assert!((l.d - dist(pts[l.a], pts[l.b])).abs() < 1e-9);
        }
    }
}

#[test]
fn connect_max_links_keeps_the_shortest() {
    let pts = cloud(300, 4.0);
    let links = within(&pts, 60.0, 2);
    let mut degree = vec![0; pts.len()];
    for l in &links {
        degree[l.a] += 1;
        degree[l.b] += 1;
    }
    assert!(degree.iter().all(|d| *d <= 2));
    // The shortest link overall always survives.
    let all = within(&pts, 60.0, 0);
    let shortest = all.iter().min_by(|x, y| x.d.total_cmp(&y.d)).unwrap();
    assert!(links.contains(shortest));
}

#[test]
fn connect_nearest_finds_true_neighbours() {
    let pts = cloud(500, 9.0);
    for k in [1, 3] {
        let links = nearest(&pts, k, 0.0);
        let have = pairs(&links);
        for (i, p) in pts.iter().enumerate() {
            let mut d: Vec<(f64, usize)> = pts
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(j, q)| (dist(*p, *q), j))
                .collect();
            d.sort_by(|x, y| x.0.total_cmp(&y.0));
            for &(_, j) in d.iter().take(k) {
                let pair = (i.min(j), i.max(j));
                assert!(have.binary_search(&pair).is_ok(), "{i}→{j} missing (k {k})");
            }
        }
        // No duplicate lines.
        let mut dedup = have.clone();
        dedup.dedup();
        assert_eq!(dedup, have);
    }
    // A reach caps the links.
    assert!(nearest(&pts, 3, 5.0).iter().all(|l| l.d < 5.0));
}

#[test]
fn connect_edge_cases() {
    assert!(within(&[], 10.0, 0).is_empty());
    assert!(within(&[v(0.0, 0.0)], 10.0, 0).is_empty());
    assert!(within(&cloud(10, 1.0), 0.0, 0).is_empty());
    assert!(within(&cloud(10, 1.0), f64::NAN, 0).is_empty());
    assert!(nearest(&[v(0.0, 0.0)], 3, 0.0).is_empty());
    // Coincident points: a zero-length link, and k beyond n is fine.
    let same = [v(1.0, 1.0), v(1.0, 1.0)];
    assert_eq!(within(&same, 5.0, 0).len(), 1);
    assert_eq!(nearest(&same, 10, 0.0).len(), 1);
    // All on one line (a flat grid).
    let row: Vec<Vec2> = (0..50).map(|i| v(i as f64, 0.0)).collect();
    assert_eq!(within(&row, 1.5, 0).len(), 49);
    // Two each: both sides, and the ends reach two along (49 + 2).
    assert_eq!(nearest(&row, 2, 0.0).len(), 51);
}

#[test]
fn connect_node_fades_long_lines() {
    let mut n = node("geometry/connect", json!({ "mode": "distance" }));
    let red = Style::chain(None, [StyleOp::Stroke(Some(Paint::css("#ff0000")))]);
    // points, distance, maxLinks, fade, taper, colors, colorBy, style
    let pts = vec![
        Value::Vec2(v(0.0, 0.0)),
        Value::Vec2(v(10.0, 0.0)),
        Value::Vec2(v(0.0, 50.0)),
        Value::Vec2(v(f64::NAN, 0.0)),
    ];
    let out = n
        .eval(vec![
            pts,
            vec![Value::Num(100.0)],
            vec![],
            vec![Value::Num(1.0)],
            vec![Value::Num(0.5)],
            vec![],
            vec![],
            vec![Value::Style(red)],
        ])
        .unwrap();
    assert_eq!(out[1].len(), 3);
    let Value::Scene(g) = &out[0][0] else {
        panic!()
    };
    let SceneNode::Group { children, .. } = &**g else {
        panic!()
    };
    let style_of = |i: usize| match &*children[i] {
        SceneNode::Line { style, .. } => style.clone(),
        other => panic!("{other:?}"),
    };
    // Lines in input order: (0,1) 10 long, (0,2) 50, (1,2) ~51.
    let (a, b) = (style_of(0), style_of(1));
    let alpha = |s: &ResolvedStyle| s.stroke.as_ref().unwrap().rgba[3];
    assert!(alpha(&a) > alpha(&b));
    assert!((alpha(&a) - 0.9).abs() < 0.02, "{}", alpha(&a));
    assert!(a.stroke_weight > b.stroke_weight);
    let strengths: Vec<f64> = out[3].iter().map(|s| s.to_number()).collect();
    assert!((strengths[0] - 0.9).abs() < 1e-9);
    // Nearest mode pins.
    let cat = crate::spec::Catalogue::builtin();
    let spec = cat.get("geometry/connect").unwrap();
    let mut p = Params::new();
    p.insert("mode".into(), json!("nearest"));
    let names: Vec<String> = cat
        .resolved_inputs(spec, &p)
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        [
            "points",
            "neighbours",
            "distance",
            "fade",
            "taper",
            "colors",
            "colorBy",
            "style"
        ]
    );
    assert_eq!(spec.inputs.len(), 8);
    let statics: Vec<&str> = spec.inputs.iter().map(|p| &*p.name).collect();
    let ins: Vec<String> = cat
        .resolved_inputs(spec, &Params::new())
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(statics, ins);
}

/// The Connect Points lines for `pts` (all within reach) with `colors`
/// and `colorBy`; no fade or taper.
fn coloured_lines(pts: &[Vec2], colors: Vec<Value>, by: &str) -> Vec<Arc<ResolvedStyle>> {
    let mut n = node(
        "geometry/connect",
        json!({ "mode": "distance", "colorBy": by }),
    );
    let out = n
        .eval(vec![
            pts.iter().map(|p| Value::Vec2(*p)).collect(),
            vec![Value::Num(100.0)],
            vec![],
            vec![Value::Num(0.0)],
            vec![Value::Num(0.0)],
            colors,
            vec![Value::str(by)],
            vec![],
        ])
        .unwrap();
    let Value::Scene(g) = &out[0][0] else {
        panic!()
    };
    let SceneNode::Group { children, .. } = &**g else {
        panic!()
    };
    children
        .iter()
        .map(|c| match &**c {
            SceneNode::Line { style, .. } => style.clone(),
            other => panic!("{other:?}"),
        })
        .collect()
}

fn col(r: f64, g: f64, b: f64) -> Value {
    Value::Color(crate::value::Color { r, g, b, a: 1.0 })
}

#[test]
fn connect_colours_lines_by_their_points() {
    let pts = [v(0.0, 0.0), v(10.0, 0.0), v(20.0, 0.0)];
    // Unwired: the style's own colour (white), as before.
    let plain = coloured_lines(&pts, vec![], "points");
    assert!(plain
        .iter()
        .all(|s| s.stroke.as_ref().unwrap().rgba == [1.0, 1.0, 1.0, 1.0]));
    // Point colours (cycled): lines between different colours get a
    // gradient from one end to the other; same colours stay flat.
    let styles = coloured_lines(&pts, vec![col(1.0, 0.0, 0.0), col(0.0, 0.0, 1.0)], "points");
    // Lines in order: (0,1), (0,2), (1,2): red–blue, red–red, blue–red.
    let g = styles[0].stroke.as_ref().unwrap().gradient.clone().unwrap();
    assert_eq!(g.stops[0].rgba, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(g.stops[1].rgba, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(
        g.shape,
        crate::scene::GradientShape::Linear {
            start: v(0.0, 0.0),
            end: v(10.0, 0.0)
        }
    );
    assert_eq!(g.sample(0.5), [0.5, 0.0, 0.5, 1.0]);
    let flat = styles[1].stroke.as_ref().unwrap();
    assert!(flat.gradient.is_none());
    assert_eq!(flat.rgba, [1.0, 0.0, 0.0, 1.0]);
    let back = styles[2].stroke.as_ref().unwrap().gradient.clone().unwrap();
    assert_eq!(back.stops[0].rgba, [0.0, 0.0, 1.0, 1.0]);
    // One colour: every line flat in it.
    let one = coloured_lines(&pts, vec![col(0.0, 1.0, 0.0)], "points");
    assert!(one.iter().all(|s| {
        let p = s.stroke.as_ref().unwrap();
        p.gradient.is_none() && p.rgba == [0.0, 1.0, 0.0, 1.0]
    }));
}

#[test]
fn connect_colours_lines_by_length() {
    // Lines 0, 50 and 100 long against a reach of 100 (the last is cut).
    let pts = [v(0.0, 0.0), v(0.0, 0.0), v(50.0, 0.0)];
    let styles = coloured_lines(&pts, vec![col(1.0, 0.0, 0.0), col(0.0, 0.0, 1.0)], "length");
    let rgba = |i: usize| styles[i].stroke.as_ref().unwrap().rgba;
    // Touching: the first colour; halfway to the reach: the blend.
    assert_eq!(rgba(0), [1.0, 0.0, 0.0, 1.0]);
    let mid = rgba(1);
    assert!(
        (mid[0] - 0.5).abs() < 0.02 && (mid[2] - 0.5).abs() < 0.02,
        "{mid:?}"
    );
    // Lines of the same length share a style.
    assert!(Arc::ptr_eq(&styles[1], &styles[2]));
}

// ─── Shape Morph ─────────────────────────────────────────────────────────────

fn circle(cx: f64, cy: f64, r: f64, n: usize) -> Vec<Vec2> {
    (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            v(cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

#[test]
fn morph_ends_are_the_shapes() {
    let a = [contour(square(), true)];
    let b = [contour(circle(300.0, 50.0, 50.0, 64), true)];
    let pairs = prepare(&a, &b, 64);
    assert_eq!(pairs.len(), 1);
    let at = |t: f64| blend(&pairs, t, None)[0].rings[0].clone();
    // t = 0 is the square resampled (on its outline), t = 1 the circle.
    for p in at(0.0) {
        let on = p.x.abs() < 1e-6
            || (p.x - 100.0).abs() < 1e-6
            || p.y.abs() < 1e-6
            || (p.y - 100.0).abs() < 1e-6;
        assert!(on, "{p:?}");
    }
    for p in at(1.0) {
        assert!((dist(p, v(300.0, 50.0)) - 50.0).abs() < 1e-6);
    }
    // Halfway: centred halfway.
    let c = centroid(&at(0.5));
    assert!(
        (c.x - 175.0).abs() < 2.0 && (c.y - 50.0).abs() < 2.0,
        "{c:?}"
    );
}

#[test]
fn morph_lines_up_starts_and_windings() {
    let sq = Measured::new(&square(), true).unwrap().even(40);
    // The same square, starting elsewhere and running the other way.
    let mut b = sq.clone();
    b.rotate_left(13);
    b.reverse();
    let a = [contour(sq.clone(), true)];
    let pairs = prepare(&a, &[contour(b, true)], 40);
    // Lined up, the blend never moves: every point stays put.
    let mid = &blend(&pairs, 0.5, None)[0].rings[0];
    assert!(near(mid, &pairs[0].a), "twisted");
    // Open paths: the reversed direction is picked when it matches.
    let line: Vec<Vec2> = (0..10).map(|i| v(i as f64, 0.0)).collect();
    let mut back = line.clone();
    back.reverse();
    assert!(near(&align(&line, back, false), &line));
}

#[test]
fn morph_different_counts_and_open_closed() {
    // A triangle (3 points) into a 200-point circle.
    let tri = vec![v(0.0, 0.0), v(100.0, 0.0), v(50.0, 80.0)];
    let pairs = prepare(
        &[contour(tri.clone(), true)],
        &[contour(circle(0.0, 0.0, 40.0, 200), true)],
        0,
    );
    assert_eq!(pairs[0].a.len(), pairs[0].b.len());
    assert!(pairs[0].a.len() >= 24);
    // An open line into a closed square: open until halfway, then closed.
    let pairs = prepare(
        &[contour(vec![v(0.0, 0.0), v(100.0, 0.0)], false)],
        &[contour(square(), true)],
        32,
    );
    assert!(!blend(&pairs, 0.4, None)[0].closed);
    assert!(blend(&pairs, 0.6, None)[0].closed);
}

#[test]
fn morph_extra_contours_shrink_to_their_centre() {
    // `a`: a square with a square hole (one shape); `b`: a plain square.
    let mut hole = vec![v(40.0, 40.0), v(60.0, 40.0), v(60.0, 60.0), v(40.0, 60.0)];
    hole.reverse();
    let a = [contour(square(), true), contour(hole, true)];
    let b = [contour(square(), true)];
    let pairs = prepare(&a, &b, 16);
    assert_eq!(pairs.len(), 2);
    let end = blend(&pairs, 1.0, None);
    // One polygon: the outer ring and the collapsed hole.
    assert_eq!(end.len(), 1);
    assert_eq!(end[0].rings.len(), 2);
    assert!(end[0].rings[1]
        .iter()
        .all(|p| near(&[*p], &[v(50.0, 50.0)])));
    // The hole runs the other way round from the outer ring.
    let start = blend(&pairs, 0.0, None);
    assert!(signed_area(&start[0].rings[0]) * signed_area(&start[0].rings[1]) < 0.0);
    // Only `b`: it grows from its centre; nothing at all: nothing.
    let grow = prepare(&[], &b, 8);
    assert!(blend(&grow, 0.0, None)[0].rings[0]
        .iter()
        .all(|p| near(&[*p], &[v(50.0, 50.0)])));
    assert!(prepare(&[], &[], 8).is_empty());
    // Degenerate contours are skipped.
    let dot = [contour(vec![v(1.0, 1.0)], true)];
    assert!(prepare(&dot, &dot, 8).is_empty());
}

#[test]
fn morph_blends_colours() {
    let red = filled("#ff0000");
    let blue = filled("#0000ff");
    let mid = blend_style(Some(&red), Some(&blue), 0.5);
    let rgba = mid.fill.as_ref().unwrap().rgba;
    assert!((rgba[0] - 0.5).abs() < 0.01 && (rgba[2] - 0.5).abs() < 0.01);
    // Through the node: t = 0 is all `a`; a NaN t is the default 0.5.
    let mut n = node("geometry/morph", json!({}));
    // a, b, t, points, style
    let out = n
        .eval(vec![
            vec![poly_layer_of(square(), red)],
            vec![poly_layer_of(circle(0.0, 0.0, 50.0, 32), blue)],
            vec![Value::Num(0.0)],
            vec![Value::Num(20.0)],
            vec![],
        ])
        .unwrap();
    assert_eq!(out[3].len(), 1);
    assert_eq!(out[3][0].to_number(), 20.0);
    let Value::Scene(l) = &out[1][0] else {
        panic!()
    };
    let SceneNode::Polygon { style, .. } = &**l else {
        panic!()
    };
    assert_eq!(style.fill.as_ref().unwrap().rgba, [1.0, 0.0, 0.0, 1.0]);
    let out = n
        .eval(vec![
            vec![poly_layer_of(square(), filled("#ff0000"))],
            vec![poly_layer_of(circle(0.0, 0.0, 50.0, 32), filled("#0000ff"))],
            vec![Value::Num(f64::NAN)],
            vec![],
            vec![],
        ])
        .unwrap();
    let Value::Scene(l) = &out[1][0] else {
        panic!()
    };
    let SceneNode::Polygon { style, .. } = &**l else {
        panic!()
    };
    assert!((style.fill.as_ref().unwrap().rgba[2] - 0.5).abs() < 0.01);
}

// ─── Point relax (Scatter `relax`) ───────────────────────────────────────────

/// The smallest distance from any point to its nearest neighbour.
fn min_gap(pts: &[Vec2]) -> f64 {
    let mut best = f64::INFINITY;
    for (i, a) in pts.iter().enumerate() {
        for b in &pts[i + 1..] {
            best = best.min(dist(*a, *b));
        }
    }
    best
}

#[test]
fn grid_nearest_matches_brute_force() {
    let pts = cloud(300, 2.0);
    let grid = Grid::new(&pts, 25.0);
    for q in cloud(200, 3.0)
        .into_iter()
        .chain([v(-5000.0, 3000.0), v(0.0, 0.0)])
    {
        let want = (0..pts.len())
            .min_by(|&a, &b| dist(pts[a], q).total_cmp(&dist(pts[b], q)))
            .unwrap();
        let got = grid.nearest(&pts, q).unwrap();
        assert!((dist(pts[got], q) - dist(pts[want], q)).abs() < 1e-9);
    }
    assert!(Grid::new(&[], 10.0).nearest(&[], v(0.0, 0.0)).is_none());
}

#[test]
fn relax_spreads_points_evenly() {
    let area = Area::rect(v(0.0, 0.0), v(400.0, 300.0));
    let pts = random(&area, 300, 5.0);
    let before = min_gap(&pts);
    let after = lloyd(&area, pts.clone(), 25);
    assert_eq!(after.len(), 300);
    assert!(after.iter().all(|p| area.contains(*p)));
    // Random points crowd; relaxed ones keep a good fraction of the even
    // spacing sqrt(area / n) ≈ 20 apart.
    let gap = min_gap(&after);
    assert!(gap > 3.0 * before && gap > 10.0, "{before} → {gap}");
    // Deterministic.
    assert_eq!(lloyd(&area, pts, 25), after);
}

#[test]
fn relax_stays_inside_shapes() {
    let circle = Value::Scene(Arc::new(SceneNode::Ellipse {
        x: 0.0,
        y: 0.0,
        w: 300.0,
        h: 300.0,
        mode: Arc::from("center"),
        style: default_style().clone(),
    }));
    let area = Area::shape(&[circle]).unwrap();
    let pts = random(&area, 120, 1.0);
    let after = lloyd(&area, pts, 15);
    assert_eq!(after.len(), 120);
    assert!(
        after.iter().all(|p| p.x.hypot(p.y) <= 151.0),
        "left the circle"
    );
    assert!(min_gap(&after) > 8.0);
}

#[test]
fn relax_edge_cases() {
    let area = Area::rect(v(0.0, 0.0), v(100.0, 100.0));
    assert!(lloyd(&area, vec![], 5).is_empty());
    // One point stays put (nothing to even out).
    assert_eq!(lloyd(&area, vec![v(10.0, 10.0)], 5), vec![v(10.0, 10.0)]);
    // NaN points are dropped; zero passes change nothing.
    let pts = vec![v(1.0, 1.0), v(f64::NAN, 0.0), v(-20.0, 30.0)];
    assert_eq!(lloyd(&area, pts.clone(), 0).len(), 2);
    // Coincident points still come apart.
    let two = lloyd(&area, vec![v(0.0, 0.0), v(0.0, 0.0), v(0.0, 0.0)], 5);
    assert_eq!(two.len(), 3);
    assert!(min_gap(&two) > 10.0, "{two:?}");
    // A zero-sized area leaves them where they are.
    let flat = Area::rect(v(0.0, 0.0), v(0.0, 100.0));
    assert_eq!(lloyd(&flat, vec![v(0.0, 1.0), v(0.0, 2.0)], 5).len(), 2);
}

#[test]
fn scatter_relax_pins_and_mode() {
    let cat = crate::spec::Catalogue::builtin();
    let spec = cat.get("geometry/scatter").unwrap();
    let names = |mode: &str| -> Vec<String> {
        let mut p = Params::new();
        p.insert("mode".into(), json!(mode));
        cat.resolved_inputs(spec, &p)
            .into_iter()
            .map(|p| p.name)
            .collect()
    };
    assert_eq!(
        names("random"),
        ["center", "size", "shape", "count", "relax", "seed"]
    );
    assert_eq!(
        names("relax"),
        ["center", "size", "shape", "points", "relax"]
    );
    let statics: Vec<&str> = spec.inputs.iter().map(|p| &*p.name).collect();
    assert_eq!(statics, names("poisson"));
    // Wired points, spread out inside the 200 × 200 square.
    let mut n = node("geometry/scatter", json!({ "mode": "relax" }));
    let crowd: Vec<Value> = (0..40)
        .map(|i| Value::Vec2(v(f64::from(i % 5), f64::from(i / 5))))
        .collect();
    let out = n
        .eval(vec![
            vec![],
            vec![Value::Vec2(v(200.0, 200.0))],
            vec![],
            crowd,
            vec![Value::Num(20.0)],
        ])
        .unwrap();
    let pts = vecs(&out[0]);
    assert_eq!(pts.len(), 40);
    assert!(min_gap(&pts) > 15.0, "{}", min_gap(&pts));
}

// ─── Contours ────────────────────────────────────────────────────────────────

use super::contours::{grid_field, image_field, isolines, metaballs, Field};

/// `n × n` samples of `f(x, y)` over a 200 × 200 square centred on 0.
fn sampled(n: usize, f: impl Fn(f64, f64) -> f64) -> Field {
    let mut values = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let x = -100.0 + 200.0 * i as f64 / (n - 1) as f64;
            let y = -100.0 + 200.0 * j as f64 / (n - 1) as f64;
            values.push(f(x, y));
        }
    }
    grid_field(&values, n, v(0.0, 0.0), v(200.0, 200.0)).unwrap()
}

#[test]
fn contours_of_a_cone_are_circles_round_clockwise() {
    let cone = sampled(41, |x, y| 100.0 - x.hypot(y));
    let rings = isolines(&cone, 50.0);
    assert_eq!(rings.len(), 1);
    for p in &rings[0] {
        assert!((p.x.hypot(p.y) - 50.0).abs() < 0.6, "{p:?}");
    }
    // Higher inside, on the right going round: clockwise on screen.
    assert!(signed_area(&rings[0]) > 0.0);
    // Nested levels.
    assert_eq!(isolines(&cone, 20.0).len(), 1);
    assert!(isolines(&cone, 200.0).is_empty());
}

#[test]
fn contours_close_along_the_edge_and_cut_holes() {
    // A ramp: above the level on the right half, cut off by the border.
    let ramp = sampled(21, |x, _| x);
    let rings = isolines(&ramp, 0.0);
    assert_eq!(rings.len(), 1);
    let area = signed_area(&rings[0]);
    assert!((area - 100.0 * 200.0).abs() < 1.0, "{area}");
    // A plateau with a pit: an outer ring and a hole running the other way.
    let pit = sampled(41, |x, y| if x.hypot(y) < 30.0 { 0.0 } else { 1.0 });
    let rings = isolines(&pit, 0.5);
    assert_eq!(rings.len(), 2);
    let signs: Vec<f64> = rings.iter().map(|r| signed_area(r).signum()).collect();
    assert!(signs.contains(&1.0) && signs.contains(&-1.0), "{signs:?}");
}

#[test]
fn contours_split_saddles_and_skip_bad_input() {
    // A checkerboard of highs and lows: every ring closes.
    let checks = sampled(9, |x, y| {
        if ((x / 25.0).round() + (y / 25.0).round()) as i64 % 2 == 0 {
            1.0
        } else {
            0.0
        }
    });
    let rings = isolines(&checks, 0.5);
    assert!(!rings.is_empty());
    assert!(rings.iter().all(|r| r.len() >= 3));
    // Degenerate fields and levels.
    assert!(grid_field(&[1.0, 2.0, 3.0], 3, v(0.0, 0.0), v(10.0, 10.0)).is_none());
    assert!(grid_field(&[1.0; 8], 1, v(0.0, 0.0), v(10.0, 10.0)).is_none());
    assert!(isolines(&checks, f64::NAN).is_empty());
    // NaN samples count as low.
    let holes = sampled(11, |x, _| if x > 0.0 { f64::NAN } else { 1.0 });
    assert_eq!(isolines(&holes, 0.5).len(), 1);
}

#[test]
fn metaballs_merge_and_images_outline() {
    // One ball: the level-1 contour is its radius.
    let one = metaballs(&[v(0.0, 0.0)], &[40.0], v(0.0, 0.0), v(200.0, 200.0), 2.0).unwrap();
    let rings = isolines(&one, 1.0);
    assert_eq!(rings.len(), 1);
    assert!(rings[0].iter().all(|p| (p.x.hypot(p.y) - 40.0).abs() < 1.5));
    // Two close balls merge into one blob; apart they're two.
    let close = [v(-30.0, 0.0), v(30.0, 0.0)];
    let f = metaballs(&close, &[30.0], v(0.0, 0.0), v(300.0, 200.0), 2.0).unwrap();
    assert_eq!(isolines(&f, 1.0).len(), 1);
    let far = [v(-100.0, 0.0), v(100.0, 0.0)];
    let f = metaballs(&far, &[20.0], v(0.0, 0.0), v(300.0, 200.0), 2.0).unwrap();
    assert_eq!(isolines(&f, 1.0).len(), 2);
    assert!(metaballs(&[], &[], v(0.0, 0.0), v(0.0, 10.0), 2.0).is_none());
    // An image: a white square on black, drawn 2× its pixel size.
    let mut px = vec![0u8; 20 * 20 * 4];
    for y in 5..15 {
        for x in 5..15 {
            let o = (y * 20 + x) * 4;
            px[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
    }
    let img = crate::image::Image::new(20, 20, px);
    let f = image_field(&img, v(0.0, 0.0), v(40.0, 40.0)).unwrap();
    let rings = isolines(&f, 0.5);
    assert_eq!(rings.len(), 1);
    let (lo, hi) = crate::text::outline::bounds(&rings).unwrap();
    assert!(
        lo.x > -12.0 && hi.x < 12.0 && lo.x < -8.0 && hi.x > 8.0,
        "{lo:?} {hi:?}"
    );
}

#[test]
fn contours_node_levels() {
    // values, cols, center, size, threshold, levels, step, style
    let mut n = node("geometry/contours", json!({ "mode": "field" }));
    let cone = sampled(21, |x, y| 100.0 - x.hypot(y));
    let out = n
        .eval(vec![
            cone.values.iter().map(|x| Value::Num(*x)).collect(),
            vec![Value::Num(21.0)],
            vec![],
            vec![Value::Vec2(v(200.0, 200.0))],
            vec![Value::Num(20.0)],
            vec![Value::Num(3.0)],
            vec![Value::Num(25.0)],
            vec![],
        ])
        .unwrap();
    // Three nested rings, one polygon per level.
    assert_eq!(out[1].len(), 3);
    let levels: Vec<f64> = out[4].iter().map(Value::to_number).collect();
    assert_eq!(levels, [20.0, 45.0, 70.0]);
    // Points mode pins; nothing wired draws nothing.
    let mut n = node("geometry/contours", json!({ "mode": "points" }));
    let out = n
        .eval(vec![
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        ])
        .unwrap();
    assert!(out[1].is_empty());
    let cat = crate::spec::Catalogue::builtin();
    let spec = cat.get("geometry/contours").unwrap();
    let statics: Vec<&str> = spec.inputs.iter().map(|p| &*p.name).collect();
    let ins: Vec<String> = cat
        .resolved_inputs(spec, &Params::new())
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(statics, ins);
}

#[test]
fn draw_text_layers_read_as_glyph_outlines() {
    let style = Arc::new(ResolvedStyle {
        font_size: 40.0,
        ..(**default_style()).clone()
    });
    let text = Value::Scene(Arc::new(SceneNode::Text {
        text: Arc::from("Ab"),
        x: 0.0,
        y: 0.0,
        style: style.clone(),
    }));
    let contours = layer_contours(std::slice::from_ref(&text));
    let glyphs = crate::nodes::text_animate::glyphs(&[text]);
    let run = crate::scene::GlyphRun {
        glyphs: glyphs.iter().map(|g| g.glyph).collect(),
        size: 40.0,
        style,
    };
    let want = crate::text::outline::run_contours(&run);
    assert!(want.len() >= 3, "{}", want.len());
    assert_eq!(contours.len(), want.len());
    assert!(contours.iter().zip(&want).all(|(c, w)| near(&c.pts, w)));
}

// ─── Contours: colours and shape distance ────────────────────────────────────

use super::contours::shape_field;

fn square_layer(half: f64) -> Value {
    let pts = vec![
        v(-half, -half),
        v(half, -half),
        v(half, half),
        v(-half, half),
    ];
    poly_layer_of(pts, default_style().clone())
}

#[test]
fn shape_field_is_signed_distance() {
    let sq = square_layer(50.0);
    let f = shape_field(std::slice::from_ref(&sq), v(0.0, 0.0), v(400.0, 400.0), 4.0).unwrap();
    let at = |x: f64, y: f64| {
        let i = ((x - f.origin.x) / f.dx).round() as usize;
        let j = ((y - f.origin.y) / f.dy).round() as usize;
        f.values[j * f.cols + i]
    };
    // Inside positive, outside negative, by distance to the nearest edge.
    assert!((at(0.0, 0.0) - 50.0).abs() < 1e-6, "{}", at(0.0, 0.0));
    assert!((at(100.0, 0.0) + 50.0).abs() < 0.5, "{}", at(100.0, 0.0));
    assert!((at(48.0, 0.0) - 2.0).abs() < 0.5);
    // Round a corner: the distance to the corner point.
    let corner = at(100.0, 100.0);
    assert!((corner + 50.0 * 2f64.sqrt()).abs() < 1.0, "{corner}");
    // Far corner of the grid (reached by dead reckoning) stays accurate.
    let far = at(-200.0, 200.0);
    assert!((far + 150.0 * 2f64.sqrt()).abs() < 1.0, "{far}");
    // Nothing to measure from: no field.
    assert!(shape_field(&[], v(0.0, 0.0), v(100.0, 100.0), 4.0).is_none());
    // Open paths: distance only, never inside.
    let line = line_layer(0.0);
    let f = shape_field(&[line], v(0.0, 0.0), v(100.0, 100.0), 2.0).unwrap();
    assert!(f.values.iter().all(|d| *d <= 1e-9));
}

#[test]
fn contours_shape_mode_rings_round_outlines() {
    // shape, center, size, cell, threshold, levels, step, colors, style
    let mut n = node("geometry/contours", json!({ "mode": "shape" }));
    let out = n
        .eval(vec![
            vec![square_layer(40.0)],
            vec![],
            vec![Value::Vec2(v(400.0, 400.0))],
            vec![Value::Num(2.0)],
            vec![Value::Num(0.0)],
            vec![Value::Num(3.0)],
            vec![Value::Num(20.0)],
            vec![col(1.0, 0.0, 0.0), col(0.0, 0.0, 1.0)],
            vec![],
        ])
        .unwrap();
    // Three rings: the square, then 20 and 40 out; the outer ones first
    // (underneath), each level in its colour.
    let levels: Vec<f64> = out[4].iter().map(Value::to_number).collect();
    assert_eq!(levels, [40.0, 20.0, 0.0]);
    let fills: Vec<[f64; 4]> = out[1]
        .iter()
        .map(|l| match l {
            Value::Scene(s) => match &**s {
                SceneNode::Polygon { style, .. } => style.fill.as_ref().unwrap().rgba,
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(fills[0], [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(fills[2], [1.0, 0.0, 0.0, 1.0]);
    assert!((fills[1][0] - 0.5).abs() < 0.01);
    // The 20-out ring: about the square grown by 20 (round corners).
    let ring_pts: Vec<Vec2> = match &out[1][1] {
        Value::Scene(s) => match &**s {
            SceneNode::Polygon { points, .. } => points.to_vec(),
            _ => panic!(),
        },
        _ => panic!(),
    };
    let max_x = ring_pts.iter().map(|p| p.x).fold(f64::MIN, f64::max);
    assert!((max_x - 60.0).abs() < 1.0, "{max_x}");
    // Nothing wired: nothing drawn.
    let out = n.eval(vec![vec![]; 9]).unwrap();
    assert!(out[1].is_empty());
}

#[test]
fn contours_colour_levels_in_every_mode() {
    // values, cols, center, size, threshold, levels, step, colors, style
    let mut n = node("geometry/contours", json!({ "mode": "field" }));
    let cone = sampled(21, |x, y| 100.0 - x.hypot(y));
    let out = n
        .eval(vec![
            cone.values.iter().map(|x| Value::Num(*x)).collect(),
            vec![Value::Num(21.0)],
            vec![],
            vec![Value::Vec2(v(200.0, 200.0))],
            vec![Value::Num(20.0)],
            vec![Value::Num(2.0)],
            vec![Value::Num(25.0)],
            vec![col(0.0, 1.0, 0.0)],
            vec![Value::Style(Style::chain(
                None,
                [
                    StyleOp::Fill(None),
                    StyleOp::Stroke(Some(Paint::css("#ffffff"))),
                ],
            ))],
        ])
        .unwrap();
    // No fill: the stroke takes the level's colour (one colour: all).
    for l in out[1].iter() {
        let Value::Scene(s) = l else { panic!() };
        let SceneNode::Polygon { style, .. } = &**s else {
            panic!()
        };
        assert!(style.fill.is_none());
        assert_eq!(style.stroke.as_ref().unwrap().rgba, [0.0, 1.0, 0.0, 1.0]);
    }
    assert_eq!(out[1].len(), 2);
}

// ─── Scatter: stipple ────────────────────────────────────────────────────────

/// A `w × h` image, black left of `split` (0..1 across) and white right.
fn half_black(w: u32, h: u32, split: f64) -> Value {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..h {
        for i in 0..w {
            let v = if f64::from(i) < split * f64::from(w) {
                0
            } else {
                255
            };
            px.extend([v, v, v, 255]);
        }
    }
    Value::Image(Arc::new(crate::image::Image::new(w, h, px)))
}

/// Stipple points: center, size, shape, density, count, gamma, invert,
/// relax, seed.
fn stipple(
    n: &mut crate::nodes::value::harness::Harness,
    density: Vec<Value>,
    count: f64,
    invert: bool,
    relax: f64,
) -> Vec<Value> {
    let out = n
        .eval(vec![
            vec![],
            vec![Value::Vec2(v(200.0, 100.0))],
            vec![],
            density,
            vec![Value::Num(count)],
            vec![],
            vec![Value::Bool(invert)],
            vec![Value::Num(relax)],
            vec![Value::Num(3.0)],
        ])
        .unwrap();
    out[0].clone()
}

#[test]
fn stipple_gathers_points_where_the_image_is_dark() {
    let mut n = node("geometry/scatter", json!({ "mode": "stipple" }));
    let img = half_black(40, 20, 0.25);
    let pts = vecs(&stipple(&mut n, vec![img.clone()], 300.0, false, 10.0));
    assert_eq!(pts.len(), 300);
    // Dark: the left quarter of the 200-wide area (x < -50).
    let left = pts.iter().filter(|p| p.x < -50.0).count();
    assert!(left > 285, "{left} of 300 on the dark side");
    assert!(pts.iter().all(|p| p.x.abs() <= 100.0 && p.y.abs() <= 50.0));
    // Relaxed: no two on top of each other.
    assert!(min_gap(&pts) > 1.0, "{}", min_gap(&pts));
    // `invert`: the light side instead.
    let mut m = node("geometry/scatter", json!({ "mode": "stipple" }));
    let inv = vecs(&stipple(&mut m, vec![img.clone()], 300.0, true, 10.0));
    assert!(inv.iter().filter(|p| p.x > -50.0).count() > 285);
    // No image: spread evenly, all of them.
    let mut m = node("geometry/scatter", json!({ "mode": "stipple" }));
    let even = vecs(&stipple(&mut m, vec![], 300.0, false, 10.0));
    assert_eq!(even.len(), 300);
    let l = even.iter().filter(|p| p.x < 0.0).count();
    assert!((120..180).contains(&l), "{l}");
    // A blank (white) image: uniform too, never stuck.
    let mut m = node("geometry/scatter", json!({ "mode": "stipple" }));
    let blank = vecs(&stipple(
        &mut m,
        vec![half_black(8, 8, 0.0)],
        50.0,
        false,
        5.0,
    ));
    assert_eq!(blank.len(), 50);
    // Zero points; deterministic.
    assert!(stipple(&mut m, vec![img.clone()], 0.0, false, 5.0).is_empty());
    let mut a = node("geometry/scatter", json!({ "mode": "stipple" }));
    let mut b = node("geometry/scatter", json!({ "mode": "stipple" }));
    assert_eq!(
        vecs(&stipple(&mut a, vec![img.clone()], 100.0, false, 3.0)),
        vecs(&stipple(&mut b, vec![img.clone()], 100.0, false, 3.0))
    );
}

#[test]
fn stipple_follows_its_inputs() {
    let mut n = node("geometry/scatter", json!({ "mode": "stipple" }));
    let img = half_black(20, 20, 0.5);
    let a = stipple(&mut n, vec![img.clone()], 120.0, false, 5.0);
    let b = stipple(&mut n, vec![img.clone()], 120.0, false, 5.0);
    assert_eq!(vecs(&a), vecs(&b), "same inputs: the same points");
    let c = stipple(&mut n, vec![img.clone()], 121.0, false, 5.0);
    assert_eq!(c.len(), 121);
    // A different picture moves them.
    let d = stipple(&mut n, vec![half_black(20, 20, 0.9)], 121.0, false, 5.0);
    assert_ne!(vecs(&c), vecs(&d));
    // Stipple pins.
    let cat = crate::spec::Catalogue::builtin();
    let spec = cat.get("geometry/scatter").unwrap();
    let mut p = Params::new();
    p.insert("mode".into(), json!("stipple"));
    let names: Vec<String> = cat
        .resolved_inputs(spec, &p)
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        ["center", "size", "shape", "density", "count", "gamma", "invert", "relax", "seed"]
    );
}
