//! Expected values recorded from the TypeScript implementation (node 22,
//! vitest) into `ts_expected.json`: every force / collider / integrator /
//! sampling mode applied to the same small buffer, plus an emitter run.
//! Buffer contents must match bit for bit (f32).

use super::*;
use crate::node::{EvalCtx, FrameCtx, ParamStore};
use crate::particles::Force;
use crate::spec::{Catalogue, Params};
use serde_json::{json, Value as J};

fn expected() -> &'static J {
    static E: std::sync::OnceLock<J> = std::sync::OnceLock::new();
    E.get_or_init(|| serde_json::from_str(include_str!("ts_expected.json")).unwrap())
}

/// Build a node and evaluate it once per `inputs` entry.
struct Node {
    imp: Box<dyn NodeImpl>,
    params: Params,
    store: ParamStore,
}

impl Node {
    fn new(ty: &str) -> Node {
        let cat = Catalogue::builtin();
        let spec = cat.get(ty).unwrap();
        let params = Params::new();
        let resolved = cat.resolved_inputs(spec, &params);
        let init = NodeInit {
            id: "n",
            spec,
            params: &params,
            inputs: &resolved,
        };
        Node {
            imp: crate::nodes::factory(ty).unwrap()(&init),
            params,
            store: ParamStore::default(),
        }
    }

    fn eval(&mut self, inputs: Vec<Vec<Value>>) -> Vec<Spread> {
        let frame = FrameCtx::default();
        let mut cx = EvalCtx {
            node_id: "n",
            frame: &frame,
            tick_frame: &frame,
            params: &mut self.store,
            saved_params: &self.params,
        };
        let ins: Vec<Spread> = inputs.into_iter().map(Arc::from).collect();
        self.imp.eval(&ins, &mut cx).unwrap()
    }
}

fn run(ty: &str, inputs: Vec<Vec<Value>>) -> Vec<Spread> {
    Node::new(ty).eval(inputs)
}

fn n(x: f64) -> Vec<Value> {
    vec![Value::Num(x)]
}
fn v2(x: f64, y: f64) -> Vec<Value> {
    vec![Value::Vec2(Vec2 { x, y })]
}
fn s(x: &str) -> Vec<Value> {
    vec![Value::str(x)]
}
fn b(x: bool) -> Vec<Value> {
    vec![Value::Bool(x)]
}
fn scene(j: J) -> Vec<Value> {
    // Shapes built from TS-style JSON (style is irrelevant to sampling /
    // colliders).
    let st = scene::default_style().clone();
    let f = |k: &str| j[k].as_f64().unwrap_or(0.0);
    let node = match j["kind"].as_str().unwrap() {
        "rect" | "ellipse" => {
            let (x, y, w, h) = (f("x"), f("y"), f("w"), f("h"));
            let mode: Arc<str> = Arc::from(j["mode"].as_str().unwrap());
            if j["kind"] == "rect" {
                SceneNode::Rect {
                    x,
                    y,
                    w,
                    h,
                    mode,
                    style: st,
                }
            } else {
                SceneNode::Ellipse {
                    x,
                    y,
                    w,
                    h,
                    mode,
                    style: st,
                }
            }
        }
        "triangle" => SceneNode::Triangle {
            pts: [f("x1"), f("y1"), f("x2"), f("y2"), f("x3"), f("y3")],
            style: st,
        },
        "line" => SceneNode::Line {
            x1: f("x1"),
            y1: f("y1"),
            x2: f("x2"),
            y2: f("y2"),
            style: st,
        },
        "point" => SceneNode::Point {
            x: f("x"),
            y: f("y"),
            style: st,
        },
        "polygon" => SceneNode::Polygon {
            points: j["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| Vec2 {
                    x: p["x"].as_f64().unwrap(),
                    y: p["y"].as_f64().unwrap(),
                })
                .collect(),
            closed: j["closed"].as_bool().unwrap(),
            style: st,
            tension: None,
        },
        other => panic!("{other}"),
    };
    vec![Value::Scene(Arc::new(node))]
}

