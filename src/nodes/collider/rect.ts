import { registerNode } from '../registry';
import type { Collider, ParticleBuffer, World } from '../../runtime/particles';
import type { Vec2 } from '../../runtime/types';
import { COLLIDER_MODES, coerceMode } from './_shared';

/**
 * collider/rect — axis-aligned rectangle collider. Cheap, branchless
 * inside/outside test; AABB bounce reflects on whichever axis is hit
 * first.
 */
export const colliderRectNode = registerNode({
  type: 'collider/rect',
  category: 'Collider',
  label: 'Rect',
  description: 'Axis-aligned rectangle collider.',
  inputs: [
    { name: 'center',      type: 'vec2',    default: { x: 0, y: 0 } },
    { name: 'size',        type: 'vec2',    default: { x: 200, y: 200 } },
    { name: 'mode',        type: 'string',  default: 'bounce', param: 'mode', constant: true },
    { name: 'restitution', type: 'number',  default: 0.8 },
    { name: 'inverted',    type: 'boolean', default: false, param: 'inverted', constant: true },
  ],
  outputs: [{ name: 'collider', type: 'collider' }],
  defaultParams: { mode: 'bounce', inverted: false },
  paramOptions: { mode: [...COLLIDER_MODES] },
  computeRaw: (inputs) => {
    const center = (inputs[0]?.[0] as Vec2 | null) ?? { x: 0, y: 0 };
    const size = (inputs[1]?.[0] as Vec2 | null) ?? { x: 200, y: 200 };
    const mode = coerceMode(inputs[2]?.[0]);
    const restitution = Math.max(0, Math.min(1, Number(inputs[3]?.[0] ?? 0.8)));
    const inverted = Boolean(inputs[4]?.[0]);
    const hw = Math.abs(size.x) * 0.5;
    const hh = Math.abs(size.y) * 0.5;
    const minX = center.x - hw, maxX = center.x + hw;
    const minY = center.y - hh, maxY = center.y + hh;

    const apply = (buf: ParticleBuffer, _dt: number, _w: World): void => {
      const cap = buf.capacity;
      const alive = buf.alive;
      const px = buf.posX, py = buf.posY;
      const vx = buf.velX, vy = buf.velY;
      for (let i = 0; i < cap; i++) {
        if (!alive[i]) continue;
        const x = px[i] ?? 0, y = py[i] ?? 0;
        const inside = x > minX && x < maxX && y > minY && y < maxY;
        const hit = inverted ? !inside : inside;
        if (!hit) continue;
        if (mode === 'kill') { buf.kill(i); continue; }
        if (mode === 'absorb') { vx[i] = 0; vy[i] = 0; continue; }
        // Bounce on the nearer edge.
        const dxL = x - minX, dxR = maxX - x;
        const dyT = y - minY, dyB = maxY - y;
        const minDx = Math.min(dxL, dxR);
        const minDy = Math.min(dyT, dyB);
        if (minDx < minDy) {
          vx[i] = -((vx[i] ?? 0) * restitution);
          px[i] = dxL < dxR ? minX - 0.1 : maxX + 0.1;
        } else {
          vy[i] = -((vy[i] ?? 0) * restitution);
          py[i] = dyT < dyB ? minY - 0.1 : maxY + 0.1;
        }
      }
    };
    const collider: Collider = { apply };
    return [[collider]];
  },
});
