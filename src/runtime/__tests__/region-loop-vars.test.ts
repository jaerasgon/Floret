import { describe, expect, it } from 'vitest';
import { compileGraph, type RegistryLike } from '../compile';
import { tick } from '../tick';
import { nodeRegistry } from '../../nodes/registry';

// Side-effect import — registers all built-in NodeDefs (regions + splicers
// + loop-var).
import '../../nodes';

const frame = {
  t: 0, dt: 0, frameCount: 0, width: 400, height: 300,
  mouseX: 0, mouseY: 0, mouseIsPressed: false, key: '', keyIsPressed: false,
};

function realRegistry(): RegistryLike {
  return nodeRegistry;
}

/**
 * Build a ForEach driven by a Range(count) splicer-in, with a single
 * built-in loop-var pin wired straight to a number splicer-out. Returns
 * the region's aggregated output spread. `extraEdges` / `extraNodes` let
 * individual tests add a delay wire etc.
 */
function runLoopVar(loopVar: 'index' | 'count' | 'fraction', count: number) {
  const result = compileGraph(
    [
      { id: 'count', defType: 'value/number', params: { value: count } },
      { id: 'range', defType: 'spread/range', params: {} },
      { id: 'reg', defType: 'region/foreach', params: {} },
      { id: 'sIn', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
      { id: 'sOut', defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
    ],
    [
      { fromNode: 'count', fromPort: 'number', toNode: 'range', toPort: 'count' },
      { fromNode: 'range', fromPort: 'indices', toNode: 'sIn', toPort: 'outer' },
      // Built-in loop variable → splicer-out, collected as the outer spread.
      { fromNode: 'reg', fromPort: `__loopvar_${loopVar}`, toNode: 'sOut', toPort: 'inner' },
    ],
    {
      registry: realRegistry(),
      paramReader: (id) => (id === 'count' ? { value: count } : {}),
    },
  );
  expect(result.warnings).toEqual([]);
  return tick(result.graph, frame).outputs.get('reg');
}

describe('ForEach loop variables', () => {
  it('`index` yields 0..N-1', () => {
    expect(runLoopVar('index', 5)).toEqual([[0, 1, 2, 3, 4]]);
  });

  it('`count` yields N on every iteration', () => {
    expect(runLoopVar('count', 4)).toEqual([[4, 4, 4, 4]]);
  });

  it('`fraction` yields 0..1 across the spread', () => {
    expect(runLoopVar('fraction', 5)).toEqual([[0, 0.25, 0.5, 0.75, 1]]);
  });

  it('`fraction` is 0 for a single-element spread (no divide-by-zero)', () => {
    expect(runLoopVar('fraction', 1)).toEqual([[0]]);
  });

  it('loop-var pins do not affect the iteration count', () => {
    // index drives the only splicer-out, but N is still set by the
    // Range(3) splicer-in — not by the presence of three loop-var pins.
    expect(runLoopVar('index', 3)).toEqual([[0, 1, 2]]);
  });

  it('does not add a splicer-in slot for the loop variable', () => {
    // If loop-var counted as a splicer-in, N would collapse to min(3, 0)=0
    // because the synthetic node has no outer spread. It must stay 3.
    const out = runLoopVar('index', 3) as number[][];
    expect(out[0]?.length).toBe(3);
  });
});

describe('ForEach delay (per-iteration time offset)', () => {
  it('rewinds the inner time source by index * delay', () => {
    // Inside the region: time/seconds → splicer-out. delay = 2, t = 10,
    // N = 3 → inner reads 10, 8, 6.
    const result = compileGraph(
      [
        { id: 'count', defType: 'value/number', params: { value: 3 } },
        { id: 'range', defType: 'spread/range', params: {} },
        { id: 'delayVal', defType: 'value/number', params: { value: 2 } },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'clock', defType: 'time/seconds', params: {}, parentId: 'reg' },
        { id: 'sOut', defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'count', fromPort: 'number', toNode: 'range', toPort: 'count' },
        { fromNode: 'range', fromPort: 'indices', toNode: 'sIn', toPort: 'outer' },
        { fromNode: 'delayVal', fromPort: 'number', toNode: 'reg', toPort: 'delay' },
        { fromNode: 'clock', fromPort: 't', toNode: 'sOut', toPort: 'inner' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) =>
          id === 'count' ? { value: 3 } : id === 'delayVal' ? { value: 2 } : {},
      },
    );
    expect(result.warnings).toEqual([]);
    const out = tick(result.graph, { ...frame, t: 10 }).outputs.get('reg');
    expect(out).toEqual([[10, 8, 6]]);
  });

  it('delay defaults to 0 — every iteration reads the same time', () => {
    const result = compileGraph(
      [
        { id: 'count', defType: 'value/number', params: { value: 3 } },
        { id: 'range', defType: 'spread/range', params: {} },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'clock', defType: 'time/seconds', params: {}, parentId: 'reg' },
        { id: 'sOut', defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'count', fromPort: 'number', toNode: 'range', toPort: 'count' },
        { fromNode: 'range', fromPort: 'indices', toNode: 'sIn', toPort: 'outer' },
        { fromNode: 'clock', fromPort: 't', toNode: 'sOut', toPort: 'inner' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) => (id === 'count' ? { value: 3 } : {}),
      },
    );
    expect(result.warnings).toEqual([]);
    const out = tick(result.graph, { ...frame, t: 7 }).outputs.get('reg');
    expect(out).toEqual([[7, 7, 7]]);
  });
});