fn poly() -> J {
    json!({"kind": "polygon", "points": [{"x": -60, "y": -30}, {"x": 40, "y": -30}, {"x": 40, "y": 50}, {"x": -60, "y": 50}], "closed": true})
}
fn tri() -> J {
    json!({"kind": "triangle", "x1": 0, "y1": 0, "x2": 100, "y2": 0, "x3": 0, "y3": 100})
}

/// The probe's buffer: five particles (one killed and respawned), masses
/// 1, 2, 0, 1, 0.5.
fn make_buf() -> ParticleBuffer {
    let mut buf = ParticleBuffer::new(6.0);
    let parts = [
        (10.0, 20.0, 30.0, -40.0, 1.0),
        (-50.0, 5.0, -12.5, 7.25, 2.0),
        (12.0, 6.0, 0.0, 0.0, 0.0),
        (130.0, -90.0, 55.0, 60.0, 1.0),
        (-3.3, 44.4, -8.0, 19.0, 0.5),
    ];
    for (x, y, vx, vy, m) in parts {
        let i = buf.spawn().unwrap();
        buf.init_slot(i, x, y, vx, vy, 3.0, 4.0, Color::WHITE, m);
    }
    buf.kill(2);
    buf.spawn();
    buf.init_slot(2, 12.0, 6.0, -1.0, 1.0, 3.0, 4.0, Color::WHITE, 0.0);
    buf
}

/// A JSON number as `JSON.parse` would produce it (integers stay integral).
fn jnum(x: f64) -> J {
    if x.fract() == 0.0 && x.abs() < 9e15 {
        json!(x as i64)
    } else {
        json!(x)
    }
}

/// `JSON.parse(JSON.stringify(v))` (functions dropped / `null` in arrays).
fn to_json(v: &Value) -> Option<J> {
    Some(match v {
        Value::Undefined | Value::Style(_) => return None,
        Value::Null => J::Null,
        Value::Bool(b) => json!(b),
        Value::Num(x) => jnum(*x),
        Value::Str(s) => json!(&**s),
        Value::Vec2(p) => json!({"x": jnum(p.x), "y": jnum(p.y)}),
        Value::Color(c) => json!({"r": jnum(c.r), "g": jnum(c.g), "b": jnum(c.b), "a": jnum(c.a)}),
        Value::List(items) => J::Array(
            items
                .iter()
                .map(|x| to_json(x).unwrap_or(J::Null))
                .collect(),
        ),
        Value::Object(m) => J::Object(
            m.iter()
                .filter_map(|(k, x)| to_json(x).map(|j| (k.clone(), j)))
                .collect(),
        ),
        Value::Scene(n) => return to_json(&n.to_value()),
        Value::Handle(h) => return h.to_value().and_then(|o| to_json(&o)),
        Value::Image(_) => json!({}),
    })
}

fn num(j: &J) -> f64 {
    match j {
        J::String(s) if s == "NaN" => f64::NAN,
        other => other.as_f64().unwrap(),
    }
}

fn check_f32(what: &str, got: &[f32], want: &J) {
    // serde_json's default float parser can be an f64 ulp off; the expected
    // values are exact f32s, so compare after rounding to f32.
    let want: Vec<f32> = want
        .as_array()
        .unwrap()
        .iter()
        .map(|j| num(j) as f32)
        .collect();
    let same = got.len() == want.len()
        && got
            .iter()
            .zip(&want)
            .all(|(a, b)| a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()));
    assert!(same, "{what}: rust {got:?} vs ts {want:?}");
}

fn check_buf(what: &str, buf: &ParticleBuffer, want: &J) {
    check_f32(&format!("{what}.posX"), &buf.pos_x, &want["posX"]);
    check_f32(&format!("{what}.posY"), &buf.pos_y, &want["posY"]);
    check_f32(&format!("{what}.velX"), &buf.vel_x, &want["velX"]);
    check_f32(&format!("{what}.velY"), &buf.vel_y, &want["velY"]);
    check_f32(&format!("{what}.forceX"), &buf.force_x, &want["forceX"]);
    check_f32(&format!("{what}.forceY"), &buf.force_y, &want["forceY"]);
    let alive: Vec<f32> = buf.alive.iter().map(|a| f32::from(*a)).collect();
    check_f32(&format!("{what}.alive"), &alive, &want["alive"]);
    assert_eq!(
        buf.count as f64,
        want["count"].as_f64().unwrap(),
        "{what}.count"
    );
}

