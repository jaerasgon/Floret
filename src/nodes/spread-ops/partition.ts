import { registerNode } from '../registry';

/**
 * Partitioning & windowing. Spreads are 1-D in Floret so we emit the
 * partition as a flat spread plus a parallel `groupSize` spread that
 * downstream ForEach + Take/Skip can use to consume groups.
 */

const intCount = (v: unknown): number => Math.max(0, Math.floor(Number(v ?? 0)));

export const partitionNode = registerNode({
  type: 'spread/partition',
  category: 'Spread',
  label: 'PartitionSpread',
  description: 'Chunk a spread into fixed-size groups. Outputs flat + groupSize.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'bin', type: 'number', default: 3, integer: true },
  ],
  outputs: [
    { name: 'flat', type: 'any', kind: 'spread' },
    { name: 'groupSize', type: 'number', kind: 'spread' },
  ],
  dynamicOutputs: (_p, up) => [
    { name: 'flat', type: up.spread ?? 'any', kind: 'spread' },
    { name: 'groupSize', type: 'number', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const bin = Math.max(1, intCount(inputs[1]?.[0]));
    if (src.length === 0) return [[], []];
    const sizes: number[] = [];
    let remaining = src.length;
    while (remaining > 0) {
      const s = Math.min(bin, remaining);
      sizes.push(s);
      remaining -= s;
    }
    return [src.slice(), sizes];
  },
});

export const pairwiseNode = registerNode({
  type: 'spread/pairwise',
  category: 'Spread',
  label: 'PairwiseSpread',
  description: 'Adjacent pairs: outputs (s[0..n-1]) and (s[1..n]).',
  inputs: [{ name: 'spread', type: 'any', kind: 'spread' }],
  outputs: [
    { name: 'a', type: 'any', kind: 'spread' },
    { name: 'b', type: 'any', kind: 'spread' },
  ],
  dynamicOutputs: (_p, up) => [
    { name: 'a', type: up.spread ?? 'any', kind: 'spread' },
    { name: 'b', type: up.spread ?? 'any', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    if (src.length < 2) return [[], []];
    return [src.slice(0, -1), src.slice(1)];
  },
});

export const atGroupNode = registerNode({
  type: 'spread/at-group',
  category: 'Spread',
  label: 'AtGroupSpread',
  description: 'Pick the Nth group as a sub-spread, given flat + groupSize.',
  inputs: [
    { name: 'flat', type: 'any', kind: 'spread' },
    { name: 'groupSize', type: 'number', kind: 'spread' },
    { name: 'index', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'values', type: 'any', kind: 'spread' }],
  dynamicOutputs: (_p, up) => [{ name: 'values', type: up.flat ?? 'any', kind: 'spread' }],
  computeRaw: (inputs) => {
    const flat = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const sizes = (inputs[1] ?? []) as ReadonlyArray<unknown>;
    if (flat.length === 0 || sizes.length === 0) return [[]];
    let idx = Math.floor(Number(inputs[2]?.[0] ?? 0));
    if (idx < 0) idx = sizes.length + idx;
    if (idx < 0 || idx >= sizes.length) return [[]];
    // Sum sizes[0..idx-1] for the start offset.
    let start = 0;
    for (let i = 0; i < idx; i++) start += Math.max(0, Math.floor(Number(sizes[i] ?? 0)));
    const len = Math.max(0, Math.floor(Number(sizes[idx] ?? 0)));
    return [flat.slice(start, start + len)];
  },
});

export const windowsNode = registerNode({
  type: 'spread/windows',
  category: 'Spread',
  label: 'WindowsSpread',
  description: 'Sliding windows of size N (stride 1). Outputs flat + windowSize.',
  inputs: [
    { name: 'spread', type: 'any', kind: 'spread' },
    { name: 'size', type: 'number', default: 3, integer: true },
  ],
  outputs: [
    { name: 'flat', type: 'any', kind: 'spread' },
    { name: 'windowSize', type: 'number', kind: 'spread' },
  ],
  dynamicOutputs: (_p, up) => [
    { name: 'flat', type: up.spread ?? 'any', kind: 'spread' },
    { name: 'windowSize', type: 'number', kind: 'spread' },
  ],
  computeRaw: (inputs) => {
    const src = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const size = Math.max(1, intCount(inputs[1]?.[0]));
    if (src.length === 0 || size > src.length) return [[], []];
    const numWin = src.length - size + 1;
    const flat = new Array<unknown>(numWin * size);
    const sizes = new Array<number>(numWin);
    for (let w = 0; w < numWin; w++) {
      sizes[w] = size;
      for (let i = 0; i < size; i++) flat[w * size + i] = src[w + i];
    }
    return [flat, sizes];
  },
});
