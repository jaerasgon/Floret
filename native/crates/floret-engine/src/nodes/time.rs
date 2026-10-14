//! `time/*` nodes. Ported from `src/nodes/time/time.ts`.
//!
//! Context nodes: they read the evaluation frame (`cx.frame`), which inside
//! a ForEach with `delay` is shifted per iteration — same as the frame TS
//! passes to `readFromContext`.

use super::shared::first;
use super::Registrar;
use crate::node::{always_eval, raw, NodeImpl, NodeInit};
use crate::value::{one, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn register(r: &mut Registrar) {
    r.node("time/seconds", seconds);
    r.node("time/frame", frame);
    r.node("time/delta", delta);
    r.node("time/clock", clock);
}

/// `t` and the `loop` phase: 0..1 over the patch's loop length (native
/// only; TS has just `t`, see `legacy::ONCE_SINGLE_OUTPUT`).
fn seconds(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| {
        let f = cx.frame;
        Ok(vec![
            one(Value::Num(f.t)),
            one(Value::Num(loop_phase(f.t, f.loop_length))),
        ])
    })
}

/// Where `t` is in a loop of `length` seconds, 0..1. 0 for a length that
/// isn't positive.
///
/// Times within a hair (1e-9 loops) of a loop boundary are exactly 0, so
/// the frame after a recording of whole loops (`t = N · dt`, give or take
/// rounding) is bit-for-bit frame 0 again and the recording joins
/// seamlessly.
pub fn loop_phase(t: f64, length: f64) -> f64 {
    if length.is_nan() || length <= 0.0 || !t.is_finite() {
        return 0.0;
    }
    let x = t / length;
    if (x - x.round()).abs() < 1e-9 {
        return 0.0;
    }
    x.rem_euclid(1.0)
}

fn frame(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| Ok(vec![one(Value::Num(cx.frame.frame_count))]))
}

/// `dt`, capped at `max` when that's above 0 (after a stall — a dragged
/// window, a breakpoint — motion steps once at most `max` instead of
/// jumping). Reads the frame, so it runs every tick.
fn delta(_: &NodeInit) -> Box<dyn NodeImpl> {
    always_eval(raw(|inputs, cx| {
        let dt = cx.frame.dt;
        let max = first(inputs, 0).to_number();
        let dt = if max > 0.0 { dt.min(max) } else { dt };
        Ok(vec![one(Value::Num(dt))])
    }))
}

// ─── time/clock ──────────────────────────────────────────────────────────────

/// The local time zone's offset from UTC, in seconds, at a Unix time:
/// the app installs one (the engine has no OS dependencies); without it
/// `local` reads as UTC.
pub type LocalOffset = fn(unix_seconds: i64) -> i64;

static LOCAL_OFFSET: OnceLock<LocalOffset> = OnceLock::new();

/// Install the local time zone lookup `time/clock` uses (once; later calls
/// are ignored).
pub fn install_local_offset(f: LocalOffset) {
    let _ = LOCAL_OFFSET.set(f);
}

/// [`freeze_clock`]'s time as `f64` bits; NaN (the initial value) = the
/// wall clock.
static FROZEN_CLOCK: AtomicU64 = AtomicU64::new(0x7ff8_0000_0000_0000);

/// Test hook: while `Some(unix_seconds)`, every `time/clock` reads that
/// time plus the frame's `t` instead of the wall clock, so two runs of a
/// patch see the same times (comparing a rebuilt sample with the
/// original). `None` returns to the wall clock. Process-wide.
pub fn freeze_clock(at: Option<f64>) {
    let bits = at.filter(|s| s.is_finite()).unwrap_or(f64::NAN).to_bits();
    FROZEN_CLOCK.store(bits, Ordering::Relaxed);
}

