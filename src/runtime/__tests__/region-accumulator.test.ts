import { describe, expect, it } from 'vitest';
import { compileGraph, type RegistryLike } from '../compile';
import { tick } from '../tick';
import { nodeRegistry } from '../../nodes/registry';

import '../../nodes';

const frame = {
  t: 0, dt: 0, frameCount: 0, width: 400, height: 300,
  mouseX: 0, mouseY: 0, mouseIsPressed: false, key: '', keyIsPressed: false,
};

function realRegistry(): RegistryLike {
  return nodeRegistry;
}

describe('ForEach accumulator', () => {
  it('running sum: sum(0..4) = 10', () => {
    // Accumulator init = 0; each iteration writes `read + splicerIn` back.
    const result = compileGraph(
      [
        { id: 'c', defType: 'value/number', params: { value: 5 } },
        { id: 'range', defType: 'spread/range', params: {} },
        { id: 'init', defType: 'value/number', params: { value: 0 } },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'acc', defType: 'region/accumulator/number', params: {}, parentId: 'reg' },
        { id: 'add', defType: 'math/add', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'c', fromPort: 'out', toNode: 'range', toPort: 'count' },
        { fromNode: 'range', fromPort: 'out', toNode: 'sIn', toPort: 'outer' },
        // init from outside
        { fromNode: 'init', fromPort: 'out', toNode: 'acc', toPort: 'init' },
        // add = read + slot
        { fromNode: 'acc', fromPort: 'read', toNode: 'add', toPort: 'a' },
        { fromNode: 'sIn', fromPort: 'inner', toNode: 'add', toPort: 'b' },
        // write the sum back
        { fromNode: 'add', fromPort: 'out', toNode: 'acc', toPort: 'write' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) =>
          id === 'c' ? { value: 5 } : id === 'init' ? { value: 0 } : {},
      },
    );

    expect(result.warnings).toEqual([]);
    const t = tick(result.graph, frame);
    // Region outputs: [splicer-out spreads..., accumulator finals...]
    // No splicer-outs; one accumulator. Output[0] = final = 0+0+1+2+3+4 = 10.
    expect(t.outputs.get('reg')).toEqual([[10]]);
  });

  it('accumulator with N=0 emits its init value unchanged', () => {
    const result = compileGraph(
      [
        { id: 'c0', defType: 'value/number', params: { value: 0 } },
        { id: 'range', defType: 'spread/range', params: {} },
        { id: 'init', defType: 'value/number', params: { value: 99 } },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'acc', defType: 'region/accumulator/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'c0', fromPort: 'out', toNode: 'range', toPort: 'count' },
        { fromNode: 'range', fromPort: 'out', toNode: 'sIn', toPort: 'outer' },
        { fromNode: 'init', fromPort: 'out', toNode: 'acc', toPort: 'init' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) =>
          id === 'c0' ? { value: 0 } : id === 'init' ? { value: 99 } : {},
      },
    );

    expect(result.warnings).toEqual([]);
    const t = tick(result.graph, frame);
    expect(t.outputs.get('reg')).toEqual([[99]]);
  });

  it('accumulator + splicer-out work together in the same region', () => {
    // For each i ∈ [0, 1, 2]:  running += i; splicer-out emits running
    // Expected splicer-out spread = [0, 1, 3]; final = 3.
    const result = compileGraph(
      [
        { id: 'c', defType: 'value/number', params: { value: 3 } },
        { id: 'r3', defType: 'spread/range', params: {} },
        { id: 'init0', defType: 'value/number', params: { value: 0 } },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'acc', defType: 'region/accumulator/number', params: {}, parentId: 'reg' },
        { id: 'add', defType: 'math/add', params: {}, parentId: 'reg' },
        { id: 'sOut', defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'c', fromPort: 'out', toNode: 'r3', toPort: 'count' },
        { fromNode: 'r3', fromPort: 'out', toNode: 'sIn', toPort: 'outer' },
        { fromNode: 'init0', fromPort: 'out', toNode: 'acc', toPort: 'init' },
        { fromNode: 'acc', fromPort: 'read', toNode: 'add', toPort: 'a' },
        { fromNode: 'sIn', fromPort: 'inner', toNode: 'add', toPort: 'b' },
        { fromNode: 'add', fromPort: 'out', toNode: 'acc', toPort: 'write' },
        { fromNode: 'add', fromPort: 'out', toNode: 'sOut', toPort: 'inner' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) =>
          id === 'c' ? { value: 3 } : id === 'init0' ? { value: 0 } : {},
      },
    );

    expect(result.warnings).toEqual([]);
    const out = tick(result.graph, frame).outputs.get('reg');
    // Output 0 = splicer-out spread; Output 1 = accumulator final.
    expect(out).toEqual([[0, 1, 3], [3]]);
  });
});
