import { describe, expect, it } from 'vitest';
import { buildRuntimeGraph, type RuntimeNode } from '../graph';
import type { PortValue } from '../types';

const emptyDefaults = (n: number): PortValue[] => Array.from({ length: n }, () => [0]);

function nodeOf(id: string, inArity: number, outArity: number, parentId?: string): RuntimeNode {
  return {
    id,
    type: 'test/passthrough',
    inArity,
    outArity,
    defaults: emptyDefaults(inArity),
    compute: () => Array.from({ length: outArity }, () => [0]),
    parentId,
  };
}

describe('buildRuntimeGraph (scope-aware)', () => {
  it('topo-sorts a linear chain in the root scope', () => {
    const a = nodeOf('a', 0, 1);
    const b = nodeOf('b', 1, 1);
    const c = nodeOf('c', 1, 1);
    const g = buildRuntimeGraph({
      nodes: [c, a, b],
      edges: [
        { fromNode: 'a', fromPort: 0, toNode: 'b', toPort: 0 },
        { fromNode: 'b', fromPort: 0, toNode: 'c', toPort: 0 },
      ],
    });
    const order = g.scopes.get(null)!.order;
    expect(order).toEqual(['a', 'b', 'c']);
  });

  it('handles a diamond graph in the root scope', () => {
    const a = nodeOf('a', 0, 1);
    const b = nodeOf('b', 1, 1);
    const c = nodeOf('c', 1, 1);
    const d = nodeOf('d', 2, 1);
    const g = buildRuntimeGraph({
      nodes: [a, b, c, d],
      edges: [
        { fromNode: 'a', fromPort: 0, toNode: 'b', toPort: 0 },
        { fromNode: 'a', fromPort: 0, toNode: 'c', toPort: 0 },
        { fromNode: 'b', fromPort: 0, toNode: 'd', toPort: 0 },
        { fromNode: 'c', fromPort: 0, toNode: 'd', toPort: 1 },
      ],
    });
    const order = g.scopes.get(null)!.order;
    const idx = (id: string) => order.indexOf(id);
    expect(idx('a')).toBeLessThan(idx('b'));
    expect(idx('a')).toBeLessThan(idx('c'));
    expect(idx('b')).toBeLessThan(idx('d'));
    expect(idx('c')).toBeLessThan(idx('d'));
  });

  it('builds separate scopes for nodes with different parentIds', () => {
    const root1 = nodeOf('root1', 0, 1);
    const region: RuntimeNode = {
      id: 'r',
      type: 'region/foreach',
      inArity: 0,
      outArity: 0,
      defaults: [],
      regionKind: 'foreach',
    };
    const inner1 = nodeOf('inner1', 0, 1, 'r');
    const inner2 = nodeOf('inner2', 1, 1, 'r');

    const g = buildRuntimeGraph({
      nodes: [root1, region, inner1, inner2],
      edges: [
        { fromNode: 'inner1', fromPort: 0, toNode: 'inner2', toPort: 0 },
      ],
    });

    expect(g.scopes.get(null)!.order).toContain('root1');
    expect(g.scopes.get(null)!.order).toContain('r');
    const innerOrder = g.scopes.get('r')!.order;
    expect(innerOrder).toEqual(['inner1', 'inner2']);
  });

  it('rejects cycles within a scope', () => {
    const a = nodeOf('a', 1, 1);
    const b = nodeOf('b', 1, 1);
    expect(() =>
      buildRuntimeGraph({
        nodes: [a, b],
        edges: [
          { fromNode: 'a', fromPort: 0, toNode: 'b', toPort: 0 },
          { fromNode: 'b', fromPort: 0, toNode: 'a', toPort: 0 },
        ],
      }),
    ).toThrow(/cycle/);
  });

  it('accepts cross-scope broadcast edges (outer → inner non-splicer)', () => {
    // The region runs after the outer source. This is the broadcast pattern:
    // an inner scalar reading from an outer scalar (e.g. Time → inner Sin.x).
    const outer = nodeOf('outer', 0, 1);
    const region: RuntimeNode = {
      id: 'r', type: 'region/foreach', inArity: 0, outArity: 0,
      defaults: [], regionKind: 'foreach',
    };
    const inner = nodeOf('inner', 1, 1, 'r');
    const g = buildRuntimeGraph({
      nodes: [outer, region, inner],
      edges: [{ fromNode: 'outer', fromPort: 0, toNode: 'inner', toPort: 0 }],
    });
    const rootOrder = g.scopes.get(null)!.order;
    // The region must come after `outer` (synthetic topo dep).
    expect(rootOrder.indexOf('outer')).toBeLessThan(rootOrder.indexOf('r'));
    // The broadcast edge stays in the global map.
    expect(g.edgesByTarget.get('inner')).toHaveLength(1);
  });

  it('rejects duplicate node ids', () => {
    expect(() =>
      buildRuntimeGraph({
        nodes: [nodeOf('a', 0, 1), nodeOf('a', 0, 1)],
        edges: [],
      }),
    ).toThrow(/duplicate node id/);
  });
});
