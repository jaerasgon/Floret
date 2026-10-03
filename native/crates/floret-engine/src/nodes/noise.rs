//! `noise/*` nodes. Ported from `src/nodes/random/noise.ts`.
//!
//! Hash-gradient 2D Perlin noise (no permutation table: gradients come from
//! `hash32(ix, iy, seed) & 7`), fBm over it, and curl noise as central
//! differences of the fBm potential.
//!
//! `noise/noise` (native-only) is the one Noise node that supersedes both:
//! Perlin, simplex, value, cellular (Worley F1 / F2 / F2−F1) and curl
//! modes, fBm octaves, domain warp, evaluated per element over spreads.
//! Its Perlin and curl modes reuse the functions above, so at default
//! settings they give exactly what `noise/perlin` / `noise/curl` give.

use super::random::{hash32, hash_unit};
use super::shared::{cyclic, cyclic_len, to_vec2};
use super::Registrar;
use crate::js;
use crate::node::{raw, scalar, NodeImpl, NodeInit};
use crate::spec::{Params, PortSpec, PortType};
use crate::value::{Spread, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("noise/perlin", perlin);
    r.node("noise/curl", curl);
    r.node("noise/noise", noise);
    r.dynamic("noise/noise", noise_inputs);
}

const GRADIENTS_2D: [(f64, f64); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (1.0, 1.0),
    (-1.0, 1.0),
    (1.0, -1.0),
    (-1.0, -1.0),
];

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn grad_dot(ix: f64, iy: f64, fx: f64, fy: f64, seed: f64) -> f64 {
    let g = GRADIENTS_2D[(hash32(ix, iy, seed) & 7) as usize];
    g.0 * fx + g.1 * fy
}

/// `perlin2D`: in [0, 1].
pub(crate) fn perlin_2d(x: f64, y: f64, seed: f64) -> f64 {
    let ix = x.floor();
    let iy = y.floor();
    let fx = x - ix;
    let fy = y - iy;
    let u = fade(fx);
    let v = fade(fy);
    let n00 = grad_dot(ix, iy, fx, fy, seed);
    let n10 = grad_dot(ix + 1.0, iy, fx - 1.0, fy, seed);
    let n01 = grad_dot(ix, iy + 1.0, fx, fy - 1.0, seed);
    let n11 = grad_dot(ix + 1.0, iy + 1.0, fx - 1.0, fy - 1.0, seed);
    let nx0 = lerp(n00, n10, u);
    let nx1 = lerp(n01, n11, u);
    lerp(nx0, nx1, v) * std::f64::consts::FRAC_1_SQRT_2 + 0.5
}

/// Octave cap. TS loops `octaves` times (forever for `Infinity`); past
/// ~1024 octaves the frequency overflows and every sample is NaN, so the
/// cap only matters for absurd inputs.
const MAX_OCTAVES: f64 = 4096.0;

/// `fbm2D`: `octaves` of perlin at doubling frequency, normalised.
pub(crate) fn fbm_2d(x: f64, y: f64, seed: f64, octaves: f64, falloff: f64) -> f64 {
    fbm(perlin_2d, x, y, seed, octaves, falloff)
}

/// `octaves` of `base` at doubling frequency (seed + 1 per octave),
/// normalised by the summed amplitudes.
fn fbm(base: Base, x: f64, y: f64, seed: f64, octaves: f64, falloff: f64) -> f64 {
    let oct = js::max(1.0, octaves.floor());
    if oct == 1.0 {
        return base(x, y, seed);
    }
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut total = 0.0;
    let mut freq = 1.0;
    let mut i = 0.0;
    // `i < NaN` is false: NaN octaves run zero iterations.
    while i < oct && i < MAX_OCTAVES {
        sum += base(x * freq, y * freq, seed + i) * amp;
        total += amp;
        amp *= falloff;
        freq *= 2.0;
        i += 1.0;
    }
    if total > 0.0 {
        sum / total
    } else {
        0.0
    }
}

fn perlin(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        let n: Vec<f64> = a.iter().map(Value::to_number).collect();
        let seed = crate::seed::mix(n[2]);
        Ok(vec![Value::Num(fbm_2d(n[0], n[1], seed, n[3], n[4]))])
    })
}

