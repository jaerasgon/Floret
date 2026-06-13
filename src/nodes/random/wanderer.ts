import { registerNode } from '../registry';
import { perlin2D } from './noise';

/**
 * Wanderer2D — a smoothly drifting 2D position. Pure function of `t`,
 * so the patch stays deterministic and rewindable. Implementation is
 * the classic "noise wanderer": sample two distant points in the same
 * Perlin field for x and y.
 *
 *   x = (noise(t·speed,    0, seed) − ½) · 2 · area
 *   y = (noise(t·speed, 1000, seed) − ½) · 2 · area
 *
 * The y-offset of 1000 is large enough that x and y are uncorrelated
 * for normal sampling rates. Increase `speed` for faster drift; use the
 * seed pin to get different trajectories per particle.
 */
export const wanderer2DNode = registerNode({
  type: 'random/wanderer2d',
  category: 'Random',
  label: 'Wanderer2D',
  description: 'Drift a 2D point smoothly using Perlin noise.',
  inputs: [
    { name: 't', type: 'number', default: 0 },
    { name: 'speed', type: 'number', default: 1 },
    { name: 'area', type: 'number', default: 100 },
    { name: 'seed', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'position', type: 'vec2' }],
  compute: (t, speed, area, seed) => {
    const tv = (t as number) * (speed as number);
    const a = area as number;
    const s = (seed as number) | 0;
    const x = (perlin2D(tv, 0, s) - 0.5) * 2 * a;
    const y = (perlin2D(tv, 1000, s) - 0.5) * 2 * a;
    return [{ x, y }];
  },
});
