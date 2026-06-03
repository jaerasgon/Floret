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

describe('If region', () => {
  it('runs the inner scope once when condition=true and forwards splicer-out', () => {
    const result = compileGraph(
      [
        { id: 'tgl', defType: 'io/toggle', params: { value: true } },
        { id: 'k', defType: 'value/number', params: { value: 42 } },
        { id: 'reg', defType: 'region/if', params: {} },
        { id: 'sOut', defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'tgl', fromPort: 'out', toNode: 'reg', toPort: 'condition' },
        // k is in the outer scope; broadcast into the splicer-out's inner input.
        { fromNode: 'k', fromPort: 'out', toNode: 'sOut', toPort: 'inner' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) =>
          id === 'tgl' ? { value: true } : id === 'k' ? { value: 42 } : {},
      },
    );

    expect(result.warnings).toEqual([]);
    const out = tick(result.graph, frame).outputs.get('reg');
    expect(out).toEqual([[42]]);
  });

  it('emits empty splicer-out spread when condition=false', () => {
    const result = compileGraph(
      [
        { id: 'tgl', defType: 'io/toggle', params: { value: false } },
        { id: 'k', defType: 'value/number', params: { value: 42 } },
        { id: 'reg', defType: 'region/if', params: {} },
        { id: 'sOut', defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'tgl', fromPort: 'out', toNode: 'reg', toPort: 'condition' },
        { fromNode: 'k', fromPort: 'out', toNode: 'sOut', toPort: 'inner' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) =>
          id === 'tgl' ? { value: false } : id === 'k' ? { value: 42 } : {},
      },
    );

    expect(result.warnings).toEqual([]);
    expect(tick(result.graph, frame).outputs.get('reg')).toEqual([[]]);
  });

  it('forwards accumulator init unchanged when condition=false', () => {
    const result = compileGraph(
      [
        { id: 'tgl', defType: 'io/toggle', params: { value: false } },
        { id: 'init', defType: 'value/number', params: { value: 7 } },
        { id: 'reg', defType: 'region/if', params: {} },
        { id: 'acc', defType: 'region/accumulator/number', params: {}, parentId: 'reg' },
      ],
      [
        { fromNode: 'tgl', fromPort: 'out', toNode: 'reg', toPort: 'condition' },
        { fromNode: 'init', fromPort: 'out', toNode: 'acc', toPort: 'init' },
      ],
      {
        registry: realRegistry(),
        paramReader: (id) =>
          id === 'tgl' ? { value: false } : id === 'init' ? { value: 7 } : {},
      },
    );

    expect(result.warnings).toEqual([]);
    expect(tick(result.graph, frame).outputs.get('reg')).toEqual([[7]]);
  });
});

describe('Cache region', () => {
  it('returns the same outputs when inputs are unchanged across ticks', () => {
    // Counter-style: outer count = 3 always, region range → splicer-out.
    // Run twice and observe the runtime cache hit on the second tick.
    const editorNodes = [
      { id: 'c', defType: 'value/number', params: { value: 4 } },
      { id: 'range', defType: 'spread/range', params: {} },
      { id: 'reg', defType: 'region/cache', params: {} },
      { id: 'sIn', defType: 'region/splicer-in/number', params: {}, parentId: 'reg' },
      { id: 'sOut', defType: 'region/splicer-out/number', params: {}, parentId: 'reg' },
    ];
    const edges = [
      { fromNode: 'c', fromPort: 'out', toNode: 'range', toPort: 'count' },
      { fromNode: 'range', fromPort: 'out', toNode: 'sIn', toPort: 'outer' },
      { fromNode: 'sIn', fromPort: 'inner', toNode: 'sOut', toPort: 'inner' },
    ];
    const result = compileGraph(editorNodes, edges, {
      registry: realRegistry(),
      paramReader: (id) => (id === 'c' ? { value: 4 } : {}),
    });

    const a = tick(result.graph, frame).outputs.get('reg');
    const b = tick(result.graph, frame).outputs.get('reg');
    // Same input → cache hit → SAME reference returned.
    expect(a).toBe(b);
    expect(a).toEqual([[0, 1, 2, 3]]);
  });
});
