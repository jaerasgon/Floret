import { registerNode } from '../registry';
import type { PortValue } from '../../runtime/types';
import { clamp01, color01ToHsl, hexToColor01, hslToColor01 } from '../_shared/color-conv';
import { toColor, WHITE_COLOR as WHITE } from '../_shared/coerce';

/**
 * Color constructors, decomposers, and operators. Channel conventions
 * match the Fill node: r/g/b/a are 0..255 on node boundaries, but the
 * internal Color type stores them as 0..1 floats. h is 0..360, s/l are
 * 0..100.
 */

// ─── Constructors ──────────────────────────────────────────────────────────

export const rgbNode = registerNode({
  type: 'color/rgb',
  category: 'Color',
  label: 'RGB',
  description: 'Build a color from red, green, blue, alpha (each 0..255).',
  inputs: [
    { name: 'r', type: 'number', default: 255 },
    { name: 'g', type: 'number', default: 255 },
    { name: 'b', type: 'number', default: 255 },
    { name: 'a', type: 'number', default: 255 },
  ],
  outputs: [{ name: 'out', label: 'color', type: 'color' }],
  compute: (r, g, b, a) => [{
    r: clamp01((r as number) / 255),
    g: clamp01((g as number) / 255),
    b: clamp01((b as number) / 255),
    a: clamp01((a as number) / 255),
  }],
});

export const hslNode = registerNode({
  type: 'color/hsl',
  category: 'Color',
  label: 'HSL',
  description: 'Build a color from hue (0..360), saturation/lightness (0..100), alpha (0..255).',
  inputs: [
    { name: 'h', type: 'number', default: 0 },
    { name: 's', type: 'number', default: 0 },
    { name: 'l', type: 'number', default: 100 },
    { name: 'a', type: 'number', default: 255 },
  ],
  outputs: [{ name: 'out', label: 'color', type: 'color' }],
  compute: (h, s, l, a) => [hslToColor01(h as number, s as number, l as number, (a as number) / 255)],
});

export const hexNode = registerNode({
  type: 'color/hex',
  category: 'Color',
  label: 'Hex',
  description: 'Parse a "#rrggbb" string into a color.',
  inputs: [{ name: 'hex', type: 'string', default: '#ffffff' }],
  outputs: [{ name: 'out', label: 'color', type: 'color' }],
  compute: (hex) => [hexToColor01(hex as string)],
});

// ─── Decomposers ───────────────────────────────────────────────────────────

export const splitRgbNode = registerNode({
  type: 'color/split-rgb',
  category: 'Color',
  label: 'SplitRGB',
  description: 'Decompose a color into red, green, blue, alpha (each 0..255).',
  inputs: [{ name: 'color', type: 'color', default: WHITE }],
  outputs: [
    { name: 'r', type: 'number' },
    { name: 'g', type: 'number' },
    { name: 'b', type: 'number' },
    { name: 'a', type: 'number' },
  ],
  compute: (c) => {
    const col = toColor(c);
    return [col.r * 255, col.g * 255, col.b * 255, col.a * 255];
  },
});

export const splitHslNode = registerNode({
  type: 'color/split-hsl',
  category: 'Color',
  label: 'SplitHSL',
  description: 'Decompose a color into hue, saturation, lightness, alpha.',
  inputs: [{ name: 'color', type: 'color', default: WHITE }],
  outputs: [
    { name: 'h', type: 'number' },
    { name: 's', type: 'number' },
    { name: 'l', type: 'number' },
    { name: 'a', type: 'number' },
  ],
  compute: (c) => {
    const hsl = color01ToHsl(toColor(c));
    return [hsl.h, hsl.s, hsl.l, hsl.a * 255];
  },
});

// ─── Ops ───────────────────────────────────────────────────────────────────

export const setAlphaNode = registerNode({
  type: 'color/set-alpha',
  category: 'Color',
  label: 'SetAlpha',
  description: 'Replace the alpha channel (0..255) of a color.',
  inputs: [
    { name: 'color', type: 'color', default: WHITE },
    { name: 'a', type: 'number', default: 255 },
  ],
  outputs: [{ name: 'out', label: 'color', type: 'color' }],
  compute: (c, a) => {
    const col = toColor(c);
    return [{ ...col, a: clamp01((a as number) / 255) }];
  },
});

export const lerpNode = registerNode({
  type: 'color/lerp',
  category: 'Color',
  label: 'Lerp',
  description: 'Linear interpolation in RGB between a and b.',
  inputs: [
    { name: 'a', type: 'color', default: WHITE },
    { name: 'b', type: 'color', default: WHITE },
    { name: 't', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'out', label: 'color', type: 'color' }],
  compute: (a, b, t) => {
    const pa = toColor(a);
    const pb = toColor(b);
    const tv = t as number;
    return [{
      r: pa.r + (pb.r - pa.r) * tv,
      g: pa.g + (pb.g - pa.g) * tv,
      b: pa.b + (pb.b - pa.b) * tv,
      a: pa.a + (pb.a - pa.a) * tv,
    }];
  },
});

/**
 * Gradient — given a spread of color stops and a `t` in [0, 1], return
 * the interpolated colour. Was previously named `color/palette`; the
 * new `color/palette` is the builder that produces the input spread.
 */
export const gradientNode = registerNode({
  type: 'color/gradient',
  category: 'Color',
  label: 'Gradient',
  description: 'Sample a gradient of color stops at t ∈ [0, 1].',
  inputs: [
    { name: 'colors', type: 'color', kind: 'spread' },
    { name: 't', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'out', label: 'color', type: 'color' }],
  computeRaw: (inputs): PortValue[] => {
    const stops = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const t = Number(inputs[1]?.[0] ?? 0);
    if (stops.length === 0) return [[WHITE]];
    if (stops.length === 1) return [[toColor(stops[0])]];
    // clamp t to [0, 1]
    const tc = t < 0 ? 0 : t > 1 ? 1 : t;
    const scaled = tc * (stops.length - 1);
    const lo = Math.floor(scaled);
    const hi = Math.min(stops.length - 1, lo + 1);
    const f = scaled - lo;
    const a = toColor(stops[lo]);
    const b = toColor(stops[hi]);
    return [[{
      r: a.r + (b.r - a.r) * f,
      g: a.g + (b.g - a.g) * f,
      b: a.b + (b.b - a.b) * f,
      a: a.a + (b.a - a.a) * f,
    }]];
  },
});
