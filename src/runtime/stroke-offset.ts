/**
 * Variable-width stroke → offset polygon.
 *
 * The Illustrator-style model: given a polyline of N vertices and a
 * width sampled at each vertex, walk both "banks" (left and right
 * offsets perpendicular to the local tangent) and stitch them into a
 * closed polygon. The downstream renderer FILLS the polygon — no
 * `ctx.stroke()` involved — which is what gives clean tapers, smooth
 * width interpolation along the path, and a single `<path fill="…" />`
 * on SVG export.
 *
 * Design notes:
 *
 *   - Width is *sampled* at each vertex using {@link sampleWidthCurve}.
 *     A widths array shorter than the polyline becomes a curve sampled
 *     evenly across t ∈ [0, 1]. Same-length arrays act per-vertex.
 *   - Interior corners use the BISECTOR direction scaled so the offset
 *     distance stays constant — keeps the band width uniform even at
 *     bends. Spike protection via a miter clamp.
 *   - Endpoints (open paths) use the single adjacent edge's
 *     perpendicular. Caps are BUTT by default — we close the polygon
 *     by jumping directly between the left- and right-bank endpoints.
 *     Round / square caps are room to grow.
 *   - Closed paths wrap around naturally; the polygon emitted has
 *     2N vertices (N left + N right).
 *
 * Returned polygon is always closed and intended for filling. Empty
 * input → empty result.
 */

import type { Vec2 } from './types';

/** Maximum miter ratio before the join collapses (SVG `stroke-miterlimit`).
 *  Without this clamp, near-180° corners produce arbitrarily long spikes. */
const MITER_LIMIT = 4;

/**
 * Sample a width curve at parameter `t ∈ [0, 1]`. Linear interpolation
 * between adjacent stops. Empty array → fallback.
 *
 *   widths = []          → fallback everywhere
 *   widths = [w]         → uniform width = w
 *   widths = [a, b]      → linear ramp a → b
 *   widths = [a, b, c]   → a → b at t=0.5 → c
 */
export function sampleWidthCurve(
  widths: ReadonlyArray<number>,
  t: number,
  fallback: number,
): number {
  if (widths.length === 0) return fallback;
  if (widths.length === 1) return widths[0] ?? fallback;
  const last = widths.length - 1;
  const u = t < 0 ? 0 : t > 1 ? 1 : t;
  const idx = u * last;
  const i0 = Math.floor(idx);
  const i1 = i0 >= last ? last : i0 + 1;
  const f = idx - i0;
  return (widths[i0] ?? fallback) * (1 - f) + (widths[i1] ?? fallback) * f;
}

/**
 * Build a closed offset polygon from a polyline + per-vertex widths.
 *
 * @param points    polyline vertices, in order
 * @param widths    optional widths sampled across the path; when empty,
 *                  the polygon shape collapses to zero — caller should
 *                  fall back to a regular stroke draw
 * @param baseWidth fallback width if `widths` is empty or a sample
 *                  evaluates to a non-finite number
 * @param closed    whether the polyline is a closed loop
 */
export function buildOffsetPolygon(
  points: ReadonlyArray<Vec2>,
  widths: ReadonlyArray<number>,
  baseWidth: number,
  closed: boolean,
): Vec2[] {
  const n = points.length;
  if (n < 2) return [];

  const left: Vec2[] = new Array(n);
  const right: Vec2[] = new Array(n);

  for (let i = 0; i < n; i++) {
    const t = closed ? i / n : (n === 1 ? 0 : i / (n - 1));
    const w = Math.max(0, sampleWidthCurve(widths, t, baseWidth)) * 0.5;

    // Tangent vectors on either side of vertex i.
    let inDx = 0, inDy = 0, outDx = 0, outDy = 0;
    let hasIn = false, hasOut = false;
    if (closed) {
      const prev = points[(i - 1 + n) % n]!;
      const next = points[(i + 1) % n]!;
      inDx = points[i]!.x - prev.x; inDy = points[i]!.y - prev.y; hasIn = true;
      outDx = next.x - points[i]!.x; outDy = next.y - points[i]!.y; hasOut = true;
    } else {
      if (i > 0) {
        inDx = points[i]!.x - points[i - 1]!.x;
        inDy = points[i]!.y - points[i - 1]!.y;
        hasIn = true;
      }
      if (i < n - 1) {
        outDx = points[i + 1]!.x - points[i]!.x;
        outDy = points[i + 1]!.y - points[i]!.y;
        hasOut = true;
      }
    }

    // Perpendiculars (left-side, in screen-y-down → rotate 90° CCW).
    const inLen = Math.hypot(inDx, inDy);
    const outLen = Math.hypot(outDx, outDy);
    let nx = 0, ny = 0;
    if (hasIn && hasOut && inLen > 1e-9 && outLen > 1e-9) {
      const inPx = -inDy / inLen, inPy = inDx / inLen;
      const outPx = -outDy / outLen, outPy = outDx / outLen;
      // Sum + normalise → bisector direction.
      const bx = inPx + outPx;
      const by = inPy + outPy;
      const blen = Math.hypot(bx, by);
      if (blen < 1e-9) {
        // 180° turn — fall back to the incoming perpendicular.
        nx = inPx; ny = inPy;
      } else {
        // Scale so the offset distance stays w along the perpendicular
        // (miter math). Dot of the unit bisector with one of the
        // unit perpendiculars gives the cosine of the half-angle.
        const ubx = bx / blen, uby = by / blen;
        const cos = ubx * inPx + uby * inPy;
        // Miter ratio = 1 / cos(half-angle).
        const miter = cos !== 0 ? 1 / cos : MITER_LIMIT;
        const ratio = Math.min(MITER_LIMIT, Math.abs(miter));
        nx = ubx * ratio;
        ny = uby * ratio;
      }
    } else if (hasIn && inLen > 1e-9) {
      nx = -inDy / inLen; ny = inDx / inLen;
    } else if (hasOut && outLen > 1e-9) {
      nx = -outDy / outLen; ny = outDx / outLen;
    }

    left[i]  = { x: points[i]!.x + nx * w, y: points[i]!.y + ny * w };
    right[i] = { x: points[i]!.x - nx * w, y: points[i]!.y - ny * w };
  }

  // Stitch: left bank forward, then right bank backwards. For closed
  // polylines this still works — both banks become independent loops
  // but for filling purposes a single concatenation is fine because
  // even-odd / non-zero fills both produce the band correctly.
  const out: Vec2[] = new Array(n * 2);
  for (let i = 0; i < n; i++) out[i] = left[i]!;
  for (let i = 0; i < n; i++) out[n + i] = right[n - 1 - i]!;
  return out;
}

/**
 * Convenience — does the widths spread carry meaningful variation worth
 * building an offset polygon for? Returns false for empty or
 * effectively-uniform widths, in which case the caller should use the
 * cheaper uniform-stroke path instead of an offset polygon.
 */
export function widthsAreVariable(widths: ReadonlyArray<number>): boolean {
  if (widths.length < 2) return false;
  const first = widths[0]!;
  for (let i = 1; i < widths.length; i++) {
    if (Math.abs((widths[i] ?? first) - first) > 1e-6) return true;
  }
  return false;
}
