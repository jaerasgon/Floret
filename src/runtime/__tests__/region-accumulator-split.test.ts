import { describe, expect, it } from 'vitest';
import { compileGraph, type RegistryLike } from '../compile';
import { tick } from '../tick';
import { nodeRegistry } from '../../nodes/registry';
import '../../nodes';

const frame = {
  t: 0, dt: 0, frameCount: 0, width: 400, height: 300,
  mouseX: 0, mouseY: 0, mouseIsPressed: false, key: '', keyIsPressed: false,
};

const realRegistry: RegistryLike = nodeRegistry;

describe('ForEach accumulator (split Read/Write, LTR)', () => {
  it('running sum via Read+Write pair: sum(0..4) = 10', () => {
    const result = compileGraph(
      [
        { id: 'c', defType: 'value/number', params: { value: 5 } },
        { id: 'range', defType: 'spread/range', params: {} },
        { id: 'init', defType: 'value/number', params: { value: 0 } },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn',  defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'read', defType: 'region/accumulator-read/number',  params: { slot: 1 }, parentId: 'reg' },
        { id: 'add',  defType: 'math/add', params: {}, parentId: 'reg' },
        { id: 'write',defType: 'region/accumulator-write/number', params: { slot: 1 }, parentId: 'reg' },
      ],
      [
        { fromNode: 'c', fromPort: 'number', toNode: 'range', toPort: 'count' },
        { fromNode: 'range', fromPort: 'indices', toNode: 'sIn', toPort: 'outer' },
        { fromNode: 'init', fromPort: 'number', toNode: 'read', toPort: 'init' },
        // LTR: read.read → add.a; splicer-in.inner → add.b; add.out → write.write
        { fromNode: 'read', fromPort: 'read', toNode: 'add', toPort: 'a' },
        { fromNode: 'sIn',  fromPort: 'inner', toNode: 'add', toPort: 'b' },
        { fromNode: 'add', fromPort: 'result', toNode: 'write', toPort: 'write' },
      ],
      {
        registry: realRegistry,
        paramReader: (id) =>
          id === 'c' ? { value: 5 } :
          id === 'init' ? { value: 0 } :
          (id === 'read' || id === 'write') ? { slot: 1 } : {},
      },
    );
    expect(result.warnings).toEqual([]);
    const t = tick(result.graph, frame);
    // Region output[0] = final of pair slot 1 = 0+0+1+2+3+4 = 10.
    expect(t.outputs.get('reg')).toEqual([[10]]);
  });

  it('split accumulator with N=0 emits init unchanged', () => {
    const result = compileGraph(
      [
        { id: 'c0', defType: 'value/number', params: { value: 0 } },
        { id: 'range', defType: 'spread/range', params: {} },
        { id: 'init', defType: 'value/number', params: { value: 99 } },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn',  defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'read', defType: 'region/accumulator-read/number',  params: { slot: 1 }, parentId: 'reg' },
        { id: 'write',defType: 'region/accumulator-write/number', params: { slot: 1 }, parentId: 'reg' },
      ],
      [
        { fromNode: 'c0', fromPort: 'number', toNode: 'range', toPort: 'count' },
        { fromNode: 'range', fromPort: 'indices', toNode: 'sIn', toPort: 'outer' },
        { fromNode: 'init', fromPort: 'number', toNode: 'read', toPort: 'init' },
        // Loop never executes — Write never gets a value. Final = init.
        { fromNode: 'read', fromPort: 'read', toNode: 'write', toPort: 'write' },
      ],
      {
        registry: realRegistry,
        paramReader: (id) =>
          id === 'c0' ? { value: 0 } :
          id === 'init' ? { value: 99 } :
          (id === 'read' || id === 'write') ? { slot: 1 } : {},
      },
    );
    expect(result.warnings).toEqual([]);
    const t = tick(result.graph, frame);
    expect(t.outputs.get('reg')).toEqual([[99]]);
  });

  it('accumulator pair + splicer-out work together in the same region', () => {
    // For each i ∈ [0, 1, 2]:  running += i; splicer-out emits running.
    // Expected splicer-out spread = [0, 1, 3]; final = 3.
    const result = compileGraph(
      [
        { id: 'c', defType: 'value/number', params: { value: 3 } },
        { id: 'r3', defType: 'spread/range', params: {} },
        { id: 'init0', defType: 'value/number', params: { value: 0 } },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn',   defType: 'region/splicer-in/number',  params: {}, parentId: 'reg' },
        { id: 'read',  defType: 'region/accumulator-read/number',  params: { slot: 1 }, parentId: 'reg' },
        { id: 'add',   defType: 'math/add', params: {}, parentId: 'reg' },
        { id: 'write', defType: 'region/accumulator-write/number', params: { slot: 1 }, parentId: 'reg' },
        { id: 'sOut',  defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'c', fromPort: 'number', toNode: 'r3', toPort: 'count' },
        { fromNode: 'r3', fromPort: 'indices', toNode: 'sIn', toPort: 'outer' },
        { fromNode: 'init0', fromPort: 'number', toNode: 'read', toPort: 'init' },
        { fromNode: 'read', fromPort: 'read', toNode: 'add', toPort: 'a' },
        { fromNode: 'sIn',  fromPort: 'inner', toNode: 'add', toPort: 'b' },
        { fromNode: 'add', fromPort: 'result', toNode: 'write', toPort: 'write' },
        { fromNode: 'add', fromPort: 'result', toNode: 'sOut',  toPort: 'inner' },
      ],
      {
        registry: realRegistry,
        paramReader: (id) =>
          id === 'c' ? { value: 3 } :
          id === 'init0' ? { value: 0 } :
          (id === 'read' || id === 'write') ? { slot: 1 } : {},
      },
    );
    expect(result.warnings).toEqual([]);
    const out = tick(result.graph, frame).outputs.get('reg');
    // Output 0 = splicer-out spread; Output 1 = accumulator final.
    expect(out).toEqual([[0, 1, 3], [3]]);
  });

  it('warns when Read has no paired Write', () => {
    const result = compileGraph(
      [
        { id: 'range', defType: 'spread/range', params: { count: 3 } },
        { id: 'init', defType: 'value/number', params: { value: 0 } },
        { id: 'reg', defType: 'region/foreach', params: {} },
        { id: 'sIn',  defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
        { id: 'read', defType: 'region/accumulator-read/number', params: { slot: 1 }, parentId: 'reg' },
      ],
      [
        { fromNode: 'range', fromPort: 'indices', toNode: 'sIn', toPort: 'outer' },
        { fromNode: 'init', fromPort: 'number', toNode: 'read', toPort: 'init' },
      ],
      {
        registry: realRegistry,
        paramReader: (id) =>
          id === 'init' ? { value: 0 } :
          id === 'read' ? { slot: 1 } : {},
      },
    );
    expect(result.warnings?.some(w => w.message.includes('no matching Write'))).toBe(true);
  });
});
