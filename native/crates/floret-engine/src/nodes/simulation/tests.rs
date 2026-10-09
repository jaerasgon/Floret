use super::super::value::harness::{node, Harness};
use super::*;
use crate::compile::default_for_port;
use crate::spec::Catalogue;
use serde_json::json;

/// A simulation node in `mode`, ticked at a fixed frame time.
struct Run {
    h: Harness,
    ports: Vec<PortSpec>,
    /// Pin values by name (others take their defaults).
    set: Vec<(String, Vec<Value>)>,
}

impl Run {
    fn new(mode: &str) -> Run {
        let params = json!({ "mode": mode });
        let cat = Catalogue::builtin();
        let spec = cat.get("particles/simulation").unwrap();
        let ports = cat.resolved_inputs(spec, params.as_object().unwrap());
        let mut h = node("particles/simulation", params);
        h.frame.dt = 1.0 / 60.0;
        Run {
            h,
            ports,
            set: Vec::new(),
        }
    }

    fn with(mut self, name: &str, v: Value) -> Run {
        self.pin(name, vec![v]);
        self
    }

    fn pin(&mut self, name: &str, v: Vec<Value>) {
        assert!(
            self.ports.iter().any(|p| p.name == name),
            "no pin {name} in this mode"
        );
        self.set.retain(|(n, _)| n != name);
        self.set.push((name.to_owned(), v));
    }

    /// One frame; the outputs.
    fn tick(&mut self) -> Vec<Vec<Value>> {
        let inputs = self
            .ports
            .iter()
            .map(|p| match self.set.iter().find(|(n, _)| *n == p.name) {
                Some((_, v)) => v.clone(),
                None => default_for_port(p).to_vec(),
            })
            .collect();
        self.h.eval(inputs).expect("eval")
    }

    fn ticks(&mut self, n: usize) -> Vec<Vec<Value>> {
        for _ in 1..n {
            self.tick();
        }
        self.tick()
    }
}

fn v2(x: f64, y: f64) -> Value {
    Value::Vec2(Vec2 { x, y })
}

fn points(out: &[Vec<Value>]) -> Vec<Vec2> {
    out[1]
        .iter()
        .map(|v| match v {
            Value::Vec2(p) => *p,
            other => panic!("not a point: {other:?}"),
        })
        .collect()
}

fn numbers(out: &[Vec<Value>]) -> Vec<f64> {
    out[2].iter().map(|v| v.as_f64().unwrap()).collect()
}

fn image(out: &[Vec<Value>]) -> Arc<Image> {
    match &out[5][0] {
        Value::Image(i) => i.clone(),
        other => panic!("not an image: {other:?}"),
    }
}

fn layer_len(out: &[Vec<Value>]) -> usize {
    match &out[0][0] {
        Value::Scene(n) => match &**n {
            SceneNode::Group { children, .. } => children.len(),
            _ => 1,
        },
        other => panic!("not a layer: {other:?}"),
    }
}

#[test]
fn pins_follow_the_mode() {
    let cat = Catalogue::builtin();
    let spec = cat.get("particles/simulation").unwrap();
    let names = |mode: &str| -> Vec<String> {
        let p = json!({ "mode": mode });
        cat.resolved_inputs(spec, p.as_object().unwrap())
            .into_iter()
            .map(|p| p.name)
            .collect()
    };
    // The catalogue's static pins are flocking's.
    let statics: Vec<String> = spec.inputs.iter().map(|p| p.name.clone()).collect();
    assert_eq!(names("flocking"), statics);
    assert!(names("physarum").contains(&"sensorAngle".to_owned()));
    assert!(names("dla").contains(&"stickiness".to_owned()));
    assert!(names("cloth").contains(&"pin".to_owned()));
    for mode in ["flocking", "physarum", "dla", "cloth"] {
        assert_eq!(names(mode).last().map(String::as_str), Some("reset"));
    }
}

#[test]
fn every_mode_runs_and_draws() {
    for mode in ["flocking", "physarum", "dla", "cloth"] {
        let mut r = Run::new(mode);
        if mode == "physarum" {
            r = r
                .with("agents", Value::Num(2000.0))
                .with("resolution", Value::Num(64.0));
        }
        let out = r.ticks(30);
        assert_eq!(out.len(), 6, "{mode}");
        assert!(layer_len(&out) > 0, "{mode} drew nothing");
        assert_eq!(out[3].len(), out[4].len(), "{mode} segments");
        assert_eq!(out[1].len(), out[2].len(), "{mode} values per point");
        assert_eq!(image(&out).is_ready(), mode == "physarum", "{mode}");
    }
}

