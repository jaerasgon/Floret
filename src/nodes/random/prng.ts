/**
 * Deterministic 32-bit hash over (x, y, seed) integer triples.
 * Used by the noise and random nodes — same inputs always yield the
 * same output, so patches are reproducible across frames and reloads.
 */
export function hash32(x: number, y: number, seed: number): number {
  let h = Math.imul(x | 0, 2654435761);
  h = Math.imul(h ^ (y | 0), 2246822519);
  h = Math.imul(h ^ (seed | 0), 3266489917);
  h ^= h >>> 16;
  h = Math.imul(h, 668265263);
  h ^= h >>> 13;
  h = Math.imul(h, 374761393);
  h ^= h >>> 16;
  return h >>> 0;
}

/** Hash → [0, 1) float. */
export function hashUnit(x: number, y: number, seed: number): number {
  return hash32(x, y, seed) / 0x100000000;
}
