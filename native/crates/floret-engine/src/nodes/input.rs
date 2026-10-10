//! `input/*` nodes. Ported from `src/nodes/input/`.
//!
//! `input/mouse` and `input/keyboard` read the evaluation frame. MIDI,
//! camera and landmarks read the app's devices (`crate::devices`); with
//! none installed they emit what the TS nodes did without browser media.

use super::shared::first;
use super::Registrar;
use crate::devices::{self, CameraRequest, LandmarkLayer, LandmarksRequest};
use crate::landmarks::Tracker;
use crate::node::{raw, CarriedState, EvalCtx, NodeImpl, NodeInit, NodeResult};
use crate::scene::{line_default_style, resolve_style, Paint, ResolvedStyle, SceneNode};
use crate::spec::{Params, PortSpec, PortType};
use crate::value::{one, Color, Spread, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("input/mouse", mouse);
    r.node("input/keyboard", keyboard);
    r.dynamic("input/keyboard", keyboard_inputs);
    r.node("input/midi-cc", midi_cc);
    r.node("input/midi-note", midi_note);
    r.node("input/camera", camera);
    r.node("input/landmarks", landmarks);
}

fn mouse(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let f = cx.frame;
        Ok(vec![
            one(Value::Num(f.mouse_x)),
            one(Value::Num(f.mouse_y)),
            one(Value::Bool(f.mouse_is_pressed)),
        ])
    })
}

// ─── input/keyboard ──────────────────────────────────────────────────────────

/// `input/keyboard`'s `mode`: what the node listens for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyMode {
    /// The last key pressed and whether any key is down (the original
    /// node; no pins).
    Last,
    /// Which of a list of keys are held (`keys`), for instruments and games.
    Keys,
    /// The text typed so far (`clear`, `max`), for type toys.
    Text,
}

impl KeyMode {
    fn of(params: &Params) -> KeyMode {
        match params.get("mode").and_then(|v| v.as_str()) {
            Some("keys") => KeyMode::Keys,
            Some("text") => KeyMode::Text,
            _ => KeyMode::Last,
        }
    }
}

/// The keys a new Keyboard node listens for in `keys` mode: a row of a
/// piano laid on the home row.
const DEFAULT_KEYS: &str = "awsedftgyhujk";
/// Text mode keeps at most this many characters unless `max` says.
const DEFAULT_MAX_TEXT: f64 = 64.0;

/// The pins for the node's `mode` (`DynamicInputsFn`).
fn keyboard_inputs(params: &Params) -> Vec<PortSpec> {
    match KeyMode::of(params) {
        KeyMode::Last => Vec::new(),
        KeyMode::Keys => {
            vec![PortSpec::new("keys", PortType::String).default(Value::str(DEFAULT_KEYS))]
        }
        KeyMode::Text => {
            let mut max =
                PortSpec::new("max", PortType::Number).default(Value::Num(DEFAULT_MAX_TEXT));
            max.integer = true;
            vec![
                PortSpec::new("clear", PortType::Boolean).default(Value::Bool(false)),
                max,
            ]
        }
    }
}

/// A key name as the preview reports it, for comparing: lowercase, and
/// `space` read as the space bar (`" "`).
fn key_name(name: &str) -> String {
    let n = name.to_lowercase();
    if n == "space" {
        " ".to_owned()
    } else {
        n
    }
}

/// The keys listed on the `keys` pin: one per character (`"asdf"`), or,
/// when the list has commas or spaces between names, one per name
/// (`"ArrowLeft, ArrowRight, space"`).
fn key_list(spec: &str) -> Vec<String> {
    let t = spec.trim();
    if t.chars().count() > 1 && t.contains([',', ' ']) {
        t.split([',', ' ', '\t', '\n'])
            .filter(|s| !s.is_empty())
            .map(key_name)
            .collect()
    } else {
        spec.chars().map(|c| key_name(&c.to_string())).collect()
    }
}

/// What a Keyboard node remembers between frames.
#[derive(Default)]
struct KeyState {
    /// Keys down last frame (names as reported).
    down: Vec<String>,
    /// Keys still held, oldest press first.
    order: Vec<String>,
    /// Text mode's text.
    text: String,
    pressed: bool,
    clear: bool,
}