fn force_of(v: &Spread) -> Arc<Force> {
    match &v[0] {
        Value::Handle(Handle::Force(f)) => f.clone(),
        other => panic!("not a force: {other:?}"),
    }
}

fn collider_of(v: &Spread) -> Arc<Collider> {
    match &v[0] {
        Value::Handle(Handle::Collider(c)) => c.clone(),
        other => panic!("not a collider: {other:?}"),
    }
}

const DT: f64 = 1.0 / 60.0;

fn forces() -> Vec<(String, Arc<Force>)> {
    let mut out = vec![
        ("gravity", run("force/gravity", vec![v2(3.0, 4.0), n(50.0)])),
        ("drag", run("force/drag", vec![n(0.5)])),
        (
            "turbulence",
            run("force/turbulence", vec![n(0.02), n(0.7), n(120.0), n(3.0)]),
        ),
        (
            "wind",
            run("force/wind", vec![v2(1.0, 1.0), n(80.0), n(0.3), n(2.0)]),
        ),
        (
            "windCalm",
            run("force/wind", vec![v2(0.0, 0.0), n(80.0), n(0.0), n(2.0)]),
        ),
        (
            "vortex",
            run(
                "force/vortex",
                vec![
                    v2(10.0, 5.0),
                    n(150.0),
                    n(40.0),
                    n(120.0),
                    s("smooth"),
                    b(false),
                ],
            ),
        ),
        (
            "repulsor",
            run(
                "force/repulsor",
                vec![
                    v2(10.0, 5.0),
                    n(90.0),
                    n(100.0),
                    s("inverse-square"),
                    b(false),
                ],
            ),
        ),
    ]
    .into_iter()
    .map(|(k, o)| (k.to_string(), force_of(&o[0])))
    .collect::<Vec<_>>();
    for c in ["constant", "linear", "smooth", "inverse", "inverse-square"] {
        let o = run(
            "force/attractor",
            vec![v2(10.0, 5.0), n(-150.0), n(100.0), s(c), b(false)],
        );
        out.push((format!("attractor-{c}"), force_of(&o[0])));
    }
    out
}

#[test]
fn forces_match_ts() {
    let world = World::default_world();
    for (k, f) in forces() {
        let mut buf = make_buf();
        buf.zero_forces();
        f.apply(&mut buf, DT, &world);
        f.apply(&mut buf, DT, &world);
        let want = &expected()[format!("force:{k}")];
        check_f32(&format!("{k}.forceX"), &buf.force_x, &want["forceX"]);
        check_f32(&format!("{k}.forceY"), &buf.force_y, &want["forceY"]);
    }
}

#[test]
fn force_viz_matches_ts() {
    let a = run(
        "force/attractor",
        vec![v2(1.0, 2.0), n(-5.0), n(30.0), s("linear"), b(true)],
    );
    let v = run(
        "force/vortex",
        vec![v2(1.0, 2.0), n(5.0), n(5.0), n(30.0), s("linear"), b(true)],
    );
    for (k, out) in [("attractor", a), ("vortex", v)] {
        let got = to_json(&Value::List(out[1].clone()));
        assert_eq!(got, Some(expected()[format!("viz:{k}")].clone()), "{k}");
    }
}

