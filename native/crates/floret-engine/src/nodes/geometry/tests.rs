use super::boolean::overlay;
use super::cells::{convex_hull, delaunay, voronoi};
use super::lsystem::{expand, parse_rules, turtle};
use super::path_ops::{chaikin, offset_area, resample, simplify, subdivide};
use super::scatter::{halton, jittered_grid, poisson, random, Area};
use super::*;
use crate::nodes::value::harness::node;
use crate::scene::{Paint, Style, StyleOp};
use serde_json::json;

fn v(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

fn square(x: f64, y: f64, s: f64) -> Vec<Vec2> {
    vec![v(x, y), v(x + s, y), v(x + s, y + s), v(x, y + s)]
}

fn arr(r: &[Vec2]) -> Vec<[f64; 2]> {
    r.iter().map(|p| [p.x, p.y]).collect()
}

/// Filled area of shapes made of outer ring + holes (holes run the other
/// way, so signed areas add up).
fn area(shapes: &[Vec<Vec<Vec2>>]) -> f64 {
    shapes
        .iter()
        .map(|s| s.iter().map(|r| signed_area(r)).sum::<f64>().abs())
        .sum()
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn rect_area() -> Area {
    Area::rect(v(0.0, 0.0), v(400.0, 300.0))
}

fn in_rect(p: Vec2) -> bool {
    p.x >= -200.0 && p.x <= 200.0 && p.y >= -150.0 && p.y <= 150.0
}

fn vecs(out: &[Value]) -> Vec<Vec2> {
    out.iter()
        .map(|p| match p {
            Value::Vec2(p) => *p,
            other => panic!("not a vec2: {other:?}"),
        })
        .collect()
}

fn ellipse_layer(w: f64, h: f64) -> Value {
    Value::Scene(Arc::new(SceneNode::Ellipse {
        x: 0.0,
        y: 0.0,
        w,
        h,
        mode: Arc::from("center"),
        style: default_style().clone(),
    }))
}

fn polygon_layer(pts: Vec<Vec2>) -> Value {
    Value::Scene(polygon(pts, true, default_style().clone()))
}

// ─── Scatter ─────────────────────────────────────────────────────────────────

#[test]
fn poisson_keeps_min_distance_and_fills() {
    let (pts, capped) = poisson(&rect_area(), 20.0, 7.0);
    assert!(!capped);
    for (i, a) in pts.iter().enumerate() {
        assert!(in_rect(*a));
        for b in &pts[i + 1..] {
            assert!(dist(*a, *b) >= 20.0, "{a:?} {b:?}");
        }
    }
    // Bridson packs roughly area / (d² · 1.5..2.5) points.
    let n = pts.len() as f64;
    assert!(n > 400.0 * 300.0 / (20.0 * 20.0 * 2.5), "{n}");
    // Deterministic per seed, different across seeds.
    assert_eq!(poisson(&rect_area(), 20.0, 7.0).0, pts);
    assert_ne!(poisson(&rect_area(), 20.0, 8.0).0, pts);
}

#[test]
fn poisson_inside_a_shape_reaches_every_part() {
    // Two separate squares: both get points.
    let layers = [
        polygon_layer(square(-200.0, -50.0, 100.0)),
        polygon_layer(square(100.0, -50.0, 100.0)),
    ];
    let area = Area::shape(&layers).unwrap();
    let (pts, _) = poisson(&area, 15.0, 1.0);
    assert!(pts.iter().all(|p| area.contains(*p)));
    assert!(pts.iter().any(|p| p.x < 0.0) && pts.iter().any(|p| p.x > 0.0));
    assert!(pts.iter().all(|p| p.x <= -100.0 || p.x >= 100.0));
}

#[test]
fn random_and_halton_fill_counts() {
    let r = random(&rect_area(), 500, 3.0);
    assert_eq!(r.len(), 500);
    assert!(r.iter().all(|p| in_rect(*p)));
    let h = halton(&rect_area(), 400, 3.0);
    assert_eq!(h.len(), 400);
    assert!(h.iter().all(|p| in_rect(*p)));
    // Low discrepancy: each quadrant gets close to a quarter.
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let q = h
            .iter()
            .filter(|p| p.x * sx > 0.0 && p.y * sy > 0.0)
            .count();
        assert!((90..=110).contains(&q), "quadrant {sx},{sy}: {q}");
    }
    // Inside a circle of radius 100.
    let c = Area::shape(&[ellipse_layer(200.0, 200.0)]).unwrap();
    let pts = halton(&c, 300, 0.0);
    assert_eq!(pts.len(), 300);
    assert!(pts.iter().all(|p| p.x.hypot(p.y) <= 100.0));
}

#[test]
fn jittered_grid_puts_one_point_per_cell() {
    let (pts, capped) = jittered_grid(&rect_area(), 50.0, 1.0, 2.0);
    assert!(!capped);
    assert_eq!(pts.len(), 8 * 6);
    // Point k sits within half a cell of its cell centre.
    for (k, p) in pts.iter().enumerate() {
        let (i, j) = ((k % 8) as f64, (k / 8) as f64);
        let c = v(-175.0 + i * 50.0, -125.0 + j * 50.0);
        assert!((p.x - c.x).abs() <= 25.0 && (p.y - c.y).abs() <= 25.0);
    }
    let (still, _) = jittered_grid(&rect_area(), 50.0, 0.0, 2.0);
    assert_eq!(still[0], v(-175.0, -125.0));
}

#[test]
fn scatter_node_follows_its_mode() {
    let mut n = node("geometry/scatter", json!({"mode": "random"}));
    // center, size, shape, count, seed
    let out = n
        .eval(vec![
            vec![Value::Vec2(v(100.0, 0.0))],
            vec![Value::Vec2(v(50.0, 50.0))],
            vec![],
            vec![Value::Num(64.0)],
            vec![Value::Num(5.0)],
        ])
        .unwrap();
    let pts = vecs(&out[0]);
    assert_eq!(pts.len(), 64);
    assert!(pts.iter().all(|p| (75.0..=125.0).contains(&p.x)));
    // Poisson inside a wired shape.
    let mut n = node("geometry/scatter", json!({}));
    let out = n
        .eval(vec![
            vec![Value::Vec2(v(0.0, 0.0))],
            vec![Value::Vec2(v(400.0, 400.0))],
            vec![ellipse_layer(100.0, 100.0)],
            vec![Value::Num(10.0)],
            vec![Value::Num(1.0)],
        ])
        .unwrap();
    let pts = vecs(&out[0]);
    assert!(pts.len() > 20);
    assert!(pts.iter().all(|p| p.x.hypot(p.y) <= 50.0));
}

// ─── Cells ───────────────────────────────────────────────────────────────────

#[test]
fn delaunay_has_2n_minus_h_minus_2_triangles() {
    let (pts, _) = poisson(&rect_area(), 25.0, 11.0);
    let n = pts.len();
    let h = convex_hull(&pts).len();
    assert_eq!(delaunay(&pts).len(), 2 * n - h - 2);
    // Every triangle is empty of other points' circumcircles (spot check
    // on the area: triangles tile the hull).
    let tri_area: f64 = delaunay(&pts).iter().map(|t| signed_area(t).abs()).sum();
    assert!(close(tri_area, signed_area(&convex_hull(&pts)).abs(), 1e-6));
}

#[test]
fn hull_contains_every_point() {
    let pts = random(&rect_area(), 300, 4.0);
    let hull = convex_hull(&pts);
    assert!(hull.len() >= 3);
    let n = hull.len();
    for i in 0..n {
        let (a, b) = (hull[i], hull[(i + 1) % n]);
        for p in &pts {
            let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
            assert!(cross >= -1e-9, "point {p:?} outside edge {a:?}→{b:?}");
        }
    }
    // Collinear and duplicate points drop out.
    let line = [
        v(0.0, 0.0),
        v(1.0, 1.0),
        v(2.0, 2.0),
        v(2.0, 0.0),
        v(2.0, 0.0),
    ];
    assert_eq!(convex_hull(&line).len(), 3);
}

#[test]
fn voronoi_cells_tile_the_bounds_and_hold_their_sites() {
    let pts = random(&Area::rect(v(0.0, 0.0), v(380.0, 280.0)), 60, 9.0);
    let cells = voronoi(&pts, v(0.0, 0.0), v(400.0, 300.0));
    assert_eq!(cells.len(), pts.len());
    let total: f64 = cells.iter().map(|c| signed_area(c).abs()).sum();
    assert!(close(total, 400.0 * 300.0, 1e-6), "{total}");
    for (site, cell) in pts.iter().zip(&cells) {
        // Every corner is at least as close to its own site as to any other.
        for corner in cell {
            let own = dist(*corner, *site);
            for other in &pts {
                assert!(own <= dist(*corner, *other) + 1e-6);
            }
        }
    }
    // A single site owns the whole rectangle; nothing in → nothing out.
    assert_eq!(voronoi(&[v(1.0, 2.0)], v(0.0, 0.0), v(10.0, 10.0)).len(), 1);
    assert!(voronoi(&[], v(0.0, 0.0), v(10.0, 10.0)).is_empty());
    // Collinear sites make strips.
    let strips = voronoi(
        &[v(-50.0, 0.0), v(0.0, 0.0), v(50.0, 0.0)],
        v(0.0, 0.0),
        v(150.0, 100.0),
    );
    assert_eq!(strips.len(), 3);
    assert!(close(signed_area(&strips[1]).abs(), 50.0 * 100.0, 1e-9));
}

#[test]
fn inset_shrinks_convex_polygons() {
    let sq = square(0.0, 0.0, 100.0);
    let inner = inset_convex(&sq, 10.0);
    assert!(close(signed_area(&inner).abs(), 80.0 * 80.0, 1e-9));
    let mut rev = sq.clone();
    rev.reverse();
    assert!(close(
        signed_area(&inset_convex(&rev, 10.0)).abs(),
        6400.0,
        1e-9
    ));
    assert!(inset_convex(&sq, 60.0).is_empty());
}

#[test]
fn cells_node_outputs_polygons_colours_and_centres() {
    let mut n = node("geometry/cells", json!({"mode": "delaunay"}));
    let pts = [v(0.0, 0.0), v(100.0, 0.0), v(100.0, 100.0), v(0.0, 100.0)];
    let red = Value::Color(crate::value::Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });
    // points, gap, colors, style
    let out = n
        .eval(vec![
            pts.iter().map(|p| Value::Vec2(*p)).collect(),
            vec![Value::Num(0.0)],
            vec![red],
            vec![Value::Null],
        ])
        .unwrap();
    assert_eq!(out.len(), 5);
    assert_eq!(out[1].len(), 2, "two triangles");
    assert_eq!(out[2].len(), 6);
    let counts: Vec<f64> = out[3].iter().map(|c| c.as_f64().unwrap()).collect();
    assert_eq!(counts, [3.0, 3.0]);
    assert_eq!(out[4].len(), 2);
    let Value::Scene(cell) = &out[1][0] else {
        panic!("not a layer")
    };
    let SceneNode::Polygon { style, .. } = &**cell else {
        panic!("not a polygon")
    };
    assert_eq!(style.fill.as_ref().unwrap().css.as_ref(), "rgb(255,0,0)");
    assert!(style.stroke.is_none());
}

