import { registerNode } from '../registry';
import type { Collider, ParticleBuffer, World } from '../../runtime/particles';
import type { Vec2 } from '../../runtime/types';
import { COLLIDER_MODES, coerceMode } from './_shared';

/**
 * collider/circle — analytic circle collider. Each tick, after
 * integration, particles inside the circle are reflected (bounce),
 * killed, or zeroed in place (absorb). Inside/outside test is the
 * default; `flipNormals` inverts it (circle-as-arena: particles INSIDE
 * stay inside).
 */
export const colliderCircleNode = registerNode({
  type: 'collider/circle',
  category: 'Collider',
  label: 'Circle',
  description: 'Analytic circle collider. Cheap.',
  inputs: [
    { name: 'center',      type: 'vec2',    default: { x: 0, y: 0 } },
    { name: 'radius',      type: 'number',  default: 80 },
    { name: 'mode',        type: 'string',  default: 'bounce', param: 'mode', constant: true },
    { name: 'restitution', type: 'number',  default: 0.8 },
    { name: 'inverted',    type: 'boolean', default: false, param: 'inverted', constant: true },
  ],
  outputs: [{ name: 'collider', type: 'collider' }],
  defaultParams: { mode: 'bounce', inverted: false },
  paramOptions: { mode: [...COLLIDER_MODES] },
  computeRaw: (inputs) => {
    const center = (inputs[0]?.[0] as Vec2 | null) ?? { x: 0, y: 0 };
    const radius = Math.max(0, Number(inputs[1]?.[0] ?? 80));
    const mode = coerceMode(inputs[2]?.[0]);
    const restitution = Math.max(0, Math.min(1, Number(inputs[3]?.[0] ?? 0.8)));
    const inverted = Boolean(inputs[4]?.[0]);
    const cx = center.x, cy = center.y, r = radius, r2 = r * r;

    const apply = (buf: ParticleBuffer, _dt: number, _w: World): void => {
      const cap = buf.capacity;
      const alive = buf.alive;
      const px = buf.posX, py = buf.posY;
      const vx = buf.velX, vy = buf.velY;
      for (let i = 0; i < cap; i++) {
        if (!alive[i]) continue;
        const dx = (px[i] ?? 0) - cx;
        const dy = (py[i] ?? 0) - cy;
        const d2 = dx * dx + dy * dy;
        const hit = inverted ? d2 > r2 : d2 < r2;
        if (!hit) continue;
        if (mode === 'kill') { buf.kill(i); continue; }
        if (mode === 'absorb') { vx[i] = 0; vy[i] = 0; continue; }
        // Bounce: reflect across the surface normal.
        const dist = Math.sqrt(d2) || 1e-6;
        const nx = (inverted ? -dx : dx) / dist;
        const ny = (inverted ? -dy : dy) / dist;
        const vDotN = (vx[i] ?? 0) * nx + (vy[i] ?? 0) * ny;
        // Only reflect when moving INTO the surface.
        if (vDotN < 0) {
          vx[i] = ((vx[i] ?? 0) - 2 * vDotN * nx) * restitution;
          vy[i] = ((vy[i] ?? 0) - 2 * vDotN * ny) * restitution;
        }
        // Position correction: push the particle back to the surface.
        const overlap = inverted ? (dist - r) : (r - dist);
        px[i] = (px[i] ?? 0) + nx * overlap;
        py[i] = (py[i] ?? 0) + ny * overlap;
      }
    };
    const collider: Collider = { apply };
    return [[collider]];
  },
});