/// `time/clock`: the wall clock, effectful (read every tick).
fn clock(_: &NodeInit) -> Box<dyn NodeImpl> {
    always_eval(raw(|inputs, cx| {
        let frozen = f64::from_bits(FROZEN_CLOCK.load(Ordering::Relaxed));
        let now = if frozen.is_finite() {
            frozen + cx.frame.t
        } else {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0.0, |d| d.as_secs_f64())
        };
        let local = cx.param("zone").and_then(|v| v.as_str()) != Some("utc");
        let zone = match LOCAL_OFFSET.get() {
            Some(f) if local => f(now.floor() as i64) as f64,
            _ => 0.0,
        };
        let hours = first(inputs, 0).to_number();
        let shift = if hours.is_finite() {
            hours * 3600.0
        } else {
            0.0
        };
        let c = clock_parts(now + zone + shift);
        Ok([
            c.hours,
            c.minutes,
            c.seconds,
            c.day,
            c.month,
            c.year,
            c.weekday,
            c.year_phase,
            c.days,
        ]
        .into_iter()
        .map(|v| one(Value::Num(v)))
        .collect())
    }))
}

/// A wall-clock time split into the Clock node's outputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockParts {
    pub hours: f64,
    pub minutes: f64,
    pub seconds: f64,
    pub day: f64,
    pub month: f64,
    pub year: f64,
    /// 0 = Monday … 6 = Sunday.
    pub weekday: f64,
    pub year_phase: f64,
    /// Days since 1970-01-01 (fractional).
    pub days: f64,
}

/// Split seconds since 1970-01-01 00:00 (in whatever zone they're
/// counted) into the time of day and the calendar date.
pub fn clock_parts(t: f64) -> ClockParts {
    let days = (t / 86_400.0).floor();
    let sod = t - days * 86_400.0;
    let (y, m, d) = civil_from_days(days as i64);
    let start = days_from_civil(y, 1, 1) as f64;
    let len = days_from_civil(y + 1, 1, 1) as f64 - start;
    ClockParts {
        hours: sod / 3600.0,
        minutes: (sod % 3600.0) / 60.0,
        seconds: sod % 60.0,
        day: d as f64,
        month: m as f64,
        year: y as f64,
        // 1970-01-01 was a Thursday.
        weekday: ((days as i64 + 3).rem_euclid(7)) as f64,
        year_phase: ((t / 86_400.0 - start) / len).clamp(0.0, 1.0),
        days: t / 86_400.0,
    }
}