// ─── Path ops ────────────────────────────────────────────────────────────────

#[test]
fn resample_spaces_points_evenly() {
    // A closed 100 × 100 square at spacing 10: 40 points, 10 apart.
    let r = resample(&square(0.0, 0.0, 100.0), true, 10.0);
    assert_eq!(r.len(), 40);
    for i in 0..r.len() {
        assert!(close(dist(r[i], r[(i + 1) % r.len()]), 10.0, 1e-9));
    }
    // An open line of 95 at spacing 10: rounds to 10 gaps of 9.5, ends kept.
    let l = resample(&[v(0.0, 0.0), v(30.0, 0.0), v(95.0, 0.0)], false, 10.0);
    assert_eq!(l.len(), 11);
    assert_eq!(l[0], v(0.0, 0.0));
    assert!(close(l[10].x, 95.0, 1e-9));
    for w in l.windows(2) {
        assert!(close(dist(w[0], w[1]), 9.5, 1e-9));
    }
}

#[test]
fn chaikin_and_subdivide_double_points() {
    let sq = square(0.0, 0.0, 100.0);
    let c = chaikin(&sq, true, 2);
    assert_eq!(c.len(), 16);
    assert_eq!(chaikin(&sq, true, 1)[..2], [v(25.0, 0.0), v(75.0, 0.0)]);
    // The smoothed shape stays inside the original.
    assert!(c
        .iter()
        .all(|p| (0.0..=100.0).contains(&p.x) && (0.0..=100.0).contains(&p.y)));
    let open = chaikin(&[v(0.0, 0.0), v(50.0, 50.0), v(100.0, 0.0)], false, 1);
    assert_eq!(
        open,
        vec![v(0.0, 0.0), v(37.5, 37.5), v(62.5, 37.5), v(100.0, 0.0)]
    );
    assert_eq!(subdivide(&sq, true, 1).len(), 8);
    assert_eq!(subdivide(&sq, false, 2).len(), 13);
    assert_eq!(subdivide(&sq, true, 1)[1], v(50.0, 0.0));
}

