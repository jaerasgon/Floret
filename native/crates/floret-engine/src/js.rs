//! JavaScript semantics the TS nodes rely on, reproduced exactly.
//!
//! Rust's float functions differ from JS in small ways that change node
//! output: `f64::round` rounds halves away from zero (JS rounds toward +∞),
//! `f64::max` ignores NaN (JS propagates it), `{}` formatting isn't
//! `Number.prototype.toString`, and `{:.N}` rounds ties to even (`toFixed`
//! rounds them up). Use these helpers whenever porting JS arithmetic.

/// `Math.round`: nearest integer, ties toward +∞.
pub fn round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// `Math.max(a, b)`: NaN if either is NaN.
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b || (a == 0.0 && b == 0.0 && a.is_sign_positive()) {
        a
    } else {
        b
    }
}

/// `Math.min(a, b)`: NaN if either is NaN.
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b || (a == 0.0 && b == 0.0 && a.is_sign_negative()) {
        a
    } else {
        b
    }
}

/// `Math.max(...xs)`: -Infinity for no arguments.
pub fn max_of(xs: impl IntoIterator<Item = f64>) -> f64 {
    xs.into_iter().fold(f64::NEG_INFINITY, max)
}

/// `Math.min(...xs)`: +Infinity for no arguments.
pub fn min_of(xs: impl IntoIterator<Item = f64>) -> f64 {
    xs.into_iter().fold(f64::INFINITY, min)
}

/// `Math.sign`.
pub fn sign(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        x
    } else if x > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// `Math.trunc`.
pub fn trunc(x: f64) -> f64 {
    x.trunc()
}

/// `a % b` (same as Rust's `%` on f64: sign of the dividend).
pub fn rem(a: f64, b: f64) -> f64 {
    a % b
}

/// `Math.hypot(a, b)`.
pub fn hypot(a: f64, b: f64) -> f64 {
    a.hypot(b)
}

/// `x | 0` / `ToInt32`.
pub fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    let m = x.trunc() % 4_294_967_296.0;
    let m = if m < 0.0 { m + 4_294_967_296.0 } else { m };
    (m as u64 as u32) as i32
}

/// `x >>> 0` / `ToUint32`.
pub fn to_uint32(x: f64) -> u32 {
    to_int32(x) as u32
}

/// `Math.imul`.
pub fn imul(a: f64, b: f64) -> f64 {
    f64::from(to_int32(a).wrapping_mul(to_int32(b)))
}

/// `Number.prototype.toString()` / `String(n)`.
pub fn num_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    if x.is_infinite() {
        return if x > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    let neg = x < 0.0;
    // `{:e}` gives the shortest round-tripping digits: "1.2345e-7".
    let e = format!("{:e}", x.abs());
    let (mant, exp) = e.split_once('e').expect("exp format");
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp.parse::<i32>().expect("exponent") + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let sign = if n > 0 { '+' } else { '-' };
        let rest = if k > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        format!("{}{rest}e{sign}{}", &digits[..1], (n - 1).abs())
    };
    if neg {
        format!("-{body}")
    } else {
        body
    }
}

/// `Number.prototype.toFixed(digits)` for 0 ≤ digits ≤ 100.
pub fn to_fixed(x: f64, digits: usize) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.abs() >= 1e21 {
        return num_to_string(x);
    }
    // Exact decimal expansion with enough slack to see whether the tail is
    // exactly a tie (JS rounds exact ties up in magnitude; Rust to even).
    let exact = format!("{:.*}", digits + 1100, x.abs());
    let (int_part, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let kept = &frac[..digits.min(frac.len())];
    let rest = &frac[digits.min(frac.len())..];
    let round_up = match rest.as_bytes().first() {
        Some(b'5') => true, // ≥ half: a tie also rounds up
        Some(d) => *d > b'5',
        None => false,
    };
    let mut num: Vec<u8> = int_part.bytes().chain(kept.bytes()).collect();
    if round_up {
        let mut i = num.len();
        loop {
            if i == 0 {
                num.insert(0, b'1');
                break;
            }
            i -= 1;
            if num[i] == b'9' {
                num[i] = b'0';
            } else {
                num[i] += 1;
                break;
            }
        }
    }
    let s = String::from_utf8(num).expect("ascii");
    let split = s.len() - digits;
    let mut out = if digits == 0 {
        s.clone()
    } else {
        format!("{}.{}", &s[..split], &s[split..])
    };
    // JS keeps the sign even when the digits round to zero: "-0.00".
    if x < 0.0 {
        out.insert(0, '-');
    }
    out
}

