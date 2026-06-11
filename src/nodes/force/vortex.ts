import { registerNode } from '../registry';
import type { Force, ForceViz, ParticleBuffer, World } from '../../runtime/particles';
import { falloffAt } from '../../runtime/particles';
import type { SceneNode } from '../../runtime/scene';
import type { Vec2 } from '../../runtime/types';
import { coerceFalloff, FALLOFF_CURVES, ringViz } from './_shared';

/**
 * force/vortex — rotational + optional radial pull around a centre.
 * `axisStrength` is the tangential push (positive = counter-clockwise);
 * `pull` is the radial pull (positive = inward, like a drain).
 */
export const forceVortexNode = registerNode({
  type: 'force/vortex',
  category: 'Force',
  label: 'Vortex',
  description:
    'Rotational force around a centre, with optional radial pull. Counter-clockwise when axisStrength > 0.',
  inputs: [
    { name: 'center',       type: 'vec2',   default: { x: 0, y: 0 } },
    { name: 'axisStrength', type: 'number', default: 200 },
    { name: 'pull',         type: 'number', default: 50 },
    { name: 'radius',       type: 'number', default: 200 },
    { name: 'falloff',      type: 'string', default: 'linear', param: 'falloff', constant: true },
    { name: 'showViz',      type: 'boolean',default: false, param: 'showViz', constant: true },
  ],
  outputs: [
    { name: 'force', type: 'force' },
    { name: 'viz',   type: 'layer' },
  ],
  defaultParams: { falloff: 'linear', showViz: false },
  paramOptions: { falloff: [...FALLOFF_CURVES] },
  computeRaw: (inputs) => {
    const center = (inputs[0]?.[0] as Vec2 | null) ?? { x: 0, y: 0 };
    const axis = Number(inputs[1]?.[0] ?? 200);
    const pull = Number(inputs[2]?.[0] ?? 50);
    const radius = Math.max(0.01, Number(inputs[3]?.[0] ?? 200));
    const curve = coerceFalloff(inputs[4]?.[0]);
    const showViz = Boolean(inputs[5]?.[0]);
    const cx = center.x, cy = center.y, r = radius, invR = 1 / r;

    const force: Force = (buf: ParticleBuffer, _dt: number, _w: World) => {
      const cap = buf.capacity;
      const alive = buf.alive;
      const px = buf.posX, py = buf.posY;
      const fx = buf.forceX, fy = buf.forceY;
      for (let i = 0; i < cap; i++) {
        if (!alive[i]) continue;
        const dx = (px[i] ?? 0) - cx;
        const dy = (py[i] ?? 0) - cy;
        const dist = Math.hypot(dx, dy);
        if (dist === 0 || dist > r) continue;
        const t = dist * invR;
        const k = falloffAt(curve, t);
        const invD = 1 / dist;
        // Tangent vector (perpendicular, rotated 90° CCW from radial).
        const tx = -dy * invD, ty = dx * invD;
        // Radial pull (inward = negative outward).
        fx[i] = (fx[i] ?? 0) + tx * axis * k - dx * invD * pull * k;
        fy[i] = (fy[i] ?? 0) + ty * axis * k - dy * invD * pull * k;
      }
    };
    const viz: ForceViz | null = showViz
      ? () => ringViz(cx, cy, r, '#bf7fff')
      : null;
    const vizLayer: SceneNode = viz?.() ?? { kind: 'group', children: [] };
    return [[force], [vizLayer]];
  },
});
