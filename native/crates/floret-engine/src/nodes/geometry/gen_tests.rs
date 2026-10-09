//! Tests for the generative modes: Cells `subdivide`, Contours `flow` and
//! the Tiles node.

use super::cells::{subdivide, Split, MAX_RECTS};
use super::flow::{streamlines, Field, Trace};
use super::tiles::{collapse, parse_sockets, truchet};
use super::*;
use crate::nodes::value::harness::node;
use crate::value::Color;
use serde_json::json;

fn v(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

fn split(depth: usize, chance: f64, quad: bool) -> Split {
    Split {
        depth,
        chance,
        min_size: 4.0,
        ratio: 0.6,
        quad,
    }
}

// ─── Subdivide ───────────────────────────────────────────────────────────────

#[test]
fn subdivide_tiles_the_rectangle_exactly() {
    for quad in [false, true] {
        let (cells, capped) = subdivide(
            v(10.0, -5.0),
            v(400.0, 300.0),
            &[],
            &split(6, 0.7, quad),
            3.0,
        );
        assert!(!capped);
        assert!(cells.len() > 4, "{}", cells.len());
        let area: f64 = cells.iter().map(|c| c.w * c.h).sum();
        assert!((area - 120_000.0).abs() < 1e-6, "{area}");
        for c in &cells {
            assert!(c.x >= -190.0 - 1e-9 && c.x + c.w <= 210.0 + 1e-9);
            assert!(c.y >= -155.0 - 1e-9 && c.y + c.h <= 145.0 + 1e-9);
            assert!(c.depth >= 1 && c.depth <= 6);
            assert!(c.w > 0.0 && c.h > 0.0);
        }
        // No two overlap (a pair's intersection has no area).
        for (i, a) in cells.iter().enumerate() {
            for b in &cells[i + 1..] {
                let ox = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                let oy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                assert!(ox <= 1e-9 || oy <= 1e-9, "{a:?} overlaps {b:?}");
            }
        }
    }
}

#[test]
fn subdivide_is_seeded_and_respects_its_limits() {
    let run = |seed: f64| {
        subdivide(
            v(0.0, 0.0),
            v(400.0, 400.0),
            &[],
            &split(5, 0.7, false),
            seed,
        )
        .0
    };
    assert_eq!(run(1.0), run(1.0));
    assert_ne!(run(1.0), run(2.0));
    // Depth 0: the rectangle itself; chance 0: one split only.
    let (one, _) = subdivide(
        v(0.0, 0.0),
        v(100.0, 100.0),
        &[],
        &split(0, 1.0, false),
        1.0,
    );
    assert_eq!(one.len(), 1);
    let (two, _) = subdivide(
        v(0.0, 0.0),
        v(100.0, 100.0),
        &[],
        &split(8, 0.0, false),
        1.0,
    );
    assert_eq!(two.len(), 2);
    let (four, _) = subdivide(v(0.0, 0.0), v(100.0, 100.0), &[], &split(8, 0.0, true), 1.0);
    assert_eq!(four.len(), 4);
    // Never thinner than minSize.
    let o = Split {
        min_size: 30.0,
        ..split(16, 1.0, false)
    };
    let (cells, _) = subdivide(v(0.0, 0.0), v(400.0, 400.0), &[], &o, 5.0);
    assert!(cells
        .iter()
        .all(|c| c.w >= 30.0 - 1e-9 && c.h >= 30.0 - 1e-9));
    // A huge request stops at the cap.
    let o = Split {
        min_size: 0.0,
        ..split(16, 1.0, true)
    };
    let (cells, capped) = subdivide(v(0.0, 0.0), v(4000.0, 4000.0), &[], &o, 5.0);
    assert!(capped && cells.len() <= MAX_RECTS);
    // Degenerate sizes give nothing.
    for size in [v(0.0, 100.0), v(f64::NAN, 1.0), v(f64::INFINITY, 5.0)] {
        assert!(
            subdivide(v(0.0, 0.0), size, &[], &split(4, 1.0, false), 1.0)
                .0
                .is_empty()
        );
    }
}

#[test]
fn subdivide_with_points_splits_until_each_is_alone() {
    let pts = [v(-150.0, -150.0), v(-140.0, -140.0), v(100.0, 100.0)];
    let o = Split {
        min_size: 1.0,
        ..split(12, 0.0, true)
    };
    let (cells, _) = subdivide(v(0.0, 0.0), v(400.0, 400.0), &pts, &o, 1.0);
    for c in &cells {
        let inside = pts
            .iter()
            .filter(|p| p.x >= c.x && p.x < c.x + c.w && p.y >= c.y && p.y < c.y + c.h)
            .count();
        assert!(inside <= 1, "{c:?} holds {inside}");
    }
    // The crowded corner gets small cells, the empty side stays big.
    let smallest = cells.iter().map(|c| c.w).fold(f64::INFINITY, f64::min);
    let largest = cells.iter().map(|c| c.w).fold(0.0, f64::max);
    assert!(largest > smallest * 4.0);
}

#[test]
fn cells_node_subdivides_with_random_colours_and_depths() {
    let mut n = node("geometry/cells", json!({"mode": "subdivide"}));
    let c = |r: f64| {
        Value::Color(Color {
            r,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        })
    };
    // points, center, size, depth, chance, minSize, ratio, quad, seed, gap, colors, style
    let out = n
        .eval(vec![
            vec![],
            vec![Value::Vec2(v(0.0, 0.0))],
            vec![Value::Vec2(v(400.0, 400.0))],
            vec![Value::Num(4.0)],
            vec![Value::Num(1.0)],
            vec![Value::Num(1.0)],
            vec![Value::Num(0.5)],
            vec![Value::Bool(false)],
            vec![Value::Num(1.0)],
            vec![Value::Num(4.0)],
            vec![c(0.0), c(0.5), c(1.0)],
            vec![Value::Null],
        ])
        .unwrap();
    assert_eq!(out.len(), 6);
    // Depth 4, always splitting: 16 rectangles, all at depth 4.
    assert_eq!(out[1].len(), 16);
    assert_eq!(out[5].len(), 16);
    assert!(out[5].iter().all(|d| d.as_f64() == Some(4.0)));
    // More than one of the colours gets used (picked at random).
    let fills: std::collections::HashSet<String> = out[1]
        .iter()
        .map(|l| match l {
            Value::Scene(n) => match &**n {
                SceneNode::Polygon { style, .. } => style.fill.as_ref().unwrap().css.to_string(),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(fills.len() > 1, "{fills:?}");
}

#[test]
fn cells_node_keeps_its_other_modes_and_adds_depth() {
    let mut n = node("geometry/cells", json!({"mode": "delaunay"}));
    let pts = [v(0.0, 0.0), v(100.0, 0.0), v(100.0, 100.0), v(0.0, 100.0)];
    let out = n
        .eval(vec![
            pts.iter().map(|p| Value::Vec2(*p)).collect(),
            vec![Value::Num(0.0)],
            vec![],
            vec![Value::Null],
        ])
        .unwrap();
    assert_eq!(out[1].len(), 2);
    let depths: Vec<f64> = out[5].iter().map(|d| d.as_f64().unwrap()).collect();
    assert_eq!(depths, [0.0, 0.0]);
}

// ─── Flow ────────────────────────────────────────────────────────────────────

fn trace(spacing: f64) -> Trace {
    Trace {
        center: v(0.0, 0.0),
        size: v(400.0, 300.0),
        turn: 0.0,
        spacing,
        step: 2.0,
        max_len: 300.0,
        min_len: 10.0,
        seed: 1.0,
    }
}

fn length(l: &[Vec2]) -> f64 {
    l.windows(2).map(|p| dist(p[0], p[1])).sum()
}

#[test]
fn flow_lines_are_evenly_spaced_and_stay_inside() {
    let f = Field::Angle {
        scale: 0.006,
        seed: 1.0,
    };
    let (lines, capped) = streamlines(&f, &[], &trace(12.0));
    assert!(!capped);
    assert!(lines.len() > 20, "{}", lines.len());
    for l in &lines {
        assert!(l.len() >= 2);
        assert!(length(l) >= 10.0 - 1e-9 && length(l) <= 300.0 + 2.0 + 1e-9);
        for p in l {
            assert!(
                p.x.abs() <= 200.0 + 1e-9 && p.y.abs() <= 150.0 + 1e-9,
                "{p:?}"
            );
        }
    }
    // Points of different lines keep at least half a spacing apart
    // (the test distance), with a step's slack.
    for (i, a) in lines.iter().enumerate() {
        for b in &lines[i + 1..] {
            for p in a.iter().step_by(3) {
                for q in b.iter().step_by(3) {
                    assert!(dist(*p, *q) >= 6.0 - 2.0 - 1e-6, "{p:?} {q:?}");
                }
            }
        }
    }
    // They cover the rectangle: most of a coarse grid has a line near.
    let mut near = 0;
    let mut total = 0;
    for j in 0..10 {
        for i in 0..13 {
            let c = v(-180.0 + i as f64 * 30.0, -135.0 + j as f64 * 30.0);
            total += 1;
            if lines.iter().flatten().any(|p| dist(*p, c) < 15.0) {
                near += 1;
            }
        }
    }
    assert!(near * 10 >= total * 9, "{near} / {total}");
}

#[test]
fn flow_follows_a_wired_field_and_is_seeded() {
    // A uniform field pointing right: straight horizontal lines.
    let grid = Field::grid(vec![v(1.0, 0.0); 16], 4, v(0.0, 0.0), v(400.0, 300.0)).unwrap();
    let (lines, _) = streamlines(&grid, &[v(0.0, 0.0), v(0.0, 50.0)], &trace(10.0));
    assert_eq!(lines.len(), 2);
    for l in &lines {
        let y = l[0].y;
        assert!(l.iter().all(|p| (p.y - y).abs() < 1e-9));
        assert!(l.last().unwrap().x > l[0].x, "runs with the field");
    }
    // Too short a field grid: nothing.
    assert!(Field::grid(vec![v(1.0, 0.0); 3], 4, v(0.0, 0.0), v(10.0, 10.0)).is_none());
    // `turn` a quarter turn: vertical lines.
    let o = Trace {
        turn: std::f64::consts::FRAC_PI_2,
        ..trace(10.0)
    };
    let (lines, _) = streamlines(&grid, &[v(0.0, 0.0)], &o);
    assert!(lines[0].iter().all(|p| p.x.abs() < 1e-9));
    // Same seed, same lines; another seed, others.
    let curl = |seed| Field::Curl { scale: 0.01, seed };
    let a = streamlines(&curl(1.0), &[], &trace(10.0)).0;
    assert_eq!(a, streamlines(&curl(1.0), &[], &trace(10.0)).0);
    assert_ne!(a, streamlines(&curl(2.0), &[], &trace(10.0)).0);
    // Degenerate rectangles and NaNs give nothing, not a hang.
    for size in [v(0.0, 10.0), v(f64::NAN, 10.0)] {
        let o = Trace {
            size,
            ..trace(10.0)
        };
        assert!(streamlines(&curl(1.0), &[], &o).0.is_empty());
    }
    let o = Trace {
        step: f64::NAN,
        spacing: f64::NAN,
        max_len: f64::NAN,
        ..trace(10.0)
    };
    let (lines, _) = streamlines(&curl(1.0), &[v(0.0, 0.0)], &o);
    assert!(lines.iter().flatten().all(|p| p.x.is_finite()));
}

#[test]
fn contours_flow_mode_outputs_lines_and_lengths() {
    let mut n = node("geometry/contours", json!({"mode": "flow"}));
    let cat = crate::spec::Catalogue::builtin();
    let spec = cat.get("geometry/contours").unwrap();
    let ports = cat.resolved_inputs(spec, json!({"mode": "flow"}).as_object().unwrap());
    let inputs = ports
        .iter()
        .map(|p| crate::compile::default_for_port(p).to_vec())
        .collect();
    let out = n.eval(inputs).unwrap();
    assert_eq!(out.len(), 5);
    assert!(out[1].len() > 10, "{} lines", out[1].len());
    assert_eq!(out[1].len(), out[3].len());
    assert_eq!(out[1].len(), out[4].len());
    assert!(out[4].iter().all(|l| l.as_f64().unwrap() >= 20.0));
    let Value::Scene(first) = &out[1][0] else {
        panic!()
    };
    let SceneNode::Polygon { closed, style, .. } = &**first else {
        panic!()
    };
    assert!(!closed && style.stroke.is_some() && style.fill.is_none());
}

// ─── Tiles ───────────────────────────────────────────────────────────────────

#[test]
fn truchet_turns_are_seeded_and_noise_clumps_them() {
    let a = truchet(10, 8, "arcs", 0.0, 1.0);
    assert_eq!(a.len(), 80);
    assert!(a.iter().all(|p| p.turns < 2));
    assert_eq!(a, truchet(10, 8, "arcs", 0.0, 1.0));
    assert_ne!(a, truchet(10, 8, "arcs", 0.0, 2.0));
    assert!(truchet(6, 6, "triangles", 0.0, 1.0)
        .iter()
        .any(|p| p.turns >= 2));
    // Noise: neighbours agree more often than at random.
    let agree = |t: &[super::tiles::Placed]| {
        let mut same = 0;
        for p in t {
            if p.col > 0 && t[p.row * 20 + p.col - 1].turns == p.turns {
                same += 1;
            }
        }
        same
    };
    let noisy = truchet(20, 20, "diagonals", 0.15, 1.0);
    let random = truchet(20, 20, "diagonals", 0.0, 1.0);
    assert!(
        agree(&noisy) > agree(&random) + 60,
        "{} vs {}",
        agree(&noisy),
        agree(&random)
    );
    assert!(truchet(0, 5, "arcs", 0.0, 1.0).is_empty());
}

#[test]
fn sockets_parse_into_distinct_turns() {
    let (set, bad) = parse_sockets("0000 1100*3, 1010; 1111 x 12");
    assert_eq!(bad, ["x", "12"]);
    // 0000 and 1111 have one turn, the straight two, the corner four.
    assert_eq!(set.edges.len(), 1 + 4 + 2 + 1);
    let corner: f64 = set
        .source
        .iter()
        .zip(&set.weight)
        .filter(|((t, _), _)| *t == 1)
        .map(|(_, w)| w)
        .sum();
    assert!((corner - 3.0).abs() < 1e-12);
    assert!(parse_sockets("").0.edges.is_empty());
    assert_eq!(parse_sockets("1100*-1").1, ["1100*-1"]);
}

#[test]
fn wave_function_collapse_matches_every_edge() {
    let (set, _) = parse_sockets(super::tiles::DEFAULT_SOCKETS);
    for seed in 1..6 {
        let (cols, rows) = (14, 9);
        let (wave, failed) = collapse(&set, cols, rows, true, seed as f64);
        assert!(!failed);
        assert_eq!(wave.len(), cols * rows);
        let e = |k: usize| set.edges[wave[k]];
        for k in 0..wave.len() {
            let (i, j) = (k % cols, k / cols);
            if i + 1 < cols {
                assert_eq!(e(k)[1], e(k + 1)[3], "east of {i},{j}");
            }
            if j + 1 < rows {
                assert_eq!(e(k)[2], e(k + cols)[0], "south of {i},{j}");
            }
            // Closed: nothing leaves the grid.
            if i == 0 {
                assert_eq!(e(k)[3], '0');
            }
            if j == 0 {
                assert_eq!(e(k)[0], '0');
            }
            if i == cols - 1 {
                assert_eq!(e(k)[1], '0');
            }
            if j == rows - 1 {
                assert_eq!(e(k)[2], '0');
            }
        }
    }
    let a = collapse(&set, 10, 10, true, 1.0).0;
    assert_eq!(a, collapse(&set, 10, 10, true, 1.0).0);
    assert_ne!(a, collapse(&set, 10, 10, true, 2.0).0);
    // A set that can't tile (a lone corner, closed edges): it says so.
    let (bad, _) = parse_sockets("1100");
    let (wave, failed) = collapse(&bad, 3, 3, true, 1.0);
    assert!(failed && wave.len() == 9);
    assert!(collapse(&set, 0, 4, true, 1.0).0.is_empty());
}

#[test]
fn tiles_node_draws_every_cell() {
    for mode in ["truchet", "wfc"] {
        let mut n = node("geometry/tiles", json!({ "mode": mode }));
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("geometry/tiles").unwrap();
        let ports = cat.resolved_inputs(spec, json!({ "mode": mode }).as_object().unwrap());
        let inputs = ports
            .iter()
            .map(|p| crate::compile::default_for_port(p).to_vec())
            .collect();
        let out = n.eval(inputs).unwrap();
        assert_eq!(out.len(), 5, "{mode}");
        for (k, o) in out.iter().enumerate().skip(1) {
            assert_eq!(o.len(), 144, "{mode} output {k}");
        }
        // Centres span the grid round the origin.
        let xs: Vec<f64> = out[2]
            .iter()
            .map(|c| match c {
                Value::Vec2(p) => p.x,
                _ => panic!(),
            })
            .collect();
        assert_eq!(xs[0], -220.0);
        assert_eq!(xs[11], 220.0);
        assert!(out[4]
            .iter()
            .all(|a| [0.0, 1.0, 2.0, 3.0]
                .contains(&(a.as_f64().unwrap() / std::f64::consts::FRAC_PI_2))));
    }
}
