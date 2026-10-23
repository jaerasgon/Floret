//! The tracking showpieces (`pose-ribbons`, `face-constellation`) with a
//! stand-in tracker: a dancing figure and a talking face. The provider is
//! process-wide, so this is its own test binary.

mod common {
    pub mod fake_bodies;
}
use common::fake_bodies as bodies;

use floret_engine::devices::{
    self, CameraFrame, CameraRequest, Devices, LandmarkLayer, LandmarksRequest,
};
use floret_engine::image::Image;
use floret_engine::landmarks::Tracker;
use floret_engine::{Engine, FrameCtx, Value};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// What the stand-in tracker sees right now.
#[derive(Default)]
struct Scene {
    /// Seconds into the performance.
    t: f64,
    /// Nobody in front of the camera.
    empty: bool,
}

#[derive(Default)]
struct Fake(Mutex<Scene>);

impl Devices for Fake {
    fn camera(&self, _: &str, req: &CameraRequest) -> Result<CameraFrame, String> {
        // A 4:3 picture, as the stand-in bodies are drawn for.
        Ok(CameraFrame {
            image: if req.start {
                Arc::new(Image::new(64, 48, vec![90; 64 * 48 * 4]))
            } else {
                Image::placeholder()
            },
            ready: req.start,
        })
    }

    fn landmarks(
        &self,
        _: &str,
        req: &LandmarksRequest,
    ) -> Result<Option<Vec<LandmarkLayer>>, String> {
        let s = self.0.lock().unwrap();
        if !req.start || s.empty {
            return Ok(Some(Vec::new()));
        }
        let mut layers = Vec::new();
        for tracker in &req.trackers {
            let det = match tracker {
                Tracker::Pose => bodies::pose(s.t),
                Tracker::Face => bodies::talking_face(s.t),
                Tracker::Hands => continue,
            };
            layers.push(LandmarkLayer {
                tracker: *tracker,
                dets: vec![det],
                ids: vec![0],
            });
        }
        Ok(Some(layers))
    }
}

fn fake() -> Arc<Fake> {
    static FAKE: OnceLock<Arc<Fake>> = OnceLock::new();
    let f = FAKE.get_or_init(|| Arc::new(Fake::default())).clone();
    devices::install(Some(f.clone()));
    f
}

/// Tests share the provider: one at a time.
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn text(id: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../samples/{id}.flora"));
    std::fs::read_to_string(path).unwrap()
}

fn frame(n: u32) -> FrameCtx {
    FrameCtx {
        t: f64::from(n) / 60.0,
        dt: 1.0 / 60.0,
        frame_count: f64::from(n),
        width: 640.0,
        height: 480.0,
        ..FrameCtx::default()
    }
}

/// A performance of one sample: the engine and its last outputs.
struct Run {
    engine: Engine,
    out: floret_engine::tick::Outputs,
    frame: u32,
}

impl Run {
    fn new(id: &str) -> Run {
        let (engine, warnings) = Engine::load_with_warnings(&text(id)).unwrap();
        assert!(warnings.is_empty(), "{id}: {warnings:?}");
        Run {
            engine,
            out: Default::default(),
            frame: 0,
        }
    }

    /// Tick `n` frames of the performance, from time `from` seconds on at
    /// real speed (or standing still when `still`), `empty` when nobody
    /// is there. Every frame must run without node errors.
    fn play(&mut self, f: &Fake, n: u32, from: f64, still: bool, empty: bool) {
        for i in 0..n {
            {
                let mut s = f.0.lock().unwrap();
                s.t = if still {
                    from
                } else {
                    from + f64::from(i) / 60.0
                };
                s.empty = empty;
            }
            let r = self.engine.tick(&frame(self.frame));
            assert!(r.errors.is_empty(), "frame {}: {:?}", self.frame, r.errors);
            self.out = r.outputs;
            self.frame += 1;
        }
    }

    fn all(&self, node: &str, port: usize) -> Vec<Value> {
        let idx = self
            .engine
            .graph
            .idx(node)
            .unwrap_or_else(|| panic!("{node} in the graph"));
        self.out[idx]
            .as_ref()
            .unwrap_or_else(|| panic!("{node} has no output"))[port]
            .to_vec()
    }

    fn num(&self, node: &str) -> f64 {
        self.all(node, 0)
            .first()
            .and_then(Value::as_f64)
            .unwrap_or_else(|| panic!("{node} has no number"))
    }

    fn points(&self, node: &str) -> Vec<(f64, f64)> {
        self.all(node, 0)
            .iter()
            .map(|v| match v {
                Value::Vec2(p) => (p.x, p.y),
                other => panic!("{node}: {other:?} isn't a point"),
            })
            .collect()
    }
}

/// Where the stand-in tracker puts landmark `i` of `det` on the canvas
/// the way the Landmarks node does with the sample's `size` of 640 and
/// the mirror on, then flattened by the 4:3 picture.
fn on_canvas(det: &[[f32; 3]], i: usize) -> (f64, f64) {
    let p = det[i];
    (
        (0.5 - (f64::from(p[0]) - 0.5)) * 640.0 - 320.0,
        (f64::from(p[1]) - 0.5) * 640.0 * 0.75,
    )
}

