import { registerNode } from '../registry';
import type { Force, ParticleBuffer, World } from '../../runtime/particles';

/**
 * force/drag — proportional velocity damping. Adds a force opposite to
 * each particle's velocity scaled by `strength`. Distinct from the
 * world's built-in `drag` (which damps multiplicatively, framerate-
 * independent) because Drag-as-force composes through the Force
 * pipeline — you can scope it to a single emitter or modulate it
 * with a slider.
 */
export const forceDragNode = registerNode({
  type: 'force/drag',
  category: 'Force',
  label: 'Drag',
  description: 'Velocity-proportional damping, applied per-particle.',
  inputs: [
    { name: 'strength', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'force', type: 'force' }],
  compute: (strength) => {
    const k = Number(strength as number);
    const force: Force = (buf: ParticleBuffer, _dt: number, _w: World) => {
      const cap = buf.capacity;
      const alive = buf.alive;
      const vx = buf.velX, vy = buf.velY;
      const fx = buf.forceX, fy = buf.forceY;
      for (let i = 0; i < cap; i++) {
        if (!alive[i]) continue;
        fx[i] = (fx[i] ?? 0) - (vx[i] ?? 0) * k;
        fy[i] = (fy[i] ?? 0) - (vy[i] ?? 0) * k;
      }
    };
    return [force];
  },
});
