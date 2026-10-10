//! `geometry/curve` (Curve): classic parametric curves as points and a path
//! layer — Lissajous figures, roses, hypotrochoids and epitrochoids
//! (Spirograph), Archimedean and logarithmic spirals, stars and Gielis'
//! superformula.
//!
//! Every mode draws `count` points, centred on `center`, fitted to
//! `radius`, turned by `rotation`. `phase` (0..1) animates the curve
//! through one seamless cycle: Lissajous, hypo- and epitrochoids morph,
//! roses / stars / superformula shapes turn by one petal or point, spirals
//! turn once (they seem to flow). Closed curves run once round exactly (the
//! period is found from the ratios), their path is closed and the first
//! point isn't repeated; spirals are open and end on their outer point.
//! Hypo- and epitrochoids take `laps`: above 0 the pen runs round that
//! many times instead, as an open path (`count` even steps, the end not
//! repeated, as on closed curves).

use super::{int_pin, num_pin, pin_style, style_pin, vec2_pin, Pins, MAX_POINTS};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::scene::SceneNode;
use crate::spec::{Params, PortSpec};
use crate::value::{one, Spread, Value, Vec2};
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

#[cfg(test)]
const MODES: [&str; 8] = [
    "lissajous",
    "rose",
    "hypotrochoid",
    "epitrochoid",
    "spiral",
    "log-spiral",
    "star",
    "superformula",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Lissajous,
    Rose,
    Hypotrochoid,
    Epitrochoid,
    Spiral,
    LogSpiral,
    Star,
    Superformula,
}

impl Mode {
    fn parse(s: &str) -> Mode {
        match s {
            "rose" => Mode::Rose,
            "hypotrochoid" => Mode::Hypotrochoid,
            "epitrochoid" => Mode::Epitrochoid,
            "spiral" => Mode::Spiral,
            "log-spiral" => Mode::LogSpiral,
            "star" => Mode::Star,
            "superformula" => Mode::Superformula,
            _ => Mode::Lissajous,
        }
    }

    fn closed(self) -> bool {
        !matches!(self, Mode::Spiral | Mode::LogSpiral)
    }
}

fn mode_of(params: &Params) -> Mode {
    Mode::parse(super::param_str(params, "mode", "lissajous"))
}

/// The mode's own pins, after the shared ones.
fn mode_pins(mode: Mode) -> Vec<PortSpec> {
    match mode {
        Mode::Lissajous => vec![num_pin("a", 3.0), num_pin("b", 2.0)],
        Mode::Rose => vec![num_pin("n", 5.0), num_pin("d", 1.0)],
        Mode::Hypotrochoid => vec![
            num_pin("R", 5.0),
            num_pin("r", 3.0),
            num_pin("d", 5.0),
            num_pin("laps", 0.0),
        ],
        Mode::Epitrochoid => vec![
            num_pin("R", 3.0),
            num_pin("r", 1.0),
            num_pin("d", 0.5),
            num_pin("laps", 0.0),
        ],
        Mode::Spiral => vec![num_pin("turns", 3.0), num_pin("inner", 0.0)],
        Mode::LogSpiral => vec![num_pin("turns", 3.0), num_pin("inner", 4.0)],
        Mode::Star => vec![int_pin("points", 5.0), num_pin("inner", 0.5)],
        Mode::Superformula => vec![
            num_pin("m", 6.0),
            num_pin("n1", 1.0),
            num_pin("n2", 1.0),
            num_pin("n3", 1.0),
        ],
    }
}

pub(super) fn inputs(params: &Params) -> Vec<PortSpec> {
    let mut v = vec![
        int_pin("count", 400.0),
        vec2_pin("center", 0.0, 0.0),
        num_pin("radius", 150.0),
        num_pin("phase", 0.0),
        num_pin("rotation", 0.0),
    ];
    v.extend(mode_pins(mode_of(params)));
    v.push(style_pin());
    v
}

