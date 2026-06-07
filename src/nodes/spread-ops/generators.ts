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
  label: 'RangeSpread',
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
  label: 'RepeatSpread',
  description:
    'Repeat the input N times into a spread. A scalar becomes N copies; a spread of length K tiles to length N×K.',
  inputs: [
    // `value` is spread-typed so the whole input array reaches the compute
    // function — that lets the node tile a real spread rather than just
    // taking slot 0 (the old behaviour produced [v0, v0, v0, …] when wired
    // to e.g. an RGB palette, which was obviously wrong).
    { name: 'value', type: 'any', kind: 'spread', default: 0 },
    { name: 'count', type: 'number', default: 8, integer: true },
  ],
  outputs: [{ name: 'out', type: 'any', kind: 'spread' }],
  // Output type echoes whatever's wired into `value` — connect a Color
  // spread, get a Color spread out; connect a Number, get Number out.
  // Falls back to 'any' when nothing's connected.
  dynamicOutputs: (_params, up) => [
    { name: 'out', type: up.value ?? 'any', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const value = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const count = intCount(inputs[1]?.[0]);
    if (count === 0) return [[]];
    // Scalar (or single-slot) input → classic behaviour: N copies of v0.
    if (value.length <= 1) {
      return [new Array<unknown>(count).fill(value[0] ?? 0)];
    }
    // True spread input → tile the whole spread `count` times.
    const k = value.length;
    const out = new Array<unknown>(count * k);
    for (let i = 0; i < count; i++) {
      for (let j = 0; j < k; j++) {
        out[i * k + j] = value[j];
      }
    }
    return [out];
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
  label: 'RandomizeSpread',
  description:
    'Shuffle the input spread into a random order. Output length matches input; output type echoes input.',
  inputs: [
    // Polymorphic spread input — palette, vec2 grid, number range,
    // whatever. The shuffle is a seeded Fisher–Yates so the same
    // (seed, input length) pair always produces the same permutation.
    { name: 'value', type: 'any', kind: 'spread', default: 0 },
    { name: 'seed', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'out', type: 'any', kind: 'spread' }],
  // Output type mirrors whatever's wired into `value` — same pattern as
  // Repeat / Take / Skip / Slice. Falls back to 'any' when unwired so
  // the port colour stays neutral instead of guessing.
  dynamicOutputs: (_params, up) => [
    { name: 'out', type: up.value ?? 'any', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const value = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const seed = Number(inputs[1]?.[0] ?? 0) | 0;
    const n = value.length;
    if (n === 0) return [[]];
    // Seeded Fisher–Yates. hashUnit(i, 0, seed) ∈ [0,1) is stable for a
    // fixed (i, seed), so the permutation is deterministic across
    // frames — change `seed` to reroll.
    const out = value.slice() as unknown[];
    for (let i = n - 1; i > 0; i--) {
      const j = Math.min(i, Math.floor(hashUnit(i, 0, seed) * (i + 1)));
      const tmp = out[i];
      out[i] = out[j];
      out[j] = tmp;
    }
    return [out];
  },
});

// Suppress unused-type warning while we keep the Spread alias around for
// future generators (e.g. NoiseSpread).
void (null as unknown as Spread<number>);