/// The text a Keyboard node in text mode carries through an edit of the
/// patch.
struct CarriedText(String);

struct Keyboard {
    mode: KeyMode,
    st: KeyState,
}

fn keyboard(init: &NodeInit) -> Box<dyn NodeImpl> {
    Box::new(Keyboard {
        mode: KeyMode::of(init.params),
        st: KeyState::default(),
    })
}

impl NodeImpl for Keyboard {
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        let f = cx.frame;
        let st = &mut self.st;
        // Presses since last frame, in the order the preview lists them.
        let new: Vec<&String> = f
            .keys_down
            .iter()
            .filter(|k| !st.down.contains(k))
            .collect();
        st.order.retain(|k| f.keys_down.contains(k));
        st.order.extend(new.iter().map(|k| (*k).clone()));
        let any_new = !new.is_empty() || (f.key_is_pressed && !st.pressed);
        st.down.clone_from(&f.keys_down);
        st.pressed = f.key_is_pressed;

        let mut down = Vec::new();
        let mut hits = Vec::new();
        let mut index = -1.0;
        let hit;
        match self.mode {
            KeyMode::Last => hit = any_new,
            KeyMode::Keys => {
                let keys = first(inputs, 0);
                let list = key_list(&super::convert::js_string_or_empty(&keys));
                let held: Vec<String> = st.order.iter().map(|k| key_name(k)).collect();
                down = list.iter().map(|k| Value::Bool(held.contains(k))).collect();
                // The latest press still held that is one of ours.
                index = held
                    .iter()
                    .rev()
                    .find_map(|h| list.iter().position(|k| k == h))
                    .map_or(-1.0, |i| i as f64);
                let pressed: Vec<String> = new.iter().map(|k| key_name(k)).collect();
                hits = list
                    .iter()
                    .map(|k| Value::Bool(pressed.contains(k)))
                    .collect();
                hit = pressed.iter().any(|k| list.contains(k));
            }
            KeyMode::Text => {
                let clear = first(inputs, 0).truthy();
                let max = first(inputs, 1).to_number();
                let max = if max.is_finite() && max >= 1.0 {
                    max.min(100_000.0) as usize
                } else {
                    DEFAULT_MAX_TEXT as usize
                };
                for c in f.typed.chars() {
                    match c {
                        '\u{8}' => {
                            st.text.pop();
                        }
                        '\n' => {
                            if st.text.chars().count() < max {
                                st.text.push('\n');
                            }
                        }
                        c if c.is_control() => {}
                        c => {
                            if st.text.chars().count() < max {
                                st.text.push(c);
                            }
                        }
                    }
                }
                hit = !f.typed.is_empty();
                if clear && !st.clear {
                    st.text.clear();
                }
                st.clear = clear;
            }
        }
        Ok(vec![
            one(Value::Bool(f.key_is_pressed)),
            one(Value::str(&f.key)),
            Arc::from(down),
            num(index),
            one(Value::str(&st.text)),
            one(Value::Bool(hit)),
            Arc::from(hits),
        ])
    }

    fn memoise(&self) -> bool {
        false
    }

    fn carry(&mut self) -> Option<CarriedState> {
        (self.mode == KeyMode::Text)
            .then(|| Box::new(CarriedText(std::mem::take(&mut self.st.text))) as CarriedState)
    }

    fn adopt(&mut self, state: CarriedState) {
        if let (KeyMode::Text, Ok(t)) = (self.mode, state.downcast::<CarriedText>()) {
            self.st.text = t.0;
        }
    }
}

/// `params.param<string>(key, '')` for a device id.
fn param_str(cx: &EvalCtx, key: &str) -> String {
    match cx.param_value(key) {
        Value::Str(s) => s.to_string(),
        Value::Undefined | Value::Null => String::new(),
        other => js_string(&other),
    }
}

