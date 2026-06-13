import { describe, expect, it } from 'vitest';
import { compileGraph, type RegistryLike } from '../compile';
import { tick } from '../tick';
import { nodeRegistry } from '../../nodes/registry';

// Side-effect import — registers all built-in NodeDefs (regions + splicers).
import '../../nodes';

const frame = {
  t: 0, dt: 0, frameCount: 0, width: 400, height: 300,
  mouseX: 0, mouseY: 0, mouseIsPressed: false, key: '', keyIsPressed: false,
};

function realRegistry(): RegistryLike {
  return nodeRegistry;
}

describe('ForEach region', () => {
  it('iterates N times where N = splicer-in spread length', () => {
    // Patch:
    //   Range(count=5) → splicerIn ─┬→ (region) ─→ splicerOut → (outer downstream)
    //                                ↓
    //                            (inner echo: passthrough through splicerOut.in)
    //
    // Inside the region: splicerIn.inner → splicerOut.inner. So splicerOut
    // collects [0, 1, 2, 3, 4] as the outer spread.
    const result = compileGraph(
      [
        { id: 'range', defType: 'spread/range', params: {} },
        { id: 'count', defType: 'value/number', params: { value: 5 } },
        { id: 'r1', defType: 'region/foreach', params: { width: 400, height: 200 } },
        { id: 'sIn', defType: 'region/splicer-in/number', params: {}, parentId: 'r1' },
        { id: 'sOut', defType: 'region/splicer-out/number', params: {}, parentId: 'r1' },
      ],
      [
        { fromNode: 'count', fromPort: 'number', toNode: 'range', toPort: 'count' },
        { fromNode: 'range', fromPort: 'indices', toNode: 'sIn', toPort: 'outer' },
        { fromNode: 'sIn', fromPort: 'inner', toNode: 'sOut', toPort: 'inner' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) => (id === 'count' ? { value: 5 } : {}),
      },
    );

    expect(result.warnings).toEqual([]);
    const t = tick(result.graph, frame);
    // Region's output 0 = aggregated splicer-out spread.
    expect(t.outputs.get('r1')).toEqual([[0, 1, 2, 3, 4]]);
  });

  it('N = min over multiple splicer-in lengths', () => {
    // Two splicer-ins: range(7) and range(3). Loop runs 3 times.
    const result = compileGraph(
      [
        { id: 'c7', defType: 'value/number', params: { value: 7 } },
        { id: 'c3', defType: 'value/number', params: { value: 3 } },
        { id: 'r7', defType: 'spread/range', params: {} },
        { id: 'r3', defType: 'spread/range', params: {} },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'inA', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'inB', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'add', defType: 'math/add', params: {}, parentId: 'reg' },
        { id: 'out', defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'c7', fromPort: 'number', toNode: 'r7', toPort: 'count' },
        { fromNode: 'c3', fromPort: 'number', toNode: 'r3', toPort: 'count' },
        { fromNode: 'r7', fromPort: 'indices', toNode: 'inA', toPort: 'outer' },
        { fromNode: 'r3', fromPort: 'indices', toNode: 'inB', toPort: 'outer' },
        { fromNode: 'inA', fromPort: 'inner', toNode: 'add', toPort: 'a' },
        { fromNode: 'inB', fromPort: 'inner', toNode: 'add', toPort: 'b' },
        { fromNode: 'add', fromPort: 'sum', toNode: 'out', toPort: 'inner' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) =>
          id === 'c7' ? { value: 7 } : id === 'c3' ? { value: 3 } : {},
      },
    );

    expect(result.warnings).toEqual([]);
    // Iterations 0..2: 0+0, 1+1, 2+2 → [0, 2, 4]
    expect(tick(result.graph, frame).outputs.get('reg')).toEqual([[0, 2, 4]]);
  });

  it('empty splicer-in yields empty splicer-out spread', () => {
    const result = compileGraph(
      [
        { id: 'c0', defType: 'value/number', params: { value: 0 } },
        { id: 'range', defType: 'spread/range', params: {} },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'sOut', defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'c0', fromPort: 'number', toNode: 'range', toPort: 'count' },
        { fromNode: 'range', fromPort: 'indices', toNode: 'sIn', toPort: 'outer' },
        { fromNode: 'sIn', fromPort: 'inner', toNode: 'sOut', toPort: 'inner' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) => (id === 'c0' ? { value: 0 } : {}),
      },
    );
    expect(tick(result.graph, frame).outputs.get('reg')).toEqual([[]]);
  });

  it('broadcasts outer scalar into inner non-splicer node', () => {
    // outer scalar(constant 100) wired directly into Add.b inside a region.
    // Per iteration, Add.b sees 100; Add.a sees the splicer-in slot.
    const result = compileGraph(
      [
        { id: 'c5', defType: 'value/number', params: { value: 5 } },
        { id: 'range', defType: 'spread/range', params: {} },
        { id: 'k100', defType: 'value/number', params: { value: 100 } },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'add', defType: 'math/add', params: {}, parentId: 'reg' },
        { id: 'sOut', defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'c5', fromPort: 'number', toNode: 'range', toPort: 'count' },
        { fromNode: 'range', fromPort: 'indices', toNode: 'sIn', toPort: 'outer' },
        { fromNode: 'sIn', fromPort: 'inner', toNode: 'add', toPort: 'a' },
        { fromNode: 'k100', fromPort: 'number', toNode: 'add', toPort: 'b' }, // broadcast!
        { fromNode: 'add', fromPort: 'sum', toNode: 'sOut', toPort: 'inner' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) =>
          id === 'c5' ? { value: 5 } : id === 'k100' ? { value: 100 } : {},
      },
    );

    expect(result.warnings).toEqual([]);
    // 0+100, 1+100, 2+100, 3+100, 4+100
    expect(tick(result.graph, frame).outputs.get('reg')).toEqual([[100, 101, 102, 103, 104]]);
  });
});
