import { nodeRegistry } from '../nodes/registry';
import type { NodeDef } from '../nodes/types';
import { useEditor, type FloretNode } from './editor-state';
import { useOverlay } from './overlay-store';

/**
 * Locate the smallest region whose bounds contain the given flow-coord
 * point and whose parent matches the current view scope. Used to
 * auto-parent newly-created nodes into the region the user dropped them in.
 */
export function findRegionAtFlow(
  nodes: ReadonlyArray<FloretNode>,
  flowPoint: { x: number; y: number },
  newNodeId: string,
  scopeParent: string | null,
): { id: string; pos: { x: number; y: number } } | null {
  let best: { id: string; pos: { x: number; y: number } } | null = null;
  let bestArea = Infinity;
  for (const n of nodes) {
    if (n.id === newNodeId) continue;
    if ((n.parentId ?? null) !== scopeParent) continue;
    const def = nodeRegistry.get(n.data.defType);
    if (!def?.regionKind) continue;
    const w = (n.data.params?.width as number | undefined) ?? 480;
    const h = (n.data.params?.height as number | undefined) ?? 280;
    if (
      flowPoint.x >= n.position.x && flowPoint.x <= n.position.x + w &&
      flowPoint.y >= n.position.y && flowPoint.y <= n.position.y + h
    ) {
      const area = w * h;
      if (area < bestArea) {
        best = { id: n.id, pos: n.position };
        bestArea = area;
      }
    }
  }
  return best;
}

/**
 * Place a node at a flow-coordinate point, auto-parenting it into any
 * containing region. Drawable regions don't drop at the point — picking one
 * arms the crosshair draw tool instead and this returns `null`.
 *
 * Shared by the node-search popover and the favorites (Q) quick-menu so both
 * produce identical placement + auto-parent behavior. Reads store actions off
 * `getState()` so it's callable outside React (no hooks).
 *
 * Returns the created node id, or `null` when a region was armed instead.
 */
export function placeNodeAtFlow(
  def: NodeDef,
  flowPos: { x: number; y: number },
): string | null {
  // Drawable regions arm the crosshair so the user sizes them with a drag.
  if (def.regionKind && def.regionKind !== 'subpatch') {
    useOverlay.getState().armRegionDraw(def.regionKind);
    return null;
  }

  const { addNode, setNodeParent } = useEditor.getState();
  const id = addNode(def.type, flowPos);

  // Auto-parent the new node into a containing region (if any). addNode
  // already handles the subpatch case via viewStack; this covers regions
  // that live at the current view's scope.
  const st = useEditor.getState();
  const created = st.nodes.find((n) => n.id === id);
  const currentView = st.viewStack[st.viewStack.length - 1] ?? null;
  const newParentScope = created?.parentId ?? null;
  if (created && newParentScope === currentView) {
    const region = findRegionAtFlow(st.nodes, flowPos, id, currentView);
    if (region && region.id !== currentView) {
      setNodeParent(id, region.id, {
        x: flowPos.x - region.pos.x,
        y: flowPos.y - region.pos.y,
      });
    }
  }

  return id;
}