fn curl(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        let p = to_vec2(&a[0]);
        let s = a[1].to_number();
        let sv = crate::seed::mix(a[2].to_number());
        let (oct, fa) = (a[3].to_number(), a[4].to_number());
        let x = p.x * s;
        let y = p.y * s;
        let eps = 1e-3;
        let dydt =
            (fbm_2d(x, y + eps, sv, oct, fa) - fbm_2d(x, y - eps, sv, oct, fa)) / (2.0 * eps);
        let dxdt =
            (fbm_2d(x + eps, y, sv, oct, fa) - fbm_2d(x - eps, y, sv, oct, fa)) / (2.0 * eps);
        Ok(vec![Value::Vec2(Vec2 { x: dydt, y: -dxdt })])
    })
}

// ─── noise/noise ─────────────────────────────────────────────────────────────

/// A 2D noise function of (x, y, seed), in [0, 1].
type Base = fn(f64, f64, f64) -> f64;

/// Every seed the Noise node (and the Expression's `noise()`) uses goes
/// through here, which mixes in the patch seed ([`crate::seed`]).
pub(crate) fn node_seed(seed: f64) -> f64 {
    crate::seed::mix(seed)
}

/// More octaves than this add nothing visible (the 33rd is 2^32 times
/// finer) and only cost time.
const NOISE_MAX_OCTAVES: f64 = 32.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Perlin,
    Simplex,
    Value,
    Cellular,
    Curl,
}

impl Mode {
    fn parse(s: Option<&str>) -> Mode {
        match s {
            Some("simplex") => Mode::Simplex,
            Some("value") => Mode::Value,
            Some("cellular") => Mode::Cellular,
            Some("curl") => Mode::Curl,
            _ => Mode::Perlin,
        }
    }

    fn of(params: &Params) -> Mode {
        Mode::parse(params.get("mode").and_then(serde_json::Value::as_str))
    }
}

/// Which cellular distance comes out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Feature {
    F1,
    F2,
    F2MinusF1,
}

impl Feature {
    fn parse(s: &str) -> Feature {
        match s {
            "F2" => Feature::F2,
            "F2-F1" => Feature::F2MinusF1,
            _ => Feature::F1,
        }
    }
}

/// Input order (as `noise_inputs` builds it). `feature` (cellular mode
/// only) comes last so the other pins keep their indices in every mode.
const POS: usize = 0;
const X: usize = 1;
const Y: usize = 2;
const SCALE: usize = 3;
const SEED: usize = 4;
const OCTAVES: usize = 5;
const FALLOFF: usize = 6;
const WARP: usize = 7;
const WARP_SCALE: usize = 8;
const FEATURE: usize = 9;

/// `dynamicInputs` for `noise/noise`: spread-kind pins (so a spread of
/// positions gives a spread of values), plus `feature` in cellular mode.
/// The sample point is `(pos + (x, y)) · scale`: `pos` takes a vec2 (a
/// grid), `x` / `y` numbers, so both old nodes' wiring carries over
/// (`noise/curl` had `pos`, `noise/perlin` had `x` and `y`).
fn noise_inputs(params: &Params) -> Vec<PortSpec> {
    let num = |name: &str, default: f64| {
        PortSpec::new(name, PortType::Number)
            .kind(crate::spec::PortKind::Spread)
            .default(Value::Num(default))
    };
    let int = |p: PortSpec| PortSpec { integer: true, ..p };
    let mut ports = vec![
        PortSpec::new("pos", PortType::Vec2)
            .kind(crate::spec::PortKind::Spread)
            .default(Value::Vec2(Vec2::default())),
        num("x", 0.0),
        num("y", 0.0),
        num("scale", 1.0),
        int(num("seed", 0.0)),
        int(num("octaves", 1.0)),
        num("falloff", 0.5),
        num("warp", 0.0),
        num("warpScale", 1.0).label("warp scale"),
    ];
    if Mode::of(params) == Mode::Cellular {
        let mut p = PortSpec::new("feature", PortType::String).default(Value::str("F1"));
        p.param = Some("feature".into());
        ports.push(p);
    }
    ports
}