/// `q` when `x` is (within 1e-9) a fraction `p / q` with `q ≤ max`, by
/// continued fractions; `None` otherwise.
fn denominator(x: f64, max: u64) -> Option<u64> {
    if !x.is_finite() {
        return None;
    }
    let x = x.abs();
    let (mut h0, mut h1) = (0.0f64, 1.0f64);
    let (mut k0, mut k1) = (1.0f64, 0.0f64);
    let mut f = x;
    for _ in 0..32 {
        let a = f.floor();
        let (h2, k2) = (a * h1 + h0, a * k1 + k0);
        if k2 > max as f64 {
            return None;
        }
        if (h2 / k2 - x).abs() <= 1e-9 * x.max(1.0) {
            return Some(k2 as u64);
        }
        (h0, h1, k0, k1) = (h1, h2, k1, k2);
        let frac = f - a;
        if frac < 1e-12 {
            return None;
        }
        f = 1.0 / frac;
    }
    None
}

/// Most turns a closed curve takes to come round (irrational ratios stop
/// here).
const MAX_TURNS: u64 = 64;

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Turns of the parameter for a curve whose terms run at these
/// frequencies to close (the lcm of their denominators).
fn turns_for(freqs: &[f64]) -> f64 {
    let mut l = 1u64;
    for f in freqs {
        let q = denominator(*f, MAX_TURNS).unwrap_or(MAX_TURNS);
        l = (l / gcd(l, q) * q).min(MAX_TURNS);
    }
    l as f64
}

/// Finite pin value or `fallback`.
struct Args<'a> {
    pins: &'a Pins,
    inputs: &'a [Spread],
}

impl Args<'_> {
    fn n(&self, name: &str, fallback: f64) -> f64 {
        self.pins.num(self.inputs, name, fallback)
    }
}

/// Turns a hypo- / epitrochoid's pen runs round when `laps` is set (above
/// 0 and finite): the curve is then that open stretch, not one closed
/// period.
fn laps_of(mode: Mode, a: &Args) -> Option<f64> {
    let laps = match mode {
        Mode::Hypotrochoid | Mode::Epitrochoid => a.n("laps", 0.0),
        _ => 0.0,
    };
    (laps > 0.0 && laps.is_finite()).then_some(laps)
}

