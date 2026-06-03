import { describe, expect, it } from 'vitest';
import { SAMPLES } from '../samples';
import { compileGraph } from '../../runtime/compile';
import { tick } from '../../runtime/tick';
import { nodeRegistry } from '../../nodes/registry';
import { parseHandle } from '../../editor/connection-rules';

// Side-effect: register all built-ins.
import '../../nodes';

const frame = {
  t: 1.5, dt: 0.016, frameCount: 90, width: 440, height: 360,
  mouseX: 100, mouseY: 100, mouseIsPressed: false, key: '', keyIsPressed: false,
};

describe('built-in samples', () => {
  for (const sample of SAMPLES) {
    // Audio-only samples don't have a Renderer (no visuals). Samples
    // that USE sound nodes can't tick in a Node test environment
    // (p5.sound needs `window`). For those we only assert that the
    // compile step succeeds with no warnings.
    const built = sample.build();
    const isVisual = built.nodes.some((n) => n.data.defType === 'render/renderer');
    const usesSound = built.nodes.some((n) => n.data.defType.startsWith('sound/'));
    // Image loading creates HTMLImageElement, which only exists in a browser.
    const usesImage = built.nodes.some((n) =>
      n.data.defType === 'io/image-url' || n.data.defType === 'draw/image',
    );

    it(`'${sample.name}' compiles with no warnings`, () => {
      const { nodes, edges } = sample.build();
      const result = compileGraph(
        nodes.map((n) => ({
          id: n.id,
          defType: n.data.defType,
          params: n.data.params,
          parentId: n.parentId,
          ports: n.data.ports,
        })),
        edges
          .map((e) => {
            const src = parseHandle(e.sourceHandle);
            const dst = parseHandle(e.targetHandle);
            if (!src || !dst) return null;
            return {
              fromNode: e.source,
              fromPort: src.name,
              toNode: e.target,
              toPort: dst.name,
            };
          })
          .filter((x): x is NonNullable<typeof x> => x !== null),
        {
        registry: nodeRegistry,
        paramReader: (id) => nodes.find((n) => n.id === id)?.data.params ?? {},
        portValueReader: (id, port) => nodes.find((n) => n.id === id)?.data.portValues?.[port],
      },
      );
      expect(result.warnings).toEqual([]);
    });

    if (!isVisual || usesSound || usesImage) continue;

    it(`'${sample.name}' has a Renderer and produces ≥1 layer per tick`, () => {
      const { nodes, edges } = sample.build();

      const result = compileGraph(
        nodes.map((n) => ({
          id: n.id,
          defType: n.data.defType,
          params: n.data.params,
          parentId: n.parentId,
          ports: n.data.ports,
        })),
        edges
          .map((e) => {
            const src = parseHandle(e.sourceHandle);
            const dst = parseHandle(e.targetHandle);
            if (!src || !dst) return null;
            return {
              fromNode: e.source,
              fromPort: src.name,
              toNode: e.target,
              toPort: dst.name,
            };
          })
          .filter((x): x is NonNullable<typeof x> => x !== null),
        {
        registry: nodeRegistry,
        paramReader: (id) => nodes.find((n) => n.id === id)?.data.params ?? {},
        portValueReader: (id, port) => nodes.find((n) => n.id === id)?.data.portValues?.[port],
      },
      );

      expect(result.graph.rendererId).not.toBeNull();

      const t = tick(result.graph, frame);
      const layers = t.layers;
      expect(layers.length).toBeGreaterThan(0);
      // Layers are retained-mode SceneNodes (objects with a `kind`
      // discriminator), not the legacy `(p: p5) => void` thunks. A
      // ForEach region can also produce a nested array of SceneNodes
      // per iteration — accept that too.
      for (const l of layers) {
        if (Array.isArray(l)) {
          for (const inner of l) expect(inner).toMatchObject({ kind: expect.any(String) });
        } else {
          expect(l).toMatchObject({ kind: expect.any(String) });
        }
      }
    });
  }

  // Spot check: Lissajous Threads should produce exactly 240 dot layers
  // (the Range count). Catches accidental off-by-one in iteration count.
  it('Lissajous Threads produces 240 dot layers', () => {
    const sample = SAMPLES.find((s) => s.id === 'lissajous')!;
    const { nodes, edges } = sample.build();
    const result = compileGraph(
      nodes.map((n) => ({
        id: n.id, defType: n.data.defType, params: n.data.params,
        parentId: n.parentId, ports: n.data.ports,
      })),
      edges
        .map((e) => {
          const src = parseHandle(e.sourceHandle);
          const dst = parseHandle(e.targetHandle);
          if (!src || !dst) return null;
          return { fromNode: e.source, fromPort: src.name, toNode: e.target, toPort: dst.name };
        })
        .filter((x): x is NonNullable<typeof x> => x !== null),
      {
        registry: nodeRegistry,
        paramReader: (id) => nodes.find((n) => n.id === id)?.data.params ?? {},
        portValueReader: (id, port) => nodes.find((n) => n.id === id)?.data.portValues?.[port],
      },
    );
    expect(tick(result.graph, frame).layers).toHaveLength(240);
  });
});
