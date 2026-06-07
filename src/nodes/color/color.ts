import { registerNode } from '../registry';
import type { PortValue } from '../../runtime/types';
import { clamp01, color01ToHsl, hexToColor01, hslToColor01 } from '../_shared/color-conv';
import { toColor, WHITE_COLOR as WHITE } from '../_shared/coerce';

/**
 * Color constructors, decomposers, and operators.
 *
 * Two user-facing nodes today: Color (the unified constructor) and
 * SplitColor (the unified decomposer). Both expose a `mode` parameter
 * so the user picks the colour space in the node UI instead of from a
 * forest of single-purpose RGB / HSL / Hex / SplitRGB / SplitHSL
 * palette entries.
 *
 * The legacy single-mode types (`color/rgb`, `color/hsl`, `color/hex`,
 * `color/split-rgb`, `color/split-hsl`) are still registered with
 * `hidden: true` so existing saved patches load and run, but they're
 * filtered out of the palette + node-search so new users don't see them.
 *
 * Channel conventions on node boundaries:
 *   r, g, b, a  → 0..255 (matches CSS / hex)
 *   h           → 0..360 degrees
 *   s, l        → 0..100 percent
 * Internally `Color` always stores 0..1 floats.
 */

// ─── Unified Color constructor ───────────────────────────────────────────

type ColorMode = 'rgb' | 'hsl' | 'hex';

export const colorNode = registerNode({
  type: 'color/color',
  category: 'Color',
  label: 'Color',
  description: 'Build a colour. Pick mode (rgb / hsl / hex) and the input pins adapt.',
  // Static placeholder — `dynamicInputs` always replaces this. The
  // registry just wants a well-formed list.
  inputs: [{ name: 'r', type: 'number', default: 255, param: 'r' }],
  outputs: [{ name: 'out', type: 'color' }],
  // `defaultParams` only carries the mode picker. Per-channel values
  // live on the dynamic input PORTS (their `default`) and end up in
  // `paramStore` via the `param: 'r'` binding when the user edits them.
  // Keeping rgb/hsl/hex keys out of defaultParams stops GenericNode
  // from rendering them as stray "pure-param" rows in the wrong mode.
  defaultParams: { mode: 'rgb' as ColorMode },
  paramOptions: { mode: ['rgb', 'hsl', 'hex'] },
  dynamicInputs: (params) => {
    const mode = (params.mode as ColorMode | undefined) ?? 'rgb';
    switch (mode) {
      case 'rgb':
        return [
          { name: 'r', type: 'number', default: 255, param: 'r' },
          { name: 'g', type: 'number', default: 255, param: 'g' },
          { name: 'b', type: 'number', default: 255, param: 'b' },
          { name: 'a', type: 'number', default: 255, param: 'a' },
        ];
      case 'hsl':
        return [
          { name: 'h', type: 'number', default: 0,   param: 'h' },
          { name: 's', type: 'number', default: 100, param: 's' },
          { name: 'l', type: 'number', default: 50,  param: 'l' },
          { name: 'a', type: 'number', default: 255, param: 'a' },
        ];
      case 'hex':
        return [{ name: 'hex', type: 'string', default: '#ffffff', param: 'hex' }];
    }
  },
  computeRaw: (inputs, params): PortValue[] => {
    const mode = params.param<ColorMode>('mode', 'rgb');
    switch (mode) {
      case 'rgb': {
        const r = Number(inputs[0]?.[0] ?? 255);
        const g = Number(inputs[1]?.[0] ?? 255);
        const b = Number(inputs[2]?.[0] ?? 255);
        const a = Number(inputs[3]?.[0] ?? 255);
        return [[{
          r: clamp01(r / 255), g: clamp01(g / 255),
          b: clamp01(b / 255), a: clamp01(a / 255),
        }]];
      }
      case 'hsl': {
        const h = Number(inputs[0]?.[0] ?? 0);
        const s = Number(inputs[1]?.[0] ?? 100);
        const l = Number(inputs[2]?.[0] ?? 50);
        const a = Number(inputs[3]?.[0] ?? 255);
        return [[hslToColor01(h, s, l, clamp01(a / 255))]];
      }
      case 'hex': {
        const hex = String(inputs[0]?.[0] ?? '#ffffff');
        return [[hexToColor01(hex)]];
      }
    }
    return [[WHITE]];
  },
});

// ─── Unified Color decomposer ─────────────────────────────────────────────

