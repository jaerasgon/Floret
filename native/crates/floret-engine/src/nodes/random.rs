//! `random/*` nodes. Ported from `src/nodes/random/` (prng.ts, random.ts,
//! wanderer.ts).
//!
//! Pure functions of their seed: `hash32` is bit-exact with the TS
//! `Math.imul` hash (wrapping u32 arithmetic).

use super::noise::perlin_2d;
use super::Registrar;
use crate::js::to_int32;
use crate::node::{scalar, NodeImpl, NodeInit};
use crate::seed;
use crate::value::{Value, Vec2};

pub fn register(r: &mut Registrar) {
    r.node("random/uniform", uniform);
    r.node("random/gaussian", gaussian);
    r.node("random/wanderer2d", wanderer2d);
    // Per element over spreads (v4 patches on): a spread of seeds gives a
    // value per seed.
    r.lifted(&["random/uniform", "random/gaussian", "random/wanderer2d"]);
}

/// `hash32(x, y, seed)` from prng.ts. Arguments go through `| 0`.
pub(crate) fn hash32(x: f64, y: f64, seed: f64) -> u32 {
    let mut h = (to_int32(x) as u32).wrapping_mul(2_654_435_761);
    h = (h ^ to_int32(y) as u32).wrapping_mul(2_246_822_519);
    h = (h ^ to_int32(seed) as u32).wrapping_mul(3_266_489_917);
    h ^= h >> 16;
    h = h.wrapping_mul(668_265_263);
    h ^= h >> 13;
    h = h.wrapping_mul(374_761_393);
    h ^= h >> 16;
    h
}

/// `hashUnit`: hash → [0, 1).
pub(crate) fn hash_unit(x: f64, y: f64, seed: f64) -> f64 {
    f64::from(hash32(x, y, seed)) / 4_294_967_296.0
}

fn uniform(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        let (mn, mx) = (a[0].to_number(), a[1].to_number());
        let s = f64::from(to_int32(seed::mix(a[2].to_number())));
        Ok(vec![Value::Num(mn + hash_unit(s, 0.0, 0.0) * (mx - mn))])
    })
}

fn gaussian(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        let (m, sd) = (a[0].to_number(), a[1].to_number());
        let s = f64::from(to_int32(seed::mix(a[2].to_number())));
        // Box–Muller; u1 is never NaN so a plain max matches Math.max.
        let u1 = hash_unit(s, 1.0, 0.0).max(1e-10);
        let u2 = hash_unit(s, 2.0, 0.0);
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        Ok(vec![Value::Num(m + sd * z)])
    })
}

fn wanderer2d(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        let tv = a[0].to_number() * a[1].to_number();
        let area = a[2].to_number();
        let s = f64::from(to_int32(seed::mix(a[3].to_number())));
        let x = (perlin_2d(tv, 0.0, s) - 0.5) * 2.0 * area;
        let y = (perlin_2d(tv, 1000.0, s) - 0.5) * 2.0 * area;
        Ok(vec![Value::Vec2(Vec2 { x, y })])
    })
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{expect, n, node};
    use super::hash32;

    // Expected values recorded from the TS nodes / `prng.ts`.
    const SEEDS: [f64; 9] = [0.0, 1.0, 2.0, 42.0, -7.0, 1e10, 3.7, -3.7, 2_147_483_648.0];

    #[test]
    fn hash32_is_bit_exact() {
        let want: [[u32; 3]; 9] = [
            [0, 673_545_267, 325_350_951],
            [1_464_382_169, 2_751_127_954, 2_339_504_550],
            [2_546_133_385, 523_653_545, 3_248_226_465],
            [2_872_286_151, 476_594_102, 1_038_858_936],
            [1_596_250_938, 3_157_675_229, 3_506_642_997],
            [2_036_558_192, 2_934_426_140, 3_161_336_723],
            [2_256_438_397, 4_153_928_206, 418_297_500],
            [3_775_250_648, 1_459_548_469, 479_689_116],
            [4_191_578_154, 3_033_055_186, 3_244_170_945],
        ];
        for (s, w) in SEEDS.iter().zip(want) {
            let got = [
                hash32(*s, 0.0, 0.0),
                hash32(*s, s + 1.0, -s),
                hash32(1.5, -2.5, *s),
            ];
            assert_eq!(got, w, "seed {s}");
        }
    }

    #[test]
    fn uniform_and_gaussian_match_ts() {
        let uniform = [
            "-3",
            "-0.27237565629184246",
            "1.7425429988652468",
            "2.350049866363406",
            "-0.02675093337893486",
            "0.793385237455368",
            "1.2029440347105265",
            "4.031952306628227",
            "4.807422716170549",
        ];
        let gaussian = [
            "6.87267344693414",
            "8.361849148214162",
            "9.166409909098576",
            "9.543813090073451",
            "8.79987152599669",
            "12.46810468874596",
            "8.56425245221407",
            "13.074895095292025",
            "8.350780247726814",
        ];
        let mut u = node("random/uniform", serde_json::json!({}));
        let mut g = node("random/gaussian", serde_json::json!({}));
        for ((s, wu), wg) in SEEDS.iter().zip(uniform).zip(gaussian) {
            expect(
                &u.eval1(&[n(-3.0), n(5.0), n(*s)]),
                &format!(r##"["#{wu}"]"##),
            );
            expect(
                &g.eval1(&[n(10.0), n(2.0), n(*s)]),
                &format!(r##"["#{wg}"]"##),
            );
        }
    }

    #[test]
    fn wanderer_matches_ts() {
        let mut w = node("random/wanderer2d", serde_json::json!({}));
        for (args, want) in [
            ([0.0, 1.0, 100.0, 0.0], r##"[{"x":"#0","y":"#0"}]"##),
            (
                [1.37, 0.5, 50.0, 3.0],
                r##"[{"x":"#-27.074318386949507","y":"#27.074318386949514"}]"##,
            ),
            (
                [-12.25, 2.0, 10.0, -1.0],
                r##"[{"x":"#7.071067811865475","y":"#-7.0710678118654755"}]"##,
            ),
            (
                [1000.1, 0.01, 1.0, 9.0],
                r##"[{"x":"#-0.0014142135482538842","y":"#0.0014141994414438308"}]"##,
            ),
        ] {
            expect(&w.eval1(&args.map(n)), want);
        }
    }
}
