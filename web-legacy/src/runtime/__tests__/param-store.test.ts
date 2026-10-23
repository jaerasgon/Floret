import { afterEach, describe, expect, it } from 'vitest';
import { paramStore } from '../param-store';
import { compileGraph } from '../compile';
import { tick } from '../tick';
import { nodeRegistry } from '../../nodes/registry';

// Side-effect import: registers built-ins (Slider, Mouse, etc.).
import '../../nodes';

const frame = {
  t: 0, dt: 0, frameCount: 0, width: 400, height: 300,
  mouseX: 0, mouseY: 0, mouseIsPressed: false, key: '', keyIsPressed: false,
};

afterEach(() => {
  paramStore.replaceAll([]);
});

describe('ParamStore + compile integration', () => {
  it('runtime reads slider value from ParamStore (the fast path)', () => {
    paramStore.setNode('s1', { value: 0.7, min: 0, max: 1 });

    const result = compileGraph(
      [{ id: 's1', defType: 'io/slider', params: { value: 0, min: 0, max: 1 } }],
      [],
      {
        registry: nodeRegistry,
        // node.data.params says 0, but the live store says 0.7 — the
        // store must win so scrubs aren't lost while React reconciles.
        paramReader: (id) =>
          id === 's1' ? { value: 0, min: 0, max: 1 } : {},
      },
    );
    expect(tick(result.graph, frame).outputs.get('s1')).toEqual([[0.7]]);
  });

  it('falls back to paramReader when ParamStore has no entry', () => {
    // Note: no paramStore.setNode call — only node.data.params provides the value.
    const result = compileGraph(
      [{ id: 's1', defType: 'value/number', params: { value: 42 } }],
      [],
      {
        registry: nodeRegistry,
        paramReader: (id) => (id === 's1' ? { value: 42 } : {}),
      },
    );
    expect(tick(result.graph, frame).outputs.get('s1')).toEqual([[42]]);
  });

  it('Mouse source node reflects FrameCtx without recompile', () => {
    const result = compileGraph(
      [{ id: 'm', defType: 'input/mouse', params: {} }],
      [],
      { registry: nodeRegistry, paramReader: () => ({}) },
    );

    const r1 = tick(result.graph, { ...frame, mouseX: 100, mouseY: 50, mouseIsPressed: true });
    expect(r1.outputs.get('m')).toEqual([[100], [50], [true]]);

    const r2 = tick(result.graph, { ...frame, mouseX: 200, mouseY: 75, mouseIsPressed: false });
    expect(r2.outputs.get('m')).toEqual([[200], [75], [false]]);
  });

  it('per-node listener fires only on that node and not on unrelated writes', () => {
    let aHits = 0;
    let bHits = 0;
    const unsubA = paramStore.subscribe('a', () => aHits++);
    const unsubB = paramStore.subscribe('b', () => bHits++);

    paramStore.setKey('a', 'value', 1);
    expect(aHits).toBe(1);
    expect(bHits).toBe(0);

    paramStore.setKey('b', 'value', 1);
    expect(aHits).toBe(1);
    expect(bHits).toBe(1);

    // No-op write (same value) doesn't notify.
    paramStore.setKey('a', 'value', 1);
    expect(aHits).toBe(1);

    unsubA();
    unsubB();
  });

  it('removeNode wipes per-node entries and notifies', () => {
    let hits = 0;
    paramStore.setNode('x', { value: 5 });
    const unsub = paramStore.subscribe('x', () => hits++);
    paramStore.removeNode('x');
    expect(paramStore.getNode('x')).toBeUndefined();
    expect(hits).toBe(1);
    unsub();
  });
});