/// `parseFloat(s)`: longest numeric prefix after leading whitespace.
pub fn parse_float(s: &str) -> f64 {
    let t = s.trim_start();
    for (prefix, v) in [
        ("Infinity", f64::INFINITY),
        ("+Infinity", f64::INFINITY),
        ("-Infinity", f64::NEG_INFINITY),
    ] {
        if t.starts_with(prefix) {
            return v;
        }
    }
    let b = t.as_bytes();
    let mut end = 0;
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let mut saw_digit = false;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
        saw_digit = true;
        end = i;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            saw_digit = true;
            end = i;
        }
    }
    if !saw_digit {
        return f64::NAN;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > start {
            end = j;
        }
    }
    t[..end].parse::<f64>().unwrap_or(f64::NAN)
}

/// `parseInt(s, 10)`.
pub fn parse_int(s: &str) -> f64 {
    let t = s.trim_start();
    let (neg, rest) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return f64::NAN;
    }
    let v: f64 = digits.parse().unwrap_or(f64::NAN);
    if neg {
        -v
    } else {
        v
    }
}

/// The mulberry32 PRNG used by several TS nodes. Bit-exact with
/// `src/nodes/particles/emitter.ts` / `draw/walker-cluster.ts`.
#[derive(Clone, Debug)]
pub struct Mulberry32(u32);

impl Mulberry32 {
    pub fn new(seed: u32) -> Mulberry32 {
        Mulberry32(seed)
    }

    pub fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(0.49999999999999994), 0.0);
        assert_eq!(round(-0.5), 0.0);
    }

    #[test]
    fn nan_propagation() {
        assert!(max(1.0, f64::NAN).is_nan());
        assert!(min(f64::NAN, 1.0).is_nan());
        assert_eq!(max_of([]), f64::NEG_INFINITY);
        assert_eq!(min_of([3.0, 1.0, 2.0]), 1.0);
    }

    #[test]
    fn number_to_string() {
        let cases = [
            (0.0, "0"),
            (1.0, "1"),
            (-1.5, "-1.5"),
            (0.1, "0.1"),
            (123456789.0, "123456789"),
            (1e21, "1e+21"),
            (1e-7, "1e-7"),
            (0.000001, "0.000001"),
            (1.5e-7, "1.5e-7"),
            (100.0, "100"),
            (1.0 / 3.0, "0.3333333333333333"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "-Infinity"),
        ];
        for (x, want) in cases {
            assert_eq!(num_to_string(x), want, "{x}");
        }
    }

    #[test]
    fn fixed() {
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(1.005, 2), "1.00"); // binary 1.00499…
        assert_eq!(to_fixed(0.125, 2), "0.13"); // exact tie rounds up
        assert_eq!(to_fixed(-1.5, 0), "-2");
        assert_eq!(to_fixed(-0.0001, 2), "-0.00");
        assert_eq!(to_fixed(9.995, 2), "9.99");
        assert_eq!(to_fixed(99.5, 0), "100");
        assert_eq!(to_fixed(3.0, 3), "3.000");
    }

    #[test]
    fn parsing() {
        assert_eq!(parse_float("  3.5px"), 3.5);
        assert_eq!(parse_float("1e3x"), 1000.0);
        assert_eq!(parse_float(".5"), 0.5);
        assert!(parse_float("abc").is_nan());
        assert_eq!(parse_int("42.9"), 42.0);
        assert_eq!(parse_int("-7a"), -7.0);
    }

    #[test]
    fn int_conversions() {
        assert_eq!(to_int32(4_294_967_297.0), 1);
        assert_eq!(to_int32(-1.0), -1);
        assert_eq!(to_uint32(-1.0), 4_294_967_295);
        assert_eq!(imul(3.0, 4.0), 12.0);
    }

    #[test]
    fn mulberry_matches_js() {
        // Reference values from the JS implementation, seed 0x5eed.
        let mut r = Mulberry32::new(0x5eed);
        assert_eq!(r.next_f64(), 0.7100320369936526);
        assert_eq!(r.next_f64(), 0.286336648510769);
        assert_eq!(r.next_f64(), 0.9519026265479624);
    }
}
