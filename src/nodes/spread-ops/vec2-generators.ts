import { registerNode } from '../registry';
import type { Vec2 } from '../../runtime/types';

/**
 * Vec2-spread generators — Grid, Spiral, Fibonacci (phyllotaxis).
 * Mirror the existing number-spread generators (LinearSpread, Range, …)
 * but emit a Spread<Vec2> so they can plug straight into `draw/polygon`,
 * a `ForEach` region, or anything that consumes vec2 spreads.
 */

const intCount = (v: unknown): number => Math.max(0, Math.floor(Number(v ?? 0)));

export const gridNode = registerNode({
  type: 'spread/grid',
  category: 'Spread',
  label: 'GridSpread',
  description: 'Generate a `cols × rows` grid of points centred on (cx, cy).',
  inputs: [
    { name: 'cols', type: 'number', default: 8, integer: true },
    { name: 'rows', type: 'number', default: 8, integer: true },
    { name: 'w', type: 'number', default: 400 },
    { name: 'h', type: 'number', default: 400 },
    { name: 'cx', type: 'number', default: 0 },
    { name: 'cy', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'out', type: 'vec2', kind: 'spread' }],
  computeRaw: (inputs) => {
    const cols = intCount(inputs[0]?.[0]);
    const rows = intCount(inputs[1]?.[0]);
    const w = Number(inputs[2]?.[0] ?? 0);
    const h = Number(inputs[3]?.[0] ?? 0);
    const cx = Number(inputs[4]?.[0] ?? 0);
    const cy = Number(inputs[5]?.[0] ?? 0);
    if (cols === 0 || rows === 0) return [[]];
    const dx = cols === 1 ? 0 : w / (cols - 1);
    const dy = rows === 1 ? 0 : h / (rows - 1);
    const ox = cx - w / 2;
    const oy = cy - h / 2;
    const out = new Array<Vec2>(cols * rows);
    let k = 0;
    for (let r = 0; r < rows; r++) {
      const y = oy + r * dy;
      for (let c = 0; c < cols; c++) {
        out[k++] = { x: ox + c * dx, y };
      }
    }
    return [out];
  },
});

export const ringNode = registerNode({
  type: 'spread/ring',
  category: 'Spread',
  label: 'RingSpread',
  description:
    'Lay `count` points evenly around a circle. `positions` are ready to ' +
    'place; `angles` (radians) drive orientation, spirals, or hue.',
  inputs: [
    { name: 'count',  type: 'number', default: 24, integer: true },
    { name: 'radius', type: 'number', default: 100 },
    { name: 'cx',     type: 'number', default: 0 },
    { name: 'cy',     type: 'number', default: 0 },
    { name: 'phase',  type: 'number', default: 0 },
  ],
  outputs: [
    { name: 'positions', type: 'vec2', kind: 'spread' },
    { name: 'angles', type: 'number', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const count = intCount(inputs[0]?.[0]);
    const radius = Number(inputs[1]?.[0] ?? 0);
    const cx = Number(inputs[2]?.[0] ?? 0);
    const cy = Number(inputs[3]?.[0] ?? 0);
    const phase = Number(inputs[4]?.[0] ?? 0);
    if (count === 0) return [[], []];
    const positions = new Array<Vec2>(count);
    const angles = new Array<number>(count);
    const step = (Math.PI * 2) / count;
    for (let i = 0; i < count; i++) {
      const a = phase + step * i;
      angles[i] = a;
      positions[i] = { x: cx + Math.cos(a) * radius, y: cy + Math.sin(a) * radius };
    }
    return [positions, angles];
  },
});

export const spiralNode = registerNode({
  type: 'spread/spiral',
  category: 'Spread',
  label: 'SpiralSpread',
  description: 'Generate points along an Archimedean spiral.',
  inputs: [
    { name: 'count', type: 'number', default: 64, integer: true },
    { name: 'turns', type: 'number', default: 3 },
    { name: 'radius', type: 'number', default: 200 },
    { name: 'phase', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'out', type: 'vec2', kind: 'spread' }],
  computeRaw: (inputs) => {
    const count = intCount(inputs[0]?.[0]);
    const turns = Number(inputs[1]?.[0] ?? 0);
    const radius = Number(inputs[2]?.[0] ?? 0);
    const phase = Number(inputs[3]?.[0] ?? 0);
    if (count === 0) return [[]];
    const out = new Array<Vec2>(count);
    const total = Math.PI * 2 * turns;
    for (let i = 0; i < count; i++) {
      const t = count === 1 ? 0 : i / (count - 1);
      const a = phase + total * t;
      const r = radius * t;
      out[i] = { x: Math.cos(a) * r, y: Math.sin(a) * r };
    }
    return [out];
  },
});

// Golden angle: π · (3 − √5) ≈ 2.39996 radians.
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

export const fibonacciNode = registerNode({
  type: 'spread/fibonacci',
  category: 'Spread',
  label: 'FibonacciSpread',
  description: 'Generate a sunflower spiral using the golden angle.',
  inputs: [
    { name: 'count', type: 'number', default: 200, integer: true },
    { name: 'scale', type: 'number', default: 10 },
    { name: 'phase', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'out', type: 'vec2', kind: 'spread' }],
  computeRaw: (inputs) => {
    const count = intCount(inputs[0]?.[0]);
    const scale = Number(inputs[1]?.[0] ?? 1);
    const phase = Number(inputs[2]?.[0] ?? 0);
    if (count === 0) return [[]];
    const out = new Array<Vec2>(count);
    for (let i = 0; i < count; i++) {
      const a = phase + i * GOLDEN_ANGLE;
      const r = scale * Math.sqrt(i);
      out[i] = { x: Math.cos(a) * r, y: Math.sin(a) * r };
    }
    return [out];
  },
});
