/**
 * Particle runtime — Struct-of-Arrays storage, force interface,
 * integrator, and shared falloff curves.
 *
 * The whole `particles/*` category is built on top of this file. The
 * design priorities, in order:
 *
 *   1. **Zero per-frame allocations in the hot path.** Forces, integrate,
 *      age, collide all mutate typed arrays in-place. No per-particle
 *      JS objects; no closures created per tick.
 *   2. **CPU "instancing" at render time.** ParticleBatch is one
 *      SceneNode that the renderer walks with a single tight loop over
 *      the buffer's typed arrays. We never materialise N Layer objects.
 *   3. **O(1) spawn and cull.** Free-slot linked list, no shifts, no
 *      compaction.
 *
 * See CONCEPTS.md → "Particle architecture" for the wider picture.
 */

import type { Color } from './types';
import type { ResolvedStyle, SceneNode } from './scene';

// ─── Falloff curves ─────────────────────────────────────────────────────

export type FalloffCurve =
  | 'constant'
  | 'linear'
  | 'smooth'
  | 'inverse'
  | 'inverse-square';

/**
 * Evaluate a falloff curve at distance ratio `t` (0 at centre,
 * 1 at the force's radius). Returns the strength multiplier in [0, 1].
 * Beyond t = 1 the force is zero. Inside t = 0 the force is at peak.
 */
export function falloffAt(curve: FalloffCurve, t: number): number {
  if (t <= 0) return 1;
  if (t >= 1) return 0;
  switch (curve) {
    case 'constant':       return 1;
    case 'linear':         return 1 - t;
    case 'smooth': {
      // Inverted smoothstep — soft edges at both centre and radius.
      const s = 1 - t;
      return s * s * (3 - 2 * s);
    }
    case 'inverse':        return 1 / (1 + 4 * t);
    case 'inverse-square': return 1 / (1 + 16 * t * t);
  }
}

// ─── World ──────────────────────────────────────────────────────────────

export type Integrator = 'euler' | 'verlet';

export interface World {
  gravityX: number;
  gravityY: number;
  /** Linear damping rate (per second). Per-tick multiplier is
   *  exp(-drag · dt) so the integrator is decoupled from frame rate. */
  drag: number;
  timeScale: number;
  bounds: { w: number; h: number } | null;
  forces: ReadonlyArray<Force>;
  colliders: ReadonlyArray<Collider>;
  integrator: Integrator;
}

export const DEFAULT_WORLD: World = {
  gravityX: 0,
  gravityY: 200,
  drag: 0.05,
  timeScale: 1,
  bounds: null,
  forces: [],
  colliders: [],
  integrator: 'euler',
};

// ─── Force interface ────────────────────────────────────────────────────

/**
 * A Force mutates the buffer's `forceX` / `forceY` accumulators in-place
 * for every alive particle. Implementations are free to skip dead slots
 * via the `alive` array.
 *
 * The contract is mutation, not return — closures may hold config
 * (strength, region SDF, noise seed) but must not allocate per frame.
 */
export type Force = (buf: ParticleBuffer, dt: number, world: World) => void;

/**
 * Optional visualisation hook for a force — returns a SceneNode that
 * draws the force's influence region. Lets force nodes expose a `viz`
 * output that the renderer can show alongside the particles.
 *
 * Returns `null` when there's nothing to show (e.g. a uniform gravity).
 */
export type ForceViz = () => SceneNode | null;

// ─── Collider interface ─────────────────────────────────────────────────

export type ColliderMode = 'bounce' | 'kill' | 'absorb';

/**
 * A Collider is applied AFTER integration each tick. It walks the live
 * particles and either reflects velocity (bounce), kills the particle
 * (kill), or zeros it in place (absorb). Same mutation contract as Force.
 *
 * Per-particle broad-phase rejection (e.g. AABB skip for polygons) lives
 * inside each collider's `apply` — colliders that need it precompute
 * their bounds locally at construction. We don't surface a `bbox` to the
 * outer interface because there's no scheduler above that would consult
 * it; analytic colliders are already cheap enough.
 */
