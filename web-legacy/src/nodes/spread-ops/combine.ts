import { registerNode } from '../registry';
import type { PortType } from '../../runtime/types';

/**
 * Combiners — Interleave / Uninterleave / CrossProduct / Cons / Append /
 * Inject. These compose two or more spreads element-wise. `Stack` already
 * exists for end-to-end concatenation.
 */

const intCount = (v: unknown): number => Math.max(0, Math.floor(Number(v ?? 0)));

function mergeType(a: PortType | undefined, b: PortType | undefined): PortType {
  if (a && a !== 'any' && (!b || b === 'any' || b === a)) return a;
  if (b && b !== 'any' && (!a || a === 'any' || a === b)) return b;
  return a ?? b ?? 'any';
}

export const interleaveNode = registerNode({
  type: 'spread/interleave',
  category: 'Spread',
  label: 'InterleaveSpread',
  description: 'Interleave two spreads element-by-element.',
  inputs: [
    { name: 'a', type: 'any', kind: 'spread' },
    { name: 'b', type: 'any', kind: 'spread' },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [{ name: 'values', type: mergeType(up.a, up.b), kind: 'spread' }],
  computeRaw: (inputs) => {
    const a = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const b = (inputs[1] ?? []) as ReadonlyArray<unknown>;
    const n = Math.min(a.length, b.length);
    if (n === 0) return [[...a, ...b]];
    const out = new Array<unknown>(n * 2);
    for (let i = 0; i < n; i++) {
      out[i * 2] = a[i];
      out[i * 2 + 1] = b[i];
    }
    return [out];
  },
});

export const uninterleaveNode = registerNode({
  type: 'spread/uninterleave',
  category: 'Spread',
  label: 'UninterleaveSpread',
  description: 'Split a spread into even- and odd-indexed elements.',
  inputs: [{ name: 'spread', type: 'any', kind: 'spread' }],
  outputs: [
    { name: 'even', type: 'any', kind: 'spread' },
    { name: 'odd', type: 'any', kind: 'spread' },
  ],
  dynamicOutputs: (_p, up) => [
    { name: 'even', type: up.spread ?? 'any', kind: 'spread' },
    { name: 'odd', type: up.spread ?? 'any', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const even: unknown[] = [];
    const odd: unknown[] = [];
    for (let i = 0; i < src.length; i++) {
      if ((i & 1) === 0) even.push(src[i]);
      else odd.push(src[i]);
    }
    return [even, odd];
  },
});

export const crossProductNode = registerNode({
  type: 'spread/cross',
  category: 'Spread',
  label: 'CrossProductSpread',
  description: 'Cartesian product of two spreads as two parallel spreads.',
  inputs: [
    { name: 'a', type: 'any', kind: 'spread' },
    { name: 'b', type: 'any', kind: 'spread' },
  ],
  outputs: [
    { name: 'a', type: 'any', kind: 'spread' },
    { name: 'b', type: 'any', kind: 'spread' },
  ],
  dynamicOutputs: (_p, up) => [
    { name: 'a', type: up.a ?? 'any', kind: 'spread' },
    { name: 'b', type: up.b ?? 'any', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const a = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const b = (inputs[1] ?? []) as ReadonlyArray<unknown>;
    if (a.length === 0 || b.length === 0) return [[], []];
    const n = a.length * b.length;
    const outA = new Array<unknown>(n);
    const outB = new Array<unknown>(n);
    let k = 0;
    for (let i = 0; i < a.length; i++) {
      for (let j = 0; j < b.length; j++) {
        outA[k] = a[i];
        outB[k] = b[j];
        k++;
      }
    }
    return [outA, outB];
  },
});

export const consNode = registerNode({
  type: 'spread/cons',
  category: 'Spread',
  label: 'ConsSpread',
  description: 'Prepend a single value to a spread.',
  inputs: [
    { name: 'head', type: 'any', default: 0 },
    { name: 'spread', type: 'any', kind: 'spread' },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [
    { name: 'values', type: up.spread ?? up.head ?? 'any', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const head = inputs[0]?.[0];
    const rest = (inputs[1] ?? []) as ReadonlyArray<unknown>;
    return [[head, ...rest]];
  },
});

export const appendNode = registerNode({
  type: 'spread/append',
  category: 'Spread',
  label: 'AppendSpread',
  description: 'Append a single value to the end of a spread.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'tail', type: 'any', default: 0 },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [
    { name: 'values', type: up.spread ?? up.tail ?? 'any', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const tail = inputs[1]?.[0];
    return [[...src, tail]];
  },
});

export const injectNode = registerNode({
  type: 'spread/inject',
  category: 'Spread',
  label: 'InjectSpread',
  description: 'Inject elements of `marks` at N evenly-spaced positions in `spread`.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'marks', type: 'any', kind: 'spread' },
    { name: 'count', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [
    { name: 'values', type: mergeType(up.spread, up.marks), kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const marks = (inputs[1] ?? []) as ReadonlyArray<unknown>;
    const n = intCount(inputs[2]?.[0]);
    if (n === 0 || marks.length === 0) return [src.slice()];
    // Distribute n mark positions evenly across an output of length
    // `total = src.length + n`. Using floor((k + 0.5) * total / n)
    // avoids the rounding-collision problem when src.length is small or
    // n is close to src.length — adjacent marks only happen when n
    // mathematically forces it (n > total / 2).
    const total = src.length + n;
    const markAt = new Set<number>();
    for (let k = 0; k < n; k++) {
      let pos = Math.floor(((k + 0.5) * total) / n);
      // De-collide: if this slot is already taken, push forward.
      while (markAt.has(pos) && pos < total - 1) pos++;
      // Wrap back if we ran off the end (only possible with very dense marks).
      while (markAt.has(pos) && pos > 0) pos--;
      markAt.add(pos);
    }
    const out = new Array<unknown>(total);
    let si = 0;
    let mi = 0;
    for (let i = 0; i < total; i++) {
      if (markAt.has(i)) out[i] = marks[mi++ % marks.length];
      else out[i] = src[si++];
    }
    return [out];
  },
});