fn colliders() -> Vec<(&'static str, Spread)> {
    let c = |ty: &str, inputs: Vec<Vec<Value>>| run(ty, inputs).remove(0);
    vec![
        (
            "circle-bounce",
            c(
                "collider/circle",
                vec![v2(0.0, 10.0), n(40.0), s("bounce"), n(0.7), b(false)],
            ),
        ),
        (
            "circle-kill",
            c(
                "collider/circle",
                vec![v2(0.0, 10.0), n(40.0), s("kill"), n(0.7), b(false)],
            ),
        ),
        (
            "circle-absorb-inv",
            c(
                "collider/circle",
                vec![v2(0.0, 10.0), n(60.0), s("absorb"), n(0.7), b(true)],
            ),
        ),
        (
            "circle-bounce-inv",
            c(
                "collider/circle",
                vec![v2(0.0, 10.0), n(60.0), s("bounce"), n(0.9), b(true)],
            ),
        ),
        (
            "rect-bounce",
            c(
                "collider/rect",
                vec![v2(0.0, 10.0), v2(80.0, 60.0), s("bounce"), n(0.5), b(false)],
            ),
        ),
        (
            "rect-bounce-inv",
            c(
                "collider/rect",
                vec![v2(0.0, 10.0), v2(80.0, 60.0), s("bounce"), n(1.5), b(true)],
            ),
        ),
        (
            "bounds-wrap",
            c(
                "particles/bounds",
                vec![v2(100.0, 80.0), v2(0.0, 0.0), s("wrap"), n(0.6)],
            ),
        ),
        (
            "bounds-kill",
            c(
                "particles/bounds",
                vec![v2(100.0, 80.0), v2(0.0, 0.0), s("kill"), n(0.6)],
            ),
        ),
        (
            "bounds-bounce",
            c(
                "particles/bounds",
                vec![v2(100.0, 80.0), v2(0.0, 0.0), s("bounce"), n(0.6)],
            ),
        ),
        (
            "layer-rect-center",
            c(
                "collider/from-layer",
                vec![
                    scene(
                        json!({"kind": "rect", "x": 0, "y": 10, "w": 80, "h": 60, "mode": "center"}),
                    ),
                    s("bounce"),
                    n(0.8),
                    b(false),
                ],
            ),
        ),
        (
            "layer-rect-corners-inv",
            c(
                "collider/from-layer",
                vec![
                    scene(
                        json!({"kind": "rect", "x": -20, "y": 0, "w": 60, "h": 50, "mode": "corners"}),
                    ),
                    s("absorb"),
                    n(0.8),
                    b(true),
                ],
            ),
        ),
        (
            "layer-ellipse-corner",
            c(
                "collider/from-layer",
                vec![
                    scene(
                        json!({"kind": "ellipse", "x": -30, "y": -20, "w": 70, "h": 50, "mode": "corner"}),
                    ),
                    s("bounce"),
                    n(0.8),
                    b(false),
                ],
            ),
        ),
        (
            "layer-triangle",
            c(
                "collider/from-layer",
                vec![scene(tri()), s("bounce"), n(0.8), b(false)],
            ),
        ),
        (
            "layer-polygon",
            c(
                "collider/from-layer",
                vec![scene(poly()), s("bounce"), n(0.6), b(false)],
            ),
        ),
        (
            "layer-polygon-inv",
            c(
                "collider/from-layer",
                vec![scene(poly()), s("bounce"), n(0.6), b(true)],
            ),
        ),
        (
            "layer-polygon-kill",
            c(
                "collider/from-layer",
                vec![scene(poly()), s("kill"), n(0.6), b(false)],
            ),
        ),
    ]
}

#[test]
fn colliders_match_ts() {
    let world = World::default_world();
    for (k, c) in colliders() {
        let mut buf = make_buf();
        collider_of(&c).apply(&mut buf, DT, &world);
        check_buf(k, &buf, &expected()[format!("collider:{k}")]);
    }
}

#[test]
fn collider_names_match_ts() {
    // `{ apply }` closures: named "apply", except from-layer's anonymous ones.
    let named = |c: &Spread| collider_of(c).fn_name;
    assert_eq!(named(&colliders()[0].1), "apply");
    assert_eq!(named(&colliders()[6].1), "apply");
    assert_eq!(named(&colliders()[9].1), "");
    let none = run(
        "collider/from-layer",
        vec![vec![Value::Null], s("bounce"), n(0.8), b(false)],
    );
    assert_eq!(named(&none[0]), "apply");
}

#[test]
fn integrators_match_ts() {
    let drag = force_of(&run("force/drag", vec![n(0.5)])[0]);
    for integ in [Integrator::Euler, Integrator::Verlet] {
        let mut buf = make_buf();
        let world = World {
            gravity_x: 3.0,
            gravity_y: 150.0,
            drag: 0.4,
            integrator: integ,
            ..World::default_world()
        };
        let apply = |b: &mut ParticleBuffer| -> Result<(), ()> {
            b.zero_forces();
            drag.apply(b, DT, &world);
            Ok(())
        };
        apply(&mut buf).unwrap();
        match integ {
            Integrator::Euler => integrate_euler(&mut buf, DT, &world),
            Integrator::Verlet => integrate_verlet(&mut buf, DT, &world, apply).unwrap(),
        }
        check_buf(
            integ.name(),
            &buf,
            &expected()[format!("integrate:{}", integ.name())],
        );
    }
}