export interface Collider {
  apply: (buf: ParticleBuffer, dt: number, world: World) => void;
  viz?: ForceViz;
}

// ─── ParticleBuffer (Struct-of-Arrays) ──────────────────────────────────

/**
 * Capacity-fixed pool of particles in SoA layout.
 *
 * `capacity` is set at construction (via the Emitter's `maxCount`
 * constant pin) and never grows. Dead slots are tracked in a free-slot
 * linked list so spawn and kill are both O(1) with no shifts.
 *
 * Per-particle state lives across multiple typed arrays — one array per
 * field. This costs a tiny bit of name-juggling at use sites but pays
 * for itself many times over: each per-field loop is cache-tight,
 * vectorisable, and avoids the megamorphic dispatch JIT pitfalls of
 * array-of-objects.
 */
export class ParticleBuffer {
  readonly capacity: number;
  /** Number of currently alive particles. Maintained by spawn / kill. */
  count: number;

  // Per-particle state.
  readonly posX: Float32Array;
  readonly posY: Float32Array;
  readonly velX: Float32Array;
  readonly velY: Float32Array;
  readonly age: Float32Array;
  readonly lifetime: Float32Array;
  readonly size: Float32Array;
  readonly rotation: Float32Array;
  readonly angVel: Float32Array;
  readonly mass: Float32Array;
  // Colour stored as 4 floats per particle in [0, 1]. Keeping them as
  // separate arrays (not a packed Uint32) makes the render loop simpler
  // and lets aging / colour-over-life mutate channels independently.
  readonly colorR: Float32Array;
  readonly colorG: Float32Array;
  readonly colorB: Float32Array;
  readonly colorA: Float32Array;
  readonly id: Uint32Array;
  /** 0 = dead, 1 = alive. */
  readonly alive: Uint8Array;
  // Force accumulators — zeroed at the start of each tick.
  readonly forceX: Float32Array;
  readonly forceY: Float32Array;

  // Free-slot linked list. `freeHead` points at the next slot a spawn
  // will consume, or -1 if full. `freeNext[i]` chains free slots.
  private freeHead: number;
  private readonly freeNext: Int32Array;
  private nextId: number;

  constructor(capacity: number) {
    if (!Number.isFinite(capacity) || capacity < 1) capacity = 1;
    capacity = Math.floor(capacity);
    this.capacity = capacity;
    this.count = 0;
    this.posX     = new Float32Array(capacity);
    this.posY     = new Float32Array(capacity);
    this.velX     = new Float32Array(capacity);
    this.velY     = new Float32Array(capacity);
    this.age      = new Float32Array(capacity);
    this.lifetime = new Float32Array(capacity);
    this.size     = new Float32Array(capacity);
    this.rotation = new Float32Array(capacity);
    this.angVel   = new Float32Array(capacity);
    this.mass     = new Float32Array(capacity);
    this.colorR   = new Float32Array(capacity);
    this.colorG   = new Float32Array(capacity);
    this.colorB   = new Float32Array(capacity);
    this.colorA   = new Float32Array(capacity);
    this.id       = new Uint32Array(capacity);
    this.alive    = new Uint8Array(capacity);
    this.forceX   = new Float32Array(capacity);
    this.forceY   = new Float32Array(capacity);
    this.freeNext = new Int32Array(capacity);
    for (let i = 0; i < capacity - 1; i++) this.freeNext[i] = i + 1;
    this.freeNext[capacity - 1] = -1;
    this.freeHead = 0;
    this.nextId = 1;
  }

  /** Pop a free slot and mark it alive. Returns the slot index or -1 if
   *  full. The caller is responsible for initialising the slot's fields
   *  (pos, vel, lifetime, size, color, mass). */
  spawn(): number {
    const i = this.freeHead;
    if (i === -1) return -1;
    this.freeHead = this.freeNext[i]!;
    this.alive[i] = 1;
    this.id[i] = this.nextId++;
    this.age[i] = 0;
    this.count++;
    return i;
  }

