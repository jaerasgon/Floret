import type { NoduleEdge, NoduleNode } from '../editor/editor-state';
import { CURRENT_PATCH_VERSION, type PatchFile, type SerializedEdge, type SerializedNode } from './schema';

/**
 * Editor state → PatchFile (JSON-friendly).
 *
 * Source of truth for serialised values is `node.data.params` — which is
 * what gets committed on pointerup / blur from IO boxes. The ParamStore
 * is intentionally NOT consulted here; if it contains a mid-drag value
 * that hasn't been committed yet, that's a UX bug we'll address in M7
 * (force-commit on save).
 */

export interface SerializeInput {
  nodes: ReadonlyArray<NoduleNode>;
  edges: ReadonlyArray<NoduleEdge>;
  name?: string;
}

export function serializePatch(input: SerializeInput): PatchFile {
  const now = new Date().toISOString();
  const nodes: SerializedNode[] = input.nodes.map((n) => {
    const out: SerializedNode = {
      id: n.id,
      defType: n.data.defType,
      position: { x: n.position.x, y: n.position.y },
      params: { ...n.data.params },
    };
    if (n.parentId) out.parentId = n.parentId;
    if (n.data.portValues && Object.keys(n.data.portValues).length > 0) {
      out.portValues = { ...n.data.portValues };
    }
    if (n.data.ports && n.data.ports.length > 0) {
      out.ports = n.data.ports.map((p) => ({
        id: p.id, name: p.name, type: p.type, direction: p.direction,
      }));
    }
    const w = (n.style?.width ?? n.width) as number | undefined;
    const h = (n.style?.height ?? n.height) as number | undefined;
    if (typeof w === 'number') out.width = w;
    if (typeof h === 'number') out.height = h;
    return out;
  });
  const edges: SerializedEdge[] = input.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    sourceHandle: e.sourceHandle ?? null,
    targetHandle: e.targetHandle ?? null,
  }));
  return {
    kind: 'nodule.patch',
    version: CURRENT_PATCH_VERSION,
    appVersion: '0.0.1',
    meta: {
      name: input.name,
      createdAt: now,
      modifiedAt: now,
    },
    nodes,
    edges,
  };
}

/**
 * PatchFile → editor state (rehydrate-ready). Pure (no side effects);
 * the editor store's loadPatch action mounts the result and seeds the
 * ParamStore.
 */
export function deserializePatch(patch: PatchFile): { nodes: NoduleNode[]; edges: NoduleEdge[] } {
  const nodes: NoduleNode[] = patch.nodes.map((n) => {
    const out: NoduleNode = {
      id: n.id,
      type: 'nodule',
      position: { ...n.position },
      data: {
        defType: n.defType,
        params: { ...n.params },
      },
    };
    if (n.parentId) {
      out.parentId = n.parentId;
      out.extent = 'parent';
    }
    if (n.portValues) out.data.portValues = { ...n.portValues };
    if (n.ports) {
      out.data.ports = n.ports.map((p) => ({
        id: p.id, name: p.name,
        type: p.type as import('../runtime/types').PortType,
        direction: p.direction,
      }));
    }
    if (typeof n.width === 'number' || typeof n.height === 'number') {
      out.style = {
        ...(typeof n.width === 'number' ? { width: n.width } : {}),
        ...(typeof n.height === 'number' ? { height: n.height } : {}),
      };
    }
    return out;
  });
  const edges: NoduleEdge[] = patch.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    sourceHandle: e.sourceHandle,
    targetHandle: e.targetHandle,
  }));
  return { nodes, edges };
}
