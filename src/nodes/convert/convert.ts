import { registerNode } from '../registry';
import type { Color } from '../../runtime/types';

/**
 * Type-coercion nodes. The input is `any`, so any wire can plug in;
 * the output is the target type. Used for stitching together values
 * that don't strictly type-check — e.g. wiring a number into a Text
 * node's `string` input via toString.
 */

function valueToString(v: unknown): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'string') return v;
  if (typeof v === 'number') {
    return Number.isInteger(v) ? String(v) : v.toFixed(3).replace(/\.?0+$/, '');
  }
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (typeof v === 'function') return '[fn]';
  if (Array.isArray(v)) return `[${v.map(valueToString).join(', ')}]`;
  if (typeof v === 'object') {
    const o = v as Record<string, unknown>;
    // vec2
    if (typeof o.x === 'number' && typeof o.y === 'number' && Object.keys(o).length === 2) {
      return `${valueToString(o.x)}, ${valueToString(o.y)}`;
    }
    // color (0..1 floats)
    if (typeof o.r === 'number' && typeof o.g === 'number' && typeof o.b === 'number') {
      const c = o as unknown as Color;
      const hx = (n: number) => Math.max(0, Math.min(255, Math.round(n * 255))).toString(16).padStart(2, '0');
      return `#${hx(c.r)}${hx(c.g)}${hx(c.b)}`;
    }
    try { return JSON.stringify(v); } catch { return '[object]'; }
  }
  return String(v);
}

function valueToNumber(v: unknown): number {
  if (typeof v === 'number') return Number.isFinite(v) ? v : 0;
  if (typeof v === 'boolean') return v ? 1 : 0;
  if (typeof v === 'string') {
    const n = Number(v);
    return Number.isFinite(n) ? n : 0;
  }
  return 0;
}

export const toStringNode = registerNode({
  type: 'convert/to-string',
  category: 'Convert',
  label: 'ToString',
  description: 'Convert any value to a string.',
  inputs: [{ name: 'in', type: 'any' }],
  outputs: [{ name: 'text', type: 'string' }],
  compute: (v) => [valueToString(v)],
});

export const toNumberNode = registerNode({
  type: 'convert/to-number',
  category: 'Convert',
  label: 'ToNumber',
  description: 'Convert any value to a number.',
  inputs: [{ name: 'in', type: 'any' }],
  outputs: [{ name: 'number', type: 'number' }],
  compute: (v) => [valueToNumber(v)],
});

export const toIntNode = registerNode({
  type: 'convert/to-int',
  category: 'Convert',
  label: 'ToInt',
  description: 'Round any value to the nearest integer.',
  inputs: [{ name: 'in', type: 'any' }],
  outputs: [{ name: 'integer', type: 'number' }],
  compute: (v) => [Math.round(valueToNumber(v))],
});

export const toBoolNode = registerNode({
  type: 'convert/to-bool',
  category: 'Convert',
  label: 'ToBool',
  description: 'Convert any value to a boolean.',
  inputs: [{ name: 'in', type: 'any' }],
  outputs: [{ name: 'bool', type: 'boolean' }],
  compute: (v) => [Boolean(v)],
});

export const toDegreesNode = registerNode({
  type: 'convert/to-degrees',
  category: 'Convert',
  label: 'ToDegrees',
  description: 'Scale a unit value [0, 1] to degrees [0, 360].',
  inputs: [{ name: 'in', type: 'number', default: 0 }],
  outputs: [{ name: 'degrees', type: 'number' }],
  compute: (v) => [(v as number) * 360],
});

export const toRadiansNode = registerNode({
  type: 'convert/to-radians',
  category: 'Convert',
  label: 'ToRadians',
  description: 'Convert an angle from degrees to radians.',
  inputs: [{ name: 'deg', type: 'number', default: 0 }],
  outputs: [{ name: 'radians', type: 'number' }],
  compute: (v) => [(v as number) * (Math.PI / 180)],
});