  /** Replace the oldest live particle (by id) and return its slot. Used
   *  when `overflowMode = 'replace-oldest'`. O(n) — caller decides when
   *  it's acceptable to pay this. */
  spawnReplacingOldest(): number {
    let oldestIdx = -1;
    let oldestId = Infinity;
    const cap = this.capacity;
    for (let i = 0; i < cap; i++) {
      if (this.alive[i] && this.id[i]! < oldestId) {
        oldestId = this.id[i]!;
        oldestIdx = i;
      }
    }
    if (oldestIdx === -1) return this.spawn();
    this.id[oldestIdx] = this.nextId++;
    this.age[oldestIdx] = 0;
    return oldestIdx;
  }

  /** Mark slot `i` dead and push it back onto the free list. Safe to
   *  call on already-dead slots; no-ops. */
  kill(i: number): void {
    if (!this.alive[i]) return;
    this.alive[i] = 0;
    this.freeNext[i] = this.freeHead;
    this.freeHead = i;
    this.count--;
  }

  /** Zero the per-particle force accumulators. Called once per tick by
   *  the Emitter, before walking the force list. */
  zeroForces(): void {
    this.forceX.fill(0);
    this.forceY.fill(0);
  }
}

// ─── Integration ────────────────────────────────────────────────────────

/**
 * Symplectic (semi-implicit) Euler — update velocity from current
 * forces, then update position from the new velocity. One pass over the
 * buffer; one force evaluation per tick.
 *
 * Stable enough for typical particle visuals. Use Verlet when you need
 * better long-term energy preservation (orbits, springs, dense
 * attractor fields). See `integrateVerlet` below.
 */
export function integrateEuler(
  buf: ParticleBuffer,
  dt: number,
  world: World,
): void {
  const dragFactor = Math.exp(-world.drag * dt);
  const gx = world.gravityX;
  const gy = world.gravityY;
  const cap = buf.capacity;
  const alive = buf.alive;
  const px = buf.posX, py = buf.posY;
  const vx = buf.velX, vy = buf.velY;
  const fx = buf.forceX, fy = buf.forceY;
  const m = buf.mass;
  for (let i = 0; i < cap; i++) {
    if (!alive[i]) continue;
    const mi = m[i] || 1;
    const invM = 1 / mi;
    const fxi = fx[i] ?? 0;
    const fyi = fy[i] ?? 0;
    let vxi = (vx[i] ?? 0) + (fxi * invM + gx) * dt;
    let vyi = (vy[i] ?? 0) + (fyi * invM + gy) * dt;
    vxi *= dragFactor;
    vyi *= dragFactor;
    vx[i] = vxi;
    vy[i] = vyi;
    px[i] = (px[i] ?? 0) + vxi * dt;
    py[i] = (py[i] ?? 0) + vyi * dt;
  }
}

/**
 * Velocity Verlet — second-order, energy-preserving integrator. Used
 * for systems where Euler's drift becomes visible (e.g. orbital
 * particles around an attractor, soft-body springs).
 *
 * Two-pass:
 *   1. Half-step v_½ = v + a · dt/2; then p = p + v_½ · dt.
 *   2. Re-apply forces (the caller's `applyForces` callback re-zeroes
 *      and re-walks the force list at the new positions).
 *   3. Complete v = v_½ + a_new · dt/2, then drag.
 *
 * Cost is ~2× Euler per tick (forces are evaluated twice). Worth it
 * when accuracy matters; Euler is the default for visual workloads.
 */
