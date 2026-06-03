import { registerNode } from '../registry';
import type { Vec2 } from '../../runtime/types';

/**
 * Random-walk one-step on a vec2 trail.
 *
 * `trail` is a spread of N vec2 points (oldest first). Each tick we
 * compute a new point = last + (cos·step, sin·step) where the angle
 * comes from a hash of (time, seed). The output trail drops the
 * oldest point and appends the new one, so length stays at N.
 *
 * Wired pattern: state/spread → walk-step → state/spread.next.
 *
 * Specialised because the equivalent done in the graph (at, math ops,
 * skip, stack, single-to-spread wrap) is six nodes deep and obscures
 * what the sample is showing.
 */
export const walkStepNode = registerNode({
  type: 'spread/walk-step',
  category: 'Spread',
  label: 'WalkStep',
  description: 'Advance a vec2 trail by one random-walk step.',
  inputs: [
    { name: 'trail', type: 'vec2', kind: 'spread' },
    { name: 'time', type: 'number', default: 0 },
    { name: 'step', type: 'number', default: 4 },
    { name: 'seed', type: 'number', default: 1, integer: true },
    { name: 'bounds', type: 'number', default: 250 },
  ],
  outputs: [{ name: 'trail', type: 'vec2', kind: 'spread' }],
  computeRaw: (inputs) => {
    const trail = (inputs[0] ?? []) as ReadonlyArray<Vec2>;
    const t = Number(inputs[1]?.[0] ?? 0);
    const step = Number(inputs[2]?.[0] ?? 4);
    const seed = Math.floor(Number(inputs[3]?.[0] ?? 1));
    const bounds = Math.max(1, Number(inputs[4]?.[0] ?? 250));
    if (trail.length === 0) return [[]];
    // A spread-typed wire can carry null/undefined entries (e.g. a
    // fresh state/read on tick zero before init has been gathered).
    // Walk back to the last real vec2; if none exists, seed at origin
    // so the trail can grow from there rather than crashing.
    let last: Vec2 = { x: 0, y: 0 };
    for (let i = trail.length - 1; i >= 0; i--) {
      const v = trail[i];
      if (v && typeof v.x === 'number' && typeof v.y === 'number') {
        last = v;
        break;
      }
    }
    // Hash (t, seed) → angle in [0, 2π).
    let h = (Math.floor(t * 60) * 0x9e3779b9 + seed * 0x85ebca6b) | 0;
    h = Math.imul(h ^ (h >>> 16), 0x85ebca6b);
    h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
    const angle = ((h ^ (h >>> 16)) >>> 0) / 0xffffffff * Math.PI * 2;
    let nx = last.x + Math.cos(angle) * step;
    let ny = last.y + Math.sin(angle) * step;
    // Reflect at bounds (centered at 250, 250 on a 500-canvas) so the
    // walker stays visible. `bounds` is the half-extent of the box.
    if (nx < 250 - bounds || nx > 250 + bounds) nx = last.x - Math.cos(angle) * step;
    if (ny < 250 - bounds || ny > 250 + bounds) ny = last.y - Math.sin(angle) * step;
    const next = trail.slice(1);
    next.push({ x: nx, y: ny });
    return [next];
  },
});
