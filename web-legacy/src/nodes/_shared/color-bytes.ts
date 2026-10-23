import type { Color } from '../../runtime/types';

/**
 * Byte ↔ Color conversions for ImageData round-trips.
 *
 * ImageData stores pixels as 4 sequential `Uint8ClampedArray` bytes per
 * pixel in row-major order: `[r, g, b, a, r, g, b, a, ...]`, channels in
 * 0..255. Floret's runtime `Color` shape is `{ r, g, b, a }` in 0..1
 * floats (see `runtime/types.ts`). Every image node that crosses the
 * boundary did the multiply/divide inline — these helpers centralise it.
 *
 * The `o` argument is the byte offset (i.e. `(y * width + x) * 4`), not
 * the pixel index, so callers don't have to multiply at every site.
 */

export function colorFromBytes(data: Uint8ClampedArray, o: number): Color {
  return {
    r: (data[o] ?? 0) / 255,
    g: (data[o + 1] ?? 0) / 255,
    b: (data[o + 2] ?? 0) / 255,
    a: (data[o + 3] ?? 0) / 255,
  };
}

export function colorToBytes(c: Color, data: Uint8ClampedArray, o: number): void {
  // Multiply then clamp; Uint8ClampedArray clamps on write, so just round.
  data[o]     = Math.round(c.r * 255);
  data[o + 1] = Math.round(c.g * 255);
  data[o + 2] = Math.round(c.b * 255);
  data[o + 3] = Math.round(c.a * 255);
}
