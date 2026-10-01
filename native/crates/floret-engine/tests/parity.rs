//! Parity against the TypeScript reference.
//!
//! For every sample in `native/samples`, compile the `.flora` with the Rust
//! engine, replay the recorded frame inputs, and compare every node's
//! outputs at the recorded frames with `native/fixtures/traces/<id>.json.gz`.
//!
//! The test prints a per-sample report and fails if any sample listed in
//! `tests/parity-passing.txt` no longer matches (a ratchet: once a sample
//! passes, it has to keep passing). Run with `-- --nocapture` to see the
//! report; set `PARITY_ONLY=<id,...>` to check specific samples and
//! `PARITY_VERBOSE=1` to print every mismatch.

use floret_engine::particles::{Handle, TypedArray};
use floret_engine::{scene, tick::Outputs, Engine, FrameCtx, Value};
use serde_json::Value as J;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ─── Fixture values ──────────────────────────────────────────────────────────

/// A decoded fixture value. Shared (`$id`/`$ref`) values decode to shared
/// `Arc`s, so huge DAGs stay linear in memory.
#[derive(Clone, Debug)]
enum Fx {
    Undef,
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Arc<Vec<Fx>>),
    Obj(Arc<Vec<(String, Fx)>>),
    /// A tagged value (`$style`, `$fn`, `$ta`, `$opaque`, `$summary`, …).
    Tag(String, Arc<J>),
    Cycle,
}

fn decode(j: &J, refs: &mut Vec<Fx>) -> Fx {
    match j {
        J::Null => Fx::Null,
        J::Bool(b) => Fx::Bool(*b),
        J::Number(n) => Fx::Num(n.as_f64().unwrap_or(f64::NAN)),
        J::String(s) => Fx::Str(s.clone()),
        J::Array(items) => Fx::Arr(Arc::new(items.iter().map(|x| decode(x, refs)).collect())),
        J::Object(map) => {
            if let Some(r) = map.get("$ref").and_then(J::as_u64) {
                return refs.get(r as usize).cloned().unwrap_or(Fx::Cycle);
            }
            if map.contains_key("$cycle") {
                return Fx::Cycle;
            }
            if map.len() == 1 && map.contains_key("$u") {
                return Fx::Undef;
            }
            if let Some(t) = map.get("$num").and_then(J::as_str) {
                return Fx::Num(match t {
                    "Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    _ => f64::NAN,
                });
            }
            let id = map.get("$id").and_then(J::as_u64).map(|i| i as usize);
            // Reserve the slot so nested back-references resolve to Cycle.
            if let Some(i) = id {
                if refs.len() <= i {
                    refs.resize(i + 1, Fx::Cycle);
                }
            }
            let out = if let Some(arr) = map.get("$arr").and_then(J::as_array) {
                Fx::Arr(Arc::new(arr.iter().map(|x| decode(x, refs)).collect()))
            } else if let Some((tag, inner)) = map
                .iter()
                .find(|(k, _)| k.starts_with('$') && *k != "$id" && *k != "$class")
            {
                Fx::Tag(
                    tag.clone(),
                    Arc::new(if tag == "$ta" {
                        j.clone()
                    } else {
                        inner.clone()
                    }),
                )
            } else {
                Fx::Obj(Arc::new(
                    map.iter()
                        .filter(|(k, _)| *k != "$id")
                        .map(|(k, v)| (k.clone(), decode(v, refs)))
                        .collect(),
                ))
            };
            if let Some(i) = id {
                refs[i] = out.clone();
            }
            out
        }
    }
}

// ─── Comparison ──────────────────────────────────────────────────────────────

fn num_eq(a: f64, b: f64) -> bool {
    if a.is_nan() || b.is_nan() {
        return a.is_nan() && b.is_nan();
    }
    if a.is_infinite() || b.is_infinite() {
        return a == b;
    }
    let diff = (a - b).abs();
    diff <= 1e-9 || diff <= 1e-6 * a.abs().max(b.abs())
}

