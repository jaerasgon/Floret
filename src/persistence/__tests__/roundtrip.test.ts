import { describe, expect, it } from 'vitest';
import { serializePatch, deserializePatch } from '../serialize';
import { migratePatch } from '../migrate';
import { CURRENT_PATCH_VERSION, emptyPatch, isPatchFile } from '../schema';
import type { NoduleEdge, NoduleNode } from '../../editor/editor-state';

const sampleNodes: NoduleNode[] = [
  {
    id: 'time_1',
    type: 'nodule',
    position: { x: 100, y: 100 },
    data: { defType: 'time/seconds', params: {} },
  },
  {
    id: 'slider_1',
    type: 'nodule',
    position: { x: 300, y: 100 },
    data: { defType: 'io/slider', params: { value: 0.42, min: 0, max: 1 } },
  },
  {
    id: 'renderer_1',
    type: 'nodule',
    position: { x: 500, y: 100 },
    data: { defType: 'render/renderer', params: {} },
  },
];

const sampleEdges: NoduleEdge[] = [
  {
    id: 'e1',
    source: 'time_1',
    target: 'slider_1',
    sourceHandle: 'out-t',
    targetHandle: 'in-value',
  },
];

describe('persistence round-trip', () => {
  it('serialize → JSON → parse → deserialize preserves the patch', () => {
    const patch = serializePatch({ nodes: sampleNodes, edges: sampleEdges, name: 'test' });
    expect(isPatchFile(patch)).toBe(true);
    expect(patch.version).toBe(CURRENT_PATCH_VERSION);

    const text = JSON.stringify(patch);
    const parsed = JSON.parse(text);
    expect(isPatchFile(parsed)).toBe(true);

    const { patch: migrated } = migratePatch(parsed);
    const { nodes, edges } = deserializePatch(migrated);

    expect(nodes).toHaveLength(3);
    expect(nodes[0]!.id).toBe('time_1');
    expect(nodes[1]!.data.params.value).toBe(0.42);
    expect(edges).toEqual(sampleEdges);
  });

  it('isPatchFile rejects non-patch objects', () => {
    expect(isPatchFile(null)).toBe(false);
    expect(isPatchFile({})).toBe(false);
    expect(isPatchFile({ kind: 'something-else', version: 1, nodes: [], edges: [] })).toBe(false);
    expect(isPatchFile({ kind: 'nodule.patch', version: 1, nodes: [], edges: [] })).toBe(true);
  });

  it('emptyPatch is shape-valid', () => {
    const p = emptyPatch('foo');
    expect(isPatchFile(p)).toBe(true);
    expect(p.nodes).toEqual([]);
    expect(p.edges).toEqual([]);
    expect(p.meta?.name).toBe('foo');
  });

  it('migrate rejects newer-than-current versions', () => {
    expect(() =>
      migratePatch({
        kind: 'nodule.patch',
        version: CURRENT_PATCH_VERSION + 1,
        nodes: [],
        edges: [],
      }),
    ).toThrow(/newer than this app/);
  });

  it('migrate is no-op for current version', () => {
    const cur: PatchAtVersion1 = {
      kind: 'nodule.patch',
      version: CURRENT_PATCH_VERSION,
      nodes: [],
      edges: [],
    };
    const { patch, applied } = migratePatch(cur);
    expect(applied).toEqual([]);
    expect(patch).toBe(cur);
  });
});

type PatchAtVersion1 = {
  kind: 'nodule.patch';
  version: number;
  nodes: never[];
  edges: never[];
};
