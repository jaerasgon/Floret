import { registerNode } from '../registry';
import {
  ParticleBuffer,
  ageAndCull,
  initSlot,
  integrateEuler,
  integrateVerlet,
  DEFAULT_WORLD,
  type Force,
  type World,
} from '../../runtime/particles';
import type { Color, PortValue, Vec2 } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * particles/emitter — the root of any particle system. Owns one
 * `ParticleBuffer` across frames; emits new particles at `rate`/sec,
 * integrates the world's forces (plus local forces) each tick, ages and
 * culls, then ships the buffer downstream as a length-1 spread on the
 * `particles` output.
 *
 * Constant pins (set once at compile):
 *   - `maxCount`     — buffer capacity. Reallocates on change.
 *   - `seed`         — initial-velocity RNG seed.
 *   - `overflowMode` — what to do when capacity is reached.
 *
 * Hot-path principles enforced here:
 *   - The `ParticleBuffer` instance lives in `runtimeState` across
 *     frames. No allocation in the per-tick path.
 *   - All math is in tight loops over typed arrays inside helpers
 *     (`integrateEuler`, `ageAndCull`); the Emitter itself just
 *     orchestrates.
 *   - We measure tick `dt` via a frame-time stash (frame ticks don't
 *     give us dt directly here) — fall back to 1/60 when missing.
 */

type OverflowMode = 'drop' | 'replace-oldest';

interface EmitterState {
  buffer: ParticleBuffer;
  /** Accumulator for sub-frame spawn timing — rate × dt summed across
   *  ticks; once it crosses 1.0 we spawn one and decrement. */
  spawnAcc: number;
  /** Stash of last frame's time, for dt calculation. */
  lastT: number;
  rng: () => number;
}