type SplitMode = 'rgb' | 'hsl';

export const splitColorNode = registerNode({
  type: 'color/split',
  category: 'Color',
  label: 'SplitColor',
  description: 'Decompose a colour into channels. Pick mode (rgb or hsl); alpha is exposed in both.',
  inputs: [{ name: 'color', type: 'color', default: null }],
  // Static port NAMES stay c1/c2/c3/a regardless of mode so wires don't
  // break on mode flip. `dynamicOutputs` flips the user-facing LABELS
  // to r/g/b vs h/s/l so the node reads correctly at a glance.
  outputs: [
    { name: 'c1', label: 'r', type: 'number' },
    { name: 'c2', label: 'g', type: 'number' },
    { name: 'c3', label: 'b', type: 'number' },
    { name: 'a',  label: 'a', type: 'number' },
  ],
  defaultParams: { mode: 'rgb' as SplitMode },
  paramOptions: { mode: ['rgb', 'hsl'] },
  dynamicOutputs: (params) => {
    const mode = (params.mode as SplitMode | undefined) ?? 'rgb';
    if (mode === 'hsl') {
      return [
        { name: 'c1', label: 'h', type: 'number' },
        { name: 'c2', label: 's', type: 'number' },
        { name: 'c3', label: 'l', type: 'number' },
        { name: 'a',  label: 'a', type: 'number' },
      ];
    }
    return [
      { name: 'c1', label: 'r', type: 'number' },
      { name: 'c2', label: 'g', type: 'number' },
      { name: 'c3', label: 'b', type: 'number' },
      { name: 'a',  label: 'a', type: 'number' },
    ];
  },
  computeRaw: (inputs, params): PortValue[] => {
    const c = toColor(inputs[0]?.[0]);
    const mode = params.param<SplitMode>('mode', 'rgb');
    if (mode === 'hsl') {
      const hsl = color01ToHsl(c);
      return [[hsl.h], [hsl.s], [hsl.l], [hsl.a * 255]];
    }
    return [[c.r * 255], [c.g * 255], [c.b * 255], [c.a * 255]];
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
  outputs: [{ name: 'out', type: 'color' }],
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
  outputs: [{ name: 'out', type: 'color' }],
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
 * the interpolated colour.
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
  outputs: [{ name: 'out', type: 'color' }],
  computeRaw: (inputs): PortValue[] => {
    const stops = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const t = Number(inputs[1]?.[0] ?? 0);
    if (stops.length === 0) return [[WHITE]];
    if (stops.length === 1) return [[toColor(stops[0])]];
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

// ─── Legacy single-mode nodes — hidden from palette but kept registered
//     so old patches & samples still load and run. ──────────────────────

export const rgbNode = registerNode({
  type: 'color/rgb',
  category: 'Color',
  label: 'RGB',
  description: 'Legacy — use the unified Color node instead.',
  hidden: true,
  inputs: [
    { name: 'r', type: 'number', default: 255 },
    { name: 'g', type: 'number', default: 255 },
    { name: 'b', type: 'number', default: 255 },
    { name: 'a', type: 'number', default: 255 },
  ],
  outputs: [{ name: 'out', type: 'color' }],
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
  description: 'Legacy — use the unified Color node instead.',
  hidden: true,
  inputs: [
    { name: 'h', type: 'number', default: 0 },
    { name: 's', type: 'number', default: 0 },
    { name: 'l', type: 'number', default: 100 },
    { name: 'a', type: 'number', default: 255 },
  ],
  outputs: [{ name: 'out', type: 'color' }],
  compute: (h, s, l, a) => [hslToColor01(h as number, s as number, l as number, (a as number) / 255)],
});

export const hexNode = registerNode({
  type: 'color/hex',
  category: 'Color',
  label: 'Hex',
  description: 'Legacy — use the unified Color node instead.',
  hidden: true,
  inputs: [{ name: 'hex', type: 'string', default: '#ffffff' }],
  outputs: [{ name: 'out', type: 'color' }],
  compute: (hex) => [hexToColor01(hex as string)],
});

export const splitRgbNode = registerNode({
  type: 'color/split-rgb',
  category: 'Color',
  label: 'SplitRGB',
  description: 'Legacy — use the unified SplitColor node instead.',
  hidden: true,
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
  description: 'Legacy — use the unified SplitColor node instead.',
  hidden: true,
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
