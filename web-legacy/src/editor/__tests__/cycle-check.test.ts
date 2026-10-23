import { describe, expect, it } from 'vitest';
import type { Edge } from '@xyflow/react';
import { nodeRegistry, registerNode } from '../../nodes/registry';
import { wouldCreateCycle } from '../connection-rules';

/**
 * Register only the NodeDefs needed for cycle-check tests. We deliberately
 * skip `import '../../nodes'` because that side-imports draw/style/render
 * nodes that pull p5 at evaluation time — JSDOM is enough for the editor,
 * but the test runner is bare Node.
 */
registerNode({
  type: 'math/add',
  category: 'Math',
  label: 'Add',
  inputs: [
    { name: 'a', type: 'number', default: 0 },
    { name: 'b', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'out', type: 'number' }],
});
registerNode({
  type: 'state/value',
  category: 'State',
  label: 'State',
  inputs: [
    { name: 'init', type: 'any' },
    { name: 'next', type: 'any' },
  ],
  outputs: [{ name: 'current', type: 'any' }],
  stateful: true,
});
registerNode({
  type: 'state/write',
  category: 'State',
  label: 'Write',
  inputs: [{ name: 'next', type: 'any' }],
  outputs: [],
  stateful: true,
  statefulNextPort: 0,
});

const mkEdge = (source: string, target: string, targetHandle: string): Edge => ({
  id: `${source}->${target}/${targetHandle}`,
  source,
  target,
  sourceHandle: 'out-result',
  targetHandle,
});

const allAdd = () => 'math/add';

describe('wouldCreateCycle', () => {
  it('allows a forward edge in an empty graph', () => {
    expect(wouldCreateCycle('a', 'b', 'in-a', [], allAdd)).toBe(false);
  });

  it('blocks a direct 2-cycle through plain nodes', () => {
    // a → b exists; adding b → a closes the loop.
    expect(
      wouldCreateCycle('b', 'a', 'in-a', [mkEdge('a', 'b', 'in-a')], allAdd),
    ).toBe(true);
  });

  it('blocks a longer cycle a → b → c → a', () => {
    const edges = [
      mkEdge('a', 'b', 'in-a'),
      mkEdge('b', 'c', 'in-a'),
    ];
    expect(wouldCreateCycle('c', 'a', 'in-a', edges, allAdd)).toBe(true);
  });

  it('allows a wire that does not close any loop', () => {
    const edges = [
      mkEdge('a', 'b', 'in-a'),
      mkEdge('b', 'c', 'in-a'),
    ];
    // a → c is fine; no cycle.
    expect(wouldCreateCycle('a', 'c', 'in-b', edges, allAdd)).toBe(false);
  });

  it('allows region self-edges (out-X → in-Y on the same node)', () => {
    // Identical source/target with different handles — region pass-through.
    expect(wouldCreateCycle('region', 'region', 'in-y', [], allAdd)).toBe(false);
  });

  it('exempts a stateful node\'s `next` input from cycle detection', () => {
    // state → walk → state.next is the canonical feedback loop and must
    // be allowed. State node's `next` is cross-frame, not within-frame.
    const defTypeFor = (id: string) => (id === 's' ? 'state/value' : 'math/add');
    const edges = [mkEdge('s', 'walk', 'in-a')];
    expect(
      wouldCreateCycle('walk', 's', 'in-next', edges, defTypeFor),
    ).toBe(false);
  });

  it('still blocks cycles into a stateful node\'s non-next input', () => {
    // s.init is NOT cross-frame; a cycle that lands there should be blocked.
    const defTypeFor = (id: string) => (id === 's' ? 'state/value' : 'math/add');
    const edges = [mkEdge('s', 'walk', 'in-a')];
    expect(
      wouldCreateCycle('walk', 's', 'in-init', edges, defTypeFor),
    ).toBe(true);
  });

  it('makes existing stateful-next edges invisible to BFS', () => {
    // a → b → state.next (cross-frame, exempt). Adding state.current → a
    // shouldn't create a cycle because the next edge isn't part of the
    // within-frame graph.
    const defTypeFor = (id: string) => (id === 's' ? 'state/value' : 'math/add');
    const edges = [
      mkEdge('a', 'b', 'in-a'),
      mkEdge('b', 's', 'in-next'),
    ];
    expect(
      wouldCreateCycle('s', 'a', 'in-b', edges, defTypeFor),
    ).toBe(false);
  });
});

describe('wouldCreateCycle — state/write (split-state pattern)', () => {
  it('exempts state/write.next at port 0', () => {
    // state/write has only one input (next) at port 0. Cycle check
    // must treat it as cross-frame just like state/value.next at port 1.
    const defTypeFor = (id: string) => (id === 'w' ? 'state/write' : 'math/add');
    const edges = [
      // Suppose Read is wired to A which is wired to Write.next.
      // Without the exemption, adding A → other (where other → A) would
      // wrongly see Write.next as participating in the cycle graph.
      mkEdge('a', 'w', 'in-next'),
    ];
    // The next edge itself is exempt.
    expect(wouldCreateCycle('a', 'w', 'in-next', edges, defTypeFor)).toBe(false);
  });

  it('Read → Step → Write pattern stays acyclic', () => {
    // Read → Step → Write is the strict L→R idiom we promote. Nothing
    // ever points backwards in the editor graph, so the cycle check
    // should accept every edge unconditionally.
    const defTypeFor = (id: string) => {
      if (id === 'r') return 'math/add'; // stand-in for state/read (no special handling needed)
      if (id === 'w') return 'state/write';
      return 'math/add';
    };
    const accepted: Edge[] = [];
    const additions = [
      mkEdge('r', 'step', 'in-a'),
      mkEdge('step', 'w', 'in-next'),
    ];
    for (const e of additions) {
      expect(wouldCreateCycle(e.source, e.target, e.targetHandle, accepted, defTypeFor)).toBe(false);
      accepted.push(e);
    }
  });
});

// Sanity: every NodeDef the helper queries should be present.
it('registry has the test nodes', () => {
  expect(nodeRegistry.get('math/add')).toBeDefined();
  expect(nodeRegistry.get('state/value')?.stateful).toBe(true);
  expect(nodeRegistry.get('state/write')?.stateful).toBe(true);
  expect(nodeRegistry.get('state/write')?.statefulNextPort).toBe(0);
});