function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a |= 0; a = (a + 0x6D2B79F5) | 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export const particlesEmitterNode = registerNode({
  type: 'particles/emitter',
  category: 'Particles',
  label: 'Emitter',
  description:
    'Particle source. Owns a fixed-capacity buffer; integrates world forces + local forces each tick.',
  // Hidden state in `runtimeState.__emitter` — buffer + RNG + dt stash
  // all evolve across ticks. Must run every frame.
  effectful: true,
  inputs: [
    { name: 'world',       type: 'world',  default: null },
    { name: 'positions',   type: 'vec2',   kind: 'spread', default: { x: 0, y: 0 } },
    { name: 'rate',        type: 'number', default: 60 },
    { name: 'lifetime',    type: 'number', default: 2 },
    { name: 'initialVel',  type: 'vec2',   default: { x: 0, y: -100 } },
    { name: 'velSpread',   type: 'number', default: 0.2 },
    { name: 'color',       type: 'color',  default: { r: 1, g: 1, b: 1, a: 1 } },
    { name: 'size',        type: 'number', default: 4 },
    { name: 'maxCount',    type: 'number', default: 1000, integer: true, param: 'maxCount', constant: true },
    { name: 'overflowMode',type: 'string', default: 'drop', param: 'overflowMode', constant: true },
    { name: 'seed',        type: 'number', default: 1, integer: true, param: 'seed', constant: true },
    { name: 'localForces', type: 'force',  kind: 'spread' },
    // Wire `time/seconds` here so the integrator gets real dt. Without
    // it the Emitter falls back to 1/60 — fine for static FPS but
    // wrong if the user's frame rate drifts.
    { name: 'time',        type: 'number', default: 0 },
  ],
  outputs: [{ name: 'particles', type: 'particle' }],
  defaultParams: {
    maxCount: 1000,
    overflowMode: 'drop' as OverflowMode,
    seed: 1,
  },
  paramOptions: { overflowMode: ['drop', 'replace-oldest'] },
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    // Buffer + per-instance state live in runtimeState. Pulled here via
    // a backdoor: the runtime allocates `runtimeState.memo` on
    // memoisable nodes; we hijack a typed slot off it.
    const stateBag = (params as unknown as { __emitter?: EmitterState });
    let state = stateBag.__emitter;

    // Read constants — they're stashed on RuntimeNode.constants by the
    // compiler but reach us here as input slots like everything else.
    const maxCount = Math.max(1, Math.floor(Number(inputs[8]?.[0] ?? 1000)));
    const overflowMode: OverflowMode =
      inputs[9]?.[0] === 'replace-oldest' ? 'replace-oldest' : 'drop';
    const seed = Math.floor(Number(inputs[10]?.[0] ?? 1));

    // Lazy init / re-init on capacity change.
    if (!state || state.buffer.capacity !== maxCount) {
      state = {
        buffer: new ParticleBuffer(maxCount),
        spawnAcc: 0,
        lastT: 0,
        rng: mulberry32(seed),
      };
      stateBag.__emitter = state;
    }

    const buf = state.buffer;
    const world = (inputs[0]?.[0] as World | null) ?? DEFAULT_WORLD;
    const positions = (inputs[1] ?? []) as ReadonlyArray<Vec2 | null>;
    const rate = Math.max(0, Number(inputs[2]?.[0] ?? 0));
    const lifetime = Math.max(0.001, Number(inputs[3]?.[0] ?? 2));
    const initialVel = (inputs[4]?.[0] as Vec2 | null) ?? { x: 0, y: 0 };
    const velSpread = Math.max(0, Number(inputs[5]?.[0] ?? 0));
    const color = (inputs[6]?.[0] as Color | null) ?? { r: 1, g: 1, b: 1, a: 1 };
    const size = Math.max(0, Number(inputs[7]?.[0] ?? 4));
    const localForces = (inputs[11] ?? []) as ReadonlyArray<Force>;
    const tNow = Number(inputs[12]?.[0] ?? 0);

    // dt from real wall clock — compare to last tick's time stash. Fall
    // back to 1/60 on the very first tick (where we have no previous
    // sample) and clamp the upper bound so a tab pause doesn't dump a
    // multi-second `dt` into the integrator and shoot particles into
    // orbit.
    let dt = tNow > 0 && state.lastT > 0 ? tNow - state.lastT : 1 / 60;
    if (!Number.isFinite(dt) || dt <= 0) dt = 1 / 60;
    if (dt > 0.1) dt = 0.1;
    dt *= world.timeScale || 1;
    state.lastT = tNow;

    // 1. Spawn.
    state.spawnAcc += rate * dt;
    const spawnNow = Math.floor(state.spawnAcc);
    state.spawnAcc -= spawnNow;
    const validPositions = positions.filter(
      (p): p is Vec2 => p !== null && typeof p === 'object' && 'x' in p,
    );
    for (let n = 0; n < spawnNow; n++) {
      let slot = buf.spawn();
      if (slot === -1) {
        if (overflowMode === 'replace-oldest') {
          slot = buf.spawnReplacingOldest();
        } else {
          break; // drop the rest
        }
      }
      if (slot === -1) break;
      const p = validPositions.length > 0
        ? validPositions[Math.floor(state.rng() * validPositions.length)]!
        : { x: 0, y: 0 };
      // Initial velocity with jitter: vSpread is a fraction of |v|.
      const jitterX = (state.rng() * 2 - 1) * velSpread * 100;
      const jitterY = (state.rng() * 2 - 1) * velSpread * 100;
      initSlot(
        buf, slot,
        p.x, p.y,
        initialVel.x + jitterX,
        initialVel.y + jitterY,
        lifetime, size, color, 1,
      );
    }

    // 2 + 3. Forces — extracted into a callback so the Verlet
    // integrator can re-run them at the half-step. For Euler this is
    // called once; for Verlet twice.
    const applyAllForces = (): void => {
      buf.zeroForces();
      for (let i = 0; i < world.forces.length; i++) {
        world.forces[i]!(buf, dt, world);
      }
      for (let i = 0; i < localForces.length; i++) {
        localForces[i]!(buf, dt, world);
      }
    };
    applyAllForces();

    // 4. Integrate. Dispatch on world.integrator — World stores this as
    // a constant pin so the choice is stable across ticks.
    if (world.integrator === 'verlet') {
      integrateVerlet(buf, dt, world, applyAllForces);
    } else {
      integrateEuler(buf, dt, world);
    }

    // 5. Colliders (post-integration).
    for (let i = 0; i < world.colliders.length; i++) {
      world.colliders[i]!.apply(buf, dt, world);
    }

    // 6. Age + cull.
    ageAndCull(buf, dt);

    return [[buf]];
  },
});