struct Cmp {
    seen: HashSet<(usize, usize)>,
    /// Non-zero while comparing a `SceneNode::to_value()` result: its lists and
    /// objects are temporaries whose addresses get reused once freed, so
    /// they must not be remembered in `seen`.
    temp: usize,
    /// Shared by every `$summary` in a frame (memoised values stay alive
    /// for the whole frame).
    summ: Summarizer,
}

impl Cmp {
    fn value(&mut self, v: &Value, f: &Fx, path: &str) -> Result<(), String> {
        let mismatch = || Err(format!("{path}: rust {} vs ts {}", short(v), short_fx(f)));
        match (v, f) {
            (Value::Undefined, Fx::Undef) | (Value::Null, Fx::Null) => Ok(()),
            (Value::Bool(a), Fx::Bool(b)) if a == b => Ok(()),
            (Value::Num(a), Fx::Num(b)) if num_eq(*a, *b) => Ok(()),
            (Value::Str(a), Fx::Str(b)) if **a == **b => Ok(()),
            (Value::Vec2(p), Fx::Obj(o)) => self
                .fields(&[("x", p.x), ("y", p.y)], o, path)
                .or_else(|_| mismatch()),
            (Value::Color(c), Fx::Obj(o)) => self
                .fields(&[("r", c.r), ("g", c.g), ("b", c.b), ("a", c.a)], o, path)
                .or_else(|_| mismatch()),
            (Value::List(a), Fx::Arr(b)) => {
                let key = (
                    Arc::as_ptr(a) as *const () as usize,
                    Arc::as_ptr(b) as usize,
                );
                if self.temp == 0 && !self.seen.insert(key) {
                    return Ok(());
                }
                if a.len() != b.len() {
                    return Err(format!("{path}: length rust {} vs ts {}", a.len(), b.len()));
                }
                for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                    self.value(x, y, &format!("{path}[{i}]"))?;
                }
                Ok(())
            }
            (Value::Object(a), Fx::Obj(b)) => {
                let key = (Arc::as_ptr(a) as usize, Arc::as_ptr(b) as usize);
                if self.temp == 0 && !self.seen.insert(key) {
                    return Ok(());
                }
                if a.len() != b.len() {
                    let ak: Vec<_> = a.keys().collect();
                    let bk: Vec<_> = b.iter().map(|(k, _)| k).collect();
                    return Err(format!("{path}: keys rust {ak:?} vs ts {bk:?}"));
                }
                for (k, y) in b.iter() {
                    let Some(x) = a.get(k) else {
                        return Err(format!("{path}: rust missing key {k}"));
                    };
                    self.value(x, y, &format!("{path}.{k}"))?;
                }
                Ok(())
            }
            (Value::Scene(a), Fx::Obj(b)) => {
                let key = (Arc::as_ptr(a) as usize, Arc::as_ptr(b) as usize);
                if !self.seen.insert(key) {
                    return Ok(());
                }
                self.temp += 1;
                let r = self.value(&a.to_value(), f, path);
                self.temp -= 1;
                r
            }
            // A TS style closure is recorded as its result on NEUTRAL_STYLE.
            (Value::Style(st), Fx::Tag(tag, inner)) if tag == "$style" => {
                let resolved = scene::style_value(&st.apply(scene::neutral_style()));
                let mut refs = Vec::new();
                self.temp += 1;
                let r = self.value(
                    &resolved,
                    &decode(inner, &mut refs),
                    &format!("{path}<style>"),
                );
                self.temp -= 1;
                r
            }
            // Opaque JS objects (worlds, forces, colliders, particle
            // buffers, typed arrays) — see `floret_engine::particles`.
            (Value::Handle(h), Fx::Tag(tag, inner)) if tag == "$fn" => {
                if h.fn_name() == inner.as_str() {
                    Ok(())
                } else {
                    mismatch()
                }
            }
            (Value::Handle(Handle::TypedArray(t)), Fx::Tag(tag, j)) if tag == "$ta" => {
                typed_array(t, j).map_err(|e| format!("{path}: {e}"))
            }
            (Value::Handle(h), Fx::Obj(o)) => {
                let key = (handle_ptr(h), Arc::as_ptr(o) as usize);
                if !self.seen.insert(key) {
                    return Ok(());
                }
                // A class instance is recorded as {"$class": name, ...fields}.
                let class = o.iter().find(|(k, _)| k == "$class").map(|(_, v)| v);
                match (h.class_name(), class) {
                    (None, None) => {}
                    (Some(want), Some(Fx::Str(got))) if want == got => {}
                    _ => return mismatch(),
                }
                let Some(view) = h.to_value() else {
                    return mismatch();
                };
                let fields = Fx::Obj(Arc::new(
                    o.iter().filter(|(k, _)| k != "$class").cloned().collect(),
                ));
                self.temp += 1;
                let r = self.value(&view, &fields, path);
                self.temp -= 1;
                r
            }
            // A canvas is recorded opaque; with no real canvas in Node its
            // pixels were blank (a no-op mock), so only the type is compared.
            (Value::Image(_), Fx::Tag(tag, inner))
                if tag == "$opaque" && inner.as_str().is_some_and(|s| s.contains("Canvas")) =>
            {
                Ok(())
            }
            (_, Fx::Tag(tag, inner)) if tag == "$summary" => {
                let got = self.summ.visit(v);
                check_summary(inner, &got, path)
            }
            _ => mismatch(),
        }
    }

    fn fields(
        &mut self,
        want: &[(&str, f64)],
        o: &[(String, Fx)],
        path: &str,
    ) -> Result<(), String> {
        if o.len() != want.len() {
            return Err(path.into());
        }
        for (k, n) in want {
            match o.iter().find(|(key, _)| key == k) {
                Some((_, Fx::Num(m))) if num_eq(*n, *m) => {}
                _ => return Err(path.into()),
            }
        }
        Ok(())
    }
}

