import type { NoduleEdge, NoduleNode } from './editor-state';

/**
 * In-memory clipboard for nodes + their internal edges.
 *
 * We keep this in a module-level singleton rather than the system
 * clipboard for now — `navigator.clipboard.readText` requires a user
 * gesture and async permission flow, and we want Ctrl+V to be instant.
 *
 * Format keeps positions *relative* to the centroid of the copied
 * selection so paste can land at any anchor point and preserve layout.
 */

export const CLIPBOARD_KIND = 'nodule.clipboard';

interface ClipboardNode {
  defType: string;
  params: Record<string, unknown>;
  rel: { x: number; y: number };
}

interface ClipboardEdge {
  sourceIdx: number;
  targetIdx: number;
  sourceHandle: string | null;
  targetHandle: string | null;
}

interface Clipboard {
  kind: typeof CLIPBOARD_KIND;
  nodes: ClipboardNode[];
  edges: ClipboardEdge[];
}

let buffer: Clipboard | null = null;

/**
 * Copy the selected nodes (and any edges where BOTH endpoints are in
 * the selection) into the buffer. No-op if nothing is selected.
 */
export function copySelectionToClipboard(
  nodes: ReadonlyArray<NoduleNode>,
  edges: ReadonlyArray<NoduleEdge>,
): number {
  const selected = nodes.filter((n) => n.selected);
  if (selected.length === 0) return 0;

  // Compute centroid so paste can land at an arbitrary anchor.
  const cx = selected.reduce((a, n) => a + n.position.x, 0) / selected.length;
  const cy = selected.reduce((a, n) => a + n.position.y, 0) / selected.length;

  const idxById = new Map<string, number>();
  selected.forEach((n, i) => idxById.set(n.id, i));

  const cNodes: ClipboardNode[] = selected.map((n) => ({
    defType: n.data.defType,
    params: { ...n.data.params },
    rel: { x: n.position.x - cx, y: n.position.y - cy },
  }));

  // Keep only edges whose BOTH ends are in the selection.
  const cEdges: ClipboardEdge[] = [];
  for (const e of edges) {
    const s = idxById.get(e.source);
    const t = idxById.get(e.target);
    if (s !== undefined && t !== undefined) {
      cEdges.push({
        sourceIdx: s,
        targetIdx: t,
        sourceHandle: e.sourceHandle ?? null,
        targetHandle: e.targetHandle ?? null,
      });
    }
  }

  buffer = { kind: CLIPBOARD_KIND, nodes: cNodes, edges: cEdges };
  return selected.length;
}

export function hasClipboard(): boolean {
  return buffer !== null && buffer.nodes.length > 0;
}

/**
 * Materialize the clipboard at the given anchor point as fresh nodes
 * and edges with new IDs. Returns the new nodes/edges plus the new IDs
 * (so the caller can re-select them after paste).
 */
export function pasteFromClipboard(anchor: { x: number; y: number }): {
  nodes: NoduleNode[];
  edges: NoduleEdge[];
  ids: string[];
} {
  if (!buffer) return { nodes: [], edges: [], ids: [] };

  const idMap = new Map<number, string>();
  const newNodes: NoduleNode[] = buffer.nodes.map((cn, i) => {
    const id = `${cn.defType.replace(/[^a-z0-9_]/gi, '_')}_paste_${randSuffix()}`;
    idMap.set(i, id);
    return {
      id,
      type: 'nodule',
      position: { x: anchor.x + cn.rel.x, y: anchor.y + cn.rel.y },
      data: { defType: cn.defType, params: { ...cn.params } },
      selected: true,
    };
  });

  const newEdges: NoduleEdge[] = buffer.edges.map((ce) => {
    const id = `e_paste_${randSuffix()}`;
    return {
      id,
      source: idMap.get(ce.sourceIdx)!,
      target: idMap.get(ce.targetIdx)!,
      sourceHandle: ce.sourceHandle,
      targetHandle: ce.targetHandle,
    };
  });

  return { nodes: newNodes, edges: newEdges, ids: [...idMap.values()] };
}

let _seq = 0;
function randSuffix(): string {
  _seq += 1;
  return `${Date.now().toString(36).slice(-4)}${_seq.toString(36)}`;
}
