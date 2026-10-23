import { registerNode } from '../registry';

/**
 * Filtering & search — Sift (boolean-mask filter), Unique, IndexOf,
 * CountWhere, IndicesByValue. Equality for non-primitive types
 * (vec2 / color objects) falls back to a JSON-stringified key.
 */

function eqKey(v: unknown): string | number | boolean | null | undefined {
  if (v === null || v === undefined) return v;
  const t = typeof v;
  if (t === 'number' || t === 'string' || t === 'boolean') {
    return v as string | number | boolean;
  }
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}

export const siftNode = registerNode({
  type: 'spread/sift',
  category: 'Spread',
  label: 'SiftSpread',
  description: 'Keep elements where the parallel boolean spread is true.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'keep', type: 'boolean', kind: 'spread' },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [{ name: 'values', type: up.spread ?? 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const keep = (inputs[1] ?? []) as ReadonlyArray<unknown>;
    if (src.length === 0 || keep.length === 0) return [[]];
    const out: unknown[] = [];
    for (let i = 0; i < src.length; i++) {
      if (Boolean(keep[i % keep.length])) out.push(src[i]);
    }
    return [out];
  },
});

export const uniqueNode = registerNode({
  type: 'spread/unique',
  category: 'Spread',
  label: 'UniqueSpread',
  description: 'Remove duplicates, preserving first occurrence.',
  inputs: [{ name: 'spread', type: 'any', kind: 'spread' }],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [{ name: 'values', type: up.spread ?? 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const seen = new Set<string | number | boolean | null | undefined>();
    const out: unknown[] = [];
    for (const v of src) {
      const k = eqKey(v);
      if (seen.has(k)) continue;
      seen.add(k);
      out.push(v);
    }
    return [out];
  },
});

export const indexOfNode = registerNode({
  type: 'spread/index-of',
  category: 'Spread',
  label: 'IndexOfSpread',
  description: 'First index of a value in the spread, or -1.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'value', type: 'any', default: 0 },
  ],
  outputs: [{ name: 'index', label: 'index', type: 'number' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const needle = inputs[1]?.[0];
    const k = eqKey(needle);
    for (let i = 0; i < src.length; i++) {
      if (eqKey(src[i]) === k) return [[i]];
    }
    return [[-1]];
  },
});

export const countWhereNode = registerNode({
  type: 'spread/count-where',
  category: 'Spread',
  label: 'CountWhereSpread',
  description: 'Count true entries in a boolean spread.',
  inputs: [{ name: 'keep', type: 'boolean', kind: 'spread' }],
  outputs: [{ name: 'count', label: 'count', type: 'number' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    let n = 0;
    for (const v of src) if (Boolean(v)) n++;
    return [[n]];
  },
});

export const indicesByValueNode = registerNode({
  type: 'spread/indices-by-value',
  category: 'Spread',
  label: 'IndicesByValueSpread',
  description: 'All indices where the spread matches `value`.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'value', type: 'any', default: 0 },
  ],
  outputs: [{ name: 'indices', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const needle = inputs[1]?.[0];
    const k = eqKey(needle);
    const out: number[] = [];
    for (let i = 0; i < src.length; i++) {
      if (eqKey(src[i]) === k) out.push(i);
    }
    return [out];
  },
});
