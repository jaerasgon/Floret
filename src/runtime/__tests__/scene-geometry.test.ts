import { describe, expect, it } from 'vitest';
import { resolveBox, catmullRomBeziers } from '../scene';

describe('resolveBox', () => {
  it('corner mode is identity', () => {
    expect(resolveBox(10, 20, 30, 40, 'corner')).toEqual([10, 20, 30, 40]);
  });
  it('center mode offsets by half', () => {
    expect(resolveBox(100, 100, 40, 20, 'center')).toEqual([80, 90, 40, 20]);
  });
  it('corners mode spans two points', () => {
    expect(resolveBox(10, 10, 40, 30, 'corners')).toEqual([10, 10, 30, 20]);
  });
  it('radius mode doubles the extents', () => {
    expect(resolveBox(50, 50, 10, 5, 'radius')).toEqual([40, 45, 20, 10]);
  });
  it('unknown mode falls back to corner', () => {
    expect(resolveBox(1, 2, 3, 4, 'whatever')).toEqual([1, 2, 3, 4]);
  });
});

describe('catmullRomBeziers', () => {
  const pts = [
    { x: 0, y: 0 }, { x: 10, y: 10 }, { x: 20, y: 0 }, { x: 30, y: 10 },
  ];

  it('open spline yields n-1 segments ending at each subsequent point', () => {
    const segs = catmullRomBeziers(pts, false, 1);
    expect(segs).toHaveLength(3);
    expect(segs[0]!.ex).toBe(10);
    expect(segs[0]!.ey).toBe(10);
    expect(segs[2]!.ex).toBe(30);
  });

  it('closed spline yields n segments wrapping back to the start', () => {
    const segs = catmullRomBeziers(pts, true, 1);
    expect(segs).toHaveLength(4);
    expect(segs[3]!.ex).toBe(0); // last segment closes onto points[0]
    expect(segs[3]!.ey).toBe(0);
  });

  it('control points use the tension/6 normalisation', () => {
    // First open segment: p0 mirrors p1 (index 0), so c1 = p1 + (p2 - p0)/6.
    // p1=(0,0), p2=(10,10), p0=(0,0) → c1 = (10/6, 10/6).
    const [seg] = catmullRomBeziers(pts, false, 1);
    expect(seg!.c1x).toBeCloseTo(10 / 6, 6);
    expect(seg!.c1y).toBeCloseTo(10 / 6, 6);
  });

  it('tension 0 flattens control points onto the segment ends', () => {
    const [seg] = catmullRomBeziers(pts, false, 0);
    expect(seg!.c1x).toBe(0);
    expect(seg!.c1y).toBe(0);
  });
});
