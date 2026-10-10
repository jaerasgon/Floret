use super::*;
use crate::nodes::value::harness::{n, node, Harness};
use crate::value::Vec2;
use serde_json::json;

fn b(x: bool) -> Value {
    Value::Bool(x)
}

fn v2(x: f64, y: f64) -> Value {
    Value::Vec2(Vec2 { x, y })
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// Inputs by name in the resolved pin order; others take their defaults.
fn ins(f: DynamicFn, params: &serde_json::Value, given: &[(&str, Vec<Value>)]) -> Vec<Vec<Value>> {
    let p: Params = serde_json::from_value(params.clone()).unwrap();
    f(&p)
        .iter()
        .map(|pin| {
            given
                .iter()
                .find(|(k, _)| *k == pin.name)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| pin.default.clone().into_iter().collect())
        })
        .collect()
}

type DynamicFn = fn(&Params) -> Vec<PortSpec>;

fn step(
    h: &mut Harness,
    f: DynamicFn,
    params: &serde_json::Value,
    dt: f64,
    given: &[(&str, Vec<Value>)],
) -> Vec<Vec<Value>> {
    h.frame.dt = dt;
    h.frame.t += dt;
    h.eval(ins(f, params, given)).unwrap()
}

fn nums_of(v: &[Value]) -> Vec<f64> {
    v.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()
}

// ─── Integrate ───────────────────────────────────────────────────────────────

#[test]
fn integrate_accumulates_rate_times_dt() {
    let p = json!({});
    let mut h = node("motion/integrate", p.clone());
    let given = [("rate", vec![n(2.0)]), ("start", vec![n(1.0)])];
    let out = step(&mut h, integrate_inputs, &p, 0.0, &given);
    assert_eq!(nums_of(&out[0]), [1.0]);
    let out = step(&mut h, integrate_inputs, &p, 0.05, &given);
    assert!(near(out[0][0].as_f64().unwrap(), 1.1));
    // A long stall counts as `maxDt` (0.1).
    let out = step(&mut h, integrate_inputs, &p, 5.0, &given);
    assert!(near(out[0][0].as_f64().unwrap(), 1.3));
    // Paused: holds.
    let paused = [
        ("rate", vec![n(2.0)]),
        ("start", vec![n(1.0)]),
        ("run", vec![b(false)]),
    ];
    let out = step(&mut h, integrate_inputs, &p, 0.1, &paused);
    assert!(near(out[0][0].as_f64().unwrap(), 1.3));
    // Reset (level): back to start while held.
    let reset = [
        ("rate", vec![n(2.0)]),
        ("start", vec![n(1.0)]),
        ("reset", vec![b(true)]),
    ];
    let out = step(&mut h, integrate_inputs, &p, 0.1, &reset);
    assert_eq!(nums_of(&out[0]), [1.0]);
}

#[test]
fn integrate_per_element_and_vec2() {
    let p = json!({});
    let mut h = node("motion/integrate", p.clone());
    let given = [
        ("rate", vec![v2(10.0, -10.0), v2(0.0, 5.0)]),
        ("start", vec![v2(0.0, 0.0)]),
    ];
    step(&mut h, integrate_inputs, &p, 0.0, &given);
    let out = step(&mut h, integrate_inputs, &p, 0.1, &given);
    assert_eq!(out[0].len(), 2);
    let Value::Vec2(a) = out[0][0] else {
        panic!("vec2 expected")
    };
    let Value::Vec2(c) = out[0][1] else {
        panic!("vec2 expected")
    };
    assert!(near(a.x, 1.0) && near(a.y, -1.0));
    assert!(near(c.x, 0.0) && near(c.y, 0.5));
    // Growing the spread starts the new elements at `start`; shrinking
    // keeps the first ones.
    let three = [
        ("rate", vec![n(1.0), n(1.0), n(1.0)]),
        ("start", vec![n(7.0)]),
    ];
    let p = json!({"valueType": "number"});
    let mut h = node("motion/integrate", p.clone());
    step(&mut h, integrate_inputs, &p, 0.0, &[("rate", vec![n(1.0)])]);
    step(&mut h, integrate_inputs, &p, 0.1, &[("rate", vec![n(1.0)])]);
    let out = step(&mut h, integrate_inputs, &p, 0.0, &three);
    assert_eq!(nums_of(&out[0])[1..], [7.0, 7.0]);
    assert!(near(out[0][0].as_f64().unwrap(), 0.1));
    // An empty spread empties the output.
    let out = step(&mut h, integrate_inputs, &p, 0.1, &[("rate", vec![])]);
    assert!(out[0].is_empty());
}