#[test]
fn simplify_drops_points_within_tolerance() {
    let mut line: Vec<Vec2> = (0..=20).map(|i| v(i as f64 * 5.0, 0.0)).collect();
    line[7].y = 0.5;
    assert_eq!(
        simplify(&line, false, 1.0),
        vec![v(0.0, 0.0), v(100.0, 0.0)]
    );
    // A dense, slightly wobbly square comes back as its corners.
    let sq = resample(&square(0.0, 0.0, 100.0), true, 5.0);
    let wobbly: Vec<Vec2> = sq
        .iter()
        .enumerate()
        .map(|(i, p)| v(p.x + if i % 2 == 0 { 0.3 } else { -0.3 }, p.y))
        .collect();
    let s = simplify(&wobbly, true, 1.0);
    assert_eq!(s.len(), 4, "{s:?}");
}

#[test]
fn offset_grows_and_shrinks_areas() {
    let sq = vec![arr(&square(0.0, 0.0, 100.0))];
    let grown = offset_area(&sq, 10.0, "miter");
    assert!(close(area(&grown), 120.0 * 120.0, 1.0), "{}", area(&grown));
    let shrunk = offset_area(&sq, -10.0, "miter");
    assert!(close(area(&shrunk), 80.0 * 80.0, 1.0), "{}", area(&shrunk));
    let round = offset_area(&sq, 10.0, "round");
    let want = 100.0 * 100.0 + 4.0 * 100.0 * 10.0 + std::f64::consts::PI * 100.0;
    assert!(close(area(&round), want, 3.0), "{}", area(&round));
    assert!(offset_area(&sq, -60.0, "round").is_empty());
}

