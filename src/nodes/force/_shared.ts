/**
 * Shared helpers for force / collider nodes.
 *
 * The pattern: each `force/*` node returns a closure that mutates a
 * ParticleBuffer's force accumulators. Helpers here factor out the
 * boring boilerplate (coercion, default falloff, viz construction) so
 * each node file stays focused on its actual math.
 */

import type { Force, ForceViz, FalloffCurve } from '../../runtime/particles';
import type { ResolvedStyle, SceneNode } from '../../runtime/scene';
import { DEFAULT_STYLE } from '../../runtime/scene';

export const FALLOFF_CURVES: ReadonlyArray<FalloffCurve> = [
  'constant',
  'linear',
  'smooth',
  'inverse',
  'inverse-square',
];

export function coerceFalloff(v: unknown): FalloffCurve {
  switch (v) {
    case 'linear':         return 'linear';
    case 'smooth':         return 'smooth';
    case 'inverse':        return 'inverse';
    case 'inverse-square': return 'inverse-square';
    default:               return 'constant';
  }
}

/** Build a translucent-ring SceneNode for visualising a circular force
 *  region — drawn at (cx, cy) with the given radius. */
export function ringViz(
  cx: number, cy: number, radius: number,
  color: string,
): SceneNode {
  const style: ResolvedStyle = {
    ...DEFAULT_STYLE,
    fill: null,
    stroke: color,
    strokeWeight: 1.5,
  };
  return {
    kind: 'ellipse',
    x: cx, y: cy,
    w: radius * 2, h: radius * 2,
    mode: 'center',
    style,
  };
}

/** Cast an unknown to a `Force`. Used when collecting forces from
 *  multi-input lists where TS sees `unknown[]`. */
export function asForce(f: unknown): Force | null {
  return typeof f === 'function' ? (f as Force) : null;
}

/** Cast an unknown to a `ForceViz`. */
export function asForceViz(f: unknown): ForceViz | null {
  return typeof f === 'function' ? (f as ForceViz) : null;
}
