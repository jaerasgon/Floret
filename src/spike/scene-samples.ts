// ─── Sample → compiled graph (harness-only) ─────────────────────────────────
// Pulls in the sample catalogue, compiler, and node registry — kept OUT of
// scene-to-batches.ts so the render worker can import the scene walk without
// dragging the whole catalogue (and its window-dependent deps) into the worker
// bundle.

import { SAMPLES } from '../persistence/samples';
import { compileGraph } from '../runtime/compile';
import { nodeRegistry } from '../nodes/registry';
import { parseHandle } from '../editor/connection-rules';
import { parseColor } from '../runtime/gl/scene-to-batches';
import '../nodes'; // register built-in node defs

export function buildSample(id: string) {
  const sample = SAMPLES.find((s) => s.id === id) ?? SAMPLES.find((s) => s.id === 'grid-ripple')!;
  const { nodes, edges } = sample.build();
  const result = compileGraph(
    nodes.map((n) => ({ id: n.id, defType: n.data.defType, params: n.data.params, parentId: n.parentId, ports: n.data.ports })),
    edges.map((e) => {
      const src = parseHandle(e.sourceHandle); const dst = parseHandle(e.targetHandle);
      if (!src || !dst) return null;
      return { fromNode: e.source, fromPort: src.name, toNode: e.target, toPort: dst.name };
    }).filter((x): x is NonNullable<typeof x> => x !== null),
    {
      registry: nodeRegistry,
      paramReader: (nid: string) => nodes.find((n) => n.id === nid)?.data.params ?? {},
      portValueReader: (nid: string, port: string) => nodes.find((n) => n.id === nid)?.data.portValues?.[port],
    },
  );
  const rNode = nodes.find((n) => n.data.defType === 'render/renderer');
  const rp = (rNode?.data.params ?? {}) as Record<string, number>;
  const bgCol = parseColor((rNode?.data.portValues?.background as string) ?? null);
  const bg: [number, number, number] = bgCol[3] > 0 ? [bgCol[0], bgCol[1], bgCol[2]] : [0.063, 0.055, 0.086];
  return { name: sample.name, graph: result.graph, warnings: result.warnings, w: rp.width ?? 500, h: rp.height ?? 500, bg };
}
