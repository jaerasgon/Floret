//! Variable-width stroke → offset polygon. Port of
//! `src/runtime/stroke-offset.ts`.
//!
//! Walks both banks of a polyline (offsets perpendicular to the local
//! tangent, bisector-mitred at interior corners) and stitches them into a
//! closed polygon the renderer fills.

use crate::js;
use crate::value::Vec2;

/// Maximum miter ratio before the join collapses (SVG `stroke-miterlimit`).
const MITER_LIMIT: f64 = 4.0;

/// `sampleWidthCurve`: linear interpolation across the stops at `t ∈ [0, 1]`.
pub fn sample_width_curve(widths: &[f64], t: f64, fallback: f64) -> f64 {
    match widths.len() {
        0 => fallback,
        1 => widths[0],
        len => {
            let last = len - 1;
            // Like the JS ternary, `clamp` passes NaN through.
            let u = t.clamp(0.0, 1.0);
            let idx = u * last as f64;
            let i0f = idx.floor();
            // `widths[i] ?? fallback`: out of range (NaN index) → fallback.
            let at = |i: f64| {
                if i >= 0.0 && i <= last as f64 {
                    widths[i as usize]
                } else {
                    fallback
                }
            };
            let i1f = if i0f >= last as f64 {
                last as f64
            } else {
                i0f + 1.0
            };
            let f = idx - i0f;
            at(i0f) * (1.0 - f) + at(i1f) * f
        }
    }
}

/// `buildOffsetPolygon`: a closed outline (left bank forward, right bank
/// back) of `points` with per-vertex `widths`. Fewer than 2 points → empty.
pub fn build_offset_polygon(
    points: &[Vec2],
    widths: &[f64],
    base_width: f64,
    closed: bool,
) -> Vec<Vec2> {
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);

    for i in 0..n {
        let t = if closed {
            i as f64 / n as f64
        } else {
            i as f64 / (n - 1) as f64
        };
        let w = js::max(0.0, sample_width_curve(widths, t, base_width)) * 0.5;
        let p = points[i];

        let (mut in_dx, mut in_dy, mut out_dx, mut out_dy) = (0.0, 0.0, 0.0, 0.0);
        let (mut has_in, mut has_out) = (false, false);
        if closed {
            let prev = points[(i + n - 1) % n];
            let next = points[(i + 1) % n];
            in_dx = p.x - prev.x;
            in_dy = p.y - prev.y;
            has_in = true;
            out_dx = next.x - p.x;
            out_dy = next.y - p.y;
            has_out = true;
        } else {
            if i > 0 {
                in_dx = p.x - points[i - 1].x;
                in_dy = p.y - points[i - 1].y;
                has_in = true;
            }
            if i < n - 1 {
                out_dx = points[i + 1].x - p.x;
                out_dy = points[i + 1].y - p.y;
                has_out = true;
            }
        }

        let in_len = js::hypot(in_dx, in_dy);
        let out_len = js::hypot(out_dx, out_dy);
        let (mut nx, mut ny) = (0.0, 0.0);
        if has_in && has_out && in_len > 1e-9 && out_len > 1e-9 {
            let (in_px, in_py) = (-in_dy / in_len, in_dx / in_len);
            let (out_px, out_py) = (-out_dy / out_len, out_dx / out_len);
            let bx = in_px + out_px;
            let by = in_py + out_py;
            let blen = js::hypot(bx, by);
            if blen < 1e-9 {
                // 180° turn — fall back to the incoming perpendicular.
                nx = in_px;
                ny = in_py;
            } else {
                let (ubx, uby) = (bx / blen, by / blen);
                let cos = ubx * in_px + uby * in_py;
                let miter = if cos != 0.0 { 1.0 / cos } else { MITER_LIMIT };
                let ratio = js::min(MITER_LIMIT, miter.abs());
                nx = ubx * ratio;
                ny = uby * ratio;
            }
        } else if has_in && in_len > 1e-9 {
            nx = -in_dy / in_len;
            ny = in_dx / in_len;
        } else if has_out && out_len > 1e-9 {
            nx = -out_dy / out_len;
            ny = out_dx / out_len;
        }

        left.push(Vec2 {
            x: p.x + nx * w,
            y: p.y + ny * w,
        });
        right.push(Vec2 {
            x: p.x - nx * w,
            y: p.y - ny * w,
        });
    }

    left.extend(right.into_iter().rev());
    left
}

/// `widthsAreVariable`: at least two widths that differ by more than 1e-6.
pub fn widths_are_variable(widths: &[f64]) -> bool {
    match widths.split_first() {
        Some((first, rest)) if !rest.is_empty() => rest.iter().any(|w| (w - first).abs() > 1e-6),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2 { x, y }
    }

    #[test]
    fn samples_width_curves() {
        assert_eq!(sample_width_curve(&[], 0.3, 7.0), 7.0);
        assert_eq!(sample_width_curve(&[4.0], 0.3, 7.0), 4.0);
        assert_eq!(sample_width_curve(&[0.0, 10.0], 0.25, 7.0), 2.5);
        assert_eq!(sample_width_curve(&[0.0, 10.0, 0.0], 0.75, 7.0), 5.0);
        assert_eq!(sample_width_curve(&[0.0, 10.0], 2.0, 7.0), 10.0);
    }

    #[test]
    fn variable_widths() {
        assert!(!widths_are_variable(&[]));
        assert!(!widths_are_variable(&[3.0]));
        assert!(!widths_are_variable(&[3.0, 3.0]));
        assert!(widths_are_variable(&[3.0, 4.0]));
        assert!(!widths_are_variable(&[3.0, f64::NAN]));
    }

    #[test]
    fn offsets_an_open_polyline() {
        // A horizontal segment tapering 2 → 4: left bank is y - w/2.
        let out = build_offset_polygon(&[v(0.0, 0.0), v(10.0, 0.0)], &[2.0, 4.0], 1.0, false);
        assert_eq!(
            out,
            vec![v(0.0, 1.0), v(10.0, 2.0), v(10.0, -2.0), v(0.0, -1.0)]
        );
        // A right-angle corner gets a √2 miter along the bisector.
        let out = build_offset_polygon(
            &[v(0.0, 0.0), v(10.0, 0.0), v(10.0, 10.0)],
            &[2.0, 2.0, 2.0],
            1.0,
            false,
        );
        let c = out[1];
        assert!(
            (c.x - 9.0).abs() < 1e-12 && (c.y - 1.0).abs() < 1e-12,
            "{c:?}"
        );
        assert!(build_offset_polygon(&[v(1.0, 1.0)], &[1.0], 1.0, true).is_empty());
    }
}
