import { registerNode } from '../registry';
import { hashUnit } from './prng';

/**
 * Seeded pseudo-random number generators. Pure functions of `seed` —
 * wire `time/frame` in for per-frame randomness, a constant for a
 * stable value, or the iteration index inside a `region/foreach` for
 * one random per slot.
 */

export const uniformNode = registerNode({
  type: 'random/uniform',
  category: 'Random',
  label: 'Uniform',
  description: 'Sample a uniform random number in [min, max).',
  inputs: [
    { name: 'min', type: 'number', default: 0 },
    { name: 'max', type: 'number', default: 1 },
    { name: 'seed', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'sample', type: 'number' }],
  compute: (min, max, seed) => {
    const mn = min as number;
    const mx = max as number;
    const s = (seed as number) | 0;
    return [mn + hashUnit(s, 0, 0) * (mx - mn)];
  },
});

export const gaussianNode = registerNode({
  type: 'random/gaussian',
  category: 'Random',
  label: 'Gaussian',
  description: 'Sample a Gaussian random number with given mean and stddev.',
  inputs: [
    { name: 'mean', type: 'number', default: 0 },
    { name: 'sd', type: 'number', default: 1 },
    { name: 'seed', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'sample', type: 'number' }],
  compute: (mean, sd, seed) => {
    const m = mean as number;
    const s = sd as number;
    const sd0 = (seed as number) | 0;
    // Box–Muller — clamp u1 to avoid log(0).
    const u1 = Math.max(1e-10, hashUnit(sd0, 1, 0));
    const u2 = hashUnit(sd0, 2, 0);
    const z = Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2);
    return [m + s * z];
  },
});