#[test]
fn the_same_seed_gives_the_same_flock() {
    let run = |seed: f64| {
        let mut r = Run::new("flocking").with("seed", Value::Num(seed));
        points(&r.ticks(90))
    };
    assert_eq!(run(3.0), run(3.0));
    assert_ne!(run(3.0), run(4.0));
}

#[test]
fn frame_times_only_matter_through_their_sum() {
    // 60 frames at 1/60 s = 30 frames at 1/30 s: the same steps.
    let mut a = Run::new("flocking");
    let mut b = Run::new("flocking");
    b.h.frame.dt = 1.0 / 30.0;
    assert_eq!(points(&a.ticks(61)), points(&b.ticks(31)));
    let mut a = Run::new("dla");
    let mut b = Run::new("dla");
    b.h.frame.dt = 1.0 / 30.0;
    assert_eq!(points(&a.ticks(61)), points(&b.ticks(31)));
}

#[test]
fn a_long_frame_runs_a_few_steps_not_all_of_them() {
    let mut a = Run::new("flocking");
    let first = points(&a.tick());
    a.h.frame.dt = 10.0;
    let jumped = points(&a.tick());
    let mut b = Run::new("flocking");
    b.tick();
    assert_eq!(jumped, points(&b.ticks(MAX_STEPS as usize)));
    assert_ne!(first, jumped);
}

#[test]
fn a_reset_edge_starts_over() {
    for mode in ["flocking", "physarum", "dla", "cloth"] {
        let mut r = Run::new(mode);
        if mode == "physarum" {
            r = r
                .with("agents", Value::Num(500.0))
                .with("resolution", Value::Num(32.0));
        }
        if mode == "cloth" {
            r = r.with("wind", v2(300.0, 0.0));
        }
        let state = |out: &[Vec<Value>]| (points(out), numbers(out), image(out).rgba().to_vec());
        let start = state(&r.tick());
        let later = state(&r.ticks(40));
        assert_ne!(start, later, "{mode} didn't move");
        r.pin("reset", vec![Value::Bool(true)]);
        let reset = state(&r.tick());
        assert!(start == reset, "{mode} didn't start over");
        // Held high, it runs again; only the edge resets.
        let held = state(&r.ticks(5));
        assert_ne!(reset, held, "{mode} stuck in reset");
    }
}

#[test]
fn a_new_seed_starts_over() {
    let mut r = Run::new("flocking");
    r.ticks(20);
    r.pin("seed", vec![Value::Num(9.0)]);
    let reseeded = points(&r.tick());
    let fresh = points(&Run::new("flocking").with("seed", Value::Num(9.0)).tick());
    assert_eq!(reseeded, fresh);
}

#[test]
fn boids_stay_on_the_field_and_wrap() {
    let mut r = Run::new("flocking")
        .with("size", v2(200.0, 120.0))
        .with("count", Value::Num(60.0))
        .with("maxSpeed", Value::Num(400.0));
    let mut wrapped = false;
    let mut last = points(&r.tick());
    for _ in 0..240 {
        let now = points(&r.tick());
        for (p, q) in last.iter().zip(&now) {
            assert!(
                q.x >= -100.0 && q.x < 100.0 && q.y >= -60.0 && q.y < 60.0,
                "{q:?}"
            );
            // A jump across the field is a wrap (a step is under 7 units).
            wrapped |= (q.x - p.x).abs() > 100.0 || (q.y - p.y).abs() > 60.0;
        }
        last = now;
    }
    assert!(wrapped, "nobody crossed an edge");
}

#[test]
fn boids_fly_at_a_bounded_speed_and_follow_count() {
    let mut r = Run::new("flocking").with("count", Value::Num(40.0));
    let out = r.ticks(30);
    assert_eq!(points(&out).len(), 40);
    for (a, b) in out[3].iter().zip(&out[4]) {
        let (Value::Vec2(a), Value::Vec2(b)) = (a, b) else {
            panic!()
        };
        // `to` is 0.1 s of velocity ahead.
        let speed = (b.x - a.x).hypot(b.y - a.y) / 0.1;
        assert!(
            (140.0 / 3.0 - 1e-6..=140.0 + 1e-6).contains(&speed),
            "{speed}"
        );
    }
    r.pin("count", vec![Value::Num(55.0)]);
    let grown = points(&r.tick());
    assert_eq!(grown.len(), 55);
    // The first 40 kept flying (no restart).
    assert_ne!(
        &grown[..40],
        &points(&Run::new("flocking").with("count", Value::Num(55.0)).tick())[..40]
    );
}

