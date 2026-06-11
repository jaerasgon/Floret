import { registerNode } from '../registry';
import type { Force, ForceViz, ParticleBuffer, World } from '../../runtime/particles';
import { falloffAt } from '../../runtime/particles';
import type { SceneNode } from '../../runtime/scene';
import type { Vec2 } from '../../runtime/types';
import { coerceFalloff, FALLOFF_CURVES, ringViz } from './_shared';

/**
 * force/repulsor — radial push AWAY from a point. Identical math to
 * Attractor but with strength sign flipped — kept as its own node so
 * authors can wire it semantically without thinking about signs.
 */
export const forceRepulsorNode = registerNode({
  type: 'force/repulsor',
  category: 'Force',
  label: 'Repulsor',
  description: 'Radial push away from a point.',
  inputs: [
    { name: 'center',   type: 'vec2',   default: { x: 0, y: 0 } },
    { name: 'strength', type: 'number', default: 200 },
    { name: 'radius',   type: 'number', default: 200 },
    { name: 'falloff',  type: 'string', default: 'inverse', param: 'falloff', constant: true },
    { name: 'showViz',  type: 'boolean',default: false, param: 'showViz', constant: true },
  ],
  outputs: [
    { name: 'force', type: 'force' },
    { name: 'viz',   type: 'layer' },
  ],
  defaultParams: { falloff: 'inverse', showViz: false },
  paramOptions: { falloff: [...FALLOFF_CURVES] },
  computeRaw: (inputs) => {
    const center = (inputs[0]?.[0] as Vec2 | null) ?? { x: 0, y: 0 };
    const strength = Number(inputs[1]?.[0] ?? 200);
    const radius = Math.max(0.01, Number(inputs[2]?.[0] ?? 200));
    const curve = coerceFalloff(inputs[3]?.[0]);
    const showViz = Boolean(inputs[4]?.[0]);
    const cx = center.x, cy = center.y, r = radius, invR = 1 / r;

    const force: Force = (buf: ParticleBuffer, _dt: number, _w: World) => {
      const cap = buf.capacity;
      const alive = buf.alive;
      const px = buf.posX, py = buf.posY;
      const fx = buf.forceX, fy = buf.forceY;
      for (let i = 0; i < cap; i++) {
        if (!alive[i]) continue;
        const dx = (px[i] ?? 0) - cx; // outward
        const dy = (py[i] ?? 0) - cy;
        const dist = Math.hypot(dx, dy);
        if (dist === 0 || dist > r) continue;
        const t = dist * invR;
        const mag = strength * falloffAt(curve, t);
        const invD = 1 / dist;
        fx[i] = (fx[i] ?? 0) + dx * invD * mag;
        fy[i] = (fy[i] ?? 0) + dy * invD * mag;
      }
    };
    const viz: ForceViz | null = showViz
      ? () => ringViz(cx, cy, r, '#ff7f7f')
      : null;
    const vizLayer: SceneNode = viz?.() ?? { kind: 'group', children: [] };
    return [[force], [vizLayer]];
  },
});
