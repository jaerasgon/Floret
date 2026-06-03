import { registerNode } from '../registry';
import {
  type Color,
  type Style,
  type ResolvedStyle,
  colorToCss,
} from '../../runtime/types';
import type { PortSpec } from '../types';
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
 */

type FillMode = 'rgb' | 'hsl' | 'hex' | 'color';

const BASE_PORT: PortSpec = { name: 'base', type: 'style' };

/** Fold a possibly-null base Style into the current resolved style. */
function applyBase(base: Style | null | undefined, current: ResolvedStyle): ResolvedStyle {
  return base ? base(current) : current;
}

export const fillNode = registerNode({
  type: 'style/fill',
  category: 'Style',
  label: 'Fill',
  description: 'Set the fill colour for downstream shapes.',
  inputs: [BASE_PORT],
  outputs: [
    { name: 'out', type: 'style' },
    { name: 'color', type: 'color' },
  ],
  defaultParams: { mode: 'rgb' as FillMode },
  paramOptions: { mode: ['rgb', 'hsl', 'hex', 'color'] },
  dynamicInputs: (params) => {
    const mode = (params.mode as FillMode | undefined) ?? 'rgb';
    switch (mode) {
      case 'rgb':
        return [
          { name: 'r', type: 'number', default: 255, param: 'r' },
          { name: 'g', type: 'number', default: 255, param: 'g' },
          { name: 'b', type: 'number', default: 255, param: 'b' },
          BASE_PORT,
        ];
      case 'hsl':
        return [
          { name: 'h', type: 'number', default: 0, param: 'h' },
          { name: 's', type: 'number', default: 50, param: 's' },
          { name: 'l', type: 'number', default: 50, param: 'l' },
          BASE_PORT,
        ];
      case 'hex':
        return [
          { name: 'hex', type: 'string', default: '#ffffff', param: 'hex' },
          BASE_PORT,
        ];
      case 'color':
        return [
          { name: 'color', type: 'color', default: null },
          BASE_PORT,
        ];
    }
  },
  computeRaw: (inputs, params) => {
    const mode = params.param<FillMode>('mode', 'rgb');
    let color: Color;
    let base: Style | null;
    switch (mode) {
      case 'rgb': {
        const r = (inputs[0]?.[0] as number | undefined) ?? 255;
        const g = (inputs[1]?.[0] as number | undefined) ?? 255;
        const b = (inputs[2]?.[0] as number | undefined) ?? 255;
        base = (inputs[3]?.[0] as Style | undefined) ?? null;
        color = { r: r / 255, g: g / 255, b: b / 255, a: 1 };
        break;
      }
      case 'hsl': {
        const h = (inputs[0]?.[0] as number | undefined) ?? 0;
        const s = (inputs[1]?.[0] as number | undefined) ?? 50;
        const l = (inputs[2]?.[0] as number | undefined) ?? 50;
        base = (inputs[3]?.[0] as Style | undefined) ?? null;
        color = hslToColor01(h, s, l);
        break;
      }
      case 'hex': {
        const hex = (inputs[0]?.[0] as string | undefined) ?? '#ffffff';
        base = (inputs[1]?.[0] as Style | undefined) ?? null;
        color = hexToColor01(hex);
        break;
      }
      case 'color': {
        const c = inputs[0]?.[0] as Color | null | undefined;
        base = (inputs[1]?.[0] as Style | undefined) ?? null;
        color = c ?? { r: 1, g: 1, b: 1, a: 1 };
        break;
      }
    }
    const fillCss = colorToCss(color);
    const style: Style = (s) => ({ ...applyBase(base, s), fill: fillCss });
    return [[style], [color]];
  },
});

export const strokeNode = registerNode({
  type: 'style/stroke',
  category: 'Style',
  label: 'Stroke',
  description: 'Set the stroke colour and width.',
  inputs: [
    { name: 'color', type: 'color', default: null },
    { name: 'width', type: 'number', default: 1 },
    { name: 'base', type: 'style' },
  ],
  outputs: [{ name: 'out', type: 'style' }],
  compute: (color, width, base) => {
    const c = (color as Color | null) ?? { r: 0, g: 0, b: 0, a: 1 };
    const w = width as number;
    const baseStyle = base as Style | null;
    const strokeCss = colorToCss(c);
    const style: Style = (s) => {
      const inherited = applyBase(baseStyle, s);
      return {
        ...inherited,
        stroke: strokeCss,
        strokeWeight: w > 0 ? w : inherited.strokeWeight,
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
  outputs: [{ name: 'out', type: 'style' }],
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
  outputs: [{ name: 'out', type: 'style' }],
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
  outputs: [{ name: 'out', type: 'style' }],
  compute: (base) => {
    const baseStyle = base as Style | null;
    const style: Style = (s) => ({ ...applyBase(baseStyle, s), stroke: null });
    return [style];
  },
});
