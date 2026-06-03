import type { Color } from '../../runtime/types';
import { WHITE_COLOR } from './coerce';

/**
 * Colour-space conversion helpers shared between `style/fill` and the
 * `color/*` constructors / decomposers. Channel conventions:
 *
 *   r, g, b, a  internal: 0..1 floats   (matches Color in runtime/types)
 *               external: 0..255 ints   (what node pins expose)
 *   h           0..360 degrees
 *   s, l        0..100 percent
 *
 * Conversion to/from external scales lives in the individual node
 * computes (so the pin contracts stay visible there); these helpers
 * deal in 0..1 / HSL space only.
 */

export function clamp01(n: number): number {
  return n < 0 ? 0 : n > 1 ? 1 : n;
}

const HEX_PATTERN = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i;

export function hexToColor01(hex: string): Color {
  const m = HEX_PATTERN.exec(hex);
  if (!m) return WHITE_COLOR;
  return {
    r: parseInt(m[1]!, 16) / 255,
    g: parseInt(m[2]!, 16) / 255,
    b: parseInt(m[3]!, 16) / 255,
    a: 1,
  };
}

/** HSL (h 0..360, s 0..100, l 0..100) → 0..1 RGBA Color. `a` defaults
 *  to 1 so callers that don't expose alpha (style/fill in `hsl` mode)
 *  get an opaque colour. */
export function hslToColor01(h: number, s: number, l: number, a = 1): Color {
  const sN = s / 100;
  const lN = l / 100;
  const c = (1 - Math.abs(2 * lN - 1)) * sN;
  const hh = (((h % 360) + 360) % 360) / 60;
  const x = c * (1 - Math.abs((hh % 2) - 1));
  let r = 0, g = 0, b = 0;
  if (hh < 1) { r = c; g = x; }
  else if (hh < 2) { r = x; g = c; }
  else if (hh < 3) { g = c; b = x; }
  else if (hh < 4) { g = x; b = c; }
  else if (hh < 5) { r = x; b = c; }
  else { r = c; b = x; }
  const m = lN - c / 2;
  return { r: r + m, g: g + m, b: b + m, a: clamp01(a) };
}

/** 0..1 RGBA Color → HSL (h 0..360, s/l 0..100, a 0..1). */
export function color01ToHsl(col: Color): { h: number; s: number; l: number; a: number } {
  const { r, g, b } = col;
  const mx = Math.max(r, g, b);
  const mn = Math.min(r, g, b);
  const l = (mx + mn) / 2;
  if (mx === mn) return { h: 0, s: 0, l: l * 100, a: col.a };
  const d = mx - mn;
  const s = l > 0.5 ? d / (2 - mx - mn) : d / (mx + mn);
  let h: number;
  if (mx === r) h = (g - b) / d + (g < b ? 6 : 0);
  else if (mx === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return { h: h * 60, s: s * 100, l: l * 100, a: col.a };
}
