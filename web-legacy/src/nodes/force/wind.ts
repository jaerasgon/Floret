import { registerNode } from '../registry';
import type { Force, ParticleBuffer, World } from '../../runtime/particles';
import type { Vec2 } from '../../runtime/types';
import { perlin2D } from '../random/noise';

/**
 * force/wind — directional force with optional noise variation.
 * `noise` modulates the wind direction & magnitude per particle per
 * tick. Unlike gravity (which scales by mass for uniform acceleration),
 * wind applies a flat force — heavier particles get pushed less.
 */
export const forceWindNode = registerNode({
  type: 'force/wind',
  category: 'Force',
  label: 'Wind',
  description:
    'Directional force with optional noise variation. Mass-dependent (heavier = less push).',
  inputs: [
    { name: 'direction', type: 'vec2',   default: { x: 1, y: 0 } },
    { name: 'strength',  type: 'number', default: 150 },
    { name: 'noise',     type: 'number', default: 0.2 },
    { name: 'seed',      type: 'number', default: 0, integer: true, param: 'seed', constant: true },
  ],
  outputs: [{ name: 'force', type: 'force' }],
  defaultParams: { seed: 0 },
  computeRaw: (inputs) => {
    const dir = (inputs[0]?.[0] as Vec2 | null) ?? { x: 1, y: 0 };
    const strength = Number(inputs[1]?.[0] ?? 150);
    const noiseAmp = Number(inputs[2]?.[0] ?? 0.2);
    const seed = Math.floor(Number(inputs[3]?.[0] ?? 0));
    const len = Math.hypot(dir.x, dir.y) || 1;
    const dx = dir.x / len, dy = dir.y / len;

    let t = 0;
    const force: Force = (buf: ParticleBuffer, dt: number, _w: World) => {
      t += dt;
      const cap = buf.capacity;
      const alive = buf.alive;
      const px = buf.posX, py = buf.posY;
      const fx = buf.forceX, fy = buf.forceY;
      for (let i = 0; i < cap; i++) {
        if (!alive[i]) continue;
        // Per-particle noise modulation in [-noiseAmp, +noiseAmp].
        const n = noiseAmp === 0 ? 0
          : (perlin2D((px[i] ?? 0) * 0.01, (py[i] ?? 0) * 0.01 + t, seed) - 0.5) * 2 * noiseAmp;
        const mag = strength * (1 + n);
        fx[i] = (fx[i] ?? 0) + dx * mag;
        fy[i] = (fy[i] ?? 0) + dy * mag;
      }
    };
    return [[force]];
  },
});
