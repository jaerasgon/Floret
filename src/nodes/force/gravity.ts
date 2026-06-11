import { registerNode } from '../registry';
import type { Force, ParticleBuffer, World } from '../../runtime/particles';
import type { Vec2 } from '../../runtime/types';

/**
 * force/gravity — uniform vector force, applied to every alive
 * particle, scaled by mass (so heavier particles get the same
 * acceleration as lighter ones, like real gravity). Useful for adding
 * a *secondary* gravity to a single emitter on top of the world's
 * default gravity.
 *
 * Note: `physics/world` already exposes a gravity vector. This node is
 * for cases where you want gravity wired in dynamically (e.g.
 * mouse-driven) or where you want different emitters to feel different
 * gravities through their local `forces` input.
 */
export const forceGravityNode = registerNode({
  type: 'force/gravity',
  category: 'Force',
  label: 'Gravity',
  description: 'Uniform vector force (acts like gravity).',
  inputs: [
    { name: 'direction', type: 'vec2',   default: { x: 0, y: 1 } },
    { name: 'strength',  type: 'number', default: 200 },
  ],
  outputs: [{ name: 'force', type: 'force' }],
  compute: (direction, strength) => {
    const d = (direction as Vec2 | null) ?? { x: 0, y: 1 };
    const s = Number(strength as number);
    // Normalise direction once; the resulting force is `dir · strength`
    // applied per particle, scaled by mass (so a=F/m gives uniform a).
    const len = Math.hypot(d.x, d.y) || 1;
    const ax = (d.x / len) * s;
    const ay = (d.y / len) * s;
    const force: Force = (buf: ParticleBuffer, _dt: number, _w: World) => {
      const cap = buf.capacity;
      const alive = buf.alive;
      const fx = buf.forceX, fy = buf.forceY;
      const m = buf.mass;
      for (let i = 0; i < cap; i++) {
        if (!alive[i]) continue;
        const mi = m[i] || 1;
        fx[i] = (fx[i] ?? 0) + ax * mi;
        fy[i] = (fy[i] ?? 0) + ay * mi;
      }
    };
    return [force];
  },
});
