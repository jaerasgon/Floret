//! `noise/*` nodes. Ported from `src/nodes/random/noise.ts`.
//!
//! Hash-gradient 2D Perlin noise (no permutation table: gradients come from
//! `hash32(ix, iy, seed) & 7`), fBm over it, and curl noise as central
//! differences of the fBm potential.

use super::random::hash32;
use super::shared::to_vec2;
use super::Registrar;
use crate::js;
use crate::node::{scalar, NodeImpl, NodeInit};
use crate::value::{Value, Vec2};

pub fn register(r: &mut Registrar) {
    r.node("noise/perlin", perlin);
    r.node("noise/curl", curl);
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
    let oct = js::max(1.0, octaves.floor());
    if oct == 1.0 {
        return perlin_2d(x, y, seed);
    }
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut total = 0.0;
    let mut freq = 1.0;
    let mut i = 0.0;
    // `i < NaN` is false: NaN octaves run zero iterations.
    while i < oct && i < MAX_OCTAVES {
        sum += perlin_2d(x * freq, y * freq, seed + i) * amp;
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
}
