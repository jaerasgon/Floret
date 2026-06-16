import { describe, expect, it } from 'vitest';
import { wrapNode, smoothstepNode, stepNode } from '../math/map';
import { signNode } from '../math/unary';
import { toPolarNode, reflectNode, projectNode } from '../vec2/vec2';
import { compileGraph } from '../../runtime/compile';
import { tick } from '../../runtime/tick';
import { nodeRegistry } from '../registry';
import '../index';

const call = (def: { compute?: (...a: unknown[]) => unknown[] }, ...args: unknown[]) => def.compute!(...args);

describe('math helpers', () => {
  it('wrap folds into [min, max)', () => {
    expect(call(wrapNode, 7, 0, 5)).toEqual([2]);
    expect(call(wrapNode, -1, 0, 5)).toEqual([4]);
    expect(call(wrapNode, 3, 0, 5)).toEqual([3]);
    expect(call(wrapNode, 5, 0, 5)).toEqual([0]); // upper bound wraps to min
    expect(call(wrapNode, 0, 0, 0)).toEqual([0]); // degenerate range
  });

  it('step is a hard threshold', () => {
    expect(call(stepNode, 0.5, 0.4)).toEqual([0]);
    expect(call(stepNode, 0.5, 0.5)).toEqual([1]);
    expect(call(stepNode, 0.5, 0.9)).toEqual([1]);
  });

  it('smoothstep is clamped Hermite', () => {
    expect(call(smoothstepNode, 0, 1, -1)).toEqual([0]);
    expect(call(smoothstepNode, 0, 1, 2)).toEqual([1]);
    expect((call(smoothstepNode, 0, 1, 0.5)[0] as number)).toBeCloseTo(0.5, 6);
    expect((call(smoothstepNode, 0, 1, 0.25)[0] as number)).toBeCloseTo(0.15625, 6);
    expect(call(smoothstepNode, 1, 1, 2)).toEqual([1]); // degenerate edges
  });

  it('sign', () => {
    expect(call(signNode, -3)).toEqual([-1]);
    expect(call(signNode, 0)).toEqual([0]);
    expect(call(signNode, 8)).toEqual([1]);
  });
});

describe('vec2 geometry', () => {
  it('to-polar is the inverse of from-angle', () => {
    const [angle, radius] = call(toPolarNode, { x: 3, y: 4 }) as [number, number];
    expect(radius).toBeCloseTo(5, 6);
    expect(angle).toBeCloseTo(Math.atan2(4, 3), 6);
  });

  it('reflect bounces across a normal (normal auto-normalised)', () => {
    // straight down, floor normal pointing up → straight up
    expect(call(reflectNode, { x: 0, y: -1 }, { x: 0, y: 1 })).toEqual([{ x: 0, y: 1 }]);
    // 45° into a vertical wall (normal +x) flips x only
    const r = call(reflectNode, { x: 2, y: 3 }, { x: 5, y: 0 })[0] as { x: number; y: number };
    expect(r.x).toBeCloseTo(-2, 6);
    expect(r.y).toBeCloseTo(3, 6);
  });

  it('project drops a onto b', () => {
    expect(call(projectNode, { x: 3, y: 4 }, { x: 1, y: 0 })).toEqual([{ x: 3, y: 0 }]);
    expect(call(projectNode, { x: 3, y: 4 }, { x: 0, y: 0 })).toEqual([{ x: 0, y: 0 }]);
  });
});

describe('motion/delay-line', () => {
  it('echoes the value as it was `delay` seconds ago', () => {
    // value = frame index (one unit per frame); dt = 1s so t == frame index.
    // delay = 3s → output should lag the input by ~3.
    const result = compileGraph([{ id: 'd', defType: 'motion/delay-line', params: {} }], [], {
      registry: nodeRegistry,
      paramReader: () => ({}),
      portValueReader: (_id, port) => (port === 'delay' ? 3 : undefined),
    });
    expect(result.warnings).toEqual([]);
    const frame = (t: number, v: number) => ({
      t, dt: 1, frameCount: Math.round(t), width: 1, height: 1,
      mouseX: 0, mouseY: 0, mouseIsPressed: false, key: '', keyIsPressed: false,
    });
    // We can't wire `value` to t easily, so drive it via a fresh portValue per
    // frame by recompiling the reader closure's captured variable.
    let cur = 0;
    const g = compileGraph([{ id: 'd', defType: 'motion/delay-line', params: {} }], [], {
      registry: nodeRegistry,
      paramReader: () => ({}),
      portValueReader: (_id, port) => (port === 'delay' ? 3 : port === 'value' ? cur : undefined),
    }).graph;
    let out: unknown;
    for (let i = 0; i <= 10; i++) { cur = i; out = tick(g, frame(i, i)).outputs.get('d')?.[0]?.[0]; }
    // At t=10 with delay 3 → value at t=7, which was 7.
    expect(out).toBeCloseTo(7, 6);
  });
});
