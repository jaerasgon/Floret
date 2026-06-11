import { registerNode } from '../registry';
import type { Collider, Force, Integrator, World } from '../../runtime/particles';
import type { Vec2 } from '../../runtime/types';

/**
 * physics/world — the root of any particle system. Collects global
 * forces / colliders (multi-input pins) into one World handle that
 * every downstream Emitter integrates against.
 *
 * The `integrator` choice is a CONSTANT pin — different integrators
 * have different per-particle state needs (Verlet wants prev-pos), so
 * switching mid-flight would corrupt the buffer. Set at compile time
 * only.
 */
export const physicsWorldNode = registerNode({
  type: 'physics/world',
  category: 'Particles',
  label: 'World',
  description:
    'Physics world: gravity, drag, time scale, global forces, global colliders. Wire into one or more Emitters.',
  inputs: [
    { name: 'gravity',    type: 'vec2',     default: { x: 0, y: 200 } },
    { name: 'drag',       type: 'number',   default: 0.05 },
    { name: 'timeScale',  type: 'number',   default: 1 },
    { name: 'bounds',     type: 'vec2',     default: null },
    { name: 'forces',     type: 'force',    kind: 'multi' },
    { name: 'colliders',  type: 'collider', kind: 'multi' },
    {
      name: 'integrator', type: 'string',
      default: 'euler', param: 'integrator', constant: true,
    },
  ],
  outputs: [{ name: 'world', type: 'world' }],
  defaultParams: { integrator: 'euler' as Integrator },
  paramOptions: { integrator: ['euler', 'verlet'] },
  computeRaw: (inputs) => {
    const gravity = (inputs[0]?.[0] as Vec2 | null) ?? { x: 0, y: 200 };
    const drag = Number(inputs[1]?.[0] ?? 0.05);
    const timeScale = Number(inputs[2]?.[0] ?? 1);
    const boundsIn = inputs[3]?.[0] as Vec2 | null | undefined;
    const forces = (inputs[4] ?? []) as ReadonlyArray<Force>;
    const colliders = (inputs[5] ?? []) as ReadonlyArray<Collider>;
    const integratorIn = inputs[6]?.[0];
    const integrator: Integrator = integratorIn === 'verlet' ? 'verlet' : 'euler';
    const world: World = {
      gravityX: gravity.x,
      gravityY: gravity.y,
      drag,
      timeScale,
      bounds: boundsIn && typeof boundsIn === 'object' && 'x' in boundsIn
        ? { w: boundsIn.x, h: boundsIn.y }
        : null,
      forces,
      colliders,
      integrator,
    };
    return [[world]];
  },
});