#[test]
fn path_ops_node_keeps_layer_styles() {
    let mut n = node("geometry/path-ops", json!({"mode": "smooth"}));
    let red = Paint::css("#ff0000");
    let styled = Value::Scene(polygon(
        square(0.0, 0.0, 100.0),
        true,
        Arc::new(ResolvedStyle {
            fill: Some(red.clone()),
            ..(**default_style()).clone()
        }),
    ));
    // points, closed, path, iterations, style
    let out = n
        .eval(vec![
            vec![],
            vec![Value::Bool(false)],
            vec![styled],
            vec![Value::Num(1.0)],
            vec![Value::Null],
        ])
        .unwrap();
    assert_eq!(out[1].len(), 1);
    assert_eq!(out[2].len(), 8);
    let Value::Scene(l) = &out[1][0] else {
        panic!()
    };
    let SceneNode::Polygon { style, closed, .. } = &**l else {
        panic!()
    };
    assert!(closed);
    assert_eq!(style.fill.as_ref(), Some(&red));
    // A wired style wins; points in when no layer is wired.
    let blue = Style::chain(None, [StyleOp::Fill(Some(Paint::css("#0000ff")))]);
    let out = n
        .eval(vec![
            vec![
                Value::Vec2(v(0.0, 0.0)),
                Value::Vec2(v(10.0, 0.0)),
                Value::Vec2(v(10.0, 10.0)),
            ],
            vec![Value::Bool(true)],
            vec![],
            vec![Value::Num(1.0)],
            vec![Value::Style(blue)],
        ])
        .unwrap();
    assert_eq!(out[2].len(), 6);
}

