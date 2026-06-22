import type { EdgeChange, NodeChange } from '@xyflow/react';
import type { EditorEdgeRef, EditorNodeRef } from '../../runtime/compile';
import { parseHandle } from '../connection-rules';
import type { FloretEdge, FloretNode } from './types';

let _nextId = 0;
/** Monotonic editor-scoped id with a type-derived prefix. */
export function newId(prefix: string): string {
  _nextId += 1;
  return `${prefix}_${_nextId}`;
}

/**
 * React Flow v12 requires parents to appear before their children in the
 * `nodes` array. We sort by walking the parentId chain depth — roots
 * first, then one-level-deep, etc. Stable within each depth band.
 */
export function sortNodesParentFirst(nodes: FloretNode[]): FloretNode[] {
  const idIndex = new Map<string, number>();
  for (let i = 0; i < nodes.length; i++) idIndex.set(nodes[i]!.id, i);
  const depth = new Map<string, number>();
  function depthOf(id: string): number {
    if (depth.has(id)) return depth.get(id)!;
    const n = nodes[idIndex.get(id)!];
    if (!n) return 0;
    const d = n.parentId ? depthOf(n.parentId) + 1 : 0;
    depth.set(id, d);
    return d;
  }
  for (const n of nodes) depthOf(n.id);
  return [...nodes].sort((a, b) => {
    const da = depth.get(a.id) ?? 0;
    const db = depth.get(b.id) ?? 0;
    if (da !== db) return da - db;
    return (idIndex.get(a.id) ?? 0) - (idIndex.get(b.id) ?? 0);
  });
}

/** Convert editor nodes/edges to the opaque shape compileGraph expects. */
export function toEditorRefs(state: { nodes: FloretNode[]; edges: FloretEdge[] }): {
  nodes: EditorNodeRef[];
  edges: EditorEdgeRef[];
} {
  const nodes: EditorNodeRef[] = state.nodes.map((n) => ({
    id: n.id,
    defType: n.data.defType,
    params: n.data.params,
    parentId: n.parentId,
    ports: n.data.ports,
  }));

  const edges: EditorEdgeRef[] = [];
  for (const e of state.edges) {
    const src = parseHandle(e.sourceHandle);
    const dst = parseHandle(e.targetHandle);
    if (!src || src.kind !== 'out') continue;
    if (!dst || dst.kind !== 'in') continue;
    edges.push({
      fromNode: e.source,
      fromPort: src.name,
      toNode: e.target,
      toPort: dst.name,
    });
  }
  return { nodes, edges };
}

/** Identify structural node changes (affect graph topology). */
export function isStructuralNodeChange(c: NodeChange): boolean {
  return c.type === 'add' || c.type === 'remove' || c.type === 'replace';
}
export function isStructuralEdgeChange(c: EdgeChange): boolean {
  return c.type === 'add' || c.type === 'remove' || c.type === 'replace';
}
