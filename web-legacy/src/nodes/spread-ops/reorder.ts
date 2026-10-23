import { registerNode } from '../registry';

/**
 * Reordering — Rotate (cyclic shift), Permute (reorder by index spread),
 * SortByKey (sort one spread by a parallel number spread).
 */

const intOf = (v: unknown, fallback = 0): number => Math.floor(Number(v ?? fallback));

export const rotateNode = registerNode({
  type: 'spread/rotate',
  category: 'Spread',
  label: 'RotateSpread',
  description: 'Cyclic shift by N positions (negative shifts left).',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'shift', type: 'number', default: 1, integer: true },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [{ name: 'values', type: up.spread ?? 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const n = src.length;
    if (n === 0) return [[]];
    let s = intOf(inputs[1]?.[0], 1) % n;
    if (s < 0) s += n;
    if (s === 0) return [src.slice()];
    const out = new Array<unknown>(n);
    for (let i = 0; i < n; i++) out[i] = src[(i - s + n) % n];
    return [out];
  },
});

export const permuteNode = registerNode({
  type: 'spread/permute',
  category: 'Spread',
  label: 'PermuteSpread',
  description: 'Reorder elements by an index spread.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'indices', type: 'number', kind: 'spread' },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [{ name: 'values', type: up.spread ?? 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const idx = (inputs[1] ?? []) as ReadonlyArray<unknown>;
    if (src.length === 0 || idx.length === 0) return [[]];
    const n = src.length;
    const out: unknown[] = new Array(idx.length);
    for (let i = 0; i < idx.length; i++) {
      let k = Math.floor(Number(idx[i] ?? 0));
      if (k < 0) k = n + k;
      out[i] = k >= 0 && k < n ? src[k] : null;
    }
    return [out];
  },
});

export const sortByKeyNode = registerNode({
  type: 'spread/sort-by-key',
  category: 'Spread',
  label: 'SortByKeySpread',
  description: 'Sort a spread by a parallel number spread.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'keys', type: 'number', kind: 'spread' },
    { name: 'descending', type: 'boolean', default: false },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [{ name: 'values', type: up.spread ?? 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const keys = (inputs[1] ?? []) as ReadonlyArray<unknown>;
    const desc = Boolean(inputs[2]?.[0] ?? false);
    if (src.length === 0) return [[]];
    const n = src.length;
    const pairs = new Array<{ v: unknown; k: number }>(n);
    for (let i = 0; i < n; i++) {
      pairs[i] = { v: src[i], k: Number(keys[i] ?? 0) };
    }
    pairs.sort((a, b) => (desc ? b.k - a.k : a.k - b.k));
    return [pairs.map((p) => p.v)];
  },
});