fn check_summary(want: &J, got: &Stats, path: &str) -> Result<(), String> {
    let want: Stats = serde_json::from_value(want.clone()).map_err(|e| e.to_string())?;
    if want.matches(got) {
        Ok(())
    } else {
        Err(format!("{path}: summary rust {got:?} vs ts {want:?}"))
    }
}

/// Identity of a handle's shared allocation (for the `seen` set).
fn handle_ptr(h: &Handle) -> usize {
    match h {
        Handle::World(w) => Arc::as_ptr(w) as usize,
        Handle::Force(f) => Arc::as_ptr(f) as usize,
        Handle::Collider(c) => Arc::as_ptr(c) as usize,
        Handle::Particles(p) => Arc::as_ptr(&p.0) as *const () as usize,
        _ => 0,
    }
}

/// A typed array against `{"$ta": name, "len", "fnv", "head"}`: the FNV-1a
/// of the raw little-endian bytes must match exactly.
fn typed_array(t: &TypedArray, j: &J) -> Result<(), String> {
    let name = j["$ta"].as_str().unwrap_or("");
    let len = j["len"].as_u64().unwrap_or(u64::MAX);
    let fnv = j["fnv"].as_str().unwrap_or("");
    if t.name() != name || t.len() as u64 != len {
        return Err(format!(
            "typed array rust {}[{}] vs ts {name}[{len}]",
            t.name(),
            t.len()
        ));
    }
    let got = t.fnv1a();
    if got != fnv {
        let head: Vec<f64> = (0..t.len().min(8)).map(|i| t.get(i)).collect();
        return Err(format!(
            "{name} fnv rust {got} vs ts {fnv}; head rust {head:?} vs ts {}",
            j["head"]
        ));
    }
    Ok(())
}

/// Sharing-independent stats over a value as a JS tree (see the exporter's
/// `$summary`): every object/array reached counts, every time.
#[derive(Clone, Debug, Default, serde::Deserialize)]
struct Stats {
    objects: f64,
    numbers: f64,
    sum: f64,
    abs: f64,
    strings: f64,
    kinds: BTreeMap<String, f64>,
}

impl Stats {
    fn add(&mut self, o: &Stats) {
        self.objects += o.objects;
        self.numbers += o.numbers;
        self.sum += o.sum;
        self.abs += o.abs;
        self.strings += o.strings;
        for (k, n) in &o.kinds {
            *self.kinds.entry(k.clone()).or_default() += n;
        }
    }