#[test]
fn integrate_limits() {
    let run = |limit: &str, rate: f64, dts: &[f64]| -> (Vec<f64>, Vec<bool>) {
        let p = json!({ "limit": limit });
        let mut h = node("motion/integrate", p.clone());
        let given = [
            ("rate", vec![n(rate)]),
            ("min", vec![n(0.0)]),
            ("max", vec![n(1.0)]),
            ("limit", vec![Value::str(limit)]),
        ];
        let (mut vs, mut hs) = (vec![], vec![]);
        for dt in dts {
            let out = step(&mut h, integrate_inputs, &p, *dt, &given);
            vs.push(out[0][0].as_f64().unwrap());
            hs.push(out[1][0].truthy());
        }
        (vs, hs)
    };
    let (v, hit) = run("wrap", 4.0, &[0.0, 0.1, 0.1, 0.1]);
    assert!(near(v[3], 0.2), "{v:?}");
    assert_eq!(hit, [false, false, false, true]);
    let (v, hit) = run("clamp", 4.0, &[0.0, 0.1, 0.1, 0.1, 0.1]);
    assert_eq!(v[3], 1.0);
    assert_eq!(v[4], 1.0);
    assert_eq!(hit, [false, false, false, true, false]);
    // Bounce runs back from the end.
    let (v, hit) = run("bounce", 4.0, &[0.0, 0.1, 0.1, 0.1, 0.1, 0.1]);
    assert!(
        near(v[3], 0.8) && near(v[4], 0.4) && near(v[5], 0.0),
        "{v:?}"
    );
    assert!(hit[3] && !hit[4]);
    // None leaves it alone.
    let (v, _) = run("none", 4.0, &[0.0, 0.1, 0.1, 0.1]);
    assert!(near(v[3], 1.2));
    // A zero-width range pins the value; NaN rates count as 0.
    let mut l = 5.0;
    let mut d = 1.0;
    assert!(limit(&mut l, &mut d, 2.0, 2.0, Limit::Wrap, 1.0));
    assert_eq!(l, 2.0);
    let p = json!({});
    let mut h = node("motion/integrate", p.clone());
    step(
        &mut h,
        integrate_inputs,
        &p,
        0.0,
        &[("rate", vec![n(f64::NAN)])],
    );
    let out = step(
        &mut h,
        integrate_inputs,
        &p,
        0.1,
        &[("rate", vec![n(f64::NAN)])],
    );
    assert_eq!(nums_of(&out[0]), [0.0]);
}

#[test]
fn integrate_survives_an_edit() {
    let p = json!({});
    let mut h = node("motion/integrate", p.clone());
    let given = [("rate", vec![n(1.0)])];
    step(&mut h, integrate_inputs, &p, 0.0, &given);
    step(&mut h, integrate_inputs, &p, 0.1, &given);
    let mut h2 = node("motion/integrate", p.clone());
    h2.adopt(h.carry().unwrap());
    let out = step(&mut h2, integrate_inputs, &p, 0.1, &given);
    assert!(near(out[0][0].as_f64().unwrap(), 0.2));
}

#[test]
fn integrate_pins_follow_limit_and_value_type() {
    let names = |p: serde_json::Value| -> Vec<String> {
        integrate_inputs(&serde_json::from_value(p).unwrap())
            .into_iter()
            .map(|p| p.name)
            .collect()
    };
    assert_eq!(
        names(json!({})),
        ["rate", "start", "run", "reset", "limit", "maxDt"]
    );
    assert_eq!(
        names(json!({"limit": "wrap"})),
        ["rate", "start", "run", "reset", "limit", "min", "max", "maxDt"]
    );
    let p: Params = serde_json::from_value(json!({"valueType": "vec2"})).unwrap();
    assert_eq!(integrate_inputs(&p)[0].ty, PortType::Vec2);
    let spec = crate::spec::Catalogue::builtin()
        .get("motion/integrate")
        .unwrap();
    let st: Vec<&str> = spec.inputs.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(st, ["rate", "start", "run", "reset", "limit", "maxDt"]);
}