/// The curve's points before placing (unit-ish size; `fit` = the radius
/// they span, so scaling by `radius / fit` fits them), and whether the
/// path closes.
fn trace(mode: Mode, count: usize, phase: f64, a: &Args) -> (Vec<Vec2>, f64, bool) {
    let ph = TAU * phase;
    let laps = laps_of(mode, a);
    let closed = mode.closed() && laps.is_none();
    // Parameter s in [0, 1): closed curves (and set laps, which run like
    // them, just not round to the start) don't repeat the first point.
    let even = mode.closed();
    let s_of = |i: usize| {
        if even {
            i as f64 / count as f64
        } else if count > 1 {
            i as f64 / (count - 1) as f64
        } else {
            0.0
        }
    };
    let polar = |r: f64, t: f64| Vec2 {
        x: r * t.cos(),
        y: r * t.sin(),
    };
    let mut fit = 1.0;
    let pts: Vec<Vec2> = match mode {
        Mode::Lissajous => {
            let (fa, fb) = (a.n("a", 3.0), a.n("b", 2.0));
            let span = TAU * turns_for(&[fa, fb]);
            (0..count)
                .map(|i| {
                    let t = span * s_of(i);
                    // Phase 0 is the classic quarter-turn offset.
                    Vec2 {
                        x: (fa * t + ph + PI / 2.0).sin(),
                        y: (fb * t).sin(),
                    }
                })
                .collect()
        }
        Mode::Rose => {
            let (n, d) = (a.n("n", 5.0), a.n("d", 1.0));
            let k = if d != 0.0 { n / d } else { n };
            // A rose p/q closes after π·q when p·q is odd, else 2π·q.
            let q = denominator(k, MAX_TURNS);
            let span = match q {
                Some(q) => {
                    let p = (k.abs() * q as f64).round() as u64;
                    if p % 2 == 1 && q % 2 == 1 {
                        PI * q as f64
                    } else {
                        TAU * q as f64
                    }
                }
                None => TAU * MAX_TURNS as f64,
            };
            (0..count)
                .map(|i| {
                    let t = span * s_of(i);
                    polar((k * t + ph).cos(), t)
                })
                .collect()
        }
        Mode::Hypotrochoid | Mode::Epitrochoid => {
            let hypo = mode == Mode::Hypotrochoid;
            let big = a.n("R", if hypo { 5.0 } else { 3.0 });
            let r = a.n("r", if hypo { 3.0 } else { 1.0 });
            let d = a.n("d", if hypo { 5.0 } else { 0.5 });
            let r = if r == 0.0 { 1.0 } else { r };
            let arm = if hypo { big - r } else { big + r };
            let w = arm / r;
            let span = TAU * laps.unwrap_or_else(|| turns_for(&[big / r]));
            fit = (arm.abs() + d.abs()).max(1e-9);
            (0..count)
                .map(|i| {
                    let t = span * s_of(i);
                    let u = w * t + ph;
                    if hypo {
                        Vec2 {
                            x: arm * t.cos() + d * u.cos(),
                            y: arm * t.sin() - d * u.sin(),
                        }
                    } else {
                        Vec2 {
                            x: arm * t.cos() - d * u.cos(),
                            y: arm * t.sin() - d * u.sin(),
                        }
                    }
                })
                .collect()
        }
        Mode::Spiral | Mode::LogSpiral => {
            let turns = a.n("turns", 3.0);
            let radius = a.n("radius", 150.0).abs().max(1e-9);
            let log = mode == Mode::LogSpiral;
            let inner = a.n("inner", if log { 4.0 } else { 0.0 });
            // Radii in units of `radius` (placing scales them back).
            let r0 = (inner / radius).clamp(if log { 1e-6 } else { -1.0 }, 1.0);
            (0..count)
                .map(|i| {
                    let s = s_of(i);
                    let r = if log {
                        r0 * (1.0 / r0).powf(s)
                    } else {
                        r0 + (1.0 - r0) * s
                    };
                    polar(r, TAU * turns * s + ph)
                })
                .collect()
        }
        Mode::Star => {
            let n = a.n("points", 5.0).round().clamp(2.0, 1000.0);
            let inner = a.n("inner", 0.5);
            let corners = 2.0 * n;
            // Uniform along each edge; points start at the top (−y).
            let turn = ph / n - PI / 2.0;
            let corner = |k: f64| {
                let r = if k.rem_euclid(2.0) == 0.0 { 1.0 } else { inner };
                polar(r, TAU * k / corners + turn)
            };
            (0..count)
                .map(|i| {
                    let e = s_of(i) * corners;
                    let k = e.floor();
                    let f = e - k;
                    let (p, q) = (corner(k), corner(k + 1.0));
                    Vec2 {
                        x: p.x + (q.x - p.x) * f,
                        y: p.y + (q.y - p.y) * f,
                    }
                })
                .collect()
        }
        Mode::Superformula => {
            let m = a.n("m", 6.0);
            let (n1, n2, n3) = (a.n("n1", 1.0), a.n("n2", 1.0), a.n("n3", 1.0));
            // One turn of the phase = one step of the m-fold symmetry.
            let turn = if m != 0.0 { ph / m.abs() } else { ph };
            let raw: Vec<Vec2> = (0..count)
                .map(|i| {
                    let t = TAU * s_of(i);
                    let q = m * t / 4.0;
                    let v = q.cos().abs().powf(n2) + q.sin().abs().powf(n3);
                    let r = if v > 0.0 && n1 != 0.0 {
                        v.powf(-1.0 / n1)
                    } else {
                        0.0
                    };
                    let r = if r.is_finite() { r } else { 0.0 };
                    polar(r, t + turn)
                })
                .collect();
            fit = raw
                .iter()
                .map(|p| p.x.hypot(p.y))
                .fold(0.0, f64::max)
                .max(1e-9);
            raw
        }
    };
    (pts, fit, closed)
}