    fn matches(&self, o: &Stats) -> bool {
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0);
        self.objects == o.objects
            && self.numbers == o.numbers
            && self.strings == o.strings
            && self.kinds == o.kinds
            && close(self.sum, o.sum)
            && close(self.abs, o.abs)
    }
}

#[derive(Default)]
struct Summarizer {
    memo: HashMap<usize, Stats>,
    /// Scene nodes per kind already cross-checked against `to_value()`.
    checked: HashMap<&'static str, usize>,
    styles_checked: usize,
}

impl Summarizer {
    fn num(n: f64) -> Stats {
        let mut s = Stats {
            numbers: 1.0,
            ..Default::default()
        };
        if n.is_finite() {
            s.sum = n;
            s.abs = n.abs();
        }
        s
    }

    fn visit(&mut self, v: &Value) -> Stats {
        self.visit_in(v, false)
    }

    /// `temp`: `v` is inside a `SceneNode::to_value()` result, whose lists
    /// and objects are freed (and their addresses reused) right after, so
    /// only scene nodes are memoised there.
    fn visit_in(&mut self, v: &Value, temp: bool) -> Stats {
        let key = match v {
            Value::List(a) if !temp => Some(Arc::as_ptr(a) as *const () as usize),
            Value::Object(a) if !temp => Some(Arc::as_ptr(a) as usize),
            Value::Scene(a) => Some(Arc::as_ptr(a) as usize),
            _ => None,
        };
        if let Some(s) = key.and_then(|k| self.memo.get(&k)) {
            return s.clone();
        }
        let mut s = Stats::default();
        match v {
            Value::Num(n) => return Self::num(*n),
            Value::Str(_) => {
                return Stats {
                    strings: 1.0,
                    ..Default::default()
                }
            }
            Value::Vec2(p) => {
                s.objects = 1.0;
                s.add(&Self::num(p.x));
                s.add(&Self::num(p.y));
            }
            Value::Color(c) => {
                s.objects = 1.0;
                for x in [c.r, c.g, c.b, c.a] {
                    s.add(&Self::num(x));
                }
            }
            Value::List(items) => {
                s.objects = 1.0;
                for x in items.iter() {
                    s.add(&self.visit_in(x, temp));
                }
            }
            Value::Object(m) => {
                s.objects = 1.0;
                if let Some(Value::Str(k)) = m.get("kind") {
                    s.kinds.insert(k.to_string(), 1.0);
                }
                for x in m.values() {
                    s.add(&self.visit_in(x, temp));
                }
            }
            Value::Scene(n) => s = self.scene(n),
            // Typed arrays are childless objects; other handles are walked
            // as their (temporary) JS objects.
            Value::Handle(Handle::TypedArray(_)) => s.objects = 1.0,
            Value::Handle(h) => {
                if let Some(o) = h.to_value() {
                    s = self.visit_in(&o, true);
                }
            }
            // Functions are leaves; undefined / null / booleans carry nothing.
            _ => {}
        }
        if let Some(k) = key {
            self.memo.insert(k, s.clone());
        }
        s
    }

    /// Stats of `n.to_value()`, computed without building it (a walker
    /// cluster is ~10⁶ shapes per frame). The first few nodes of each kind,
    /// and the first few styles, are cross-checked against the generic walk
    /// of `to_value()` / `style_value()`.
    fn scene(&mut self, n: &scene::SceneNode) -> Stats {
        let mut s = Stats::default();
        self.scene_into(n, &mut s);
        let checked = self.checked.entry(n.kind()).or_default();
        if *checked < 8 {
            *checked += 1;
            let generic = self.visit_in(&n.to_value(), true);
            assert!(
                generic.matches(&s) && generic.objects == s.objects,
                "summary of {} drifted from to_value(): {s:?} vs {generic:?}",
                n.kind()
            );
        }
        s
    }