fn js_string(v: &Value) -> String {
    match v {
        Value::Num(n) => crate::js::num_to_string(*n),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// `clampChannel` / `clampCC`: finite numbers floored into `0..=max`,
/// anything else 0.
fn param_clamped(cx: &EvalCtx, key: &str, max: f64) -> u8 {
    match cx.param_value(key) {
        Value::Num(n) if n.is_finite() => n.floor().clamp(0.0, max) as u8,
        _ => 0,
    }
}

/// `param === true || param === 'true'`.
fn param_flag(cx: &EvalCtx, key: &str, default: bool) -> bool {
    match cx.param_value(key) {
        Value::Undefined => default,
        Value::Bool(b) => b,
        Value::Str(s) => &*s == "true",
        _ => false,
    }
}

fn num(n: f64) -> Spread {
    one(Value::Num(n))
}

/// `getCC` → `[value 0..1, raw 0..127]`, `[0, 0]` until a CC arrives.
fn midi_cc(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let device = param_str(cx, "device");
        let channel = param_clamped(cx, "channel", 16.0);
        let cc = param_clamped(cx, "cc", 127.0);
        let raw = devices::installed().and_then(|d| d.midi_cc(&device, channel, cc));
        let v = raw.map_or(0.0, f64::from);
        Ok(vec![num(v / 127.0), num(v)])
    })
}

/// `[note, velocity 0..1, pressed, freq]`; `[0, 0, pressed, 0]` with no
/// held note.
fn midi_note(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let device = param_str(cx, "device");
        let channel = param_clamped(cx, "channel", 16.0);
        let state = devices::installed()
            .map(|d| d.midi_note(&device, channel))
            .unwrap_or_default();
        let pressed = one(Value::Bool(state.pressed));
        Ok(match state.note {
            None => vec![num(0.0), num(0.0), pressed, num(0.0)],
            Some(note) => {
                let note = f64::from(note);
                vec![
                    num(note),
                    num(f64::from(state.velocity) / 127.0),
                    pressed,
                    num(440.0 * 2f64.powf((note - 69.0) / 12.0)),
                ]
            }
        })
    })
}

/// `[image, ready, width, height]` from the app's camera; an error (no
/// camera support, no device, permission denied) empties the outputs.
fn camera(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let start = inputs
            .first()
            .and_then(|s| s.first())
            .is_some_and(Value::truthy);
        let device = param_str(cx, "device");
        let mirror = param_flag(cx, "mirror", false);
        let dev = devices::installed().ok_or("camera capture isn't available")?;
        let req = CameraRequest {
            device: &device,
            start,
            mirror,
        };
        let f = dev.camera(cx.node_id, &req)?;
        let (w, h) = if f.ready {
            (f64::from(f.image.width()), f64::from(f.image.height()))
        } else {
            (0.0, 0.0)
        };
        Ok(vec![
            one(Value::Image(f.image)),
            one(Value::Bool(f.ready)),
            num(w),
            num(h),
        ])
    })
}

/// Fixed per-person palette (0..1), cycled by stable id: yellow, green,
/// blue, orange, magenta, cyan.
const PALETTE: [[f64; 3]; 6] = [
    [1.00, 0.84, 0.00],
    [0.24, 0.86, 0.35],
    [0.27, 0.51, 1.00],
    [1.00, 0.55, 0.16],
    [0.86, 0.31, 0.90],
    [0.16, 0.86, 0.90],
];

fn color_for_id(id: i64) -> Color {
    let [r, g, b] = PALETTE[id.rem_euclid(PALETTE.len() as i64) as usize];
    Color { r, g, b, a: 1.0 }
}

