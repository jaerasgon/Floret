import { describe, expect, it } from 'vitest';
import { curlNode, perlinNode, fbm2D, perlin2D } from '../noise';

const flow = (x: number, y: number, seed = 7) =>
  (curlNode.compute!({ x, y }, 1, seed, 1, 0.5)[0]) as { x: number; y: number };

describe('noise/curl', () => {
  it('returns a finite, deterministic vec2', () => {
    const a = flow(12.3, -4.5);
    const b = flow(12.3, -4.5);
    expect(a).toEqual(b);
    expect(Number.isFinite(a.x) && Number.isFinite(a.y)).toBe(true);
  });

  it('produces a non-trivial field (not all zero) and varies with seed', () => {
    const f1 = flow(3, 3, 1);
    const f2 = flow(3, 3, 2);
    expect(Math.hypot(f1.x, f1.y)).toBeGreaterThan(1e-4);
    expect(f1.x !== f2.x || f1.y !== f2.y).toBe(true);
  });

  it('is divergence-free: ∂flowx/∂x + ∂flowy/∂y ≈ 0', () => {
    const h = 0.02; // independent of the node's internal eps, so this is a real test
    let worst = 0;
    for (const [x, y] of [[0.3, 0.7], [5.1, -2.2], [-8.4, 3.9], [21.0, 17.5]]) {
      const div =
        (flow(x + h, y).x - flow(x - h, y).x) / (2 * h) +
        (flow(x, y + h).y - flow(x, y - h).y) / (2 * h);
      worst = Math.max(worst, Math.abs(div));
    }
    expect(worst).toBeLessThan(0.05);
  });

  it('scale maps world coords into noise space', () => {
    // pos·scale should match sampling the raw field at the scaled point.
    const raw = (curlNode.compute!({ x: 100, y: 50 }, 0.01, 7, 1, 0.5)[0]) as { x: number; y: number };
    const direct = flow(1.0, 0.5); // 100*0.01, 50*0.01
    expect(raw.x).toBeCloseTo(direct.x, 6);
    expect(raw.y).toBeCloseTo(direct.y, 6);
  });
});

describe('perlin / fbm (regression)', () => {
  it('perlin stays in [0,1]', () => {
    for (let i = 0; i < 50; i++) {
      const v = (perlinNode.compute!(i * 1.7, i * -0.9, 3, 1, 0.5)[0]) as number;
      expect(v).toBeGreaterThanOrEqual(0);
      expect(v).toBeLessThanOrEqual(1);
    }
  });
  it('fbm octaves=1 equals plain perlin', () => {
    expect(fbm2D(2.5, 1.5, 9, 1, 0.5)).toBe(perlin2D(2.5, 1.5, 9));
  });
});
