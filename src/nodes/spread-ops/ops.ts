import { registerNode } from '../registry';
import type { PortValue } from '../../runtime/types';
import { hash32 } from '../random/prng';

/**
 * Spread operators — they consume `Spread<number>` whole (kind: 'spread')
 * and produce derived spreads. All use `computeRaw` because the logic
 * needs to see the whole input array.
 */

const intCount = (v: unknown): number => Math.max(0, Math.floor(Number(v ?? 0)));

export const countNode = registerNode({
  type: 'spread/count',
  category: 'Spread',
  label: 'Count',
  description: 'Length of a spread.',
  inputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
  outputs: [{ name: 'out', type: 'number' }],
  computeRaw: (inputs) => [[(inputs[0] ?? []).length]],
});

export const reverseNode = registerNode({
  type: 'spread/reverse',
  category: 'Spread',
  label: 'Reverse',
  description: 'Reverse the order of elements.',
  inputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
  outputs: [{ name: 'out', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = inputs[0] ?? [];
    const out = new Array(src.length);
    for (let i = 0; i < src.length; i++) out[i] = src[src.length - 1 - i];
    return [out];
  },
});

// Spread "shape" ops (take / skip / slice / stack) are kept untyped via
// the `any` port type: their JS impl is just an array slice/concat and
// works for any element shape (number, vec2, boolean, layer, …). The
// editor's connection check passes `any → T` and `T → any` so users
// don't lose type info at the producer / consumer.

export const takeNode = registerNode({
  type: 'spread/take',
  category: 'Spread',
  label: 'Take',
  description: 'Keep the first N elements.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'count', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'out', type: 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = inputs[0] ?? [];
    const n = intCount(inputs[1]?.[0]);
    if (n === 0) return [[]];
    return [src.slice(0, n)];
  },
});

export const skipNode = registerNode({
  type: 'spread/skip',
  category: 'Spread',
  label: 'Skip',
  description: 'Drop the first N elements.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'count', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'out', type: 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = inputs[0] ?? [];
    const n = intCount(inputs[1]?.[0]);
    return [src.slice(n)];
  },
});

export const sliceNode = registerNode({
  type: 'spread/slice',
  category: 'Spread',
  label: 'Slice',
  description: 'Slice elements from start to end (end ≤ 0 counts from the end).',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'start', type: 'number', default: 0, integer: true },
    { name: 'end', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'out', type: 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = inputs[0] ?? [];
    const start = Math.floor(Number(inputs[1]?.[0] ?? 0));
    const endIn = Math.floor(Number(inputs[2]?.[0] ?? 0));
    const end = endIn === 0 ? src.length : endIn;
    return [src.slice(start, end < 0 ? src.length + end : end)];
  },
});

export const stackNode = registerNode({
  type: 'spread/stack',
  category: 'Spread',
  label: 'Stack',
  description: 'Concatenate two spreads end-to-end.',
  inputs: [
    { name: 'a', type: 'any', kind: 'spread' },
    { name: 'b', type: 'any', kind: 'spread' },
  ],
  outputs: [{ name: 'out', type: 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const a = inputs[0] ?? [];
    const b = inputs[1] ?? [];
    return [[...a, ...b]];
  },
});

/**
 * spread/at — pick a single element from a spread by integer index.
 * Negative indices count from the end (`-1` = last). Out-of-range
 * returns `null`. Output is `single`-kind so it can fan into any
 * non-spread input without going through a region.
 */
export const atNode = registerNode({
  type: 'spread/at',
  category: 'Spread',
  label: 'At',
  description: 'Pick one element from a spread by index.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'index', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'out', type: 'any', kind: 'single' }],
  computeRaw: (inputs) => {
    const src = inputs[0] ?? [];
    if (src.length === 0) return [[null]];
    let i = Math.floor(Number(inputs[1]?.[0] ?? 0));
    if (i < 0) i = src.length + i;
    if (i < 0 || i >= src.length) return [[null]];
    return [[src[i]]];
  },
});

export const resampleNode = registerNode({
  type: 'spread/resample',
  category: 'Spread',
  label: 'Resample',
  description: 'Resample a spread to a new length by linear interpolation.',
  inputs: [
    { name: 'spread', type: 'number', kind: 'spread' },
    { name: 'count', type: 'number', default: 8, integer: true },
  ],
  outputs: [{ name: 'out', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = inputs[0] ?? [];
    const count = intCount(inputs[1]?.[0]);
    if (count === 0 || src.length === 0) return [[]];
    if (src.length === 1) {
      return [new Array<number>(count).fill(Number(src[0] ?? 0))];
    }
    const out = new Array<number>(count);
    if (count === 1) {
      out[0] = Number(src[0] ?? 0);
      return [out];
    }
    for (let i = 0; i < count; i++) {
      const t = (i / (count - 1)) * (src.length - 1);
      const lo = Math.floor(t);
      const hi = Math.min(src.length - 1, lo + 1);
      const f = t - lo;
      out[i] = Number(src[lo] ?? 0) * (1 - f) + Number(src[hi] ?? 0) * f;
    }
    return [out];
  },
});

export const sortNode = registerNode({
  type: 'spread/sort',
  category: 'Spread',
  label: 'Sort',
  description: 'Sort numerically, ascending by default.',
  inputs: [
    { name: 'spread', type: 'number', kind: 'spread' },
    { name: 'descending', type: 'boolean', default: false },
  ],
  outputs: [{ name: 'out', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<number>;
    const desc = Boolean(inputs[1]?.[0] ?? false);
    const out = [...src];
    out.sort((a, b) => desc ? (b - a) : (a - b));
    return [out];
  },
});

export const shuffleNode = registerNode({
  type: 'spread/shuffle',
  category: 'Spread',
  label: 'Shuffle',
  description: 'Shuffle deterministically by seed.',
  inputs: [
    { name: 'spread', type: 'number', kind: 'spread' },
    { name: 'seed', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'out', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<number>;
    const seed = (Number(inputs[1]?.[0] ?? 0)) | 0;
    const out = [...src];
    for (let i = out.length - 1; i > 0; i--) {
      const j = hash32(i, 0, seed) % (i + 1);
      const tmp = out[i]!;
      out[i] = out[j]!;
      out[j] = tmp;
    }
    return [out];
  },
});

void (null as unknown as PortValue);