// ─── Envelope ────────────────────────────────────────────────────────────────

fn tm(attack: f64, hold: f64, time: f64, frequency: f64) -> Timing {
    Timing {
        attack,
        hold,
        time,
        frequency,
    }
}

#[test]
fn envelope_shapes() {
    let t = tm(0.0, 0.0, 1.0, 0.0);
    let (v, a) = shape_at(Shape::Decay, 0.0, t);
    assert_eq!((v, a), (1.0, true));
    assert!(near(shape_at(Shape::Decay, 1.0, t).0, 0.01));
    assert!(!shape_at(Shape::Decay, 1.0, t).1);
    // Before the event, or NaN: at rest.
    assert_eq!(shape_at(Shape::Decay, -1.0, t), (0.0, false));
    assert_eq!(shape_at(Shape::Spring, f64::NAN, t), (0.0, false));
    // Never triggered (infinite age): decays to 0, a spring has settled.
    assert_eq!(shape_at(Shape::Decay, f64::INFINITY, t).0, 0.0);
    assert_eq!(shape_at(Shape::Spring, f64::INFINITY, t).0, 1.0);
    assert_eq!(shape_at(Shape::Boing, f64::INFINITY, t).0, 0.0);
    // Attack, hold, decay.
    let ad = tm(0.1, 0.2, 1.0, 0.0);
    assert_eq!(shape_at(Shape::AttackDecay, 0.0, ad).0, 0.0);
    assert!(shape_at(Shape::AttackDecay, 0.05, ad).0 > 0.5);
    assert_eq!(shape_at(Shape::AttackDecay, 0.2, ad).0, 1.0);
    assert!(near(shape_at(Shape::AttackDecay, 1.3, ad).0, 0.01));
    // Linear fade.
    assert!(near(shape_at(Shape::Fade, 0.05, ad).0, 0.5));
    assert!(near(shape_at(Shape::Fade, 0.8, ad).0, 0.5));
    assert_eq!(shape_at(Shape::Fade, 2.0, ad), (0.0, false));
    // Boing starts at 0 and swings both ways; spring overshoots 1.
    let wob = tm(0.0, 0.0, 2.0, 3.0);
    assert_eq!(shape_at(Shape::Boing, 0.0, wob).0, 0.0);
    let vs: Vec<f64> = (0..200)
        .map(|i| shape_at(Shape::Boing, f64::from(i) * 0.005, wob).0)
        .collect();
    assert!(vs.iter().any(|v| *v > 0.3) && vs.iter().any(|v| *v < -0.3));
    let sp: Vec<f64> = (0..400)
        .map(|i| shape_at(Shape::Spring, f64::from(i) * 0.005, wob).0)
        .collect();
    assert_eq!(sp[0], 0.0);
    assert!(sp.iter().any(|v| *v > 1.05));
    assert!((sp[399] - 1.0).abs() < 0.05);
    // Zero times don't divide by zero.
    let z = tm(0.0, 0.0, 0.0, 0.0);
    for s in [
        Shape::Decay,
        Shape::AttackDecay,
        Shape::Fade,
        Shape::Boing,
        Shape::Spring,
    ] {
        for age in [0.0, 0.5] {
            let (v, _) = shape_at(s, age, z);
            assert!(v.is_finite(), "{s:?} at {age}: {v}");
        }
    }
}