#[test]
fn dla_grows_one_connected_cluster() {
    let mut r = Run::new("dla")
        .with("count", Value::Num(400.0))
        .with("rate", Value::Num(1200.0));
    let early = points(&r.ticks(5)).len();
    let out = r.ticks(60);
    let pts = points(&out);
    assert!(early > 1 && pts.len() > early, "{early} → {}", pts.len());
    assert_eq!(pts.len(), 400, "stops at count");
    assert_eq!(pts[0], Vec2::default(), "grows from the origin");
    // Every link joins two touching particles, and every particle reaches
    // the seed through them.
    let (from, to) = (&out[3], &out[4]);
    assert_eq!(from.len(), pts.len() - 1);
    let index = |v: &Value| {
        let Value::Vec2(p) = v else { panic!() };
        pts.iter().position(|q| q == p).unwrap()
    };
    let mut parent = vec![usize::MAX; pts.len()];
    for (a, b) in from.iter().zip(to) {
        let (i, j) = (index(a), index(b));
        let d = (pts[i].x - pts[j].x).hypot(pts[i].y - pts[j].y);
        assert!((d - 5.0).abs() < 1e-9, "link of {d}");
        assert!(i < j, "a parent stuck first");
        parent[j] = i;
    }
    for start in 1..pts.len() {
        let mut i = start;
        while i != 0 {
            i = parent[i];
            assert_ne!(i, usize::MAX, "detached particle");
        }
    }
    // Growth order runs 0..1.
    let order = numbers(&out);
    assert_eq!((order[0], order[order.len() - 1]), (0.0, 1.0));
}

#[test]
fn dla_starts_from_its_seeds() {
    let seeds = vec![v2(-100.0, 0.0), v2(100.0, 0.0)];
    let mut r = Run::new("dla");
    r.pin("seeds", seeds.clone());
    let out = r.ticks(30);
    assert_eq!(
        &out[1][..2]
            .iter()
            .map(|v| v.get("x").to_number())
            .collect::<Vec<_>>(),
        &[-100.0, 100.0]
    );
    assert!(out[1].len() > 2);
}

#[test]
fn cloth_pins_hold_while_the_rest_moves() {
    let mut r = Run::new("cloth")
        .with("cols", Value::Num(10.0))
        .with("rows", Value::Num(6.0))
        .with("size", v2(180.0, 100.0))
        .with("wind", v2(500.0, -200.0));
    let start = points(&r.tick());
    let later = points(&r.ticks(120));
    for c in 0..10 {
        assert_eq!(start[c], later[c], "top-row pin {c} moved");
        assert!((start[c].y - -50.0).abs() < 1e-9);
    }
    for i in 10..60 {
        assert_ne!(start[i], later[i], "point {i} didn't move");
    }
    // Pins follow `pos`.
    r.pin("pos", vec![v2(30.0, 10.0)]);
    let moved = points(&r.tick());
    assert!((moved[0].x - (start[0].x + 30.0)).abs() < 1e-9);
    assert!((moved[0].y - (start[0].y + 10.0)).abs() < 1e-9);
}

#[test]
fn cloth_corner_pins_and_grabbing() {
    let mut r = Run::new("cloth")
        .with("cols", Value::Num(8.0))
        .with("rows", Value::Num(8.0))
        .with("size", v2(140.0, 140.0));
    r.pin("pin", vec![Value::str("corners")]);
    let start = points(&r.tick());
    let later = points(&r.ticks(60));
    assert_eq!((start[0], start[7]), (later[0], later[7]));
    assert_ne!(start[3], later[3], "the top edge sags between the corners");
    // Grab the bottom-right corner and pull it.
    let corner = later[63];
    r.pin("pointer", vec![v2(corner.x + 5.0, corner.y)]);
    r.pin("grab", vec![Value::Bool(true)]);
    r.tick();
    r.pin("pointer", vec![v2(200.0, 200.0)]);
    let pulled = points(&r.tick());
    assert_eq!(pulled[63], Vec2 { x: 200.0, y: 200.0 });
}

