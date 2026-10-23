import type { ColliderMode } from '../../runtime/particles';

export const COLLIDER_MODES: ReadonlyArray<ColliderMode> = ['bounce', 'kill', 'absorb'];

export function coerceMode(v: unknown): ColliderMode {
  return v === 'kill' || v === 'absorb' ? v : 'bounce';
}