#[test]
fn envelope_from_ages_per_element() {
    let p = json!({"shape": "fade"});
    let mut h = node("motion/envelope", p.clone());
    let out = step(
        &mut h,
        envelope_inputs,
        &p,
        0.0,
        &[
            ("age", vec![n(0.0), n(0.5), n(-1.0), n(f64::NAN)]),
            ("time", vec![n(1.0)]),
        ],
    );
    let v = nums_of(&out[0]);
    assert!(near(v[0], 1.0) && near(v[1], 0.5) && v[2] == 0.0 && v[3] == 0.0);
    let active: Vec<bool> = out[2].iter().map(Value::truthy).collect();
    assert_eq!(active, [true, true, false, false]);
    // Empty ages: nothing.
    let out = step(&mut h, envelope_inputs, &p, 0.0, &[("age", vec![])]);
    assert!(out[0].is_empty());
}

#[test]
fn envelope_times_its_own_trigger() {
    let p = json!({"shape": "decay", "from": "trigger"});
    let mut h = node("motion/envelope", p.clone());
    let fire = |on: &[bool]| -> Vec<(&str, Vec<Value>)> {
        vec![
            ("trigger", on.iter().map(|x| b(*x)).collect()),
            ("time", vec![n(1.0)]),
        ]
    };
    // Not triggered yet: rests at 0, infinite age.
    let out = step(&mut h, envelope_inputs, &p, 0.1, &fire(&[false, false]));
    assert_eq!(nums_of(&out[0]), [0.0, 0.0]);
    assert!(out[1][0].as_f64().unwrap().is_infinite());
    let out = step(&mut h, envelope_inputs, &p, 0.1, &fire(&[true, false]));
    assert_eq!(nums_of(&out[0]), [1.0, 0.0]);
    // Held: no restart; the age runs on.
    let out = step(&mut h, envelope_inputs, &p, 0.5, &fire(&[true, true]));
    assert!(near(out[1][0].as_f64().unwrap(), 0.5));
    assert_eq!(out[0][1].as_f64(), Some(1.0));
    // Released and pressed again: restarts.
    step(&mut h, envelope_inputs, &p, 0.1, &fire(&[false, false]));
    let out = step(&mut h, envelope_inputs, &p, 0.1, &fire(&[true, false]));
    assert_eq!(out[1][0].as_f64(), Some(0.0));
    // The trigger times survive an edit.
    let mut h2 = node("motion/envelope", p.clone());
    h2.frame.t = h.frame.t;
    h2.adopt(h.carry().unwrap());
    let out = step(&mut h2, envelope_inputs, &p, 0.25, &fire(&[true, false]));
    assert!(near(out[1][0].as_f64().unwrap(), 0.25));
}

#[test]
fn envelope_pins_follow_shape_and_source() {
    let names = |p: serde_json::Value| -> Vec<String> {
        envelope_inputs(&serde_json::from_value(p).unwrap())
            .into_iter()
            .map(|p| p.name)
            .collect()
    };
    assert_eq!(names(json!({})), ["from", "age", "time"]);
    assert_eq!(
        names(json!({"shape": "attack-decay", "from": "trigger"})),
        ["from", "trigger", "attack", "hold", "time"]
    );
    assert_eq!(
        names(json!({"shape": "boing"})),
        ["from", "age", "frequency", "time"]
    );
    let spec = crate::spec::Catalogue::builtin()
        .get("motion/envelope")
        .unwrap();
    let st: Vec<&str> = spec.inputs.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(st, ["from", "age", "time"]);
}

// ─── Beat ────────────────────────────────────────────────────────────────────

/// (step, phase, bar, gate, trigger, count)
fn beat_out(out: &[Vec<Value>]) -> (f64, f64, f64, bool, bool, f64) {
    let f = |i: usize| out[i][0].as_f64().unwrap();
    (
        f(0),
        f(1),
        f(2),
        out[3][0].truthy(),
        out[4][0].truthy(),
        f(5),
    )
}