    /// Adds `n`'s stats to `acc`. Leaf shapes accumulate in place (no
    /// per-shape `Stats`); groups go through `visit`, so shared subtrees
    /// are memoised.
    fn scene_into(&mut self, n: &scene::SceneNode, acc: &mut Stats) {
        use scene::SceneNode as S;
        fn nums(acc: &mut Stats, xs: &[f64]) {
            for &x in xs {
                acc.numbers += 1.0;
                if x.is_finite() {
                    acc.sum += x;
                    acc.abs += x.abs();
                }
            }
        }
        acc.objects += 1.0;
        acc.strings += 1.0; // kind
        match acc.kinds.get_mut(n.kind()) {
            Some(k) => *k += 1.0,
            None => {
                acc.kinds.insert(n.kind().to_owned(), 1.0);
            }
        }
        let style = match n {
            S::Rect {
                x, y, w, h, style, ..
            }
            | S::Ellipse {
                x, y, w, h, style, ..
            } => {
                nums(acc, &[*x, *y, *w, *h]);
                acc.strings += 1.0; // mode
                style
            }
            S::Line {
                x1,
                y1,
                x2,
                y2,
                style,
            } => {
                nums(acc, &[*x1, *y1, *x2, *y2]);
                style
            }
            S::Arc {
                x,
                y,
                w,
                h,
                start,
                stop,
                style,
                ..
            } => {
                nums(acc, &[*x, *y, *w, *h, *start, *stop]);
                acc.strings += 1.0; // mode
                style
            }
            S::Triangle { pts, style } => {
                nums(acc, pts);
                style
            }
            S::Quad { pts, style } => {
                nums(acc, pts);
                style
            }
            S::Point { x, y, style } => {
                nums(acc, &[*x, *y]);
                style
            }
            S::Polygon {
                points,
                style,
                tension,
                ..
            } => {
                acc.objects += 1.0 + points.len() as f64;
                for p in points.iter() {
                    nums(acc, &[p.x, p.y]);
                }
                if let Some(t) = tension {
                    nums(acc, &[*t]);
                }
                style
            }
            S::Text { x, y, style, .. } => {
                nums(acc, &[*x, *y]);
                acc.strings += 1.0; // text
                style
            }
            S::Group {
                children,
                transform,
                blend_mode,
                clip,
            } => {
                acc.objects += 1.0; // children array
                for c in children.iter() {
                    if matches!(**c, S::Group { .. }) {
                        let cs = self.visit(&Value::Scene(c.clone()));
                        acc.add(&cs);
                    } else {
                        self.scene_into(c, acc);
                    }
                }
                if let Some(t) = transform {
                    acc.objects += 1.0;
                    nums(acc, t);
                }
                if blend_mode.is_some() {
                    acc.strings += 1.0;
                }
                if let Some(c) = clip {
                    acc.objects += 1.0;
                    nums(acc, &[c.x, c.y, c.w, c.h]);
                }
                return;
            }
            // A variant this summary doesn't know: walk its JS shape.
            _ => {
                // Undo the header counted above; the generic walk counts it.
                acc.objects -= 1.0;
                acc.strings -= 1.0;
                *acc.kinds.get_mut(n.kind()).unwrap() -= 1.0;
                acc.kinds.retain(|_, v| *v != 0.0);
                let g = self.visit_in(&n.to_value(), true);
                acc.add(&g);
                return;
            }
        };
        // `style_value(style)`: one object, 5 numbers, 10 strings + paints.
        let before = (acc.objects, acc.numbers, acc.sum, acc.abs, acc.strings);
        acc.objects += 1.0;
        acc.strings += 10.0
            + f64::from(u8::from(style.fill.is_some()))
            + f64::from(u8::from(style.stroke.is_some()));
        nums(
            acc,
            &[
                style.stroke_weight,
                style.blur,
                style.font_size,
                style.letter_spacing,
                style.line_height,
            ],
        );
        if self.styles_checked < 8 {
            self.styles_checked += 1;
            let g = self.visit_in(&scene::style_value(style), true);
            let mine = Stats {
                objects: acc.objects - before.0,
                numbers: acc.numbers - before.1,
                sum: acc.sum - before.2,
                abs: acc.abs - before.3,
                strings: acc.strings - before.4,
                ..Default::default()
            };
            assert!(
                g.matches(&mine) && g.objects == mine.objects,
                "style summary drifted from style_value(): {mine:?} vs {g:?}"
            );
        }
    }
}