#[test]
fn cloth_tears_when_overstretched() {
    let mut r = Run::new("cloth")
        .with("cols", Value::Num(6.0))
        .with("rows", Value::Num(6.0))
        .with("size", v2(100.0, 100.0))
        .with("tear", Value::Num(1.5));
    let links = r.tick()[3].len();
    assert_eq!(links, 2 * 5 * 6);
    let p = points(&r.tick())[33];
    r.pin("pointer", vec![Value::Vec2(p)]);
    r.pin("grab", vec![Value::Bool(true)]);
    r.tick();
    r.pin("pointer", vec![v2(0.0, 600.0)]);
    let torn = r.ticks(5)[3].len();
    assert!(torn < links, "{torn} of {links}");
}

#[test]
fn physarum_trail_fades_without_agents() {
    let alpha = |out: &[Vec<Value>]| -> u64 {
        image(out)
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| u64::from(p[3]))
            .sum()
    };
    let field = |decay: f64, diffuse: f64| {
        Run::new("physarum")
            .with("agents", Value::Num(0.0))
            .with("resolution", Value::Num(48.0))
            .with("decay", Value::Num(decay))
            .with("diffuse", Value::Num(diffuse))
    };
    // Fed, then left alone: every cell fades.
    let mut r = field(0.08, 0.0);
    assert_eq!(alpha(&r.tick()), 0);
    r.pin("food", vec![v2(0.0, 0.0), v2(100.0, -50.0)]);
    let fed = alpha(&r.ticks(10));
    assert!(fed > 0);
    r.pin("food", Vec::new());
    // (Saturated cells stay opaque for the first few steps.)
    let mut last = fed;
    for _ in 0..40 {
        let a = alpha(&r.tick());
        assert!(a <= last, "{a} > {last}");
        last = a;
    }
    assert!(last < fed / 2, "{last} vs {fed}");
    // Spreading out too, it still fades away.
    let mut r = field(0.08, 0.5);
    r.pin("food", vec![v2(0.0, 0.0)]);
    let fed = alpha(&r.ticks(10));
    r.pin("food", Vec::new());
    assert!(alpha(&r.ticks(60)) < fed / 4);
    // Without decay or diffusion it stays.
    let mut r = field(0.0, 0.0);
    r.pin("food", vec![v2(0.0, 0.0)]);
    r.ticks(5);
    r.pin("food", Vec::new());
    let a = alpha(&r.tick());
    assert_eq!(alpha(&r.ticks(20)), a);
}

#[test]
fn physarum_agents_build_a_trail_and_follow_the_seed() {
    let run = |seed: f64| {
        let mut r = Run::new("physarum")
            .with("agents", Value::Num(3000.0))
            .with("resolution", Value::Num(64.0))
            .with("size", v2(400.0, 200.0))
            .with("seed", Value::Num(seed));
        image(&r.ticks(20))
    };
    let a = run(1.0);
    // The grid keeps the field's aspect.
    assert_eq!((a.width(), a.height()), (64, 32));
    assert!(a.rgba().as_chunks::<4>().0.iter().any(|p| p[3] > 128));
    assert_eq!(a.rgba(), run(1.0).rgba());
    assert_ne!(a.rgba(), run(2.0).rgba());
}

#[test]
fn colors_ramp_along_values() {
    let red = Value::Color(Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });
    let blue = Value::Color(Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    });
    let mut r = Run::new("dla").with("rate", Value::Num(600.0));
    r.pin("colors", vec![red, blue]);
    let out = r.ticks(30);
    let Value::Scene(g) = &out[0][0] else {
        panic!()
    };
    let SceneNode::Group { children, .. } = &**g else {
        panic!()
    };
    let stroke = |n: &SceneNode| match n {
        SceneNode::Line { style, .. } => style.stroke.as_ref().unwrap().rgba,
        _ => panic!(),
    };
    let (first, last) = (stroke(&children[0]), stroke(&children[children.len() - 1]));
    assert!(first[0] > 0.9 && first[2] < 0.1, "{first:?}");
    assert!(last[2] > 0.9 && last[0] < 0.1, "{last:?}");
}

#[test]
fn ramp_interpolates_evenly() {
    let c = |v: f64| Color {
        r: v,
        g: v,
        b: v,
        a: 1.0,
    };
    let stops = [c(0.0), c(1.0), c(0.0)];
    assert_eq!(ramp(&stops, 0.25).r, 0.5);
    assert_eq!(ramp(&stops, 0.5).r, 1.0);
    assert_eq!(ramp(&stops, 2.0).r, 0.0);
    assert_eq!(ramp(&[], 0.5), Color::WHITE);
}

// ─── Growth and reaction–diffusion ───────────────────────────────────────────