#[test]
fn offset_mode_pins_and_glyph_like_holes() {
    let cat = crate::spec::Catalogue::builtin();
    let spec = cat.get("geometry/path-ops").unwrap();
    let mut p = Params::new();
    p.insert("mode".into(), json!("offset"));
    let names: Vec<String> = cat
        .resolved_inputs(spec, &p)
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        ["points", "closed", "path", "distance", "join", "style"]
    );
    // A ring with a hole (one source) shrinks as one area: the hole grows.
    let outer = square(0.0, 0.0, 100.0);
    let mut hole = square(30.0, 30.0, 40.0);
    hole.reverse();
    let contours: Vec<Contour> = [outer, hole]
        .into_iter()
        .map(|pts| Contour {
            pts,
            closed: true,
            style: default_style().clone(),
            source: 0,
        })
        .collect();
    let polys = super::path_ops::offset(&contours, -5.0, "miter");
    assert_eq!(polys.len(), 1);
    assert_eq!(polys[0].rings.len(), 2);
    let a = area(&[polys[0].rings.clone()]);
    assert!(close(a, 90.0 * 90.0 - 50.0 * 50.0, 1.0), "{a}");
}

// ─── Boolean ─────────────────────────────────────────────────────────────────

#[test]
fn boolean_areas_of_two_squares() {
    let a = vec![arr(&square(0.0, 0.0, 100.0))];
    let b = vec![arr(&square(50.0, 50.0, 100.0))];
    for (mode, want, shapes) in [
        ("union", 17_500.0, 1),
        ("intersect", 2_500.0, 1),
        ("subtract", 7_500.0, 1),
        ("xor", 15_000.0, 2),
    ] {
        let out = overlay(&a, &b, mode);
        assert_eq!(out.len(), shapes, "{mode}");
        assert!(close(area(&out), want, 1e-6), "{mode}: {}", area(&out));
    }
    // Subtracting an inner square leaves a ring: one shape, two contours.
    let ring = overlay(&a, &[arr(&square(25.0, 25.0, 50.0))], "subtract");
    assert_eq!(ring.len(), 1);
    assert_eq!(ring[0].len(), 2);
    assert!(close(area(&ring), 10_000.0 - 2_500.0, 1e-6));
}

#[test]
fn boolean_node_draws_holes_without_bridges() {
    let mut n = node("geometry/boolean", json!({"mode": "subtract"}));
    let stroked = Style::chain(
        None,
        [
            StyleOp::Fill(Some(Paint::css("#ffffff"))),
            StyleOp::Stroke(Some(Paint::css("#000000"))),
        ],
    );
    let out = n
        .eval(vec![
            vec![polygon_layer(square(0.0, 0.0, 100.0))],
            vec![polygon_layer(square(25.0, 25.0, 50.0))],
            vec![Value::Style(stroked)],
        ])
        .unwrap();
    assert_eq!(out[3].len(), 2, "outer ring + hole");
    let Value::Scene(l) = &out[1][0] else {
        panic!()
    };
    let SceneNode::Group { children, .. } = &**l else {
        panic!("a shape with a hole is a group")
    };
    // One unstroked fill visiting both rings, then each ring stroked.
    assert_eq!(children.len(), 3);
    let SceneNode::Polygon { points, style, .. } = &*children[0] else {
        panic!()
    };
    assert!(style.stroke.is_none() && style.fill.is_some());
    assert_eq!(points.len(), 4 + 4 + 2 * 2);
    // Nonzero winding of the bridged outline: in the ring, not in the hole.
    let ring = [points.to_vec()];
    assert_ne!(crate::text::outline::winding(&ring, v(10.0, 10.0)), 0);
    assert_eq!(crate::text::outline::winding(&ring, v(50.0, 50.0)), 0);
    for c in &children[1..] {
        let SceneNode::Polygon { style, .. } = &**c else {
            panic!()
        };
        assert!(style.fill.is_none() && style.stroke.is_some());
    }
}