/// Hand / face / pose tracking. Outputs, in order: points, z, group,
/// color, connA, connB, connColor, guides, people, personColors,
/// detections, perGroup, detected.
fn landmarks(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let first = |i: usize| inputs.get(i).and_then(|s| s.first());
        let image = match first(0) {
            Some(Value::Image(img)) if img.is_ready() => Some(img.clone()),
            _ => None,
        };
        let start = first(1).is_some_and(Value::truthy);
        let size = match first(2) {
            None | Some(Value::Undefined | Value::Null) => 500.0,
            Some(v) => v.to_number(),
        };
        let size = if size.is_nan() || size == 0.0 {
            500.0
        } else {
            size
        };
        let guides_on = !matches!(first(3), Some(Value::Bool(false)));
        let style = match first(4) {
            Some(Value::Style(s)) => Some(s.clone()),
            _ => None,
        };

        let mode = param_str(cx, "tracker");
        let trackers = match mode.as_str() {
            "face" => vec![Tracker::Face],
            "pose" => vec![Tracker::Pose],
            "all" => vec![Tracker::Hands, Tracker::Face, Tracker::Pose],
            _ => vec![Tracker::Hands],
        };
        let mirror = param_flag(cx, "mirror", true);
        let count = {
            let n = cx.param_value("count");
            let n = if n.is_undefined() {
                2.0
            } else {
                n.to_number().floor()
            };
            if n.is_nan() || n == 0.0 {
                1.0
            } else {
                n.clamp(1.0, 6.0)
            }
        } as usize;
        let person = {
            let n = cx.param_value("person");
            let n = if n.is_undefined() {
                0.0
            } else {
                n.to_number().floor()
            };
            if n.is_nan() {
                0.0
            } else {
                n.max(0.0)
            }
        };
        let per_group = if trackers.len() > 1 {
            0
        } else {
            trackers[0].group()
        };

        let layers = match devices::installed() {
            Some(d) => {
                let req = LandmarksRequest {
                    image,
                    start,
                    trackers,
                    count,
                };
                d.landmarks(cx.node_id, &req)?.unwrap_or_default()
            }
            None => Vec::new(),
        };
        Ok(landmark_outputs(
            &layers,
            &LandmarkOptions {
                size,
                mirror,
                count,
                person,
                per_group,
                guides: guides_on,
                style: style.as_ref(),
            },
        ))
    })
}

struct LandmarkOptions<'a> {
    size: f64,
    mirror: bool,
    count: usize,
    person: f64,
    per_group: usize,
    guides: bool,
    style: Option<&'a crate::scene::Style>,
}

