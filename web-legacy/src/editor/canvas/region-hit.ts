import type { Node as RFNode } from '@xyflow/react';
import { nodeRegistry } from '../../nodes/registry';
import { buildNodeIndex, absolutePosition, isDescendantOf } from '../canvas-geometry';

export interface NodeBox {
  position: { x: number; y: number };
  size: { w: number; h: number };
}

/** Bbox of a React-Flow rendered node. Prefers `measured` (set after
 *  first render); falls back to `style.width/height` (region frames),
 *  then a constant default. Used by the auto-grow-regions logic in
 *  onNodeDrag. */
const RF_FALLBACK_SIZE = { w: 200, h: 100 };
export function rfNodeBox(n: RFNode): NodeBox {
  const m = n.measured;
  if (m?.width != null && m?.height != null) {
    return { position: n.position, size: { w: m.width, h: m.height } };
  }
  const s = n.style as { width?: number; height?: number } | undefined;
  if (s?.width != null && s?.height != null) {
    return { position: n.position, size: { w: s.width, h: s.height } };
  }
  return { position: n.position, size: RF_FALLBACK_SIZE };
}

/** Region size used when no `style.width/height` is set. Matches the
 *  NodeResizer's minWidth/minHeight in RegionNode.tsx. */
const REGION_FALLBACK_W = 200;
const REGION_FALLBACK_H = 140;

/** When a child is dragged within this many px of the region's right or
 *  bottom edge, the region starts growing in that direction. Trigger
 *  larger than the padding so growth feels anticipatory, not reactive. */
export const REGION_GROW_TRIGGER = 24;
/** Padding maintained between a child and the region's edge after growth. */
export const REGION_GROW_PADDING = 32;

/** Read a region's current width/height from its style/params. */
export function regionSize(region: RFNode): { w: number; h: number } {
  const s = region.style as { width?: number; height?: number } | undefined;
  const d = region.data as { params?: { width?: number; height?: number } } | undefined;
  return {
    w: s?.width ?? d?.params?.width ?? REGION_FALLBACK_W,
    h: s?.height ?? d?.params?.height ?? REGION_FALLBACK_H,
  };
}

/**
 * Move a node to a new parent (or detach to root), preserving its
 * visual position. Used by both the shift+drag handler and the
 * right-click "Move into / out of region" menu items.
 */
export function moveNodeToParent(
  node: RFNode,
  newParentId: string | null,
  allNodes: ReadonlyArray<RFNode>,
  setNodeParent: (id: string, parentId: string | null, position?: { x: number; y: number }) => void,
): void {
  const currentParent = node.parentId ?? null;
  if (currentParent === newParentId) return;

  let absPos = { ...node.position };
  if (currentParent) {
    const parent = allNodes.find((n) => n.id === currentParent);
    if (parent) {
      absPos = { x: absPos.x + parent.position.x, y: absPos.y + parent.position.y };
    }
  }
  let newPos = absPos;
  if (newParentId) {
    const target = allNodes.find((n) => n.id === newParentId);
    if (target) {
      newPos = { x: absPos.x - target.position.x, y: absPos.y - target.position.y };
    }
  }
  setNodeParent(node.id, newParentId, newPos);
}

/**
 * Find a region whose bounds contain the given flow-coord point.
 * Used for shift+drag reparenting. Skips the node being dragged (you
 * can't parent a node to itself) and any descendant of it.
 */
export function findRegionContaining(
  nodes: ReadonlyArray<RFNode>,
  flowPoint: { x: number; y: number },
  draggedId: string,
): RFNode | null {
  const byId = buildNodeIndex(nodes);

  // Iterate in reverse so deeper / later-added regions win when stacked.
  let best: RFNode | null = null;
  let bestArea = Infinity;
  for (const n of nodes) {
    const defType = (n.data as { defType?: string } | undefined)?.defType;
    if (!defType) continue;
    const def = nodeRegistry.get(defType);
    if (!def?.regionKind) continue;
    if (n.id === draggedId) continue;
    if (isDescendantOf(n.id, draggedId, byId)) continue;

    const w = (n.width ?? (n.style?.width as number | undefined)) ?? 480;
    const h = (n.height ?? (n.style?.height as number | undefined)) ?? 280;
    const pos = absolutePosition(n.id, byId);
    if (
      flowPoint.x >= pos.x && flowPoint.x <= pos.x + w &&
      flowPoint.y >= pos.y && flowPoint.y <= pos.y + h
    ) {
      const area = w * h;
      if (area < bestArea) {
        best = n;
        bestArea = area;
      }
    }
  }
  return best;
}

/**
 * Hit-test a flow-coord point against every region's left/right edge
 * band. Returns the region id and which edge (input = left band,
 * output = right band) was hit, or null if none. Picks the smallest
 * region when multiple are stacked.
 *
 * The "band" extends `EDGE_BAND_PX` outside the region as well as just
 * inside, so the user can drop on either side of the border.
 */
const EDGE_BAND_PX = 24;
export function findRegionEdgeAtFlow(
  nodes: ReadonlyArray<RFNode>,
  flowPoint: { x: number; y: number },
): { regionId: string; edge: 'input' | 'output'; localY: number } | null {
  const byId = buildNodeIndex(nodes);

  let best: { regionId: string; edge: 'input' | 'output'; localY: number } | null = null;
  let bestArea = Infinity;
  for (const n of nodes) {
    const defType = (n.data as { defType?: string } | undefined)?.defType;
    if (!defType) continue;
    const def = nodeRegistry.get(defType);
    if (!def?.regionKind) continue;

    const w = (n.width ?? (n.style?.width as number | undefined)) ?? 480;
    const h = (n.height ?? (n.style?.height as number | undefined)) ?? 280;
    const pos = absolutePosition(n.id, byId);

    // Within vertical band of the region?
    if (flowPoint.y < pos.y - EDGE_BAND_PX || flowPoint.y > pos.y + h + EDGE_BAND_PX) continue;

    let hit: 'input' | 'output' | null = null;
    if (
      flowPoint.x >= pos.x - EDGE_BAND_PX &&
      flowPoint.x <= pos.x + EDGE_BAND_PX
    ) {
      hit = 'input';
    } else if (
      flowPoint.x >= pos.x + w - EDGE_BAND_PX &&
      flowPoint.x <= pos.x + w + EDGE_BAND_PX
    ) {
      hit = 'output';
    }
    if (!hit) continue;

    const area = w * h;
    if (area < bestArea) {
      // Clamp local y inside the region body so the pill renders on-frame
      // regardless of how close to the band edge the drop was.
      const localY = Math.max(8, Math.min(h - 8, flowPoint.y - pos.y));
      best = { regionId: n.id, edge: hit, localY };
      bestArea = area;
    }
  }
  return best;
}