fn short(v: &Value) -> String {
    let s = format!("{v:?}");
    if s.len() > 120 {
        format!("{}…", &s[..120])
    } else {
        s
    }
}

fn short_fx(f: &Fx) -> String {
    let s = match f {
        Fx::Tag(t, j) => format!("{t}:{j}"),
        other => format!("{other:?}"),
    };
    if s.len() > 120 {
        format!("{}…", &s[..120])
    } else {
        s
    }
}

// ─── Running a sample ────────────────────────────────────────────────────────

/// Node types whose output depends on browser media (audio, camera).
/// Neither side can produce real output in tests; they're not compared.
/// Image nodes are: the exporter's canvases were blank mocks, which is what
/// the Rust engine produces without a rasteriser, and URL loads either had
/// no URL or stopped the trace (`tickError`).
fn is_media(ty: &str) -> bool {
    ty.starts_with("sound/") || matches!(ty, "input/camera" | "input/landmarks")
}

fn native_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_trace(id: &str) -> J {
    let path = native_dir().join(format!("fixtures/traces/{id}.json.gz"));
    let file = std::fs::File::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut text = String::new();
    flate2::read::GzDecoder::new(file)
        .read_to_string(&mut text)
        .expect("gzip");
    serde_json::from_str(&text).expect("trace json")
}

fn frame_from(j: &J) -> FrameCtx {
    let n = |k: &str| j.get(k).and_then(J::as_f64).unwrap_or(0.0);
    FrameCtx {
        t: n("t"),
        dt: n("dt"),
        frame_count: n("frameCount"),
        width: n("width"),
        height: n("height"),
        mouse_x: n("mouseX"),
        mouse_y: n("mouseY"),
        mouse_is_pressed: j
            .get("mouseIsPressed")
            .and_then(J::as_bool)
            .unwrap_or(false),
        key: j.get("key").and_then(J::as_str).unwrap_or("").to_owned(),
        key_is_pressed: j.get("keyIsPressed").and_then(J::as_bool).unwrap_or(false),
    }
}

#[derive(Default)]
struct SampleReport {
    compile_error: Option<String>,
    compile_diffs: Vec<String>,
    unimplemented: Vec<String>,
    /// Node ids that matched on every recorded frame.
    matched: usize,
    mismatched: BTreeMap<String, String>,
    skipped_media: usize,
}

impl SampleReport {
    fn passes(&self) -> bool {
        self.compile_error.is_none()
            && self.compile_diffs.is_empty()
            && self.unimplemented.is_empty()
            && self.mismatched.is_empty()
    }
}

