import { registerNode } from '../registry';

/**
 * One-step of a 1D elementary cellular automaton (Wolfram rule).
 *
 * `row` is a flat boolean spread (length N). For each cell, look at
 * the (left, self, right) triplet — a 3-bit number 0..7 — and the
 * corresponding bit of `rule` (0..255) decides whether the cell is
 * alive next step. Edges wrap toroidally.
 *
 * Rule 30 = chaotic. Rule 90 = Sierpinski triangle (from a single
 * seed cell). Rule 110 = Turing-complete. Try them with `seed-cell`
 * for the classic triangle pattern.
 */
export const elementaryCAStepNode = registerNode({
  type: 'spread/elementary-ca',
  category: 'Spread',
  label: 'ElementaryCA',
  description: 'Advance a 1D elementary cellular automaton by one tick.',
  inputs: [
    { name: 'row', type: 'boolean', kind: 'spread' },
    { name: 'rule', type: 'number', default: 90, integer: true },
  ],
  outputs: [{ name: 'next', type: 'boolean', kind: 'spread' }],
  computeRaw: (inputs) => {
    const row = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const rule = Math.max(0, Math.min(255, Math.floor(Number(inputs[1]?.[0] ?? 90))));
    const n = row.length;
    if (n === 0) return [[]];
    const out = new Array<boolean>(n);
    for (let i = 0; i < n; i++) {
      const l = row[(i - 1 + n) % n] ? 1 : 0;
      const c = row[i] ? 1 : 0;
      const r = row[(i + 1) % n] ? 1 : 0;
      const pattern = (l << 2) | (c << 1) | r; // 0..7
      out[i] = ((rule >> pattern) & 1) === 1;
    }
    return [out];
  },
});

/**
 * Seed a 1D CA row with a single live cell in the middle (or at a
 * given index). Yields the classic single-seed start state for
 * pattern-generating rules like 90 and 30.
 */
export const caSeedNode = registerNode({
  type: 'spread/ca-seed',
  category: 'Spread',
  label: 'CASeed',
  description: 'Seed a 1D CA row with one live cell at `index`.',
  inputs: [
    { name: 'count', type: 'number', default: 64, integer: true },
    { name: 'index', type: 'number', default: -1, integer: true },
  ],
  outputs: [{ name: 'row', type: 'boolean', kind: 'spread' }],
  computeRaw: (inputs) => {
    const count = Math.max(1, Math.floor(Number(inputs[0]?.[0] ?? 64)));
    const idxIn = Math.floor(Number(inputs[1]?.[0] ?? -1));
    const idx = idxIn < 0 ? Math.floor(count / 2) : Math.min(count - 1, idxIn);
    const out = new Array<boolean>(count).fill(false);
    out[idx] = true;
    return [out];
  },
});

/**
 * Scroll a flat row-major 2D boolean grid up by one row, appending a
 * new row at the bottom. `grid` is length `cols * rows`; `newRow` is
 * length `cols`; the output is the same length as `grid`, with the
 * first row dropped and `newRow` placed at the last row.
 *
 * Used in the Wolfram Rule 90 sample to accumulate CA history into the
 * classic Sierpinski-style triangle visualisation.
 */
export const scrollUpNode = registerNode({
  type: 'spread/scroll-up',
  category: 'Spread',
  label: 'ScrollUp',
  description: 'Scroll a 2D boolean grid up one row and append newRow at the bottom.',
  inputs: [
    { name: 'grid', type: 'boolean', kind: 'spread' },
    { name: 'newRow', type: 'boolean', kind: 'spread' },
    { name: 'cols', type: 'number', default: 64, integer: true },
  ],
  outputs: [{ name: 'grid', type: 'boolean', kind: 'spread' }],
  computeRaw: (inputs) => {
    const grid = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const newRow = (inputs[1] ?? []) as ReadonlyArray<unknown>;
    const cols = Math.max(1, Math.floor(Number(inputs[2]?.[0] ?? 64)));
    if (grid.length === 0 || grid.length % cols !== 0) {
      // Inconsistent grid — emit empty so state.value resets next frame.
      return [[]];
    }
    const out = new Array<boolean>(grid.length);
    for (let i = 0; i < grid.length - cols; i++) out[i] = !!grid[i + cols];
    for (let i = 0; i < cols; i++) {
      out[grid.length - cols + i] = !!newRow[i];
    }
    return [out];
  },
});

/**
 * Get the last `cols` elements of a flat boolean grid — the "bottom row".
 * Used to feed the elementary-ca step from the accumulated history grid.
 */
export const lastRowNode = registerNode({
  type: 'spread/last-row',
  category: 'Spread',
  label: 'LastRow',
  description: 'Slice the bottom row from a 2D boolean grid.',
  inputs: [
    { name: 'grid', type: 'boolean', kind: 'spread' },
    { name: 'cols', type: 'number', default: 64, integer: true },
  ],
  outputs: [{ name: 'row', type: 'boolean', kind: 'spread' }],
  computeRaw: (inputs) => {
    const grid = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const cols = Math.max(1, Math.floor(Number(inputs[1]?.[0] ?? 64)));
    if (grid.length < cols) return [[]];
    const out = new Array<boolean>(cols);
    for (let i = 0; i < cols; i++) out[i] = !!grid[grid.length - cols + i];
    return [out];
  },
});

/**
 * Seed a flat 2D boolean grid: all dead except a single live cell at
 * `index` in the BOTTOM row (the row that gets stepped to produce the
 * next generation). Used to seed `spread/scroll-up`'s grid.
 */
export const caGridSeedNode = registerNode({
  type: 'spread/ca-grid-seed',
  category: 'Spread',
  label: 'CAGridSeed',
  description: 'Seed a 2D boolean grid with one live cell in the bottom row.',
  inputs: [
    { name: 'cols', type: 'number', default: 64, integer: true },
    { name: 'rows', type: 'number', default: 32, integer: true },
    { name: 'index', type: 'number', default: -1, integer: true },
  ],
  outputs: [{ name: 'grid', type: 'boolean', kind: 'spread' }],
  computeRaw: (inputs) => {
    const cols = Math.max(1, Math.floor(Number(inputs[0]?.[0] ?? 64)));
    const rows = Math.max(1, Math.floor(Number(inputs[1]?.[0] ?? 32)));
    const idxIn = Math.floor(Number(inputs[2]?.[0] ?? -1));
    const idx = idxIn < 0 ? Math.floor(cols / 2) : Math.min(cols - 1, idxIn);
    const out = new Array<boolean>(cols * rows).fill(false);
    out[(rows - 1) * cols + idx] = true;
    return [out];
  },
});
