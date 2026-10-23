import { describe, expect, it } from 'vitest';
import { SAMPLES, SAMPLE_CATEGORIES } from '../samples';
import { serializePatch, deserializePatch } from '../serialize';
import { compileGraph } from '../../runtime/compile';
import { tick } from '../../runtime/tick';
import { nodeRegistry } from '../../nodes/registry';
import { parseHandle } from '../../editor/connection-rules';
import type { FloretEdge, FloretNode } from '../../editor/editor-state';

// Side-effect: register all built-ins.
import '../../nodes';

/**
 * Round-trip + warmup coverage for the sample catalogue.
 *
 * `samples.test.ts` compiles each sample straight from `build()` and ticks
 * it once. This file is the companion that protects the *real* load path:
 * every built-in library is produced by `serializePatch(build())` and
 * loaded back via `deserializePatch` (see `buildBuiltInSampleLibraries` +
 * `loadLibraryPatch`). A serialization bug — or a slip during the
 * per-category split (#14) — would pass the build()-based test but corrupt
 * what users actually open. So here we go build → serialize → deserialize →
 * compile, and tick several frames to surface state/particle crashes that
 * only appear after warmup rather than on frame 0.
 */

const frames = [0, 1, 2, 5].map((i) => ({
  t: i * 0.016, dt: 0.016, frameCount: i, width: 440, height: 360,
  mouseX: 100, mouseY: 100, mouseIsPressed: false, key: '', keyIsPressed: false,
}));

// Convert editor nodes/edges into the shape compileGraph expects. Mirrors
// the construction in samples.test.ts so both tests exercise compile the
// same way.
function compile(nodes: FloretNode[], edges: FloretEdge[]) {
  return compileGraph(
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
        return { fromNode: e.source, fromPort: src.name, toNode: e.target, toPort: dst.name };
      })
      .filter((x): x is NonNullable<typeof x> => x !== null),
    {
      registry: nodeRegistry,
      paramReader: (id) => nodes.find((n) => n.id === id)?.data.params ?? {},
      portValueReader: (id, port) => nodes.find((n) => n.id === id)?.data.portValues?.[port],
    },
  );
}

describe('sample catalogue — coverage invariants', () => {
  it('is non-empty and every entry is well-formed', () => {
    expect(SAMPLES.length).toBeGreaterThan(0);
    for (const s of SAMPLES) {
      expect(s.id, `sample id`).toBeTruthy();
      expect(s.name, `name for "${s.id}"`).toBeTruthy();
      expect(typeof s.build, `build() for "${s.id}"`).toBe('function');
      // Every sample declares a real category — otherwise it silently
      // vanishes from the built-in libraries (which group by category).
      expect(
        SAMPLE_CATEGORIES.includes(s.category as (typeof SAMPLE_CATEGORIES)[number]),
        `category "${s.category}" of "${s.id}" must be one of SAMPLE_CATEGORIES`,
      ).toBe(true);
    }
  });

  it('has no duplicate sample ids', () => {
    const ids = SAMPLES.map((s) => s.id);
    const dupes = ids.filter((id, i) => ids.indexOf(id) !== i);
    expect(dupes, `duplicate sample ids: ${dupes.join(', ')}`).toEqual([]);
  });
});

describe('sample catalogue — serialize round-trip', () => {
  for (const sample of SAMPLES) {
    const built = sample.build();
    const isVisual = built.nodes.some((n) => n.data.defType === 'render/renderer');
    const usesSound = built.nodes.some((n) => n.data.defType.startsWith('sound/'));
    // Image loading needs HTMLImageElement (browser-only); skip layer
    // assertions for those (compile still round-trips fine).
    const usesImage = built.nodes.some(
      (n) => n.data.defType === 'io/image-url' || n.data.defType === 'draw/image',
    );
    // Camera / body tracking need getUserMedia + a webcam (browser-only),
    // so they produce no layers here — same treatment as images.
    const usesCamera = built.nodes.some(
      (n) => n.data.defType === 'input/camera' || n.data.defType === 'input/landmarks',
    );

    it(`'${sample.name}' survives serialize → deserialize → compile`, () => {
      const { nodes, edges } = sample.build();
      // The real built-in-library path: build → PatchFile → back to editor nodes.
      const file = serializePatch({ nodes, edges, name: sample.name });
      const restored = deserializePatch(file);

      // Structure is preserved across the round-trip.
      expect(restored.nodes).toHaveLength(nodes.length);
      expect(restored.edges).toHaveLength(edges.length);

      const result = compile(restored.nodes, restored.edges);
      expect(result.warnings).toEqual([]);
    });

    if (!isVisual || usesSound || usesImage || usesCamera) continue;

    it(`'${sample.name}' ticks 4 frames without throwing (post round-trip)`, () => {
      const { nodes, edges } = sample.build();
      const restored = deserializePatch(serializePatch({ nodes, edges, name: sample.name }));
      const result = compile(restored.nodes, restored.edges);
      expect(result.graph.rendererId).not.toBeNull();

      // Tick a warmup sequence — stateful nodes (trails, particles,
      // accumulators) can be fine on frame 0 and crash on frame 2.
      let last: ReturnType<typeof tick> | null = null;
      for (const f of frames) {
        expect(() => { last = tick(result.graph, f); }).not.toThrow();
      }
      // Still producing visual output after warmup.
      expect(last!.layers.length).toBeGreaterThan(0);
    });
  }
});