pub(super) fn curve(init: &NodeInit) -> Box<dyn NodeImpl> {
    let pins = Pins::new(init);
    let mode = mode_of(init.params);
    raw(move |inputs, cx| {
        let a = Args {
            pins: &pins,
            inputs,
        };
        let mut count = a.n("count", 400.0).round().max(0.0);
        if count > MAX_POINTS as f64 {
            cx.warn(format!("{count} points, over {MAX_POINTS}: capped."));
            count = MAX_POINTS as f64;
        }
        let count = count as usize;
        let center = pins.vec2(inputs, "center", Vec2::default());
        let radius = a.n("radius", 150.0);
        let rotation = a.n("rotation", 0.0);
        let (pts, fit, closed) = trace(mode, count, a.n("phase", 0.0), &a);
        let scale = radius / fit;
        let (sin, cos) = rotation.sin_cos();
        let pts: Vec<Vec2> = pts
            .into_iter()
            .map(|p| {
                let (x, y) = (p.x * scale, p.y * scale);
                Vec2 {
                    x: center.x + x * cos - y * sin,
                    y: center.y + x * sin + y * cos,
                }
            })
            .collect();
        // Direction of travel at each point (central differences, wrapping
        // round on closed curves).
        let n = pts.len();
        let tangents: Vec<Value> = (0..n)
            .map(|i| {
                let (p, q) = if closed {
                    (pts[(i + n - 1) % n], pts[(i + 1) % n])
                } else {
                    (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)])
                };
                Value::Num((q.y - p.y).atan2(q.x - p.x))
            })
            .collect();
        let style = super::as_line(&pin_style(&pins.value(inputs, "style"), false));
        let shape = Value::Scene(Arc::new(SceneNode::Polygon {
            points: Arc::from(pts.clone()),
            closed,
            style,
            tension: None,
        }));
        Ok(vec![
            one(shape),
            pts.into_iter().map(Value::Vec2).collect(),
            Spread::from(tangents),
        ])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::value::harness::{n, node};
    use serde_json::json;

    fn run(mode: &str, given: &[(&str, Value)]) -> (Vec<Vec2>, bool) {
        let p = json!({ "mode": mode });
        let params: Params = serde_json::from_value(p.clone()).unwrap();
        let ins: Vec<Vec<Value>> = inputs(&params)
            .iter()
            .map(|pin| {
                given
                    .iter()
                    .find(|(k, _)| *k == pin.name)
                    .map(|(_, v)| vec![v.clone()])
                    .unwrap_or_else(|| pin.default.clone().into_iter().collect())
            })
            .collect();
        let mut h = node("geometry/curve", p);
        let out = h.eval(ins).unwrap();
        let pts = out[1]
            .iter()
            .map(|v| match v {
                Value::Vec2(p) => *p,
                _ => panic!("point expected"),
            })
            .collect();
        let closed = matches!(&out[0][0], Value::Scene(s) if matches!(**s, SceneNode::Polygon { closed: true, .. }));
        (pts, closed)
    }

    fn max_r(pts: &[Vec2]) -> f64 {
        pts.iter().map(|p| p.x.hypot(p.y)).fold(0.0, f64::max)
    }

    #[test]
    fn every_mode_fits_its_radius_and_is_finite() {
        for m in MODES {
            let (pts, closed) = run(m, &[("count", n(500.0)), ("radius", n(100.0))]);
            assert_eq!(pts.len(), 500, "{m}");
            assert!(
                pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()),
                "{m}"
            );
            // Lissajous fills a square of half-side `radius`.
            let r = if m == "lissajous" {
                pts.iter()
                    .map(|p| p.x.abs().max(p.y.abs()))
                    .fold(0.0, f64::max)
            } else {
                max_r(&pts)
            };
            assert!(r <= 100.0 + 1e-6 && r > 60.0, "{m}: {r}");
            assert_eq!(closed, !m.contains("spiral"), "{m}");
        }
    }

    #[test]
    fn closed_curves_join_up() {
        // The point after the last is the first one again: step from the
        // last to the first is like any other step.
        for m in ["lissajous", "rose", "hypotrochoid", "epitrochoid", "star"] {
            let (pts, _) = run(m, &[("count", n(2000.0))]);
            let step = |a: Vec2, b: Vec2| (a.x - b.x).hypot(a.y - b.y);
            let longest = pts.windows(2).map(|w| step(w[0], w[1])).fold(0.0, f64::max);
            let close = step(pts[pts.len() - 1], pts[0]);
            assert!(close <= longest * 1.01 + 1e-9, "{m}: {close} vs {longest}");
        }
    }

    #[test]
    fn phase_cycles_seamlessly() {
        for m in MODES {
            let (a, _) = run(m, &[("count", n(300.0)), ("phase", n(0.0))]);
            let (b, _) = run(m, &[("count", n(300.0)), ("phase", n(1.0))]);
            let (c, _) = run(m, &[("count", n(300.0)), ("phase", n(0.37))]);
            // Same set of points (as a set, the start may move round).
            let d = |x: &[Vec2], y: &[Vec2]| {
                x.iter()
                    .map(|p| {
                        y.iter()
                            .map(|q| (p.x - q.x).hypot(p.y - q.y))
                            .fold(f64::MAX, f64::min)
                    })
                    .fold(0.0, f64::max)
            };
            assert!(d(&a, &b) < 2.0, "{m}: {}", d(&a, &b));
            assert!(d(&a, &c) > 1e-6, "{m}: phase does nothing");
        }
    }

    #[test]
    fn rose_and_star_shapes() {
        // A 5-petal rose passes through the centre; a star's corners sit
        // at the radius and its notches at inner × radius.
        let (pts, _) = run("rose", &[("count", n(1000.0)), ("radius", n(100.0))]);
        assert!(pts.iter().any(|p| p.x.hypot(p.y) < 1.0));
        let (pts, _) = run(
            "star",
            &[("count", n(10.0)), ("radius", n(100.0)), ("inner", n(0.4))],
        );
        let r: Vec<f64> = pts.iter().map(|p| p.x.hypot(p.y)).collect();
        for (i, r) in r.iter().enumerate() {
            let want = if i % 2 == 0 { 100.0 } else { 40.0 };
            assert!((r - want).abs() < 1e-9, "{i}: {r}");
        }
        // The first corner is at the top.
        assert!(pts[0].y < -99.0);
    }

    #[test]
    fn spirals_run_from_inner_to_radius() {
        let (pts, _) = run(
            "spiral",
            &[
                ("count", n(100.0)),
                ("radius", n(200.0)),
                ("inner", n(20.0)),
            ],
        );
        assert!((pts[0].x.hypot(pts[0].y) - 20.0).abs() < 1e-9);
        assert!((pts[99].x.hypot(pts[99].y) - 200.0).abs() < 1e-9);
        let (pts, _) = run(
            "log-spiral",
            &[("count", n(100.0)), ("radius", n(200.0)), ("inner", n(2.0))],
        );
        let r: Vec<f64> = pts.iter().map(|p| p.x.hypot(p.y)).collect();
        assert!((r[0] - 2.0).abs() < 1e-9 && (r[99] - 200.0).abs() < 1e-6);
        // Constant ratio between equally spaced points.
        assert!((r[50] / r[49] - r[20] / r[19]).abs() < 1e-9);
    }

    #[test]
    fn odd_inputs() {
        // Zero / negative counts: nothing; zero radius: a point.
        assert!(run("rose", &[("count", n(0.0))]).0.is_empty());
        assert!(run("rose", &[("count", n(-5.0))]).0.is_empty());
        let (pts, _) = run("lissajous", &[("radius", n(0.0))]);
        assert!(pts.iter().all(|p| p.x == 0.0 && p.y == 0.0));
        // One point on an open curve; irrational ratios still finish.
        assert_eq!(run("spiral", &[("count", n(1.0))]).0.len(), 1);
        let (pts, _) = run(
            "hypotrochoid",
            &[("R", n(std::f64::consts::E)), ("r", n(1.0))],
        );
        assert_eq!(pts.len(), 400);
        let (pts, _) = run("superformula", &[("n1", n(0.0)), ("m", n(f64::NAN))]);
        assert!(pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
        // Deterministic.
        assert_eq!(run("epitrochoid", &[]).0, run("epitrochoid", &[]).0);
    }

    #[test]
    fn laps_run_a_trochoid_open_for_that_many_turns() {
        for m in ["hypotrochoid", "epitrochoid"] {
            // Unset (0, the default), negative or NaN: one closed period.
            let whole = run(m, &[("count", n(300.0))]);
            for off in [0.0, -2.0, f64::NAN] {
                assert_eq!(
                    run(m, &[("count", n(300.0)), ("laps", n(off))]),
                    whole,
                    "{m}"
                );
            }
            assert!(whole.1);
            // Set: an open path, point i at i / count of the laps, fitted
            // to the radius as before.
            let (r, rr, d, laps) = (5.0, 3.0, 5.0, 2.5);
            let given = [
                ("count", n(100.0)),
                ("R", n(r)),
                ("r", n(rr)),
                ("d", n(d)),
                ("laps", n(laps)),
                ("radius", n(150.0)),
            ];
            let (pts, closed) = run(m, &given);
            assert!(!closed, "{m}");
            assert_eq!(pts.len(), 100);
            let hypo = m == "hypotrochoid";
            let arm: f64 = if hypo { r - rr } else { r + rr };
            let scale = 150.0 / (arm.abs() + d);
            for (i, p) in pts.iter().enumerate() {
                let t = TAU * laps * i as f64 / 100.0;
                let u = arm / rr * t;
                let (x, y) = if hypo {
                    (arm * t.cos() + d * u.cos(), arm * t.sin() - d * u.sin())
                } else {
                    (arm * t.cos() - d * u.cos(), arm * t.sin() - d * u.sin())
                };
                assert!((p.x - x * scale).abs() < 1e-9 && (p.y - y * scale).abs() < 1e-9);
            }
            // Laps past the period draw the same figure, still open.
            let (_, closed) = run(m, &[("laps", n(64.0))]);
            assert!(!closed);
        }
        // Other modes have no laps pin.
        let p: Params = serde_json::from_value(json!({ "mode": "rose" })).unwrap();
        assert!(inputs(&p).iter().all(|pin| pin.name != "laps"));
    }

    #[test]
    fn periods_from_ratios() {
        assert_eq!(denominator(1.5, 64), Some(2));
        assert_eq!(denominator(5.0, 64), Some(1));
        assert_eq!(denominator(1.0 / 3.0, 64), Some(3));
        assert_eq!(denominator(std::f64::consts::PI, 64), None);
        assert_eq!(turns_for(&[3.0, 2.0]), 1.0);
        assert_eq!(turns_for(&[1.5, 0.25]), 4.0);
        assert_eq!(turns_for(&[std::f64::consts::PI]), MAX_TURNS as f64);
    }

    #[test]
    fn static_spec_matches_default_mode() {
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("geometry/curve").unwrap();
        let st: Vec<String> = spec.inputs.iter().map(|p| p.name.clone()).collect();
        let dy: Vec<String> = inputs(&spec.default_params)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(st, dy);
        assert_eq!(spec.param_options["mode"], MODES);
    }
}
