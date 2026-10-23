import { describe, expect, it } from 'vitest';
import { wrapNode, smoothstepNode, stepNode } from '../math/map';
import { signNode } from '../math/unary';
import { toPolarNode, reflectNode, projectNode } from '../vec2/vec2';
import { neighborNode } from '../spread-ops/ops';
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

describe('spread/neighbor', () => {
  const fakeParams = (mode: string) => ({ nodeId: 'n', param: <T>(k: string, fb?: T) => (k === 'edges' ? (mode as unknown as T) : (fb as T)) });
  const compute = (spread: unknown[], index: number, offset: number, mode: string) =>
    neighborNode.computeRaw!([spread, [index], [offset]], fakeParams(mode));

  it('relative offset, clamp/wrap/zero at the edges', () => {
    const s = [10, 20, 30];
    expect(compute(s, 1, 1, 'clamp')).toEqual([[30]]);
    expect(compute(s, 0, -1, 'clamp')).toEqual([[10]]); // clamp repeats the end
    expect(compute(s, 0, -1, 'wrap')).toEqual([[30]]); // wrap → last
    expect(compute(s, 2, 1, 'wrap')).toEqual([[10]]); // wrap → first
    expect(compute(s, 2, 1, 'zero')).toEqual([[null]]); // out of range → empty
    expect(compute([], 0, 0, 'clamp')).toEqual([[null]]);
  });

  it('reads neighbours inside a ForEach (broadcast spread + index pin)', () => {
    // range(4) = [0,1,2,3]; inside the loop, neighbor(offset -1, wrap) of the
    // broadcast spread at `index` → the previous element (wrapping): [3,0,1,2].
    const result = compileGraph(
      [
        { id: 'c', defType: 'value/number', params: { value: 4 } },
        { id: 'range', defType: 'spread/range', params: {} },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'nb', defType: 'spread/neighbor', params: { edges: 'wrap' }, parentId: 'reg' },
        { id: 'sOut', defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'c', fromPort: 'number', toNode: 'range', toPort: 'count' },
        { fromNode: 'range', fromPort: 'indices', toNode: 'sIn', toPort: 'outer' }, // drives N=4
        { fromNode: 'range', fromPort: 'indices', toNode: 'nb', toPort: 'spread' }, // broadcast whole spread in
        { fromNode: 'reg', fromPort: '__loopvar_index', toNode: 'nb', toPort: 'index' },
        { fromNode: 'nb', fromPort: 'value', toNode: 'sOut', toPort: 'inner' },
      ],
      {
        registry: nodeRegistry,
        paramReader: (id) => (id === 'c' ? { value: 4 } : id === 'nb' ? { edges: 'wrap', offset: -1 } : {}),
        portValueReader: (id, port) => (id === 'nb' && port === 'offset' ? -1 : undefined),
      },
    );
    expect(result.warnings).toEqual([]);
    const f = { t: 0, dt: 0, frameCount: 0, width: 1, height: 1, mouseX: 0, mouseY: 0, mouseIsPressed: false, key: '', keyIsPressed: false };
    expect(tick(result.graph, f).outputs.get('reg')).toEqual([[3, 0, 1, 2]]);
  });
});

describe('motion/delay-line', () => {
  it('echoes the value as it was `delay` seconds ago', () => {
    // Drive `value` = frame index via a captured closure var; dt = 1s so
    // t == frame index. delay = 3 → output lags the input by 3.
    let cur = 0;
    const result = compileGraph([{ id: 'd', defType: 'motion/delay-line', params: {} }], [], {
      registry: nodeRegistry,
      paramReader: () => ({}),
      portValueReader: (_id, port) => (port === 'delay' ? 3 : port === 'value' ? cur : undefined),
    });
    expect(result.warnings).toEqual([]);
    const frame = (t: number) => ({
      t, dt: 1, frameCount: Math.round(t), width: 1, height: 1,
      mouseX: 0, mouseY: 0, mouseIsPressed: false, key: '', keyIsPressed: false,
    });
    let out: unknown;
    for (let i = 0; i <= 10; i++) { cur = i; out = tick(result.graph, frame(i)).outputs.get('d')?.[0]?.[0]; }
    expect(out).toBeCloseTo(7, 6); // at t=10, value from t=7 was 7
  });
});
