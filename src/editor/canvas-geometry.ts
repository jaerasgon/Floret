/**
 * Pure geometry helpers for the canvas — node indexing and parentId-chain
 * walks. Canvas.tsx had four hand-rolled copies of "build a Map by id, then
 * walk the parent chain": two `absPos` recursions (region hit-testing) and
 * two ancestor scans (subpatch projection). This consolidates them.
 *
 * The functions take a minimal `GeoNode` shape rather than React Flow's full
 * `Node`, so they're trivially unit-testable and don't couple to @xyflow.
 * A real `RFNode` satisfies `GeoNode` structurally.
 */

export interface GeoNode {
  id: string;
  parentId?: string;
  position: { x: number; y: number };
}

export type NodeIndex<T extends GeoNode = GeoNode> = Map<string, T>;

/** Index nodes by id for O(1) parent lookups. */
export function buildNodeIndex<T extends GeoNode>(nodes: ReadonlyArray<T>): NodeIndex<T> {
  const m = new Map<string, T>();
  for (const n of nodes) m.set(n.id, n);
  return m;
}

/**
 * Absolute (root-space) position of a node, summing its parentId chain.
 * Returns {0,0} for an unknown id. Children are positioned relative to
 * their parent in React Flow, so this recovers the on-canvas coordinate.
 */
export function absolutePosition(id: string, byId: NodeIndex): { x: number; y: number } {
  const n = byId.get(id);
  if (!n) return { x: 0, y: 0 };
  if (!n.parentId) return n.position;
  const p = absolutePosition(n.parentId, byId);
  return { x: n.position.x + p.x, y: n.position.y + p.y };
}

/**
 * Walk up the parentId chain from `id` (exclusive of `id` itself) and
 * return the first ancestor satisfying `pred`, or null. Cycle-safe via a
 * visited guard (defensive — the editor shouldn't produce parent cycles).
 */
export function findAncestor<T extends GeoNode>(
  id: string,
  byId: NodeIndex<T>,
  pred: (n: T) => boolean,
): T | null {
  const seen = new Set<string>();
  let cur = byId.get(id)?.parentId;
  while (cur && !seen.has(cur)) {
    seen.add(cur);
    const n = byId.get(cur);
    if (!n) break;
    if (pred(n)) return n;
    cur = n.parentId;
  }
  return null;
}

/**
 * True if `id` is `ancestorId` itself or a descendant of it (i.e. walking
 * up from `id` reaches `ancestorId`). Used to stop a node being reparented
 * into its own subtree.
 */
export function isDescendantOf(id: string, ancestorId: string, byId: NodeIndex): boolean {
  const seen = new Set<string>();
  let cur: string | undefined = id;
  while (cur && !seen.has(cur)) {
    if (cur === ancestorId) return true;
    seen.add(cur);
    cur = byId.get(cur)?.parentId;
  }
  return false;
}