fn run_sample(id: &str) -> SampleReport {
    let mut rep = SampleReport::default();
    let trace = read_trace(id);
    let text =
        std::fs::read_to_string(native_dir().join(format!("samples/{id}.flora"))).expect("flora");

    let (mut engine, warnings) = match Engine::load_with_warnings(&text) {
        Ok(x) => x,
        Err(e) => {
            if trace.get("compileError").is_none() {
                rep.compile_error = Some(e);
            }
            return rep;
        }
    };
    let Some(compile) = trace.get("compile") else {
        rep.compile_error = Some("rust compiled but ts failed".into());
        return rep;
    };

    // Compile parity: warnings and per-scope evaluation order.
    let ts_warn: Vec<&str> = compile["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(J::as_str)
        .collect();
    let rs_warn: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
    if ts_warn != rs_warn {
        rep.compile_diffs
            .push(format!("warnings: rust {rs_warn:?} vs ts {ts_warn:?}"));
    }
    let g = &engine.graph;
    for s in compile["scopes"].as_array().unwrap() {
        let key = s["parent"].as_str().map(|p| g.idx(p));
        let ts_order: Vec<&str> = s["order"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(J::as_str)
            .collect();
        let rs_order: Vec<&str> = match key {
            None => g.scopes.get(&None),
            Some(k) => g.scopes.get(&k),
        }
        .map(|sc| sc.order.iter().map(|&i| g.nodes[i].id.as_str()).collect())
        .unwrap_or_default();
        if ts_order != rs_order {
            rep.compile_diffs
                .push(format!("scope {:?} order differs", s["parent"]));
        }
    }
    for n in &g.nodes {
        if n.unimplemented && !is_media(&n.ty) && !rep.unimplemented.contains(&n.ty) {
            rep.unimplemented.push(n.ty.clone());
        }
    }

    // Replay ticks.
    let inputs = trace["inputs"].as_array().cloned().unwrap_or_default();
    let tick_error_frame = trace.get("tickError").and_then(|t| t["frame"].as_u64());
    let recorded: Vec<&J> = trace["frames"]
        .as_array()
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let mut rec_iter = recorded.into_iter().peekable();
    let mut node_ok: BTreeMap<String, bool> = BTreeMap::new();

    for (i, input) in inputs.iter().enumerate() {
        if tick_error_frame.is_some_and(|f| f as usize <= i) {
            break;
        }
        let frame = frame_from(input);
        let result = engine.tick(&frame);
        let Some(rec) = rec_iter.peek() else { continue };
        if rec["frame"]["frameCount"].as_f64() != Some(i as f64) {
            continue;
        }
        let rec = rec_iter.next().unwrap();
        compare_frame(&engine, &result.outputs, rec, i, &mut rep, &mut node_ok);
        probe_functions_like_exporter(&result.outputs);
    }
    rep.matched = node_ok.values().filter(|ok| **ok).count();
    rep
}

fn compare_frame(
    engine: &Engine,
    outputs: &Outputs,
    rec: &J,
    frame: usize,
    rep: &mut SampleReport,
    node_ok: &mut BTreeMap<String, bool>,
) {
    let mut refs = Vec::new();
    let Fx::Obj(ts_outputs) = decode(&rec["outputs"], &mut refs) else {
        return;
    };
    let g = &engine.graph;
    let mut cmp = Cmp {
        seen: HashSet::new(),
        temp: 0,
        summ: Summarizer::default(),
    };
    for (id, ts_ports) in ts_outputs.iter() {
        let Some(idx) = g.idx(id) else {
            rep.mismatched
                .entry(id.clone())
                .or_insert_with(|| "node missing in rust graph".into());
            continue;
        };
        let node = &g.nodes[idx];
        if is_media(&node.ty) {
            rep.skipped_media += usize::from(frame == 0);
            continue;
        }
        if node.unimplemented {
            continue;
        }
        let res = match (&outputs[idx], ts_ports) {
            (None, _) => Err("rust produced no output".to_owned()),
            (Some(rs), Fx::Arr(ts)) if rs.len() == ts.len() => {
                let mut r = Ok(());
                for (p, (rv, tv)) in rs.iter().zip(ts.iter()).enumerate() {
                    if let Err(e) = cmp.value(&Value::List(rv.clone()), tv, &format!("out[{p}]")) {
                        r = Err(e);
                        break;
                    }
                }
                r
            }
            (Some(rs), Fx::Arr(ts)) => {
                Err(format!("port count rust {} vs ts {}", rs.len(), ts.len()))
            }
            // A huge output is summarised as a whole (all ports together).
            (Some(rs), Fx::Tag(tag, want)) if tag == "$summary" => {
                let mut got = Stats {
                    objects: 1.0,
                    ..Default::default()
                };
                for port in rs.iter() {
                    got.add(&cmp.summ.visit(&Value::List(port.clone())));
                }
                check_summary(want, &got, "out")
            }
            (Some(_), other) => Err(format!("unexpected ts shape {}", short_fx(other))),
        };
        let ok = res.is_ok();
        let entry = node_ok.entry(id.clone()).or_insert(true);
        *entry &= ok;
        if let Err(e) = res {
            rep.mismatched
                .entry(format!("{id} ({})", node.ty))
                .or_insert_with(|| format!("frame {frame}: {e}"));
        }
    }
}

/// The exporter's `canon()` calls every function it encodes as
/// `fn(NEUTRAL_STYLE)` to detect style closures (`styleOf`). For a force
/// closure that is a call with `dt = undefined`: turbulence / wind advance
/// their private clock by NaN, so every frame after the first recorded one
/// sees a NaN clock (particles-attractor goes all-NaN at frame 1). Replay
/// that side effect: apply each force on the recorded outputs once with
/// `dt = NaN` to a buffer with no live particles.
fn probe_functions_like_exporter(outputs: &Outputs) {
    use floret_engine::particles::{ParticleBuffer, World};
    fn walk(v: &Value, seen: &mut HashSet<usize>, world: &World) {
        match v {
            Value::Handle(Handle::Force(f)) => {
                f.apply(&mut ParticleBuffer::new(1.0), f64::NAN, world);
            }
            Value::Handle(Handle::World(w)) if seen.insert(Arc::as_ptr(w) as usize) => {
                for x in w.forces.iter().chain(w.colliders.iter()) {
                    walk(x, seen, world);
                }
            }
            Value::List(items) if seen.insert(Arc::as_ptr(items) as *const () as usize) => {
                for x in items.iter() {
                    walk(x, seen, world);
                }
            }
            Value::Object(m) if seen.insert(Arc::as_ptr(m) as usize) => {
                for x in m.values() {
                    walk(x, seen, world);
                }
            }
            _ => {}
        }
    }
    let world = World::default_world();
    let mut seen = HashSet::new();
    for ports in outputs.iter().flatten() {
        for spread in ports.iter() {
            walk(&Value::List(spread.clone()), &mut seen, &world);
        }
    }
}

// ─── Test entry ──────────────────────────────────────────────────────────────

fn sample_ids() -> Vec<String> {
    let mut ids: Vec<String> = std::fs::read_dir(native_dir().join("samples"))
        .expect("samples dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".flora")
                .map(str::to_owned)
        })
        .collect();
    ids.sort();
    if let Ok(only) = std::env::var("PARITY_ONLY") {
        let want: HashSet<&str> = only.split(',').collect();
        ids.retain(|i| want.contains(i.as_str()));
    }
    ids
}

fn expected_passing() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity-passing.txt");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn samples_match_typescript_reference() {
    let verbose = std::env::var("PARITY_VERBOSE").is_ok();
    let expected = expected_passing();
    let mut passing = Vec::new();
    let mut regressions = Vec::new();
    let mut missing_types: BTreeMap<String, usize> = BTreeMap::new();
    let mut lines = Vec::new();

    for id in sample_ids() {
        let rep = run_sample(&id);
        for t in &rep.unimplemented {
            *missing_types.entry(t.clone()).or_default() += 1;
        }
        let status = if rep.passes() {
            passing.push(id.clone());
            "PASS".to_owned()
        } else if let Some(e) = &rep.compile_error {
            format!("COMPILE ERROR {e}")
        } else {
            format!(
                "{} matched, {} mismatched, {} unimplemented types{}",
                rep.matched,
                rep.mismatched.len(),
                rep.unimplemented.len(),
                if rep.compile_diffs.is_empty() {
                    String::new()
                } else {
                    format!(", compile: {:?}", rep.compile_diffs)
                }
            )
        };
        lines.push(format!("{id:32} {status}"));
        if verbose {
            for (node, why) in &rep.mismatched {
                lines.push(format!("    ✗ {node}: {why}"));
            }
        }
        if expected.contains(&id) && !rep.passes() {
            regressions.push(id.clone());
        }
    }

    println!(
        "\n── Parity report ({} / {} samples pass) ──",
        passing.len(),
        lines.len()
    );
    for l in &lines {
        println!("{l}");
    }
    if !missing_types.is_empty() {
        let mut by_count: Vec<_> = missing_types.into_iter().collect();
        by_count.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        println!("\nUnimplemented node types (samples blocked):");
        for (t, n) in by_count {
            println!("  {n:3}  {t}");
        }
    }
    let newly: Vec<_> = passing.iter().filter(|p| !expected.contains(p)).collect();
    if !newly.is_empty() {
        println!("\nNewly passing (add to tests/parity-passing.txt): {newly:?}");
    }
    assert!(regressions.is_empty(), "samples regressed: {regressions:?}");
}