export function integrateVerlet(
  buf: ParticleBuffer,
  dt: number,
  world: World,
  applyForces: () => void,
): void {
  const halfDt = dt * 0.5;
  const gx = world.gravityX;
  const gy = world.gravityY;
  const cap = buf.capacity;
  const alive = buf.alive;
  const px = buf.posX, py = buf.posY;
  const vx = buf.velX, vy = buf.velY;
  const fx = buf.forceX, fy = buf.forceY;
  const m = buf.mass;

  // Phase 1: half-velocity → new position.
  for (let i = 0; i < cap; i++) {
    if (!alive[i]) continue;
    const invM = 1 / (m[i] || 1);
    const ax = (fx[i] ?? 0) * invM + gx;
    const ay = (fy[i] ?? 0) * invM + gy;
    const vhx = (vx[i] ?? 0) + ax * halfDt;
    const vhy = (vy[i] ?? 0) + ay * halfDt;
    vx[i] = vhx;
    vy[i] = vhy;
    px[i] = (px[i] ?? 0) + vhx * dt;
    py[i] = (py[i] ?? 0) + vhy * dt;
  }

  // Re-evaluate forces at the new positions.
  applyForces();

  // Phase 2: complete velocity → drag.
  const dragFactor = Math.exp(-world.drag * dt);
  for (let i = 0; i < cap; i++) {
    if (!alive[i]) continue;
    const invM = 1 / (m[i] || 1);
    const ax = (fx[i] ?? 0) * invM + gx;
    const ay = (fy[i] ?? 0) * invM + gy;
    vx[i] = ((vx[i] ?? 0) + ax * halfDt) * dragFactor;
    vy[i] = ((vy[i] ?? 0) + ay * halfDt) * dragFactor;
  }
}

// ─── Aging ──────────────────────────────────────────────────────────────

/**
 * Advance every alive particle's age and cull anything past its
 * lifetime. Returns the number culled this tick.
 */
export function ageAndCull(buf: ParticleBuffer, dt: number): number {
  let killed = 0;
  const cap = buf.capacity;
  for (let i = 0; i < cap; i++) {
    if (!buf.alive[i]) continue;
    const a = buf.age[i]! + dt;
    if (a >= buf.lifetime[i]!) {
      buf.kill(i);
      killed++;
    } else {
      buf.age[i] = a;
    }
  }
  return killed;
}

// ─── Spawn helpers ──────────────────────────────────────────────────────

/**
 * Initialise a freshly spawned slot. Pulled out into a single helper so
 * every spawn site (Emitter init, replace-oldest) writes the same field
 * set in the same order — avoiding "oh we forgot to set rotation"
 * regressions.
 */
export function initSlot(
  buf: ParticleBuffer,
  i: number,
  x: number, y: number,
  vx: number, vy: number,
  lifetime: number,
  size: number,
  color: Color,
  mass = 1,
  rotation = 0,
  angVel = 0,
): void {
  buf.posX[i] = x;       buf.posY[i] = y;
  buf.velX[i] = vx;      buf.velY[i] = vy;
  buf.lifetime[i] = lifetime;
  buf.size[i] = size;
  buf.colorR[i] = color.r;
  buf.colorG[i] = color.g;
  buf.colorB[i] = color.b;
  buf.colorA[i] = color.a;
  buf.mass[i] = mass;
  buf.rotation[i] = rotation;
  buf.angVel[i] = angVel;
}

// ─── SceneNode template (for ParticleBatch in scene.ts) ─────────────────

/**
 * How the renderer draws each live particle. The actual SceneNode
 * variant lives in `scene.ts` (`ParticleBatch.template`); the type is
 * defined here so both the runtime and the scene layer agree on the
 * shape.
 */
export type ParticleTemplate =
  | { kind: 'circle' }
  | { kind: 'rect' }
  | { kind: 'layer'; node: SceneNode };

// ─── Style helper ───────────────────────────────────────────────────────

/**
 * Some particle render-paths fall back to a flat style (stroke / no
 * fill, etc.) when per-particle colour is irrelevant. Centralised so
 * the renderer doesn't reach into scene.ts for it.
 */
export function pickParticleStyle(
  buf: ParticleBuffer,
  i: number,
  base: ResolvedStyle,
): ResolvedStyle {
  return {
    ...base,
    fill: `rgba(${(buf.colorR[i]! * 255) | 0},${(buf.colorG[i]! * 255) | 0},${(buf.colorB[i]! * 255) | 0},${buf.colorA[i]})`,
  };
}
