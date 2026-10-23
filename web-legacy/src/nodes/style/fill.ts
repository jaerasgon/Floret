import { registerNode } from '../registry';
import {
  type Color,
  type Style,
  type ResolvedStyle,
  colorToCss,
} from '../../runtime/types';
import { hexToColor01, hslToColor01 } from '../_shared/color-conv';

/**
 * Style nodes return Style functions — pure transforms on
 * `ResolvedStyle`. Chained via the `base` input: the base runs first,
 * then this node's override layers on top.
 *
 *   Fill(red) → Stroke(blue,2) → NoFill → shape
 *   = shape.style = NoFill( Stroke(blue,2)( Fill(red)(DEFAULT) ) )
 *
 * No mutation, no leakage. Every shape gets its own resolved snapshot.
 *
 * Fill takes a single `color` input — colour authoring lives in the
 * unified Color node (with its rgb/hsl/hex mode picker). The old
 * `mode` param + per-channel inputs that used to live here are gone;
 * legacy saved patches that set `params.mode = 'rgb'` etc. flow through
 * `legacyComputeFill` below so they keep rendering until the user
 * rewires through a Color node.
 */

/** Fold a possibly-null base Style into the current resolved style. */
function applyBase(base: Style | null | undefined, current: ResolvedStyle): ResolvedStyle {
  return base ? base(current) : current;
}

type LegacyFillMode = 'rgb' | 'hsl' | 'hex' | 'color';

function deriveLegacyColor(
  mode: LegacyFillMode,
  params: { param<T>(key: string, fallback?: T): T },
): Color {
  switch (mode) {
    case 'hsl':
      return hslToColor01(
        params.param<number>('h', 0),
        params.param<number>('s', 50),
        params.param<number>('l', 50),
      );
    case 'hex':
      return hexToColor01(params.param<string>('hex', '#ffffff'));
    case 'rgb':
    default: {
      const r = params.param<number>('r', 255);
      const g = params.param<number>('g', 255);
      const b = params.param<number>('b', 255);
      const a = params.param<number>('a', 255);
      return { r: r / 255, g: g / 255, b: b / 255, a: a / 255 };
    }
  }
}

export const fillNode = registerNode({
  type: 'style/fill',
  category: 'Style',
  label: 'Fill',
  description: 'Set the fill colour for downstream shapes. Wire a Color node into `color`.',
  inputs: [
    { name: 'color', type: 'color', default: null },
    { name: 'base', type: 'style' },
  ],
  outputs: [{ name: 'style', type: 'style' }],
  computeRaw: (inputs, params) => {
    const wired = inputs[0]?.[0] as Color | null | undefined;
    const base = (inputs[1]?.[0] as Style | undefined) ?? null;
    // Wired colour wins. If nothing's wired, check whether the patch
    // came from the old multi-mode Fill (params.mode set) and replay
    // that here so legacy patches keep their colour until rewired.
    // New patches with nothing wired fall back to white.
    let color: Color;
    if (wired) {
      color = wired;
    } else {
      const legacyMode = params.param<LegacyFillMode | undefined>('mode', undefined);
      color = legacyMode
        ? deriveLegacyColor(legacyMode, params)
        : { r: 1, g: 1, b: 1, a: 1 };
    }
    const fillCss = colorToCss(color);
    const style: Style = (s) => ({ ...applyBase(base, s), fill: fillCss });
    return [[style]];
  },
});

export const strokeNode = registerNode({
  type: 'style/stroke',
  category: 'Style',
  label: 'Stroke',
  description: 'Set the stroke colour, width, and alignment (center / inside / outside the outline). Alignment applies to closed shapes.',
  inputs: [
    { name: 'color', type: 'color', default: null },
    { name: 'width', type: 'number', default: 1 },
    { name: 'align', type: 'string', default: 'center', param: 'align' },
    { name: 'base', type: 'style' },
  ],
  outputs: [{ name: 'style', type: 'style' }],
  defaultParams: { align: 'center' },
  paramOptions: { align: ['center', 'inside', 'outside'] },
  compute: (color, width, align, base) => {
    const c = (color as Color | null) ?? { r: 0, g: 0, b: 0, a: 1 };
    const w = width as number;
    const baseStyle = base as Style | null;
    const strokeCss = colorToCss(c);
    const al = align === 'inside' || align === 'outside' ? align : 'center';
    const style: Style = (s) => {
      const inherited = applyBase(baseStyle, s);
      return {
        ...inherited,
        stroke: strokeCss,
        strokeWeight: w > 0 ? w : inherited.strokeWeight,
        strokeAlign: al,
      };
    };
    return [style];
  },
});

export const strokeWeightNode = registerNode({
  type: 'style/stroke-weight',
  category: 'Style',
  label: 'StrokeWeight',
  description: 'Set the stroke thickness in pixels.',
  inputs: [
    { name: 'w', type: 'number', default: 1 },
    { name: 'base', type: 'style' },
  ],
  outputs: [{ name: 'style', type: 'style' }],
  compute: (w, base) => {
    const wv = w as number;
    const baseStyle = base as Style | null;
    const style: Style = (s) => ({ ...applyBase(baseStyle, s), strokeWeight: wv });
    return [style];
  },
});

export const noFillNode = registerNode({
  type: 'style/no-fill',
  category: 'Style',
  label: 'NoFill',
  description: 'Disable the fill on downstream shapes.',
  inputs: [{ name: 'base', type: 'style' }],
  outputs: [{ name: 'style', type: 'style' }],
  compute: (base) => {
    const baseStyle = base as Style | null;
    const style: Style = (s) => ({ ...applyBase(baseStyle, s), fill: null });
    return [style];
  },
});

export const noStrokeNode = registerNode({
  type: 'style/no-stroke',
  category: 'Style',
  label: 'NoStroke',
  description: 'Disable the stroke on downstream shapes.',
  inputs: [{ name: 'base', type: 'style' }],
  outputs: [{ name: 'style', type: 'style' }],
  compute: (base) => {
    const baseStyle = base as Style | null;
    const style: Style = (s) => ({ ...applyBase(baseStyle, s), stroke: null });
    return [style];
  },
});

export const blurNode = registerNode({
  type: 'style/blur',
  category: 'Style',
  label: 'Blur',
  description: 'Gaussian-blur downstream shapes by N pixels (fill, stroke, and text alike). 0 = sharp.',
  inputs: [
    { name: 'amount', type: 'number', default: 0 },
    { name: 'base', type: 'style' },
  ],
  outputs: [{ name: 'style', type: 'style' }],
  compute: (amount, base) => {
    const a = Math.max(0, amount as number);
    const baseStyle = base as Style | null;
    const style: Style = (s) => ({ ...applyBase(baseStyle, s), blur: a });
    return [style];
  },
});
