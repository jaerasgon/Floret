import { registerNode } from '../registry';
import { hash32 } from './prng';

/**
 * 2D Perlin noise — classic Ken Perlin "improved" gradient noise.
 * Returns values in [0, 1] (theoretical bounds; rarely hits extremes).
 * Use `octaves` > 1 for fractal Brownian motion; `falloff` is the
 * amplitude multiplier between successive octaves (0.5 = standard fBm).
 */

const GRADIENTS_2D: ReadonlyArray<readonly [number, number]> = [
  [1, 0], [-1, 0], [0, 1], [0, -1],
  [1, 1], [-1, 1], [1, -1], [-1, -1],
];

function fade(t: number): number {
  return t * t * t * (t * (t * 6 - 15) + 10);
}

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

function gradDot(ix: number, iy: number, fx: number, fy: number, seed: number): number {
  const g = GRADIENTS_2D[hash32(ix, iy, seed) & 7]!;
  return g[0] * fx + g[1] * fy;
}

export function perlin2D(x: number, y: number, seed: number): number {
  const ix = Math.floor(x);
  const iy = Math.floor(y);
  const fx = x - ix;
  const fy = y - iy;
  const u = fade(fx);
  const v = fade(fy);
  const n00 = gradDot(ix, iy, fx, fy, seed);
  const n10 = gradDot(ix + 1, iy, fx - 1, fy, seed);
  const n01 = gradDot(ix, iy + 1, fx, fy - 1, seed);
  const n11 = gradDot(ix + 1, iy + 1, fx - 1, fy - 1, seed);
  const nx0 = lerp(n00, n10, u);
  const nx1 = lerp(n01, n11, u);
  // raw range ≈ ±sqrt(2)/2; map to [0, 1]
  return lerp(nx0, nx1, v) * Math.SQRT1_2 + 0.5;
}

export const perlinNode = registerNode({
  type: 'noise/perlin',
  category: 'Random',
  label: 'Perlin',
  description: 'Sample 2D Perlin noise in [0, 1].',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
    { name: 'seed', type: 'number', default: 0, integer: true },
    { name: 'octaves', type: 'number', default: 1, integer: true },
    { name: 'falloff', type: 'number', default: 0.5 },
  ],
  outputs: [{ name: 'out', type: 'number' }],
  compute: (x, y, seed, octaves, falloff) => {
    const xv = x as number;
    const yv = y as number;
    const sv = seed as number;
    const oct = Math.max(1, Math.floor(octaves as number));
    const fa = falloff as number;
    if (oct === 1) return [perlin2D(xv, yv, sv)];
    let sum = 0;
    let amp = 1;
    let total = 0;
    let freq = 1;
    for (let i = 0; i < oct; i++) {
      sum += perlin2D(xv * freq, yv * freq, sv + i) * amp;
      total += amp;
      amp *= fa;
      freq *= 2;
    }
    return [total > 0 ? sum / total : 0];
  },
});
