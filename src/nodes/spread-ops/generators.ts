import { registerNode } from '../registry';
import type { Spread } from '../../runtime/types';
import { hashUnit } from '../random/prng';

/**
 * Spread generators — these are the vvvv-defining nodes. They emit a
 * Spread<number> whose *length* is determined by the `count` input.
 *
 * All generators use `computeRaw` because they don't iterate per-slot;
 * they consume scalars (slot 0 of each input) and produce one whole spread.
 * Nested-spread support (producing a Spread<Spread<number>>) is deferred
 * to post-MVP, per the plan.
 */

const intCount = (v: unknown): number => Math.max(0, Math.floor(Number(v ?? 0)));

export const linearSpreadNode = registerNode({
  type: 'spread/linear',
  category: 'Spread',
  label: 'LinearSpread',
  description: 'Generate a spread evenly stepping from `from` to `to`.',
  inputs: [
    { name: 'from', type: 'number', default: 0 },
    { name: 'to', type: 'number', default: 1 },
    { name: 'count', type: 'number', default: 8, integer: true },
  ],
  outputs: [{ name: 'out', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const from = Number(inputs[0]?.[0] ?? 0);
    const to = Number(inputs[1]?.[0] ?? 1);
    const count = intCount(inputs[2]?.[0]);
    if (count === 0) return [[]];
    if (count === 1) return [[from]];
    const out = new Array<number>(count);
    const step = (to - from) / (count - 1);
    for (let i = 0; i < count; i++) out[i] = from + step * i;
    return [out];
  },
});

export const rangeNode = registerNode({
  type: 'spread/range',
  category: 'Spread',
  label: 'Range',
  description: 'Generate the integers 0, 1, …, count − 1.',
  inputs: [{ name: 'count', type: 'number', default: 8, integer: true }],
  outputs: [{ name: 'out', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const count = intCount(inputs[0]?.[0]);
    if (count === 0) return [[]];
    const out = new Array<number>(count);
    for (let i = 0; i < count; i++) out[i] = i;
    return [out];
  },
});

export const repeatNode = registerNode({
  type: 'spread/repeat',
  category: 'Spread',
  label: 'Repeat',
  description: 'Repeat a value N times into a spread.',
  inputs: [
    { name: 'value', type: 'any', default: 0 },
    { name: 'count', type: 'number', default: 8, integer: true },
  ],
  outputs: [{ name: 'out', type: 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const value = inputs[0]?.[0] ?? 0;
    const count = intCount(inputs[1]?.[0]);
    if (count === 0) return [[]];
    return [new Array<unknown>(count).fill(value)];
  },
});

export const circularSpreadNode = registerNode({
  type: 'spread/circular',
  category: 'Spread',
  label: 'CircularSpread',
  description: 'Generate `count` angles evenly spaced across [0, TAU).',
  inputs: [
    { name: 'count', type: 'number', default: 12, integer: true },
    { name: 'phase', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'out', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const count = intCount(inputs[0]?.[0]);
    const phase = Number(inputs[1]?.[0] ?? 0);
    if (count === 0) return [[]];
    const out = new Array<number>(count);
    const step = (Math.PI * 2) / count;
    for (let i = 0; i < count; i++) out[i] = phase + step * i;
    return [out];
  },
});

export const randomizeNode = registerNode({
  type: 'spread/randomize',
  category: 'Spread',
  label: 'Randomize',
  description: 'Generate `count` uniform random numbers in [min, max).',
  inputs: [
    { name: 'count', type: 'number', default: 8, integer: true },
    { name: 'min', type: 'number', default: 0 },
    { name: 'max', type: 'number', default: 1 },
    { name: 'seed', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'out', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const count = intCount(inputs[0]?.[0]);
    const min = Number(inputs[1]?.[0] ?? 0);
    const max = Number(inputs[2]?.[0] ?? 1);
    const seed = Number(inputs[3]?.[0] ?? 0) | 0;
    if (count === 0) return [[]];
    const span = max - min;
    const out = new Array<number>(count);
    for (let i = 0; i < count; i++) out[i] = min + hashUnit(i, 0, seed) * span;
    return [out];
  },
});

// Suppress unused-type warning while we keep the Spread alias around for
// future generators (e.g. NoiseSpread).
void (null as unknown as Spread<number>);