#[test]
fn from_layer_samples_text_inside_its_glyphs() {
    let mut st = (**scene::default_style()).clone();
    st.font_size = 200.0;
    st.text_align = Arc::from("center");
    let text = Value::Scene(Arc::new(SceneNode::Text {
        text: Arc::from("O"),
        x: 0.0,
        y: 0.0,
        style: Arc::new(st.clone()),
    }));
    let contours = {
        let placed = crate::nodes::text_animate::glyphs(std::slice::from_ref(&text));
        crate::text::outline::run_contours(&scene::GlyphRun {
            glyphs: placed.iter().map(|g| g.glyph).collect(),
            size: 200.0,
            style: Arc::new(st),
        })
    };
    assert!(contours.len() >= 2, "an O has a hole: {}", contours.len());
    let out = run(
        "particles/from-layer",
        vec![vec![text.clone()], s("fill"), n(300.0)],
    );
    assert_eq!(out[0].len(), 300);
    for p in out[0].iter() {
        let Value::Vec2(p) = p else { panic!() };
        // Inside the ink, never in the counter or the box's corners.
        assert_ne!(crate::text::outline::winding(&contours, *p), 0, "{p:?}");
    }
    // Replaying TS reference data keeps its bounding-box fallback.
    let _ts = scene::ts_reference_line_default();
    let bbox = run(
        "particles/from-layer",
        vec![vec![text], s("fill"), n(300.0)],
    );
    assert!(bbox[0].iter().any(|p| match p {
        Value::Vec2(p) => crate::text::outline::winding(&contours, *p) == 0,
        _ => false,
    }));
}

#[test]
fn from_layer_sampling_matches_ts() {
    // The TS cases recorded the text bounding-box fallback.
    let _ts = scene::ts_reference_line_default();
    let text = {
        let mut st = (**scene::default_style()).clone();
        st.font_size = 30.0;
        st.text_align = Arc::from("right");
        st.font_family = Arc::from("serif");
        vec![Value::Scene(Arc::new(SceneNode::Text {
            text: Arc::from("Hi!"),
            x: 10.0,
            y: 20.0,
            style: Arc::new(st),
        }))]
    };
    let small_tri = json!({"kind": "polygon", "points": [{"x": 0, "y": 0}, {"x": 50, "y": 0}, {"x": 0, "y": 50}], "closed": true});
    let mut open = poly();
    open["closed"] = json!(false);
    let group = vec![Value::Scene(Arc::new(SceneNode::group(vec![
        match &scene(poly())[0] {
            Value::Scene(s) => s.clone(),
            _ => unreachable!(),
        },
        match &scene(small_tri.clone())[0] {
            Value::Scene(s) => s.clone(),
            _ => unreachable!(),
        },
        Arc::new(SceneNode::Arc {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            start: 0.0,
            stop: 1.0,
            mode: Arc::from("center"),
            style: scene::default_style().clone(),
        }),
    ])))];
    let cases: Vec<(&str, Vec<Value>, &str, f64)> = vec![
        (
            "rect-fill",
            scene(json!({"kind": "rect", "x": 5, "y": 6, "w": 40, "h": 20, "mode": "center"})),
            "fill",
            5.0,
        ),
        (
            "rect-outline",
            scene(json!({"kind": "rect", "x": 5, "y": 6, "w": 40, "h": 20, "mode": "corners"})),
            "outline",
            7.0,
        ),
        (
            "ellipse-fill",
            scene(json!({"kind": "ellipse", "x": 5, "y": 6, "w": 40, "h": 20, "mode": "radius"})),
            "fill",
            4.0,
        ),
        (
            "ellipse-outline",
            scene(json!({"kind": "ellipse", "x": 5, "y": 6, "w": 40, "h": 20, "mode": "corner"})),
            "outline",
            5.0,
        ),
        (
            "line",
            scene(json!({"kind": "line", "x1": 0, "y1": 0, "x2": 10, "y2": 20})),
            "fill",
            4.0,
        ),
        (
            "line-1",
            scene(json!({"kind": "line", "x1": 3, "y1": 4, "x2": 10, "y2": 20})),
            "fill",
            1.0,
        ),
        ("polygon-outline", scene(poly()), "outline", 6.0),
        ("polygon-open-fill", scene(open), "fill", 5.0),
        ("polygon-fill", scene(small_tri), "fill", 5.0),
        ("triangle-fill", scene(tri()), "fill", 4.0),
        ("triangle-outline", scene(tri()), "outline", 7.0),
        (
            "point",
            scene(json!({"kind": "point", "x": 3, "y": 4})),
            "fill",
            9.0,
        ),
        ("group", group, "fill", 4.0),
        ("text-outline", text, "outline", 6.0),
    ];
    for (k, layer, mode, density) in cases {
        let out = run("particles/from-layer", vec![layer, s(mode), n(density)]);
        let want = expected()[format!("sample:{k}")].as_array().unwrap();
        assert_eq!(out[0].len(), want.len(), "{k}: count");
        for (i, (got, w)) in out[0].iter().zip(want).enumerate() {
            let Value::Vec2(p) = got else { panic!("{k}") };
            let (wx, wy) = (num(&w["x"]), num(&w["y"]));
            // cos / sin may differ from V8's in the last bit.
            let close = |a: f64, b: f64| (a - b).abs() <= 1e-12 * (1.0 + b.abs());
            assert!(
                close(p.x, wx) && close(p.y, wy),
                "{k}[{i}]: rust {p:?} vs ts ({wx}, {wy})"
            );
        }
    }
}