// ─── Layers ──────────────────────────────────────────────────────────────────

#[test]
fn layer_contours_apply_group_transforms() {
    let rect = Arc::new(SceneNode::Rect {
        x: 0.0,
        y: 0.0,
        w: 10.0,
        h: 20.0,
        mode: Arc::from("corner"),
        style: default_style().clone(),
    });
    let group = SceneNode::Group {
        children: Arc::from([
            rect,
            Arc::new(SceneNode::Point {
                x: 0.0,
                y: 0.0,
                style: default_style().clone(),
            }),
        ]),
        transform: Some(scene::translate_mat(100.0, 0.0)),
        blend_mode: None,
        clip: None,
    };
    let cs = layer_contours(&[Value::Scene(Arc::new(group))]);
    assert_eq!(cs.len(), 1);
    assert!(cs[0].closed);
    assert_eq!(cs[0].pts[2], v(110.0, 20.0));
}

// ─── L-system ────────────────────────────────────────────────────────────────

#[test]
fn lsystem_strings_expand() {
    let (algae, bad) = parse_rules("A=AB; B -> A");
    assert!(bad.is_empty());
    let want = ["A", "AB", "ABA", "ABAAB", "ABAABABA"];
    for (i, w) in want.iter().enumerate() {
        assert_eq!(expand("A", &algae, i).0, *w);
    }
    // Koch: every F becomes five.
    let (koch, _) = parse_rules("F=F+F-F-F+F");
    let s = expand("F", &koch, 3).0;
    assert_eq!(s.chars().filter(|c| *c == 'F').count(), 125);
    let (_, bad) = parse_rules("F=FF\nnonsense; XY=F");
    assert_eq!(bad, ["nonsense", "XY=F"]);
    // Growth stops at the cap instead of running away.
    let (dbl, _) = parse_rules("F=FF");
    let (s, capped) = expand("F", &dbl, 40);
    assert!(capped && s.len() <= super::lsystem::MAX_SYMBOLS);
}

#[test]
fn turtle_draws_and_branches() {
    let sq = turtle("F+F+F+F", 90.0, 10.0, 0.0, 0.0, 1.0);
    assert_eq!(sq.len(), 1);
    assert_eq!(sq[0].len(), 5);
    assert!(dist(sq[0][0], sq[0][4]) < 1e-9);
    // `+` turns left on screen (y down): heading east, then north.
    assert!(close(sq[0][2].y, -10.0, 1e-9));
    // A branch is its own line; the trunk carries on after it.
    let tree = turtle("F[+F]F", 90.0, 10.0, -90.0, 0.0, 1.0);
    assert_eq!(tree.len(), 2);
    assert_eq!(tree[0].len(), 3);
    assert_eq!(tree[1].len(), 2);
    // `f` moves without drawing.
    assert_eq!(turtle("FfF", 0.0, 1.0, 0.0, 0.0, 1.0).len(), 2);
}

#[test]
fn lsystem_node_fits_its_box() {
    let mut n = node("geometry/lsystem", json!({}));
    let s = |t: &str| vec![Value::str(t)];
    let num = |x: f64| vec![Value::Num(x)];
    let out = n
        .eval(vec![
            s("F"),
            s("F=F[+F]F[-F]F"),
            num(3.0),
            num(25.7),
            num(10.0),
            num(300.0),
            vec![Value::Vec2(v(20.0, 0.0))],
            num(-90.0),
            num(0.0),
            num(1.0),
            vec![Value::Null],
        ])
        .unwrap();
    let pts = vecs(&out[2]);
    let (lo, hi) = crate::text::outline::bounds(&[pts]).unwrap();
    assert!(close((hi.x - lo.x).max(hi.y - lo.y), 300.0, 1e-9));
    assert!(close((lo.y + hi.y) / 2.0, 0.0, 1e-9));
    assert!(close((lo.x + hi.x) / 2.0, 20.0, 1e-9));
    let total: f64 = out[3].iter().map(|c| c.as_f64().unwrap()).sum();
    assert_eq!(total as usize, out[2].len());
}