/// The proleptic Gregorian date of a day count from 1970-01-01 (Howard
/// Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Days from 1970-01-01 to a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{node, num};
    use super::loop_phase;

    #[test]
    fn reads_frame() {
        for (ty, want) in [
            ("time/seconds", 1.5),
            ("time/frame", 90.0),
            ("time/delta", 0.25),
        ] {
            let mut n = node(ty, serde_json::json!({}));
            n.frame.t = 1.5;
            n.frame.frame_count = 90.0;
            n.frame.dt = 0.25;
            assert_eq!(num(&n.eval1(&[])[0]), want, "{ty}");
        }
    }

    #[test]
    fn delta_caps_at_max() {
        let mut d = node("time/delta", serde_json::json!({}));
        let dt = |d: &mut crate::nodes::value::harness::Harness, frame_dt: f64, max: f64| {
            d.frame.dt = frame_dt;
            num(&d.eval1(&[super::Value::Num(max)])[0])
        };
        // 0 (the default) leaves dt alone, as before.
        assert_eq!(dt(&mut d, 0.5, 0.0), 0.5);
        // After a stall, at most `max`; ordinary frames pass.
        assert_eq!(dt(&mut d, 2.0, 0.05), 0.05);
        assert_eq!(dt(&mut d, 1.0 / 60.0, 0.05), 1.0 / 60.0);
        assert_eq!(dt(&mut d, 0.5, -1.0), 0.5);
        // It reads the frame, so it never memoises.
        let spec = crate::spec::Catalogue::builtin().get("time/delta").unwrap();
        assert!(spec.effectful);
    }

    #[test]
    fn loop_output_is_the_phase_through_the_loop_length() {
        let mut n = node("time/seconds", serde_json::json!({}));
        n.frame.loop_length = 4.0;
        for (t, want) in [
            (0.0, 0.0),
            (1.0, 0.25),
            (3.0, 0.75),
            (4.0, 0.0),
            (9.0, 0.25),
        ] {
            n.frame.t = t;
            let out = n.eval1(&[]);
            assert_eq!(num(&out[0]), t, "t at {t}");
            assert!((num(&out[1]) - want).abs() < 1e-12, "loop at {t}");
        }
    }

    #[test]
    fn loop_phase_wraps_and_guards() {
        assert_eq!(loop_phase(2.0, 2.0), 0.0);
        assert!((loop_phase(-0.5, 2.0) - 0.75).abs() < 1e-12);
        assert_eq!(loop_phase(-1e-300, 2.0), 0.0);
        assert_eq!(loop_phase(1.0, 0.0), 0.0);
        assert_eq!(loop_phase(f64::NAN, 2.0), 0.0);
        // N frames of a loop at a fixed dt: frame N is exactly frame 0
        // again, however the products round.
        for (len, n) in [(3.0, 90), (1.01, 30), (4.7, 141), (0.3, 7)] {
            let dt = len / f64::from(n);
            let at = |i: i32| f64::from(i) * dt;
            assert_eq!(loop_phase(at(n), len), 0.0, "{len} s / {n}");
            assert_eq!(loop_phase(at(2 * n), len), 0.0, "{len} s / {n}, twice");
            for i in 1..n {
                let p = loop_phase(at(i), len);
                assert!((p - f64::from(i) / f64::from(n)).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn clock_parts_split_dates_and_times() {
        use super::clock_parts;
        // (unix seconds, y, m, d, weekday Monday = 0, day of year from 1)
        for (t, y, m, d, wd, yday) in [
            (1_709_209_815.0, 2024.0, 2.0, 29.0, 3.0, 60.0),
            (0.0, 1970.0, 1.0, 1.0, 3.0, 1.0),
            (978_307_199.0, 2000.0, 12.0, 31.0, 6.0, 366.0),
            (-21_600.0, 1969.0, 12.0, 31.0, 2.0, 365.0),
            (1_791_531_900.0, 2026.0, 10.0, 9.0, 4.0, 282.0),
        ] {
            let c = clock_parts(t);
            assert_eq!((c.year, c.month, c.day, c.weekday), (y, m, d, wd), "{t}");
            let leap = y == 2024.0 || y == 2000.0;
            let len = if leap { 366.0 } else { 365.0 };
            let sod = t.rem_euclid(86_400.0);
            let want = (yday - 1.0 + sod / 86_400.0) / len;
            assert!((c.year_phase - want).abs() < 1e-9, "{t}: {}", c.year_phase);
            assert!((c.days - t / 86_400.0).abs() < 1e-9);
        }
        // 12:30:15 → 12.504…, 30.25, 15.
        let c = clock_parts(1_709_209_815.5);
        assert!((c.hours - (12.0 + 30.0 / 60.0 + 15.5 / 3600.0)).abs() < 1e-9);
        assert!((c.minutes - (30.0 + 15.5 / 60.0)).abs() < 1e-9);
        assert!((c.seconds - 15.5).abs() < 1e-6);
    }

    #[test]
    fn clock_node_reads_the_wall_clock_shifted_by_offset() {
        let mut n = node("time/clock", serde_json::json!({ "zone": "utc" }));
        let out = n.eval1(&[crate::value::Value::Num(0.0)]);
        assert_eq!(out.len(), 9);
        let year = num(&out[5]);
        assert!((2024.0..2200.0).contains(&year), "{year}");
        let h0 = num(&out[0]);
        // Twelve hours on: the hour moves by 12 (mod 24).
        let h12 = num(&n.eval1(&[crate::value::Value::Num(12.0)])[0]);
        let d = (h12 - h0 - 12.0).rem_euclid(24.0);
        assert!(d < 0.01 || d > 23.99, "{h0} → {h12}");
    }
}