#[test]
fn new_modes_have_their_pins_and_run() {
    let cat = Catalogue::builtin();
    let spec = cat.get("particles/simulation").unwrap();
    let names = |params: serde_json::Value| -> Vec<String> {
        cat.resolved_inputs(spec, params.as_object().unwrap())
            .into_iter()
            .map(|p| p.name)
            .collect()
    };
    let growth = names(json!({ "mode": "growth" }));
    for pin in ["shape", "repel", "align", "maxEdge", "maxNodes", "reset"] {
        assert!(growth.contains(&pin.to_owned()), "growth lacks {pin}");
    }
    let reaction = names(json!({ "mode": "reaction" }));
    assert!(reaction.contains(&"preset".to_owned()));
    assert!(
        !reaction.contains(&"feed".to_owned()),
        "feed only for custom"
    );
    let custom = names(json!({ "mode": "reaction", "preset": "custom" }));
    assert!(custom.contains(&"feed".to_owned()) && custom.contains(&"kill".to_owned()));
    assert_eq!(custom.last().map(String::as_str), Some("reset"));
    for mode in ["growth", "reaction"] {
        let mut r = Run::new(mode);
        if mode == "reaction" {
            r = r.with("resolution", Value::Num(48.0));
        }
        let out = r.ticks(20);
        assert_eq!(out.len(), 6, "{mode}");
        assert!(layer_len(&out) > 0, "{mode} drew nothing");
        assert_eq!(out[3].len(), out[4].len(), "{mode} segments");
        assert_eq!(image(&out).is_ready(), mode == "reaction", "{mode}");
    }
}

#[test]
fn growth_grows_folds_and_stays_bounded() {
    let mut r = Run::new("growth")
        .with("maxNodes", Value::Num(400.0))
        .with("size", v2(300.0, 300.0));
    let start = points(&r.tick()).len();
    let out = r.ticks(600);
    let pts = points(&out);
    assert!(pts.len() > start, "{} → {}", start, pts.len());
    assert!(pts.len() <= 400);
    for p in &pts {
        assert!(
            p.x.abs() <= 150.0 && p.y.abs() <= 150.0,
            "{p:?} left the bounds"
        );
        assert!(p.x.is_finite() && p.y.is_finite());
    }
    // A closed curve: an edge per node; ages run 0..1.
    assert_eq!(out[3].len(), pts.len());
    let ages = numbers(&out);
    assert!(ages.iter().all(|a| (0.0..=1.0).contains(a)));
    assert!(ages.contains(&0.0) && ages.contains(&1.0));
    // Edges stay near maxEdge (5) once split.
    let long = out[3]
        .iter()
        .zip(&out[4])
        .filter(|(a, b)| match (a, b) {
            (Value::Vec2(a), Value::Vec2(b)) => (a.x - b.x).hypot(a.y - b.y) > 15.0,
            _ => true,
        })
        .count();
    assert_eq!(long, 0, "edges stretched far past maxEdge");
}

#[test]
fn growth_is_the_same_for_a_seed_and_starts_from_a_shape() {
    let run = |seed: f64| points(&Run::new("growth").with("seed", Value::Num(seed)).ticks(60));
    assert_eq!(run(2.0), run(2.0));
    assert_ne!(run(2.0), run(3.0));
    // An open line: an open curve (one edge fewer than nodes).
    let line = Value::Scene(Arc::new(SceneNode::Line {
        x1: -100.0,
        y1: 0.0,
        x2: 100.0,
        y2: 0.0,
        style: crate::scene::line_default_style().clone(),
    }));
    let mut r = Run::new("growth").with("shape", line);
    let out = r.ticks(30);
    let n = points(&out).len();
    assert!(n > 20, "the 200-long line split into {n} nodes");
    assert_eq!(out[3].len(), n - 1);
    // Degenerate pins fall back rather than breaking.
    let mut r = Run::new("growth")
        .with("radius", Value::Num(f64::NAN))
        .with("repel", Value::Num(f64::INFINITY))
        .with("maxNodes", Value::Num(-5.0));
    let out = r.ticks(10);
    assert!(points(&out)
        .iter()
        .all(|p| p.x.is_finite() && p.y.is_finite()));
}