fn apart(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Every wire of a sample starts at an output and ends at an input its
/// node really has (the compiler drops a wire to a missing pin without
/// a word, so a typo would only show as a silent patch).
fn wires_land_on_pins(id: &str) {
    use floret_engine::patch::PatchNode;
    use floret_engine::spec::Catalogue;
    let cat = Catalogue::builtin();
    let patch = floret_engine::patch::PatchFile::load(&text(id)).unwrap();
    let node = |n: &str| patch.nodes.iter().find(|x| x.id == n).unwrap();
    let region_port = |n: &PatchNode, pin: &str| {
        n.ports
            .as_ref()
            .is_some_and(|ps| ps.iter().any(|p| p.id == pin))
    };
    let mut bad = Vec::new();
    for e in &patch.edges {
        let (src, dst) = (node(&e.source), node(&e.target));
        let out = e
            .source_handle
            .as_deref()
            .and_then(|h| h.strip_prefix("out-"));
        let inp = e
            .target_handle
            .as_deref()
            .and_then(|h| h.strip_prefix("in-"));
        let (Some(out), Some(inp)) = (out, inp) else {
            bad.push(format!("handles of {} to {}", e.source, e.target));
            continue;
        };
        let has_out = region_port(src, out)
            || out.starts_with("__loopvar_")
            || cat
                .resolved_outputs(cat.get(&src.def_type).unwrap(), &src.params)
                .iter()
                .any(|p| p.name == out);
        let has_in = region_port(dst, inp)
            || cat
                .resolved_inputs(cat.get(&dst.def_type).unwrap(), &dst.params)
                .iter()
                .any(|p| p.name == inp);
        if !has_out {
            bad.push(format!("{} has no output `{out}`", src.id));
        }
        if !has_in {
            bad.push(format!("{} has no input `{inp}`", dst.id));
        }
    }
    assert!(bad.is_empty(), "{id}:\n{}", bad.join("\n"));
}

#[test]
fn showpiece_wires_land_on_pins() {
    wires_land_on_pins("pose-ribbons");
    wires_land_on_pins("face-constellation");
}

/// The pose landmarks the ribbons follow: head, hands, feet.
const RIBBON_POINTS: [usize; 5] = [0, 15, 16, 27, 28];

#[test]
fn a_dancing_figure_leaves_five_ribbons() {
    let _g = lock();
    let f = fake();
    let mut run = Run::new("pose-ribbons");
    // The memory fills five points a frame, up to 110 frames.
    run.play(&f, 20, 0.0, false, false);
    assert_eq!(run.all("pr_keep", 0).len(), 100);
    run.play(&f, 200, 0.4, false, false);
    assert_eq!(run.all("pr_keep", 0).len(), 550);
    let now = 0.4 + 199.0 / 60.0;
    let figure = bodies::pose(now);
    for (k, &landmark) in RIBBON_POINTS.iter().enumerate() {
        let trail = run.points(&format!("pr_trail{k}"));
        assert_eq!(trail.len(), 110, "ribbon {k}");
        // The newest point of each ribbon is where its landmark is now.
        let here = on_canvas(&figure, landmark);
        assert!(
            apart(trail[0], here) < 1.0,
            "ribbon {k}: {:?} vs {here:?}",
            trail[0]
        );
        // Behind it, the frame before.
        let before = on_canvas(&bodies::pose(now - 1.0 / 60.0), landmark);
        assert!(apart(trail[1], before) < 1.0, "ribbon {k} one frame back");
    }
    // The hands sweep wide; the ribbon follows.
    let hand = run.points("pr_trail1");
    let reach = hand.iter().map(|p| apart(*p, hand[0])).fold(0.0, f64::max);
    assert!(reach > 100.0, "the hand's ribbon spans {reach}");
    // Fat now, thin behind, all of them visible.
    let widths: Vec<f64> = run
        .all("pr_thin", 0)
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    assert_eq!(widths.len(), 110);
    assert!(widths[0] > 14.0 && widths[109] < 1.0, "{widths:?}");
    assert!(widths.windows(2).all(|w| w[0] >= w[1]), "thinner with age");
    // Every ribbon is a drawing.
    for k in 0..5 {
        assert!(
            matches!(
                run.all(&format!("pr_ribbon{k}"), 0).first(),
                Some(Value::Scene(_))
            ),
            "ribbon {k} draws"
        );
    }
    // And the skeleton behind it: the tracker's connections, as lines.
    assert_eq!(run.all("pr_bones", 0).len(), 35);
}

#[test]
fn the_ribbons_melt_away_when_nobody_is_there() {
    let _g = lock();
    let f = fake();
    let mut run = Run::new("pose-ribbons");
    run.play(&f, 200, 0.0, false, false);
    assert!(run.num("pr_stay") > 0.99);
    run.play(&f, 200, 0.0, true, true);
    // The memory holds the last place seen: every point of a ribbon the
    // same, so nothing smears across the picture, and the width is gone.
    let hand = run.points("pr_trail1");
    assert!(apart(hand[0], hand[109]) < 1e-9, "the ribbon has collapsed");
    assert!(apart(hand[0], on_canvas(&bodies::pose(199.0 / 60.0), 15)) < 1.0);
    let widths: Vec<f64> = run
        .all("pr_thin", 0)
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    assert!(widths.iter().all(|w| *w < 0.01), "{widths:?}");
    // Back in front of the camera, the ribbons grow again from where the
    // figure stands.
    run.play(&f, 30, 1.0, false, false);
    assert!(run.num("pr_stay") > 0.9);
    let hand = run.points("pr_trail1");
    assert!(apart(hand[0], on_canvas(&bodies::pose(1.0 + 29.0 / 60.0), 15)) < 1.0);
}

#[test]
fn nobody_at_the_start_draws_nothing_and_breaks_nothing() {
    let _g = lock();
    let f = fake();
    let mut run = Run::new("pose-ribbons");
    run.play(&f, 40, 0.0, true, true);
    assert_eq!(run.all("pr_keep", 0).len(), 0, "nothing to remember");
    // Somebody walks in: the memory starts clean, five points a frame.
    run.play(&f, 10, 0.0, false, false);
    assert_eq!(run.all("pr_keep", 0).len(), 50);
    let head = run.points("pr_trail0");
    assert_eq!(head.len(), 10);
    assert!(apart(head[0], on_canvas(&bodies::pose(9.0 / 60.0), 0)) < 1.0);
}

#[test]
fn a_talking_face_lights_the_constellation() {
    let _g = lock();
    let f = fake();
    let mut run = Run::new("face-constellation");
    // Mouth shut (time 0 of the stand-in's talking), then wide open (0.5 s).
    run.play(&f, 60, 0.0, true, false);
    let (shut, shut_reach) = (run.num("fc_open"), run.num("fc_reach"));
    let shut_threads = run.all("fc_web", 1).len();
    assert!(shut < 0.15, "shut mouth reads {shut}");
    assert!(shut_reach < 45.0, "reach {shut_reach}");
    run.play(&f, 60, 0.5, true, false);
    let (wide, wide_reach) = (run.num("fc_open"), run.num("fc_reach"));
    assert!(wide > 0.75, "open mouth reads {wide}");
    assert!(
        wide_reach > shut_reach + 40.0,
        "reach {shut_reach} → {wide_reach}"
    );
    // Wider reach, more threads; the glow follows the mouth too.
    assert!(run.all("fc_web", 1).len() > shut_threads);
    assert!(run.num("fc_gleam") > 1.9, "the glow rises with the mouth");
    assert_eq!(run.all("fc_present", 0).len(), 1, "a face is drawn");

    // 156 stars: every third point of the mesh, all on the face.
    let stars = run.points("fc_star_at");
    assert_eq!(stars.len(), 156);
    let face = bodies::talking_face(0.5);
    for (i, s) in stars.iter().enumerate() {
        let landmark = on_canvas(&face, i * 3);
        assert!(apart(*s, landmark) < 1.0, "star {i}: {s:?} vs {landmark:?}");
    }
    // Each star on its own beat: sizes differ, all within the twinkle.
    let sizes: Vec<f64> = run
        .all("fc_size", 0)
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    assert_eq!(sizes.len(), 156);
    assert!(sizes.iter().all(|s| (1.8..=6.5).contains(s)));
    assert!(sizes.iter().cloned().fold(f64::MAX, f64::min) < 2.5);
    assert!(sizes.iter().cloned().fold(0.0, f64::max) > 5.8);

    // The eyes, lips and outline are traced on the real landmarks.
    let eye = run.points("fc_eyeRPts");
    assert_eq!(eye.len(), 16);
    assert!(apart(eye[0], on_canvas(&face, 33)) < 1.0);
    assert_eq!(run.points("fc_lipsPts").len(), 20);
    assert_eq!(run.points("fc_ovalPts").len(), 36);
    assert_eq!(run.points("fc_irisLPts").len(), 4);
    for shape in [
        "fc_eyeR", "fc_eyeL", "fc_irisR", "fc_irisL", "fc_lips", "fc_oval",
    ] {
        assert!(
            matches!(run.all(shape, 0).first(), Some(Value::Scene(_))),
            "{shape} draws"
        );
    }

    // The head turns, the stars go with it.
    run.play(&f, 5, 1.5, true, false);
    let turned = run.points("fc_star_at");
    let shift = apart(turned[40], stars[40]);
    assert!(shift > 5.0, "a turned head moved a star {shift}");
}

#[test]
fn an_empty_picture_leaves_the_constellation_dark_and_quiet() {
    let _g = lock();
    let f = fake();
    let mut run = Run::new("face-constellation");
    run.play(&f, 30, 0.5, true, false);
    assert_eq!(run.all("fc_present", 0).len(), 1);
    // Nobody there: the stars would all sit at the centre, so nothing is
    // drawn at all, and the mouth sensor settles at shut.
    run.play(&f, 60, 0.5, true, true);
    assert_eq!(run.all("fc_present", 0).len(), 0, "no face, no drawing");
    assert!(run.num("fc_open") < 0.05, "nobody's mouth is shut");
}