/// Hash-value noise: smoothly interpolated random values at the lattice
/// points. In [0, 1].
fn value_2d(x: f64, y: f64, seed: f64) -> f64 {
    let ix = x.floor();
    let iy = y.floor();
    let u = fade(x - ix);
    let v = fade(y - iy);
    let h = |dx: f64, dy: f64| hash_unit(ix + dx, iy + dy, seed);
    lerp(
        lerp(h(0.0, 0.0), h(1.0, 0.0), u),
        lerp(h(0.0, 1.0), h(1.0, 1.0), u),
        v,
    )
}

/// 2D simplex noise (Gustavson's skewed triangular grid) with the same
/// hashed gradients as [`perlin_2d`]. In [0, 1].
fn simplex_2d(x: f64, y: f64, seed: f64) -> f64 {
    const F2: f64 = 0.366_025_403_784_438_6; // (√3 − 1) / 2
    const G2: f64 = 0.211_324_865_405_187_1; // (3 − √3) / 6
    let s = (x + y) * F2;
    let i = (x + s).floor();
    let j = (y + s).floor();
    let t = (i + j) * G2;
    let x0 = x - (i - t);
    let y0 = y - (j - t);
    let (i1, j1) = if x0 > y0 { (1.0, 0.0) } else { (0.0, 1.0) };
    let x1 = x0 - i1 + G2;
    let y1 = y0 - j1 + G2;
    let x2 = x0 - 1.0 + 2.0 * G2;
    let y2 = y0 - 1.0 + 2.0 * G2;
    let corner = |ci: f64, cj: f64, dx: f64, dy: f64| {
        let t = 0.5 - dx * dx - dy * dy;
        if t <= 0.0 {
            0.0
        } else {
            let t2 = t * t;
            t2 * t2 * grad_dot(ci, cj, dx, dy, seed)
        }
    };
    let n =
        corner(i, j, x0, y0) + corner(i + i1, j + j1, x1, y1) + corner(i + 1.0, j + 1.0, x2, y2);
    // ±70·n spans about [-1, 1].
    (0.5 + 35.0 * n).clamp(0.0, 1.0)
}