fn landmark_outputs(layers: &[LandmarkLayer], o: &LandmarkOptions) -> Vec<Spread> {
    let to_canvas = |p: &[f32; 3]| {
        let x = f64::from(p[0]);
        Vec2 {
            x: ((if o.mirror { 1.0 - x } else { x }) - 0.5) * o.size,
            y: (f64::from(p[1]) - 0.5) * o.size,
        }
    };
    let mut points: Vec<Vec2> = Vec::new();
    let mut zs = Vec::new();
    let mut group = Vec::new();
    let mut color: Vec<Color> = Vec::new();
    let mut conns: Vec<(Vec2, Vec2, Color)> = Vec::new();
    let mut people: Vec<Vec<Value>> = vec![Vec::new(); o.count];
    let mut detections = 0;
    for layer in layers {
        for (di, det) in layer.dets.iter().enumerate() {
            let id = layer.ids.get(di).map_or(di as i64, |&i| i64::from(i));
            if o.person > 0.0 && id as f64 != o.person - 1.0 {
                continue;
            }
            detections += 1;
            let col = color_for_id(id);
            let bucket = usize::try_from(id).ok().filter(|&i| i < o.count);
            for lm in det {
                let p = to_canvas(lm);
                points.push(p);
                zs.push(Value::Num(f64::from(lm[2]) * o.size));
                group.push(Value::Num(id as f64));
                color.push(col);
                if let Some(b) = bucket {
                    people[b].push(Value::Vec2(p));
                }
            }
            for &[a, b] in layer.tracker.connections() {
                if let (Some(a), Some(b)) = (det.get(usize::from(a)), det.get(usize::from(b))) {
                    conns.push((to_canvas(a), to_canvas(b), col));
                }
            }
        }
    }

    let mut guides = Vec::new();
    if o.guides {
        let fallback = Arc::new(ResolvedStyle {
            stroke_weight: 2.0,
            ..(**line_default_style()).clone()
        });
        let base = resolve_style(o.style, &fallback);
        let styled = |c: Color| {
            Arc::new(ResolvedStyle {
                stroke: Some(Paint::from_color(c)),
                ..(*base).clone()
            })
        };
        for &(a, b, c) in &conns {
            guides.push(Value::Scene(Arc::new(SceneNode::Line {
                x1: a.x,
                y1: a.y,
                x2: b.x,
                y2: b.y,
                style: styled(c),
            })));
        }
        for (p, &c) in points.iter().zip(&color) {
            guides.push(Value::Scene(Arc::new(SceneNode::Point {
                x: p.x,
                y: p.y,
                style: styled(c),
            })));
        }
    }

    let detected = !points.is_empty();
    let spread = |v: Vec<Value>| -> Spread { Arc::from(v) };
    vec![
        spread(points.into_iter().map(Value::Vec2).collect()),
        spread(zs),
        spread(group),
        spread(color.into_iter().map(Value::Color).collect()),
        spread(conns.iter().map(|c| Value::Vec2(c.0)).collect()),
        spread(conns.iter().map(|c| Value::Vec2(c.1)).collect()),
        spread(conns.iter().map(|c| Value::Color(c.2)).collect()),
        spread(guides),
        spread(people.into_iter().map(Value::list).collect()),
        spread(
            (0..o.count as i64)
                .map(|i| Value::Color(color_for_id(i)))
                .collect(),
        ),
        num(f64::from(detections)),
        num(o.per_group as f64),
        one(Value::Bool(detected)),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{node, num};
    use super::*;

    #[test]
    fn mouse_and_keyboard() {
        let mut m = node("input/mouse", serde_json::json!({}));
        m.frame.mouse_x = -12.0;
        m.frame.mouse_y = 30.0;
        m.frame.mouse_is_pressed = true;
        let out = m.eval1(&[]);
        assert_eq!(num(&out[0]), -12.0);
        assert_eq!(num(&out[1]), 30.0);
        assert!(matches!(out[2], Value::Bool(true)));

        let mut k = node("input/keyboard", serde_json::json!({}));
        k.frame.key = "a".into();
        let out = k.eval1(&[]);
        assert!(matches!(out[0], Value::Bool(false)));
        assert!(matches!(&out[1], Value::Str(s) if &**s == "a"));
    }

    fn press(k: &mut crate::nodes::value::harness::Harness, down: &[&str]) {
        k.frame.keys_down = down.iter().map(|s| (*s).to_owned()).collect();
        k.frame.key_is_pressed = !down.is_empty();
        if let Some(last) = down.last() {
            k.frame.key = (*last).to_owned();
        }
    }

    fn bools(v: &[Value]) -> Vec<bool> {
        v.iter().map(Value::truthy).collect()
    }

    #[test]
    fn keyboard_keys_mode_reports_held_keys_and_the_latest() {
        let mut k = node("input/keyboard", serde_json::json!({"mode": "keys"}));
        let keys = || vec![vec![Value::str("asdf")]];
        let out = k.eval(keys()).unwrap();
        assert_eq!(bools(&out[2]), [false; 4]);
        assert_eq!(num(&out[3][0]), -1.0);
        assert!(!out[5][0].truthy());
        // Press s, then d while s is held: d is the latest; each press hits once.
        press(&mut k, &["s"]);
        let out = k.eval(keys()).unwrap();
        assert_eq!(bools(&out[2]), [false, true, false, false]);
        assert_eq!(num(&out[3][0]), 1.0);
        assert!(out[5][0].truthy());
        let out = k.eval(keys()).unwrap();
        assert!(!out[5][0].truthy(), "a held key hits once");
        press(&mut k, &["d", "s"]);
        let out = k.eval(keys()).unwrap();
        assert_eq!(bools(&out[2]), [false, true, true, false]);
        assert_eq!(num(&out[3][0]), 2.0);
        assert!(out[5][0].truthy());
        // Let go of d: back to s. A key not in the list doesn't hit.
        press(&mut k, &["s", "q"]);
        let out = k.eval(keys()).unwrap();
        assert_eq!(num(&out[3][0]), 1.0);
        assert!(!out[5][0].truthy());
        // Named keys, split on commas / spaces; `space` is the space bar.
        let mut k = node("input/keyboard", serde_json::json!({"mode": "keys"}));
        press(&mut k, &["ArrowLeft", " "]);
        let out = k
            .eval(vec![vec![Value::str("ArrowLeft, arrowright space")]])
            .unwrap();
        assert_eq!(bools(&out[2]), [true, false, true]);
    }

    #[test]
    fn keyboard_hits_say_which_listed_key_went_down() {
        let mut k = node("input/keyboard", serde_json::json!({"mode": "keys"}));
        let keys = || vec![vec![Value::str("asdf")]];
        assert_eq!(bools(&k.eval(keys()).unwrap()[6]), [false; 4]);
        // s and f go down together: both hit, on that frame only.
        press(&mut k, &["s", "f"]);
        let out = k.eval(keys()).unwrap();
        assert_eq!(bools(&out[6]), [false, true, false, true]);
        assert_eq!(bools(&k.eval(keys()).unwrap()[6]), [false; 4]);
        // a joins while they're held: only a hits.
        press(&mut k, &["s", "f", "a"]);
        assert_eq!(
            bools(&k.eval(keys()).unwrap()[6]),
            [true, false, false, false]
        );
        // Released and pressed again: hits again; unlisted keys never do.
        press(&mut k, &["q"]);
        assert_eq!(bools(&k.eval(keys()).unwrap()[6]), [false; 4]);
        press(&mut k, &["q", "s"]);
        assert_eq!(
            bools(&k.eval(keys()).unwrap()[6]),
            [false, true, false, false]
        );
        // Other modes: no list, no hits.
        let mut k = node("input/keyboard", serde_json::json!({}));
        press(&mut k, &["s"]);
        assert!(k.eval(vec![]).unwrap()[6].is_empty());
    }

    #[test]
    fn keyboard_text_mode_edits_a_string() {
        let mut k = node("input/keyboard", serde_json::json!({"mode": "text"}));
        let run = |k: &mut crate::nodes::value::harness::Harness, typed: &str, clear: bool| {
            k.frame.typed = typed.to_owned();
            let out = k
                .eval(vec![vec![Value::Bool(clear)], vec![Value::Num(5.0)]])
                .unwrap();
            (out[4][0].clone(), out[5][0].truthy())
        };
        let (t, hit) = run(&mut k, "Hi!", false);
        assert!(matches!(&t, Value::Str(s) if &**s == "Hi!"));
        assert!(hit);
        let (_, hit) = run(&mut k, "", false);
        assert!(!hit);
        // Backspace deletes, Enter starts a line, control keys are ignored,
        // and typing stops at `max` characters.
        let (t, _) = run(&mut k, "\u{8}\u{8}\n\tabc", false);
        assert!(matches!(&t, Value::Str(s) if &**s == "H\nabc"), "{t:?}");
        let (t, _) = run(&mut k, "xyz", false);
        assert!(matches!(&t, Value::Str(s) if &**s == "H\nabc"));
        // A rising `clear` empties it (holding it doesn't keep clearing).
        let (t, _) = run(&mut k, "", true);
        assert!(matches!(&t, Value::Str(s) if s.is_empty()));
        let (t, _) = run(&mut k, "ok", true);
        assert!(matches!(&t, Value::Str(s) if &**s == "ok"));
        // The text survives an edit of the patch.
        let carried = k.carry().expect("text mode carries its text");
        let mut again = node("input/keyboard", serde_json::json!({"mode": "text"}));
        again.adopt(carried);
        let (t, _) = run(&mut again, "!", false);
        assert!(matches!(&t, Value::Str(s) if &**s == "ok!"));
    }

    #[test]
    fn keyboard_pins_follow_the_mode() {
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("input/keyboard").unwrap();
        let pins = |p: serde_json::Value| -> Vec<String> {
            let p: Params = serde_json::from_value(p).unwrap();
            cat.resolved_inputs(spec, &p)
                .into_iter()
                .map(|p| p.name)
                .collect()
        };
        assert!(pins(serde_json::json!({})).is_empty());
        assert_eq!(pins(serde_json::json!({"mode": "keys"})), ["keys"]);
        assert_eq!(pins(serde_json::json!({"mode": "text"})), ["clear", "max"]);
        // Last mode: `hit` on a new press.
        let mut k = node("input/keyboard", serde_json::json!({}));
        let out = k.eval1(&[]);
        assert!(!out[5].truthy());
        press(&mut k, &["x"]);
        let out = k.eval1(&[]);
        assert!(out[5].truthy());
        assert!(matches!(&out[1], Value::Str(s) if &**s == "x"));
    }

    // Unit tests run without a device provider (the fake-provider test is
    // `tests/devices.rs`, its own process, since the provider is global).

    #[test]
    fn midi_defaults() {
        let mut n = node("input/midi-note", serde_json::json!({}));
        let out = n.eval1(&[]);
        assert_eq!(out.len(), 4);
        assert!(matches!(out[2], Value::Bool(false)));
        let mut c = node("input/midi-cc", serde_json::json!({ "cc": 7 }));
        let out = c.eval1(&[]);
        assert_eq!((num(&out[0]), num(&out[1])), (0.0, 0.0));
    }

    #[test]
    fn camera_without_devices_errors_like_the_traces() {
        let mut n = node(
            "input/camera",
            serde_json::json!({ "device": "", "mirror": true }),
        );
        assert!(n.eval(vec![vec![Value::Bool(true)]]).is_err());
    }

    #[test]
    fn landmarks_without_devices_match_the_traces() {
        // body-tracking's bt_lm: [[]×8, [[], []], 2 colours, [0], [21], [false]].
        let mut n = node(
            "input/landmarks",
            serde_json::json!({ "tracker": "hands", "mirror": true, "count": 2, "person": 0 }),
        );
        let out = n
            .eval(vec![
                vec![],
                vec![Value::Bool(true)],
                vec![],
                vec![],
                vec![],
            ])
            .unwrap();
        assert_eq!(out.len(), 13);
        assert!(out[..8].iter().all(Vec::is_empty));
        assert!(
            matches!(&out[8][..], [Value::List(a), Value::List(b)] if a.is_empty() && b.is_empty())
        );
        assert!(matches!(out[9][1], Value::Color(c) if c == color_for_id(1)));
        assert_eq!(color_for_id(1).g, 0.86);
        assert_eq!((num(&out[10][0]), num(&out[11][0])), (0.0, 21.0));
        assert!(matches!(out[12][0], Value::Bool(false)));
        let mut all = node(
            "input/landmarks",
            serde_json::json!({ "tracker": "all", "count": "0" }),
        );
        let out = all.eval(vec![vec![]; 5]).unwrap();
        assert_eq!((out[8].len(), num(&out[11][0])), (1, 0.0));
    }

    #[test]
    fn landmark_layers_become_canvas_points_edges_and_people() {
        let layer = LandmarkLayer {
            tracker: Tracker::Hands,
            dets: vec![(0..21).map(|i| [i as f32 / 20.0, 0.25, 0.1]).collect()],
            ids: vec![1],
        };
        let opts = LandmarkOptions {
            size: 100.0,
            mirror: true,
            count: 2,
            person: 0.0,
            per_group: 21,
            guides: true,
            style: None,
        };
        let out = landmark_outputs(std::slice::from_ref(&layer), &opts);
        assert_eq!(out[0].len(), 21);
        // Mirrored: x = 0 → +50, x = 1 → -50; y = 0.25 → -25.
        assert!(matches!(out[0][0], Value::Vec2(Vec2 { x: 50.0, y: -25.0 })));
        assert!(matches!(
            out[0][20],
            Value::Vec2(Vec2 { x: -50.0, y: -25.0 })
        ));
        assert!((num(&out[1][0]) - 10.0).abs() < 1e-5);
        assert_eq!(num(&out[2][0]), 1.0);
        assert!(matches!(out[3][0], Value::Color(c) if c == color_for_id(1)));
        assert_eq!((out[4].len(), out[5].len(), out[6].len()), (21, 21, 21));
        // 21 edges + 21 dots, stroked in the person's colour, weight 2.
        assert_eq!(out[7].len(), 42);
        let Value::Scene(s) = &out[7][0] else {
            panic!()
        };
        let SceneNode::Line { style, .. } = &**s else {
            panic!()
        };
        assert_eq!(style.stroke_weight, 2.0);
        assert_eq!(&*style.stroke.as_ref().unwrap().css, "rgb(61,219,89)");
        // Person id 1 fills slot 1 of `people`.
        assert!(
            matches!(&out[8][..], [Value::List(a), Value::List(b)] if a.is_empty() && b.len() == 21)
        );
        assert_eq!(num(&out[10][0]), 1.0);
        assert!(matches!(out[12][0], Value::Bool(true)));

        // `person` filters to one id; `guides: false` drops the overlay.
        let only_first = LandmarkOptions {
            person: 1.0,
            guides: false,
            ..opts
        };
        let out = landmark_outputs(&[layer], &only_first);
        assert!(out[0].is_empty() && out[7].is_empty());
        assert_eq!(num(&out[10][0]), 0.0);
    }
}
