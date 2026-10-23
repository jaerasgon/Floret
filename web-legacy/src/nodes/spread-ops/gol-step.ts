import { registerNode } from '../registry';

/**
 * Game-of-Life one-step on a flat boolean grid.
 *
 * `grid` is a row-major spread of booleans of length `cols * rows`.
 * The output is the next generation under Conway's rules — toroidal
 * wraparound at the edges.
 *
 * Why specialised? Doing this in the node graph would need 8 spread/at
 * lookups + arithmetic per cell, inside a ForEach over the whole grid
 * — hundreds of inner nodes. This is just a wrapper around a few
 * lines of array math, so the sample stays readable.
 */
export const golStepNode = registerNode({
  type: 'spread/gol-step',
  category: 'Spread',
  label: 'GoLStepSpread',
  description: "Advance Conway's Game of Life by one tick.",
  inputs: [
    { name: 'grid', type: 'boolean', kind: 'spread' },
    { name: 'cols', type: 'number', default: 16, integer: true },
    { name: 'rows', type: 'number', default: 16, integer: true },
  ],
  outputs: [{ name: 'next', type: 'boolean', kind: 'spread' }],
  computeRaw: (inputs) => {
    const grid = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const cols = Math.max(1, Math.floor(Number(inputs[1]?.[0] ?? 16)));
    const rows = Math.max(1, Math.floor(Number(inputs[2]?.[0] ?? 16)));
    const expected = cols * rows;
    if (grid.length !== expected) {
      // Grid size mismatch — emit an empty next so the next frame's
      // state.value rebases on init. A common cause is a slider that
      // changed `cols` after the patch was loaded.
      return [[]];
    }
    const next = new Array<boolean>(expected);
    for (let y = 0; y < rows; y++) {
      for (let x = 0; x < cols; x++) {
        let live = 0;
        for (let dy = -1; dy <= 1; dy++) {
          for (let dx = -1; dx <= 1; dx++) {
            if (dx === 0 && dy === 0) continue;
            const nx = (x + dx + cols) % cols;
            const ny = (y + dy + rows) % rows;
            if (grid[ny * cols + nx]) live++;
          }
        }
        const cur = !!grid[y * cols + x];
        next[y * cols + x] = cur ? live === 2 || live === 3 : live === 3;
      }
    }
    return [next];
  },
});

/**
 * Seed a Game-of-Life grid with random live cells.
 *
 * `density` (0..1) sets the probability of each cell starting alive;
 * `seed` makes it deterministic so patch reload gives the same start.
 * Output is a row-major boolean spread length cols*rows — drop it
 * straight into state/spread.init.
 */
export const golSeedNode = registerNode({
  type: 'spread/gol-seed',
  category: 'Spread',
  label: 'GoLSeedSpread',
  description: 'Seed a Game-of-Life grid with random live cells.',
  inputs: [
    { name: 'cols', type: 'number', default: 16, integer: true },
    { name: 'rows', type: 'number', default: 16, integer: true },
    { name: 'density', type: 'number', default: 0.35 },
    { name: 'seed', type: 'number', default: 1, integer: true },
  ],
  outputs: [{ name: 'grid', type: 'boolean', kind: 'spread' }],
  computeRaw: (inputs) => {
    const cols = Math.max(1, Math.floor(Number(inputs[0]?.[0] ?? 16)));
    const rows = Math.max(1, Math.floor(Number(inputs[1]?.[0] ?? 16)));
    const density = Math.max(0, Math.min(1, Number(inputs[2]?.[0] ?? 0.35)));
    let s = (Number(inputs[3]?.[0] ?? 1) | 0) || 1;
    const out = new Array<boolean>(cols * rows);
    // Mulberry32 — same shape as the rest of the codebase's PRNGs.
    for (let i = 0; i < out.length; i++) {
      s = (s + 0x6d2b79f5) | 0;
      let t = s;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      const r = (((t ^ (t >>> 14)) >>> 0) % 1_000_000) / 1_000_000;
      out[i] = r < density;
    }
    return [out];
  },
});
