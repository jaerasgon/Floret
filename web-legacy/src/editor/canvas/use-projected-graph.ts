import { useMemo } from 'react';
import { nodeRegistry } from '../../nodes/registry';
import { buildNodeIndex, findAncestor } from '../canvas-geometry';
import type { FloretEdge, FloretNode } from '../editor-state';

/**
 * Derive the nodes/edges React Flow actually renders from the full editor
 * graph, applying three view transforms:
 *   - subpatch filtering: when a subpatch is entered, only it + its direct
 *     children are visible (one level deep); the entered subpatch floats
 *     free at the origin so fitView lands cleanly,
 *   - reparent mode: strip `extent: 'parent'` so children can be dragged
 *     outside their region (the store stays untouched — drop re-adds it),
 *   - region drag-handles: large region frames only initiate a drag from
 *     their `.region-header`, not their whole body.
 * Edges are filtered to those whose endpoints are both visible.
 */
export function useProjectedGraph(
  nodes: FloretNode[],
  edges: FloretEdge[],
  currentViewId: string | null,
  viewStack: string[],
  reparentMode: boolean,
): { projectedNodes: FloretNode[]; projectedEdges: FloretEdge[] } {
  const projectedNodes = useMemo(() => {
    let projected = nodes;
    if (currentViewId) {
      // Build the set of subpatch ancestors that should be HIDDEN: any
      // subpatch *not* in the current view stack is treated as a closed
      // box — its children stay hidden so we only see one level deep.
      const subpatchIds = new Set<string>();
      for (const n of nodes) {
        const d = nodeRegistry.get(n.data.defType);
        if (d?.regionKind === 'subpatch') subpatchIds.add(n.id);
      }
      const viewSet = new Set(viewStack);
      // Determine nearestEnteredSubpatch for each node by walking up
      // parentId chain. A node is visible when its nearest subpatch
      // ancestor === currentViewId; root nodes (nearest = none) are
      // visible only when currentViewId === null.
      const byId = buildNodeIndex(nodes);
      const nearestSubpatchAncestor = (id: string): string | null =>
        findAncestor(id, byId, (a) => subpatchIds.has(a.id))?.id ?? null;
      projected = nodes
        .filter((n) => {
          // The entered subpatch itself is visible.
          if (n.id === currentViewId) return true;
          // Other subpatches in the view stack (ancestors of current view)
          // are NOT shown — they're the layers we've drilled past.
          if (viewSet.has(n.id)) return false;
          return nearestSubpatchAncestor(n.id) === currentViewId;
        })
        .map((n) => {
          // Float the entered subpatch free: its real parent (a region or
          // an outer subpatch) is no longer in the projected list, so
          // React Flow can't anchor it. Reset position to (0, 0) too so
          // fitView lands cleanly on the frame and its children.
          if (n.id === currentViewId && n.parentId) {
            return { ...n, parentId: undefined, extent: undefined, position: { x: 0, y: 0 } };
          }
          return n;
        });
    } else {
      // Root view: hide anything inside a subpatch.
      const subpatchIds = new Set<string>();
      for (const n of nodes) {
        const d = nodeRegistry.get(n.data.defType);
        if (d?.regionKind === 'subpatch') subpatchIds.add(n.id);
      }
      const byId = buildNodeIndex(nodes);
      const insideSubpatch = (id: string): boolean =>
        findAncestor(id, byId, (a) => subpatchIds.has(a.id)) !== null;
      projected = nodes.filter((n) => !insideSubpatch(n.id));
    }
    if (reparentMode) {
      projected = projected.map((n) => (n.parentId ? { ...n, extent: undefined } : n));
    }
    // Region frames (foreach / if / cache) are large containers — clicking
    // anywhere in their body shouldn't drag the whole frame. Restrict
    // drag-initiation to `.region-header`. Subpatches are excluded:
    // collapsed they're regular-sized nodes (drag from anywhere is fine),
    // expanded they're the floated current-view frame (not draggable).
    projected = projected.map((n) => {
      const def = nodeRegistry.get(n.data.defType);
      if (def?.regionKind && def.regionKind !== 'subpatch') {
        return { ...n, dragHandle: '.region-header' };
      }
      return n;
    });
    return projected;
  }, [nodes, reparentMode, currentViewId, viewStack]);

  const projectedEdges = useMemo(() => {
    const visible = new Set(projectedNodes.map((n) => n.id));
    return edges.filter((e) => visible.has(e.source) && visible.has(e.target));
  }, [projectedNodes, edges]);

  return { projectedNodes, projectedEdges };
}
