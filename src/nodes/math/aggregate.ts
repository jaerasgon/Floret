import { registerNode } from '../registry';

/**
 * Aggregations over a number spread → scalar (or derived spread for
 * Cumulative / Delta). All accept any-typed spread input, coerce each
 * element via Number(); non-numeric elements become NaN.
 */

const asNumbers = (v: unknown): ReadonlyArray<number> =>
  ((v ?? []) as ReadonlyArray<unknown>).map((x) => Number(x));

export const sumNode = registerNode({
  type: 'math/sum',
  category: 'Math',
  label: 'Sum',
  description: 'Sum of a number spread.',
  inputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
  outputs: [{ name: 'sum', type: 'number' }],
  computeRaw: (inputs) => {
    const src = asNumbers(inputs[0]);
    let s = 0;
    for (let i = 0; i < src.length; i++) s += src[i]!;
    return [[s]];
  },
});

export const meanNode = registerNode({
  type: 'math/mean',
  category: 'Math',
  label: 'Mean',
  description: 'Arithmetic mean of a number spread.',
  inputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
  outputs: [{ name: 'mean', type: 'number' }],
  computeRaw: (inputs) => {
    const src = asNumbers(inputs[0]);
    if (src.length === 0) return [[0]];
    let s = 0;
    for (let i = 0; i < src.length; i++) s += src[i]!;
    return [[s / src.length]];
  },
});

export const minSpreadNode = registerNode({
  type: 'math/min-spread',
  category: 'Math',
  label: 'MinSpread',
  description: 'Minimum value of a number spread.',
  inputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
  outputs: [{ name: 'min', type: 'number' }],
  computeRaw: (inputs) => {
    const src = asNumbers(inputs[0]);
    if (src.length === 0) return [[0]];
    let m = src[0]!;
    for (let i = 1; i < src.length; i++) if (src[i]! < m) m = src[i]!;
    return [[m]];
  },
});

export const maxSpreadNode = registerNode({
  type: 'math/max-spread',
  category: 'Math',
  label: 'MaxSpread',
  description: 'Maximum value of a number spread.',
  inputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
  outputs: [{ name: 'max', type: 'number' }],
  computeRaw: (inputs) => {
    const src = asNumbers(inputs[0]);
    if (src.length === 0) return [[0]];
    let m = src[0]!;
    for (let i = 1; i < src.length; i++) if (src[i]! > m) m = src[i]!;
    return [[m]];
  },
});

export const medianNode = registerNode({
  type: 'math/median',
  category: 'Math',
  label: 'Median',
  description: 'Median of a number spread.',
  inputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
  outputs: [{ name: 'median', type: 'number' }],
  computeRaw: (inputs) => {
    const src = asNumbers(inputs[0]);
    if (src.length === 0) return [[0]];
    const sorted = [...src].sort((a, b) => a - b);
    const mid = sorted.length >> 1;
    const m = sorted.length % 2 === 0 ? (sorted[mid - 1]! + sorted[mid]!) / 2 : sorted[mid]!;
    return [[m]];
  },
});

export const stdDevNode = registerNode({
  type: 'math/stddev',
  category: 'Math',
  label: 'StdDev',
  description: 'Population standard deviation of a number spread.',
  inputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
  outputs: [{ name: 'stddev', type: 'number' }],
  computeRaw: (inputs) => {
    const src = asNumbers(inputs[0]);
    if (src.length === 0) return [[0]];
    let s = 0;
    for (let i = 0; i < src.length; i++) s += src[i]!;
    const mean = s / src.length;
    let v = 0;
    for (let i = 0; i < src.length; i++) {
      const d = src[i]! - mean;
      v += d * d;
    }
    return [[Math.sqrt(v / src.length)]];
  },
});

export const cumulativeNode = registerNode({
  type: 'math/cumulative',
  category: 'Math',
  label: 'Cumulative',
  description: 'Running sum of a number spread.',
  inputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
  outputs: [{ name: 'sums', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = asNumbers(inputs[0]);
    const out = new Array<number>(src.length);
    let s = 0;
    for (let i = 0; i < src.length; i++) {
      s += src[i]!;
      out[i] = s;
    }
    return [out];
  },
});

export const deltaNode = registerNode({
  type: 'math/delta',
  category: 'Math',
  label: 'Delta',
  description: 'Pairwise differences of a number spread (length N − 1).',
  inputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
  outputs: [{ name: 'differences', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = asNumbers(inputs[0]);
    if (src.length < 2) return [[]];
    const out = new Array<number>(src.length - 1);
    for (let i = 1; i < src.length; i++) out[i - 1] = src[i]! - src[i - 1]!;
    return [out];
  },
});

export const histogramNode = registerNode({
  type: 'math/histogram',
  category: 'Math',
  label: 'Histogram',
  description: 'Bin counts of a number spread between `min` and `max`.',
  inputs: [
    { name: 'spread', type: 'number', kind: 'spread' },
    { name: 'bins', type: 'number', default: 10, integer: true },
    { name: 'min', type: 'number', default: 0 },
    { name: 'max', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'counts', type: 'number', kind: 'spread' }],
  computeRaw: (inputs) => {
    const src = asNumbers(inputs[0]);
    const bins = Math.max(1, Math.floor(Number(inputs[1]?.[0] ?? 10)));
    const min = Number(inputs[2]?.[0] ?? 0);
    const max = Number(inputs[3]?.[0] ?? 1);
    const counts = new Array<number>(bins).fill(0);
    if (src.length === 0 || max === min) return [counts];
    const span = max - min;
    for (let i = 0; i < src.length; i++) {
      const t = (src[i]! - min) / span;
      if (!Number.isFinite(t)) continue;
      let b = Math.floor(t * bins);
      if (b < 0) b = 0;
      else if (b >= bins) b = bins - 1;
      counts[b]!++;
    }
    return [counts];
  },
});