#[test]
fn beat_counts_steps_and_bars() {
    // 120 bpm, 4 beats a bar, 16 steps: 8 steps a second.
    let p = json!({});
    let mut h = node("time/beat", p.clone());
    let none: [(&str, Vec<Value>); 0] = [];
    let out = beat_out(&step(&mut h, beat_inputs, &p, 0.0, &none));
    assert_eq!(out, (0.0, 0.0, 0.0, true, true, 0.0));
    let out = beat_out(&step(&mut h, beat_inputs, &p, 0.0625, &none));
    assert_eq!(out.0, 0.0);
    assert!(near(out.1, 0.5));
    assert!(!out.3 && !out.4);
    let out = beat_out(&step(&mut h, beat_inputs, &p, 0.0625, &none));
    assert_eq!((out.0, out.4), (1.0, true));
    // Two seconds on: bar 1, step 1.
    let out = beat_out(&step(&mut h, beat_inputs, &p, 2.0, &none));
    assert_eq!((out.0, out.2, out.5), (1.0, 1.0, 17.0));
    // Stopped: holds, no trigger.
    let stop = [("run", vec![b(false)])];
    let out = beat_out(&step(&mut h, beat_inputs, &p, 0.5, &stop));
    assert_eq!((out.5, out.4), (17.0, false));
    // Reset: back to 0 with a trigger.
    let reset = [("reset", vec![b(true)])];
    let out = beat_out(&step(&mut h, beat_inputs, &p, 0.5, &reset));
    assert_eq!((out.5, out.4), (0.0, true));
}

#[test]
fn beat_steps_land_on_the_frames_they_are_due() {
    // 90 bpm, 4 beats, 16 steps: 6 steps a second, one every 10 frames at
    // 60 fps — summed frame times must not make some a frame late.
    let p = json!({});
    let mut h = node("time/beat", p.clone());
    h.frame.t = -1.0 / 60.0;
    let given = [("bpm", vec![n(90.0)])];
    for f in 0..3000u32 {
        let out = beat_out(&step(&mut h, beat_inputs, &p, 1.0 / 60.0, &given));
        let due = f64::from(f / 10);
        assert_eq!(out.5, due, "frame {f}");
        assert_eq!(out.4, f % 10 == 0, "trigger on frame {f}");
    }
}

#[test]
fn beat_swing_delays_odd_steps() {
    assert_eq!(swung(0.5, 0.0), (0.0, 0.5));
    assert_eq!(swung(1.2, 0.0).0, 1.0);
    // Swing 1: the first of each pair runs 1.5 steps.
    assert_eq!(swung(1.2, 1.0).0, 0.0);
    let (k, ph) = swung(1.75, 1.0);
    assert_eq!(k, 1.0);
    assert!(near(ph, 0.5));
    assert_eq!(swung(2.1, 1.0).0, 2.0);
}

#[test]
fn beat_locks_to_the_loop() {
    let p = json!({"sync": "loop"});
    let mut h = node("time/beat", p.clone());
    // A 4.1 s loop at 120 bpm (2 s bars) fits 2 bars: 32 steps per loop.
    h.frame.loop_length = 4.1;
    let given = [("sync", vec![Value::str("loop")])];
    h.frame.t = 4.1 * 0.5 - 0.001;
    let out = beat_out(&h.eval(ins(beat_inputs, &p, &given)).unwrap());
    assert_eq!(out.5, 15.0);
    h.frame.t = 4.1;
    let out = beat_out(&h.eval(ins(beat_inputs, &p, &given)).unwrap());
    assert_eq!((out.5, out.4), (0.0, true));
}

#[test]
fn beat_survives_an_edit_and_odd_inputs() {
    let p = json!({});
    let mut h = node("time/beat", p.clone());
    let none: [(&str, Vec<Value>); 0] = [];
    step(&mut h, beat_inputs, &p, 0.0, &none);
    step(&mut h, beat_inputs, &p, 0.3, &none);
    let mut h2 = node("time/beat", p.clone());
    h2.adopt(h.carry().unwrap());
    let out = beat_out(&step(&mut h2, beat_inputs, &p, 0.0, &none));
    // Same step, and no extra trigger.
    assert_eq!((out.5, out.4), (2.0, false));
    // bpm 0 / NaN and zero steps stay finite.
    let odd = [("bpm", vec![n(0.0)]), ("steps", vec![n(0.0)])];
    let out = beat_out(&step(&mut h2, beat_inputs, &p, 0.1, &odd));
    assert!(out.0.is_finite() && out.1.is_finite());
    let odd = [("bpm", vec![n(f64::NAN)])];
    let out = beat_out(&step(&mut h2, beat_inputs, &p, 0.1, &odd));
    assert!(out.5.is_finite());
}
