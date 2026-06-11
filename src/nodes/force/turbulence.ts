import { registerNode } from '../registry';
import type { Force, ParticleBuffer, World } from '../../runtime/particles';
import { perlin2D } from '../random/noise';

/**
 * force/turbulence — sample a 2D Perlin field for each particle's
 * (x, y) and shove the particle along the curl-free gradient. Time
 * drifts the field so the turbulence breathes.
 *
 * `seed` is CONSTANT — changing it would shift the entire flow
 * mid-flight which is rarely useful and complicates the closure.
 *
 * Cost: ~5–10× a gravity force per particle (two Perlin samples per
 * particle per tick). At a few thousand particles this is still well
 * inside one tick's budget; mention it when you're at 10k+.
 */
export const forceTurbulenceNode = registerNode({
  type: 'force/turbulence',
  category: 'Force',
  label: 'Turbulence',
  description:
    'Perlin-noise vector field. Each particle gets a force from the local gradient; time drifts the field.',
  inputs: [
    { name: 'scale',    type: 'number', default: 0.01 },
    { name: 'speed',    type: 'number', default: 0.5 },
    { name: 'strength', type: 'number', default: 300 },
    { name: 'seed',     type: 'number', default: 0, integer: true, param: 'seed', constant: true },
  ],
  outputs: [{ name: 'force', type: 'force' }],
  defaultParams: { seed: 0 },
  computeRaw: (inputs) => {
    const scale = Number(inputs[0]?.[0] ?? 0.01);
    const speed = Number(inputs[1]?.[0] ?? 0.5);
    const strength = Number(inputs[2]?.[0] ?? 300);
    const seed = Math.floor(Number(inputs[3]?.[0] ?? 0));

    let t = 0;
    const force: Force = (buf: ParticleBuffer, dt: number, _w: World) => {
      t += dt * speed;
      const cap = buf.capacity;
      const alive = buf.alive;
      const px = buf.posX, py = buf.posY;
      const fx = buf.forceX, fy = buf.forceY;
      for (let i = 0; i < cap; i++) {
        if (!alive[i]) continue;
        const x = (px[i] ?? 0) * scale;
        const y = (py[i] ?? 0) * scale;
        // Sample two slightly-offset noise fields → vector. (Cheap
        // curl approximation.)
        const ax = (perlin2D(x, y + t, seed) - 0.5) * 2;
        const ay = (perlin2D(x + 50, y + t, seed + 1) - 0.5) * 2;
        fx[i] = (fx[i] ?? 0) + ax * strength;
        fy[i] = (fy[i] ?? 0) + ay * strength;
      }
    };
    return [[force]];
  },
});
