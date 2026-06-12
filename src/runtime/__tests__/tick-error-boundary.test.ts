import { describe, expect, it, vi, afterEach } from 'vitest';
import { compileGraph, type RegistryLike } from '../compile';
import { tick } from '../tick';
import { nodeRegistry } from '../../nodes/registry';
import type { NodeDef } from '../../nodes/types';

// Side-effect import — registers all built-in NodeDefs.
import '../../nodes';

const frame = {
  t: 0, dt: 0, frameCount: 0, width: 400, height: 300,
  mouseX: 0, mouseY: 0, mouseIsPressed: false, key: '', keyIsPressed: false,
};

// A synthetic node whose compute always throws. Registered only inside a
// wrapping RegistryLike so the global registry stays clean.
const throwNode: NodeDef = {
  type: 'test/throw',
  category: 'Test',
  label: 'Throw',
  inputs: [{ name: 'in', type: 'number' }],
  outputs: [{ name: 'out', type: 'number' }],
  compute: () => { throw new Error('boom'); },
};

// A passthrough that echoes its input — proves a node DOWNSTREAM of the
// thrower still evaluates (i.e. the frame didn't halt at the throw).
const echoNode: NodeDef = {
  type: 'test/echo',
  category: 'Test',
  label: 'Echo',
  inputs: [{ name: 'in', type: 'number' }],
  outputs: [{ name: 'out', type: 'number' }],
  compute: (x: unknown) => [x],
};

function registryWith(...extra: NodeDef[]): RegistryLike {
  const byType = new Map(extra.map((d) => [d.type, d]));
  return {
    get: (type: string) => byType.get(type) ?? nodeRegistry.get(type),
    require: (type: string) => {
      const d = byType.get(type) ?? nodeRegistry.get(type);
      if (!d) throw new Error(`unknown node type: ${type}`);
      return d;
    },
  };
}

afterEach(() => vi.restoreAllMocks());

describe('tick error boundary', () => {
  it('a thrown compute does not halt the frame; downstream still evaluates', () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    // num(7) → throw → echo.  If the throw aborted the frame, tick() would
    // re-raise and echo would never run.
    const { graph, warnings } = compileGraph(
      [
        { id: 'num', defType: 'value/number', params: { value: 7 } },
        { id: 'thrower', defType: 'test/throw', params: {} },
        { id: 'downstream', defType: 'test/echo', params: {} },
      ],
      [
        { fromNode: 'num', fromPort: 'out', toNode: 'thrower', toPort: 'in' },
        { fromNode: 'thrower', fromPort: 'out', toNode: 'downstream', toPort: 'in' },
      ],
      {
        registry: registryWith(throwNode, echoNode),
        paramReader: (id) => (id === 'num' ? { value: 7 } : {}),
      },
    );
    expect(warnings).toEqual([]);

    expect(() => tick(graph, frame)).not.toThrow();
    const result = tick(graph, frame);

    // Thrower emits the empty-spread fallback (one empty spread per out port).
    expect(result.outputs.get('thrower')).toEqual([[]]);
    // Frame completed: the constant upstream still produced its value.
    expect(result.outputs.get('num')).toEqual([[7]]);
    // Downstream ran (didn't crash); it just had nothing to echo.
    expect(result.outputs.has('downstream')).toBe(true);
  });

  it('warns at most once for a persistently-throwing node across many frames', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    // Distinct node id from the first test — the throttle map is
    // module-level and keyed by node id, so reusing 'thrower' would stay
    // (correctly) silent here and make the test order-dependent.
    const { graph } = compileGraph(
      [
        { id: 'num', defType: 'value/number', params: { value: 1 } },
        { id: 'thrower-throttle', defType: 'test/throw', params: {} },
      ],
      [{ fromNode: 'num', fromPort: 'out', toNode: 'thrower-throttle', toPort: 'in' }],
      {
        registry: registryWith(throwNode),
        paramReader: (id) => (id === 'num' ? { value: 1 } : {}),
      },
    );
    for (let i = 0; i < 30; i++) tick(graph, { ...frame, frameCount: i });
    // Throttled: a node throwing every frame logs once, not 30×.
    expect(warn).toHaveBeenCalledTimes(1);
  });
});