/// Distances to the nearest and second-nearest feature points of a
/// jittered grid (one point per cell, searched over the 3×3 cells around).
fn worley(x: f64, y: f64, seed: f64) -> (f64, f64) {
    let ix = x.floor();
    let iy = y.floor();
    let (mut f1, mut f2) = (f64::INFINITY, f64::INFINITY);
    for dy in -1..=1 {
        for dx in -1..=1 {
            let cx = ix + f64::from(dx);
            let cy = iy + f64::from(dy);
            let h = hash32(cx, cy, seed);
            let px = cx + f64::from(h & 0xffff) / 65_536.0;
            let py = cy + f64::from(h >> 16) / 65_536.0;
            let d = js::hypot(px - x, py - y);
            if d < f1 {
                f2 = f1;
                f1 = d;
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    (f1, f2)
}

fn worley_f1(x: f64, y: f64, seed: f64) -> f64 {
    worley(x, y, seed).0.min(1.0)
}

fn worley_f2(x: f64, y: f64, seed: f64) -> f64 {
    worley(x, y, seed).1.min(1.0)
}

fn worley_f2_f1(x: f64, y: f64, seed: f64) -> f64 {
    let (f1, f2) = worley(x, y, seed);
    (f2 - f1).min(1.0)
}

/// The noise at one point, after scaling and domain warp.
#[derive(Clone, Copy)]
struct Sampler {
    base: Base,
    seed: f64,
    octaves: f64,
    falloff: f64,
    warp: f64,
    warp_scale: f64,
}

impl Sampler {
    /// Offset the point by `warp` × a Perlin vector field (each component
    /// in [-1, 1]) sampled at `warp_scale` × the point.
    fn warped(&self, x: f64, y: f64) -> (f64, f64) {
        if self.warp == 0.0 {
            return (x, y);
        }
        let (wx, wy) = (x * self.warp_scale, y * self.warp_scale);
        let f = |dx: f64, dy: f64, s: f64| {
            2.0 * fbm_2d(wx + dx, wy + dy, s, self.octaves, self.falloff) - 1.0
        };
        let ox = f(0.0, 0.0, self.seed + 1013.0);
        let oy = f(5.2, 1.3, self.seed + 2027.0);
        (x + self.warp * ox, y + self.warp * oy)
    }

    fn at(&self, x: f64, y: f64) -> f64 {
        let (x, y) = self.warped(x, y);
        fbm(self.base, x, y, self.seed, self.octaves, self.falloff)
    }

    /// The curl of the field: (∂n/∂y, −∂n/∂x), by central differences as
    /// in `noise/curl`.
    fn curl(&self, x: f64, y: f64) -> Vec2 {
        let eps = 1e-3;
        let dydt = (self.at(x, y + eps) - self.at(x, y - eps)) / (2.0 * eps);
        let dxdt = (self.at(x + eps, y) - self.at(x - eps, y)) / (2.0 * eps);
        Vec2 { x: dydt, y: -dxdt }
    }
}

fn noise(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let mode = Mode::parse(cx.param("mode").and_then(serde_json::Value::as_str));
        let feature = match inputs.get(FEATURE).and_then(|s| s.first()) {
            Some(Value::Str(s)) => Feature::parse(s),
            _ => Feature::parse(cx.param("feature").and_then(|v| v.as_str()).unwrap_or("")),
        };
        let base: Base = match (mode, feature) {
            (Mode::Perlin | Mode::Curl, _) => perlin_2d,
            (Mode::Simplex, _) => simplex_2d,
            (Mode::Value, _) => value_2d,
            (Mode::Cellular, Feature::F1) => worley_f1,
            (Mode::Cellular, Feature::F2) => worley_f2,
            (Mode::Cellular, Feature::F2MinusF1) => worley_f2_f1,
        };
        let pins = inputs.get(..=WARP_SCALE).unwrap_or(&[]);
        if pins.len() <= WARP_SCALE {
            return Err("noise: missing inputs".into());
        }
        let count = cyclic_len(pins);
        let mut values = Vec::with_capacity(count);
        let mut flows = Vec::with_capacity(count);
        for i in 0..count {
            let n = |k: usize| cyclic(&pins[k], i).to_number();
            let pos = to_vec2(&cyclic(&pins[POS], i));
            let scale = n(SCALE);
            let (x, y) = ((pos.x + n(X)) * scale, (pos.y + n(Y)) * scale);
            let s = Sampler {
                base,
                seed: node_seed(n(SEED)),
                octaves: js::min(n(OCTAVES), NOISE_MAX_OCTAVES),
                falloff: n(FALLOFF),
                warp: n(WARP),
                warp_scale: n(WARP_SCALE),
            };
            values.push(Value::Num(s.at(x, y)));
            flows.push(Value::Vec2(if mode == Mode::Curl {
                s.curl(x, y)
            } else {
                Vec2::default()
            }));
        }
        Ok(vec![Spread::from(values), Arc::from(flows)])
    })
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{expect, n, node};
    use crate::value::{Value, Vec2};

    // Expected values recorded from the TS nodes.
    #[test]
    fn perlin_matches_ts() {
        let mut p = node("noise/perlin", serde_json::json!({}));
        for (args, want) in [
            ([0.5, 0.5, 0.0, 1.0, 0.5], "0.8535533905932737"),
            ([1.3, -2.7, 3.0, 1.0, 0.5], "0.13527397481999082"),
            ([12.34, 5.67, 7.0, 4.0, 0.5], "0.22165775808815524"),
            ([-100.5, 33.25, -2.0, 3.0, 0.7], "0.7577362848449467"),
            ([0.0, 0.0, 0.0, f64::NAN, 0.5], "0"),
            ([3.3, 4.4, 1.0, 2.0, -1.0], "0"),
            ([0.1, 0.2, 0.3, 2.9, 0.5], "0.5643396193198119"),
            ([7.5, 8.25, 5.0, 0.0, 0.5], "0.34152245480242904"),
            (
                [1e6 + 0.3, -1e6 - 0.7, 11.0, 2.0, 0.5],
                "0.4359377007336791",
            ),
            ([2.0, 3.0, 0.0, 1.0, 0.5], "0.5"),
        ] {
            expect(&p.eval1(&args.map(n)), &format!(r##"["#{want}"]"##));
        }
    }

    #[test]
    fn curl_matches_ts() {
        let mut c = node("noise/curl", serde_json::json!({}));
        for ((x, y, rest), want) in [
            (
                (100.0, 50.0, [0.012, 7.0, 2.0, 0.5]),
                r##"[{"x":"#-0.3623858307361394","y":"#0.2656697742991765"}]"##,
            ),
            (
                (-30.0, 12.0, [0.05, 0.0, 1.0, 0.5]),
                r##"[{"x":"#-0.10227725418904265","y":"#-0.19640485722380308"}]"##,
            ),
            (
                (3.0, 4.0, [1.0, 2.0, 3.0, 0.6]),
                r##"[{"x":"#-0.08666290188194536","y":"#-0.27422570080440356"}]"##,
            ),
        ] {
            let mut args = vec![Value::Vec2(Vec2 { x, y })];
            args.extend(rest.map(n));
            expect(&c.eval1(&args), want);
        }
    }

    // ─── noise/noise ─────────────────────────────────────────────────────

    use super::super::value::harness::{num, Harness};
    use serde_json::json;

    /// x, y, scale, seed, octaves, falloff, warp, warpScale (after `pos`).
    const DEFAULTS: [f64; 8] = [0.0, 0.0, 1.0, 0.0, 1.0, 0.5, 0.0, 1.0];

    fn origin() -> Vec<Value> {
        vec![Value::Vec2(Vec2::default())]
    }

    fn noise_node(mode: &str) -> Harness {
        node("noise/noise", json!({ "mode": mode }))
    }

    /// One evaluation with single values (pins past `args` at defaults);
    /// returns (noise, flow).
    fn sample(h: &mut Harness, args: &[f64]) -> (f64, Vec2) {
        let ins: Vec<Vec<Value>> = std::iter::once(origin())
            .chain(
                DEFAULTS
                    .iter()
                    .enumerate()
                    .map(|(i, d)| vec![n(*args.get(i).unwrap_or(d))]),
            )
            .collect();
        let out = h.eval(ins).unwrap();
        let Value::Vec2(flow) = out[1][0] else {
            panic!("flow is not a vec2")
        };
        (num(&out[0][0]), flow)
    }

    fn grid() -> impl Iterator<Item = (f64, f64)> {
        (0..40).flat_map(|i| (0..40).map(move |j| (i as f64 * 0.37 - 7.0, j as f64 * 0.29 - 5.0)))
    }

    #[test]
    fn perlin_mode_matches_the_perlin_node() {
        let mut old = node("noise/perlin", json!({}));
        let mut new = noise_node("perlin");
        let cases = [
            [0.5, 0.5, 0.0, 1.0, 0.5],
            [1.3, -2.7, 3.0, 1.0, 0.5],
            [12.34, 5.67, 7.0, 4.0, 0.5],
            [-100.5, 33.25, -2.0, 3.0, 0.7],
            [0.0, 0.0, 0.0, f64::NAN, 0.5],
            [3.3, 4.4, 1.0, 2.0, -1.0],
            [7.5, 8.25, 5.0, 0.0, 0.5],
            [1e6 + 0.3, -1e6 - 0.7, 11.0, 2.0, 0.5],
        ];
        for [x, y, seed, oct, fall] in cases {
            let want = num(&old.eval1(&[x, y, seed, oct, fall].map(n))[0]);
            let (got, _) = sample(&mut new, &[x, y, 1.0, seed, oct, fall]);
            assert!(want.to_bits() == got.to_bits(), "{x},{y}: {got} vs {want}");
        }
        // The old node's default pins: no wires, every pin at its default.
        let (got, _) = sample(&mut new, &[]);
        assert_eq!(got, num(&old.eval1(&[0.0, 0.0, 0.0, 1.0, 0.5].map(n))[0]));
    }

    #[test]
    fn curl_mode_matches_the_curl_node() {
        let mut old = node("noise/curl", json!({}));
        let mut new = noise_node("curl");
        for (x, y, scale, seed, oct, fall) in [
            (100.0, 50.0, 0.012, 7.0, 2.0, 0.5),
            (-30.0, 12.0, 0.05, 0.0, 1.0, 0.5),
            (3.0, 4.0, 1.0, 2.0, 3.0, 0.6),
        ] {
            let mut args = vec![Value::Vec2(Vec2 { x, y })];
            args.extend([scale, seed, oct, fall].map(n));
            let Value::Vec2(want) = old.eval1(&args)[0] else {
                panic!()
            };
            let (potential, got) = sample(&mut new, &[x, y, scale, seed, oct, fall]);
            assert_eq!(got, want);
            // The same wiring as the old node: the point on `pos`.
            let mut ins: Vec<Vec<Value>> = std::iter::once(vec![args[0].clone()])
                .chain(DEFAULTS.iter().map(|d| vec![n(*d)]))
                .collect();
            for (k, v) in [(super::SCALE, scale), (super::SEED, seed)] {
                ins[k] = vec![n(v)];
            }
            ins[super::OCTAVES] = vec![n(oct)];
            ins[super::FALLOFF] = vec![n(fall)];
            assert!(matches!(new.eval(ins).unwrap()[1][0], Value::Vec2(v) if v == want));
            assert_eq!(
                potential,
                super::fbm_2d(x * scale, y * scale, seed, oct, fall)
            );
        }
    }

    #[test]
    fn every_mode_stays_in_range_and_varies() {
        for (mode, feature) in [
            ("perlin", None),
            ("simplex", None),
            ("value", None),
            ("cellular", Some("F1")),
            ("cellular", Some("F2")),
            ("cellular", Some("F2-F1")),
            ("curl", None),
        ] {
            for octaves in [1.0, 4.0] {
                let mut h = noise_node(mode);
                h.params.set("n1", "feature", json!(feature));
                let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                for (x, y) in grid() {
                    let ins: Vec<Vec<Value>> = std::iter::once(origin())
                        .chain(
                            [x, y, 1.0, 3.0, octaves, 0.5, 0.0, 1.0]
                                .iter()
                                .map(|v| vec![n(*v)]),
                        )
                        .chain(feature.map(|f| vec![Value::str(f)]))
                        .collect();
                    let v = num(&h.eval(ins).unwrap()[0][0]);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                // Perlin (and curl's Perlin potential) is the old node's
                // unclamped noise, which overshoots [0, 1] by a hair.
                let slack = if matches!(mode, "perlin" | "curl") {
                    0.05
                } else {
                    0.0
                };
                assert!(
                    lo >= -slack && hi <= 1.0 + slack,
                    "{mode} {feature:?}: [{lo}, {hi}]"
                );
                assert!(
                    hi - lo > 0.3,
                    "{mode} {feature:?} barely varies: [{lo}, {hi}]"
                );
            }
        }
    }

    #[test]
    fn deterministic_and_seeded() {
        for mode in ["perlin", "simplex", "value", "cellular", "curl"] {
            let mut h = noise_node(mode);
            let pts: Vec<(f64, f64)> = grid().take(50).collect();
            let run = |h: &mut Harness, seed: f64| -> Vec<(f64, Vec2)> {
                pts.iter()
                    .map(|&(x, y)| sample(h, &[x, y, 1.0, seed]))
                    .collect()
            };
            let a = run(&mut h, 1.0);
            assert_eq!(
                a,
                run(&mut noise_node(mode), 1.0),
                "{mode} is deterministic"
            );
            assert_ne!(a, run(&mut h, 2.0), "{mode}: a new seed gives new noise");
        }
    }

    #[test]
    fn cellular_features() {
        let pts: Vec<(f64, f64)> = grid().collect();
        for &(x, y) in &pts {
            let (f1, f2) = super::worley(x, y, 5.0);
            assert!(f1 <= f2);
            assert_eq!(super::worley_f2_f1(x, y, 5.0), (f2 - f1).min(1.0));
        }
        // F1 is 0 on a feature point.
        let h = super::hash32(2.0, 3.0, 5.0);
        let (px, py) = (
            2.0 + f64::from(h & 0xffff) / 65_536.0,
            3.0 + f64::from(h >> 16) / 65_536.0,
        );
        assert_eq!(super::worley(px, py, 5.0).0, 0.0);
    }

    #[test]
    fn warp_moves_the_sample_point() {
        let mut h = noise_node("simplex");
        for (x, y) in grid().take(30) {
            let (plain, _) = sample(&mut h, &[x, y]);
            let (zero, _) = sample(&mut h, &[x, y, 1.0, 0.0, 1.0, 0.5, 0.0, 3.0]);
            assert_eq!(plain, zero, "warp 0 changes nothing");
        }
        let warped: Vec<f64> = grid()
            .take(30)
            .map(|(x, y)| sample(&mut h, &[x, y, 1.0, 0.0, 1.0, 0.5, 2.0, 0.5]).0)
            .collect();
        let plain: Vec<f64> = grid()
            .take(30)
            .map(|(x, y)| sample(&mut h, &[x, y]).0)
            .collect();
        assert_ne!(warped, plain);
    }

    #[test]
    fn spreads_evaluate_per_element() {
        let mut h = noise_node("value");
        let xs = [0.1, 1.7, 2.2, 3.9, -4.4];
        let mut ins: Vec<Vec<Value>> = std::iter::once(origin())
            .chain(DEFAULTS.iter().map(|d| vec![n(*d)]))
            .collect();
        ins[super::X] = xs.iter().map(|x| n(*x)).collect();
        // Two seeds cycle across the five positions.
        ins[super::SEED] = vec![n(1.0), n(2.0)];
        let out = h.eval(ins.clone()).unwrap();
        assert_eq!(out[0].len(), 5);
        assert_eq!(out[1].len(), 5);
        for (i, x) in xs.iter().enumerate() {
            let seed = [1.0, 2.0][i % 2];
            assert_eq!(num(&out[0][i]), sample(&mut h, &[*x, 0.0, 1.0, seed]).0);
        }
        // A spread of vec2s on `pos` samples each point.
        let mut by_pos = ins.clone();
        by_pos[super::POS] = xs
            .iter()
            .map(|x| Value::Vec2(Vec2 { x: *x, y: 0.0 }))
            .collect();
        by_pos[super::X] = vec![n(0.0)];
        let same = h.eval(by_pos).unwrap();
        assert!(same[0]
            .iter()
            .zip(out[0].iter())
            .all(|(a, b)| num(a) == num(b)));
        // ... and `pos` adds to (x, y).
        let mut sum = ins.clone();
        sum[super::POS] = vec![Value::Vec2(Vec2 { x: 1.5, y: 2.25 })];
        sum[super::X] = vec![n(1.0)];
        sum[super::Y] = vec![n(0.5)];
        let got = num(&h.eval(sum).unwrap()[0][0]);
        assert_eq!(got, sample(&mut h, &[2.5, 2.75, 1.0, 1.0]).0);
        // A wired empty spread gives an empty result.
        ins[super::Y] = Vec::new();
        assert!(h.eval(ins).unwrap()[0].is_empty());
    }

    #[test]
    fn feature_pin_only_in_cellular_mode() {
        let names = |mode: &str| -> Vec<String> {
            super::noise_inputs(json!({ "mode": mode }).as_object().unwrap())
                .into_iter()
                .map(|p| p.name)
                .collect()
        };
        assert_eq!(names("perlin").len(), 9);
        assert_eq!(
            names("cellular").last().map(String::as_str),
            Some("feature")
        );
        assert_eq!(names("cellular").len(), 10);
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("noise/noise").unwrap();
        let statics: Vec<_> = spec.inputs.iter().map(|p| p.name.clone()).collect();
        assert_eq!(
            statics,
            names("perlin"),
            "nodes.json matches the dynamic pins"
        );
        assert!(cat.get("noise/perlin").unwrap().hidden);
        assert!(cat.get("noise/curl").unwrap().hidden);
    }
}