#[test]
fn growth_and_reaction_survive_an_edit() {
    for mode in ["growth", "reaction"] {
        let make = || {
            let r = Run::new(mode);
            if mode == "reaction" {
                r.with("resolution", Value::Num(32.0))
            } else {
                r
            }
        };
        let state = |out: &[Vec<Value>]| (points(out), image(out).rgba().to_vec());
        let reference = state(&make().ticks(31));
        let mut a = make();
        a.ticks(30);
        let carried = a.h.carry().expect("carries its state");
        // The rebuilt node carries on from frame 30.
        let mut b = make();
        b.h.adopt(carried);
        assert!(state(&b.tick()) == reference, "{mode}");
        assert!(state(&make().tick()) != reference, "{mode}");
    }
    // The older modes start over as before.
    let mut f = Run::new("flocking");
    f.ticks(5);
    assert!(f.h.carry().is_none());
}

#[test]
fn reaction_diffusion_develops_and_is_deterministic() {
    let run = |seed: f64, preset: &str, frames: usize| {
        let mut r = Run::new("reaction")
            .with("resolution", Value::Num(64.0))
            .with("seed", Value::Num(seed));
        r = r.with("preset", Value::str(preset));
        image(&r.ticks(frames))
    };
    let lit = |img: &Image| img.rgba().chunks(4).filter(|p| p[3] > 64).count();
    let early = run(1.0, "coral", 2);
    let late = run(1.0, "coral", 120);
    assert_eq!((late.width(), late.height()), (64, 64));
    assert!(lit(&late) > lit(&early), "{} → {}", lit(&early), lit(&late));
    assert_eq!(late.rgba(), run(1.0, "coral", 120).rgba());
    assert_ne!(late.rgba(), run(2.0, "coral", 120).rgba());
    assert_ne!(late.rgba(), run(1.0, "mitosis", 120).rgba());
    // The grid follows the size's shape.
    let mut r = Run::new("reaction")
        .with("resolution", Value::Num(100.0))
        .with("size", v2(800.0, 400.0));
    let img = image(&r.tick());
    assert_eq!((img.width(), img.height()), (100, 50));
}

#[test]
fn reaction_seeds_from_a_bright_image_and_paints() {
    // A white image: B everywhere at the start.
    let white = Arc::new(Image::new(4, 4, vec![255; 64]));
    let mut r = Run::new("reaction")
        .with("resolution", Value::Num(32.0))
        .with("seedImage", Value::Image(white));
    let img = image(&r.tick());
    assert!(img.rgba().chunks(4).all(|p| p[3] == 255));
    // A transparent one seeds nothing, so random spots are used instead.
    let clear = Arc::new(Image::new(4, 4, vec![0; 64]));
    let mut r = Run::new("reaction")
        .with("resolution", Value::Num(32.0))
        .with("seedImage", Value::Image(clear));
    let img = image(&r.tick());
    assert!(img.rgba().chunks(4).any(|p| p[3] > 0));
    // Painting adds B where the point is.
    let mut r = Run::new("reaction").with("resolution", Value::Num(32.0));
    r.tick();
    r.pin("paint", vec![v2(-280.0, -280.0)]);
    let img = image(&r.tick());
    assert!(img.rgba()[3] > 0, "the top-left corner was painted");
    // feed / kill: a preset's, or the pins' for custom.
    let ins_of = |names: &[&str], vals: Vec<Spread>| {
        let names: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        let ins = Ins {
            inputs: &vals,
            names: &names,
        };
        reaction::feed_kill(&ins)
    };
    let s = |v: &str| crate::value::one(Value::str(v));
    let n = |v: f64| crate::value::one(Value::Num(v));
    assert_eq!(ins_of(&["preset"], vec![s("mitosis")]), (0.0367, 0.0649));
    assert_eq!(
        ins_of(
            &["preset", "feed", "kill"],
            vec![s("custom"), n(0.01), n(0.05)]
        ),
        (0.01, 0.05)
    );
    assert_eq!(ins_of(&["preset"], vec![s("nonsense")]), (0.0545, 0.062));
}

#[test]
fn a_seed_image_that_arrives_late_starts_reaction_over() {
    let mut r = Run::new("reaction").with("resolution", Value::Num(32.0));
    r.ticks(5);
    // A rasterised layer typically turns up a frame or two later.
    let white = Arc::new(Image::new(4, 4, vec![255; 64]));
    r.pin("seedImage", vec![Value::Image(white)]);
    let img = image(&r.tick());
    assert!(
        img.rgba().chunks(4).all(|p| p[3] == 255),
        "seeded from the image"
    );
    // Later frames of the same (or a changing) image don't restart it.
    let next = image(&r.ticks(3));
    assert_ne!(next.rgba(), img.rgba());
}
