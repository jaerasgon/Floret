import type { FloretNode, FloretEdge } from './editor-state';

/**
 * Simple left-to-right layered layout. For each scope (root + each
 * region's interior), BFS-assign nodes to columns by longest dependency
 * depth, then stack within each column.
 *
 * Heuristic, not optimal — but enough to untangle a patch with one click.
 * Comment nodes are nudged into their own column on the far right so they
 * don't squeeze the data flow.
 */

const COL_WIDTH = 220;
const ROW_HEIGHT = 110;
const REGION_INNER_PADDING_X = 20;
const REGION_INNER_PADDING_Y = 36;

export function autoLayoutPatch(
  nodes: ReadonlyArray<FloretNode>,
  edges: ReadonlyArray<FloretEdge>,
): FloretNode[] {
  const positions = new Map<string, { x: number; y: number }>();
  // Compute per-scope layouts. Run for root first, then each region in
  // depth-first order so children's local positions are stable.
  const byParent = new Map<string | null, FloretNode[]>();
  for (const n of nodes) {
    const k = n.parentId ?? null;
    const list = byParent.get(k) ?? [];
    list.push(n);
    byParent.set(k, list);
  }

  function layoutScope(parentId: string | null): void {
    const scope = byParent.get(parentId) ?? [];
    if (scope.length === 0) return;
    const ids = new Set(scope.map((n) => n.id));
    const edgesInScope = edges.filter((e) => ids.has(e.source) && ids.has(e.target));

    // Topological depth via longest-path BFS.
    const incoming = new Map<string, number>();
    for (const n of scope) incoming.set(n.id, 0);
    for (const e of edgesInScope) {
      incoming.set(e.target, (incoming.get(e.target) ?? 0) + 1);
    }
    const depth = new Map<string, number>();
    const queue: string[] = [];
    for (const n of scope) {
      if ((incoming.get(n.id) ?? 0) === 0) {
        depth.set(n.id, 0);
        queue.push(n.id);
      }
    }
    while (queue.length > 0) {
      const id = queue.shift()!;
      const d = depth.get(id)!;
      for (const e of edgesInScope) {
        if (e.source !== id) continue;
        const cur = depth.get(e.target) ?? -1;
        if (d + 1 > cur) {
          depth.set(e.target, d + 1);
          queue.push(e.target);
        }
      }
    }
    // Any node that wasn't visited (cycle or detached) lands at depth 0.
    for (const n of scope) {
      if (!depth.has(n.id)) depth.set(n.id, 0);
    }

    // Push comment nodes to the far right so they don't clutter the flow.
    const maxDepth = Math.max(0, ...Array.from(depth.values()));
    for (const n of scope) {
      if (n.data.defType === 'meta/comment') depth.set(n.id, maxDepth + 1);
    }

    // Group by column.
    const columns = new Map<number, string[]>();
    for (const [id, d] of depth) {
      const list = columns.get(d) ?? [];
      list.push(id);
      columns.set(d, list);
    }

    // Stable within-column ordering: prefer existing Y, then existing X
    // (so manual layouts don't shuffle randomly).
    const originalY = new Map<string, number>();
    const originalX = new Map<string, number>();
    for (const n of scope) {
      originalY.set(n.id, n.position.y);
      originalX.set(n.id, n.position.x);
    }
    for (const list of columns.values()) {
      list.sort((a, b) => {
        const dy = (originalY.get(a) ?? 0) - (originalY.get(b) ?? 0);
        if (dy !== 0) return dy;
        return (originalX.get(a) ?? 0) - (originalX.get(b) ?? 0);
      });
    }

    // Origin offset: child scopes lay out relative to the region's
    // interior, root lays out relative to (0,0).
    const ox = parentId ? REGION_INNER_PADDING_X : 0;
    const oy = parentId ? REGION_INNER_PADDING_Y : 0;

    const sortedDepths = Array.from(columns.keys()).sort((a, b) => a - b);
    for (const d of sortedDepths) {
      const col = columns.get(d) ?? [];
      for (let r = 0; r < col.length; r++) {
        positions.set(col[r]!, { x: ox + d * COL_WIDTH, y: oy + r * ROW_HEIGHT });
      }
    }
  }

  // Walk scopes: root first, then any node whose id is a parent (regions).
  layoutScope(null);
  for (const n of nodes) {
    if (byParent.has(n.id)) layoutScope(n.id);
  }

  return nodes.map((n) => {
    const p = positions.get(n.id);
    if (!p) return n;
    return { ...n, position: p };
  });
}
