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

/**
 * Region-frame-ports tests: same semantics as the splicer-child tests, but
 * the splicers are declared as dynamic ports on the region itself.
 * Compile expands them into synthetic splicer-in/out nodes; the runtime
 * sees exactly the same shape.
 */
describe('Region frame ports', () => {
  it('iterates N times with one input + one output port on the region', () => {
    const result = compileGraph(
      [
        { id: 'count', defType: 'value/number', params: { value: 5 } },
        { id: 'range', defType: 'spread/range', params: {} },
        {
          id: 'reg',
          defType: 'region/foreach',
          params: {},
          ports: [
            { id: 'p1', name: 'idx', type: 'number', direction: 'input' },
            { id: 'p2', name: 'out', type: 'number', direction: 'output' },
          ],
        },
      ],
      [
        { fromNode: 'count', fromPort: 'number', toNode: 'range', toPort: 'count' },
        // outer spread → region.in-p1
        { fromNode: 'range', fromPort: 'indices', toNode: 'reg', toPort: 'p1' },
        // inner: region.out-p1 (per-iter slot) → region.in-p2 (collect)
        { fromNode: 'reg', fromPort: 'p1', toNode: 'reg', toPort: 'p2' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) => (id === 'count' ? { value: 5 } : {}),
      },
    );

    expect(result.warnings).toEqual([]);
    const t = tick(result.graph, frame);
    expect(t.outputs.get('reg')).toEqual([[0, 1, 2, 3, 4]]);
  });

  it('mixes ports with a compute node inside the region', () => {
    // Add 100 to every slot.
    const result = compileGraph(
      [
        { id: 'c', defType: 'value/number', params: { value: 3 } },
        { id: 'range', defType: 'spread/range', params: {} },
        { id: 'k', defType: 'value/number', params: { value: 100 } },
        {
          id: 'reg',
          defType: 'region/foreach',
          params: {},
          ports: [
            { id: 'in1', name: 'i', type: 'number', direction: 'input' },
            { id: 'out1', name: 'sum', type: 'number', direction: 'output' },
          ],
        },
        { id: 'add', defType: 'math/add', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'c', fromPort: 'number', toNode: 'range', toPort: 'count' },
        { fromNode: 'range', fromPort: 'indices', toNode: 'reg', toPort: 'in1' },
        { fromNode: 'reg', fromPort: 'in1', toNode: 'add', toPort: 'a' },
        { fromNode: 'k', fromPort: 'number', toNode: 'add', toPort: 'b' },
        { fromNode: 'add', fromPort: 'result', toNode: 'reg', toPort: 'out1' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) =>
          id === 'c' ? { value: 3 } : id === 'k' ? { value: 100 } : {},
      },
    );

    expect(result.warnings).toEqual([]);
    expect(tick(result.graph, frame).outputs.get('reg')).toEqual([[100, 101, 102]]);
  });

  it('region with no ports compiles cleanly and runs zero iterations', () => {
    const result = compileGraph(
      [
        { id: 'reg', defType: 'region/foreach', params: {}, ports: [] },
      ],
      [],
      { registry: realRegistry(), paramReader: () => ({}) },
    );
    expect(result.warnings).toEqual([]);
    // No splicer-outs → no region outputs at all.
    expect(tick(result.graph, frame).outputs.get('reg')).toEqual([]);
  });
});
