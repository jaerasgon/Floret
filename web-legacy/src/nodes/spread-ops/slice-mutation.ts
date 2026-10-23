import { registerNode } from '../registry';

/**
 * Slice-mutation ops — vvvv-style SetSlice / InsertSlice / RemoveSlice /
 * Swap. All polymorphic over element type via dynamicOutputs echoing the
 * `spread` input.
 *
 * Negative indices count from the end (-1 = last). Out-of-range writes
 * are clamped to [0, len].
 */

const intOf = (v: unknown, fallback = 0): number => Math.floor(Number(v ?? fallback));
const intCount = (v: unknown): number => Math.max(0, Math.floor(Number(v ?? 0)));

function resolveIndex(i: number, len: number, clampMax: number): number {
  let idx = i;
  if (idx < 0) idx = len + idx;
  if (idx < 0) idx = 0;
  if (idx > clampMax) idx = clampMax;
  return idx;
}

export const setSliceNode = registerNode({
  type: 'spread/set-slice',
  category: 'Spread',
  label: 'SetSliceSpread',
  description: 'Replace element(s) at an index with another spread.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'index', type: 'number', default: 0, integer: true },
    { name: 'replacement', type: 'any', kind: 'spread' },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [
    { name: 'values', type: up.spread ?? up.replacement ?? 'any', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const idx = resolveIndex(intOf(inputs[1]?.[0]), src.length, Math.max(0, src.length - 1));
    const rep = (inputs[2] ?? []) as ReadonlyArray<unknown>;
    if (src.length === 0) return [rep.slice()];
    if (rep.length === 0) return [src.slice()];
    const out = src.slice() as unknown[];
    out.splice(idx, 1, ...rep);
    return [out];
  },
});

export const insertSliceNode = registerNode({
  type: 'spread/insert-slice',
  category: 'Spread',
  label: 'InsertSliceSpread',
  description: 'Insert a spread at an index without overwriting.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'index', type: 'number', default: 0, integer: true },
    { name: 'insertion', type: 'any', kind: 'spread' },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [
    { name: 'values', type: up.spread ?? up.insertion ?? 'any', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const ins = (inputs[2] ?? []) as ReadonlyArray<unknown>;
    if (ins.length === 0) return [src.slice()];
    const idx = resolveIndex(intOf(inputs[1]?.[0]), src.length, src.length);
    const out = src.slice() as unknown[];
    out.splice(idx, 0, ...ins);
    return [out];
  },
});

export const removeSliceNode = registerNode({
  type: 'spread/remove-slice',
  category: 'Spread',
  label: 'RemoveSliceSpread',
  description: 'Delete `count` elements starting at `index`.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'index', type: 'number', default: 0, integer: true },
    { name: 'count', type: 'number', default: 1, integer: true },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [{ name: 'values', type: up.spread ?? 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    if (src.length === 0) return [[]];
    const idx = resolveIndex(intOf(inputs[1]?.[0]), src.length, Math.max(0, src.length - 1));
    const n = intCount(inputs[2]?.[0]);
    if (n === 0) return [src.slice()];
    const out = src.slice() as unknown[];
    out.splice(idx, n);
    return [out];
  },
});

export const swapNode = registerNode({
  type: 'spread/swap',
  category: 'Spread',
  label: 'SwapSpread',
  description: 'Swap two elements by index.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'i', type: 'number', default: 0, integer: true },
    { name: 'j', type: 'number', default: 1, integer: true },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [{ name: 'values', type: up.spread ?? 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    if (src.length < 2) return [src.slice()];
    const last = src.length - 1;
    const i = resolveIndex(intOf(inputs[1]?.[0]), src.length, last);
    const j = resolveIndex(intOf(inputs[2]?.[0], 1), src.length, last);
    if (i === j) return [src.slice()];
    const out = src.slice() as unknown[];
    const tmp = out[i];
    out[i] = out[j];
    out[j] = tmp;
    return [out];
  },
});