#[test]
fn world_matches_ts() {
    let drag = run("force/drag", vec![n(0.5)]).remove(0);
    let coll = run(
        "collider/circle",
        vec![v2(0.0, 10.0), n(40.0), s("bounce"), n(0.7), b(false)],
    )
    .remove(0);
    let w = run(
        "physics/world",
        vec![
            v2(0.0, 50.0),
            n(0.2),
            n(1.5),
            v2(3.0, 4.0),
            drag.to_vec(),
            coll.to_vec(),
            s("euler"),
        ],
    );
    assert_eq!(to_json(&w[0][0]), Some(expected()["world"].clone()));
}

#[test]
fn emitter_matches_ts() {
    let drag = run("force/drag", vec![n(0.5)]).remove(0);
    let gravity = run("force/gravity", vec![v2(3.0, 4.0), n(50.0)]).remove(0);
    let coll = run(
        "collider/circle",
        vec![v2(0.0, 10.0), n(40.0), s("bounce"), n(0.7), b(false)],
    )
    .remove(0);
    let world = run(
        "physics/world",
        vec![
            v2(0.0, 50.0),
            n(0.2),
            n(1.5),
            v2(3.0, 4.0),
            drag.to_vec(),
            coll.to_vec(),
            s("euler"),
        ],
    )
    .remove(0);
    let mut em = Node::new("particles/emitter");
    let want = expected()["emitter"].as_array().unwrap();
    for (f, w) in want.iter().enumerate() {
        let out = em.eval(vec![
            world.to_vec(),
            vec![
                Value::Vec2(Vec2 { x: 1.0, y: 2.0 }),
                Value::Null,
                Value::Vec2(Vec2 { x: -5.0, y: 7.0 }),
            ],
            n(200.0),
            n(0.05),
            v2(10.0, -20.0),
            n(0.3),
            vec![Value::Color(Color {
                r: 0.5,
                g: 0.25,
                b: 1.0,
                a: 0.75,
            })],
            n(3.0),
            n(4.0),
            s("replace-oldest"),
            n(9.0),
            gravity.to_vec(),
            n(f as f64 * 0.02),
        ]);
        let Value::Handle(Handle::Particles(h)) = &out[0][0] else {
            panic!("emitter output")
        };
        let buf = h.read();
        check_buf(&format!("tick {f}"), &buf, w);
        check_f32(&format!("tick {f}.age"), &buf.age, &w["age"]);
        check_f32(&format!("tick {f}.colorB"), &buf.color_b, &w["colorB"]);
        let ids: Vec<f32> = buf.id.iter().map(|i| *i as f32).collect();
        check_f32(&format!("tick {f}.id"), &ids, &w["id"]);
    }
}
