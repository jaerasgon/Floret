import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  ReactFlowProvider,
  SelectionMode,
  useReactFlow,
  type Connection,
  type Edge,
  type Node as RFNode,
  type OnConnectStartParams,
} from '@xyflow/react';
import { FloretNode } from './nodes/FloretNode';
import { SpreadEdge } from './SpreadEdge';
import { useEditor, getDefTypeFor, getRegionPortsFor, getParamsFor, getEdgesSnapshot } from './editor-state';
import { arePortsCompatible, buildIsValidConnection, categoryColor, getUpstreamTypes, isMultiTargetPort, parseHandle } from './connection-rules';
import { nodeRegistry } from '../nodes/registry';
import { getResolvedInputs, getResolvedOutputs } from '../nodes/types';
import { buildNodeIndex, absolutePosition, findAncestor, isDescendantOf } from './canvas-geometry';
import { useOverlay, type ContextMenuItem, type PendingConnection } from './overlay-store';
import { NodeSearch } from './NodeSearch';
import { ContextMenu } from './ContextMenu';
import { prefsSnapshot } from '../app/prefs';
import { paramStore } from '../runtime/param-store';

interface NodeBox {
  position: { x: number; y: number };
  size: { w: number; h: number };
}

/** Bbox of a React-Flow rendered node. Prefers `measured` (set after
 *  first render); falls back to `style.width/height` (region frames),
 *  then a constant default. Used by the auto-grow-regions logic in
 *  onNodeDrag. */
const RF_FALLBACK_SIZE = { w: 200, h: 100 };
function rfNodeBox(n: RFNode): NodeBox {
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
const REGION_GROW_TRIGGER = 24;
/** Padding maintained between a child and the region's edge after growth. */
const REGION_GROW_PADDING = 32;

/** Read a region's current width/height from its style/params. */
function regionSize(region: RFNode): { w: number; h: number } {
  const s = region.style as { width?: number; height?: number } | undefined;
  const d = region.data as { params?: { width?: number; height?: number } } | undefined;
  return {
    w: s?.width ?? d?.params?.width ?? REGION_FALLBACK_W,
    h: s?.height ?? d?.params?.height ?? REGION_FALLBACK_H,
  };
}

function CanvasInner() {
  const nodes = useEditor((s) => s.nodes);
  const edges = useEditor((s) => s.edges);
  const viewStack = useEditor((s) => s.viewStack);
  const currentViewId = viewStack[viewStack.length - 1] ?? null;
  const setViewStack = useEditor((s) => s.setViewStack);
  const exitSubpatch = useEditor((s) => s.exitSubpatch);
  const onNodesChangeStore = useEditor((s) => s.onNodesChange);
  const onEdgesChange = useEditor((s) => s.onEdgesChange);
  const onConnectStore = useEditor((s) => s.onConnect);
  // Wrap the store action so we can flag a successful connection — used
  // by onConnectEnd below to avoid opening the node-search popover when
  // the wire actually landed on a handle.
  const onConnect = useCallback(
    (conn: Connection) => {
      connectionMadeRef.current = true;
      onConnectStore(conn);
    },
    [onConnectStore],
  );
  const addNode = useEditor((s) => s.addNode);
  const addRegionPort = useEditor((s) => s.addRegionPort);
  const addEditorEdge = useEditor((s) => s.addEdge);
  const setNodeParent = useEditor((s) => s.setNodeParent);
  const removeSelected = useEditor((s) => s.removeSelected);
  const clearRegionPortSelection = useEditor((s) => s.clearRegionPortSelection);
  const copySelected = useEditor((s) => s.copySelected);
  const pasteAt = useEditor((s) => s.pasteAt);
  const hasClipboard = useEditor((s) => s.hasClipboard);
  const duplicateNode = useEditor((s) => s.duplicateNode);
  const deleteNode = useEditor((s) => s.deleteNode);
  const groupSelectionIntoSubpatch = useEditor((s) => s.groupSelectionIntoSubpatch);
  const reorderMultiEdge = useEditor((s) => s.reorderMultiEdge);
  const removeEdge = useEditor((s) => s.removeEdge);

  const openSearch = useOverlay((s) => s.openSearch);
  const openContext = useOverlay((s) => s.openContext);

  const reactFlow = useReactFlow();
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  /** Source of an in-progress wire drag; populated by onConnectStart. */
  const pendingConnectionRef = useRef<OnConnectStartParams | null>(null);

  // ─── Region draw-mode state ───────────────────────────────────────────────
  // When a region NodeDef is dragged from the palette, instead of
  // placing the node instantly we enter draw mode: the drop point
  // becomes the first corner, the user moves the mouse to size the
  // region's rectangle, then clicks to commit. Nodes whose centres
  // fall inside the rect are reparented automatically.
  const [drawingRegion, setDrawingRegion] = useState<
    | { kind: 'foreach' | 'if' | 'cache'; startFlow: { x: number; y: number }; currentFlow: { x: number; y: number } }
    | null
  >(null);
  const addRegion = useEditor((s) => s.addRegion);
  /** Set true by onConnect when a wire successfully connects, so the
   *  subsequent onConnectEnd skips the search-on-empty-drop fallback. */
  const connectionMadeRef = useRef(false);

  // ─── Spacebar = reparent modifier ────────────────────────────────────────
  // Pressing space while dragging a node "lifts" it for reparenting. The
  // ref is read mid-drag (cheap); the state mirrors it so re-renders can
  // strip `extent: 'parent'` from child nodes (otherwise React Flow won't
  // let them be dragged outside their parent's bounds).
  const spaceDownRef = useRef(false);
  const [reparentMode, setReparentMode] = useState(false);
  const dragSpaceTouchedRef = useRef(false);
  /** Id of the region currently under the dragged node — for hover style. */
  const hoverRegionRef = useRef<string | null>(null);

  function setHoverRegion(id: string | null): void {
    if (hoverRegionRef.current === id) return;
    hoverRegionRef.current = id;
    document.querySelectorAll('.region-node[data-drop-target="1"]').forEach((el) => {
      (el as HTMLElement).removeAttribute('data-drop-target');
    });
    if (id) {
      const el = document.querySelector(`.react-flow__node[data-id="${id}"] .region-node`);
      if (el) (el as HTMLElement).setAttribute('data-drop-target', '1');
    }
  }

  const nodeTypes = useMemo(() => ({ floret: FloretNode }), []);
  const edgeTypes = useMemo(() => ({ default: SpreadEdge }), []);

  // While reparent mode is on, strip `extent: 'parent'` from every
  // parented node so the user can drag them outside their region.
  // The store itself isn't mutated — we re-add extent on drop via
  // setNodeParent. This re-renders the React Flow node list only when
  // reparent mode toggles.
  //
  // Also handles subpatch view filtering: when the user has entered a
  // subpatch, only that subpatch + its direct children are visible.
  // Regions remain inline (children always rendered with their region),
  // so the filter only cuts at the subpatch boundary.
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

  const isValidConnectionFn = useMemo(
    () => buildIsValidConnection(getDefTypeFor, getRegionPortsFor, getParamsFor, getEdgesSnapshot),
    [],
  );
  const isValidConnection = useCallback(
    (c: Connection | Edge) => isValidConnectionFn(c as Connection),
    [isValidConnectionFn],
  );

  const onDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
  }, []);

  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      const defType = e.dataTransfer.getData('application/floret-def');
      if (!defType) return;
      const position = reactFlow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
      // Region NodeDefs enter draw mode rather than placing instantly.
      // The drop point becomes the first corner of the rectangle.
      // Subpatches use the same regionKind machinery but aren't draw-
      // sized — they're created collapsed and resized lazily, so we
      // place them directly.
      const def = nodeRegistry.get(defType);
      if (def?.regionKind && def.regionKind !== 'subpatch') {
        setDrawingRegion({
          kind: def.regionKind,
          startFlow: position,
          currentFlow: position,
        });
        return;
      }
      addNode(defType, position);
    },
    [reactFlow, addNode],
  );

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') return;
      if (e.key === 'Delete' || e.key === 'Backspace') {
        removeSelected();
      }
    },
    [removeSelected],
  );

  // Document-level Delete/Backspace listener — fires regardless of which
  // element has focus. React Flow's own `onKeyDown` (above) only sees
  // events when focus is inside its wrapper, and clicking a Port Pill
  // doesn't move focus there (it's not a focusable element). Without
  // this fallback, a selected pill couldn't be deleted via keyboard.
  // We still skip when typing in inputs/textareas/contentEditable so we
  // don't intercept the user's edits.
  const hasPortSelection = useEditor((s) => s.selectedPortKeys.length > 0);
  useEffect(() => {
    if (!hasPortSelection) return;
    const onDocKey = (e: KeyboardEvent) => {
      if (e.key !== 'Delete' && e.key !== 'Backspace') return;
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
      e.preventDefault();
      removeSelected();
    };
    document.addEventListener('keydown', onDocKey);
    return () => document.removeEventListener('keydown', onDocKey);
  }, [hasPortSelection, removeSelected]);

  // ─── Double-click a wire: open node search to insert-into-edge ───────────
  const onEdgeDoubleClick = useCallback(
    (e: React.MouseEvent, edge: Edge) => {
      e.preventDefault();
      e.stopPropagation();
      // Resolve port types from the edge's source/target handles so the
      // search popover can filter to nodes that fit on BOTH sides.
      const srcParsed = edge.sourceHandle ? parseHandle(edge.sourceHandle) : null;
      const dstParsed = edge.targetHandle ? parseHandle(edge.targetHandle) : null;
      if (!srcParsed || !dstParsed || !edge.sourceHandle || !edge.targetHandle) return;
      const srcType = resolveDraggedPortType(edge.source, srcParsed.name, 'source');
      const dstType = resolveDraggedPortType(edge.target, dstParsed.name, 'target');
      if (!srcType || !dstType) return;
      const flowMid = reactFlow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
      openSearch({
        screenPosition: { x: e.clientX, y: e.clientY },
        flowPosition: flowMid,
        insertion: {
          edgeId: edge.id,
          source: edge.source,
          sourceHandle: edge.sourceHandle,
          sourcePortType: srcType,
          target: edge.target,
          targetHandle: edge.targetHandle,
          targetPortType: dstType,
          flowPosition: flowMid,
        },
      });
    },
    [openSearch, reactFlow],
  );

  // ─── Right-click an edge: reorder on multi pin, or delete ─────────────────
  const onEdgeContextMenu = useCallback(
    (e: React.MouseEvent, edge: Edge) => {
      e.preventDefault();
      e.stopPropagation();
      const isMulti =
        edge.target && edge.targetHandle
          ? isMultiTargetPort(
              edge.target, edge.targetHandle,
              getDefTypeFor, getRegionPortsFor, getParamsFor,
            )
          : false;
      const items: ContextMenuItem[] = [];
      if (isMulti) {
        // Compute this edge's current slot among siblings sharing the
        // target multi pin — so the menu can show "(1/N)" and disable
        // moves that would no-op.
        const s = useEditor.getState();
        const subset = s.edges.filter(
          (x) => x.target === edge.target && x.targetHandle === edge.targetHandle,
        );
        const idx = subset.findIndex((x) => x.id === edge.id);
        items.push({
          label: `Move up · (${idx + 1}/${subset.length})`,
          disabled: idx <= 0,
          shortcut: '[',
          onSelect: () => reorderMultiEdge(edge.id, -1),
        });
        items.push({
          label: 'Move down',
          disabled: idx >= subset.length - 1,
          shortcut: ']',
          onSelect: () => reorderMultiEdge(edge.id, +1),
        });
        items.push({
          label: 'Move to top',
          disabled: idx <= 0,
          onSelect: () => reorderMultiEdge(edge.id, -subset.length),
        });
        items.push({
          label: 'Move to bottom',
          disabled: idx >= subset.length - 1,
          onSelect: () => reorderMultiEdge(edge.id, +subset.length),
        });
        items.push({ separator: true, label: '', onSelect: () => {} });
      }
      items.push({
        label: 'Delete wire',
        shortcut: 'Del',
        danger: true,
        onSelect: () => removeEdge(edge.id),
      });
      openContext({ screenPosition: { x: e.clientX, y: e.clientY }, items });
    },
    [openContext, reorderMultiEdge, removeEdge],
  );

  // ─── Double-click empty pane: open node search ─────────────────────────────
  // Listens for the browser's native `dblclick` on the wrapper instead of
  // counting clicks via React Flow's `onPaneClick` + `e.detail`. With
  // `selectionOnDrag`, RF doesn't attach a real click handler to the pane —
  // it synthesises onPaneClick from pointerup, gated on internal flags
  // (userSelectionRect / connectionInProgress / selectionInProgress). That
  // path is brittle: pointerup's `detail` isn't a reliable click count
  // (always 0 in Firefox), and a stuck connectionInProgress after a wire
  // drag silently swallows every pane click until remount — the classic
  // "double-click worked for a while, then stopped". Native dblclick can't
  // be turned off by RF state, and `zoomOnDoubleClick={false}` keeps
  // d3-zoom from consuming it. Gating on the target being the pane itself
  // keeps node/edge double-clicks (which have their own handlers) out.
  const onWrapperDoubleClick = useCallback(
    (e: React.MouseEvent) => {
      const target = e.target as HTMLElement;
      if (!target.classList?.contains('react-flow__pane')) return;
      const flowPosition = reactFlow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
      openSearch({
        screenPosition: { x: e.clientX, y: e.clientY },
        flowPosition,
      });
    },
    [openSearch, reactFlow],
  );

  // ─── Wire-drag end on empty space: open node search with the source ──────
  const onConnectStart = useCallback(
    (_e: MouseEvent | TouchEvent, params: OnConnectStartParams) => {
      pendingConnectionRef.current = params;
      // Affinity feedback: mark every handle on the canvas as
      // compatible or incompatible with the dragged pin. CSS then
      // highlights one and dims the other.
      paintWireAffinity(params);
    },
    [],
  );

  const onConnectEnd = useCallback(
    (e: MouseEvent | TouchEvent) => {
      const pending = pendingConnectionRef.current;
      pendingConnectionRef.current = null;
      clearWireAffinity();
      // If onConnect already fired for this drag, the wire is connected
      // — don't open the node-search fallback.
      const made = connectionMadeRef.current;
      connectionMadeRef.current = false;
      if (made) return;
      if (!pending || !pending.nodeId || !pending.handleId || !pending.handleType) return;

      const { clientX, clientY } = 'changedTouches' in e
        ? e.changedTouches[0]!
        : (e as MouseEvent);

      // Resolve the dragged pin's port type. For region pins we read from
      // the region's dynamic ports list; for ordinary nodes we read from
      // the NodeDef.
      const parsed = parseHandle(pending.handleId);
      if (!parsed) return;
      const sourcePort = resolveDraggedPortType(pending.nodeId, parsed.name, pending.handleType);
      if (!sourcePort) return;

      // ─── Drop on a region's frame edge → create a port + edge ─────────
      const flowPosForHit = reactFlow.screenToFlowPosition({ x: clientX, y: clientY });
      const edgeHit = findRegionEdgeAtFlow(reactFlow.getNodes(), flowPosForHit);
      if (edgeHit && edgeHit.regionId !== pending.nodeId) {
        const direction = edgeHit.edge === 'input' ? 'input' : 'output';
        const newPortId = addRegionPort(edgeHit.regionId, {
          type: sourcePort,
          direction,
          y: edgeHit.localY,
        });
        // The new port's outer/inner handles use `in-<id>` / `out-<id>`
        // — the edge wires the dragged pin to the appropriate side.
        if (pending.handleType === 'source') {
          addEditorEdge(
            pending.nodeId, pending.handleId,
            edgeHit.regionId, `in-${newPortId}`,
          );
        } else {
          addEditorEdge(
            edgeHit.regionId, `out-${newPortId}`,
            pending.nodeId, pending.handleId,
          );
        }
        return;
      }

      // ─── Released on a handle? React Flow fired onConnect; ignore. ────
      const target = e.target as HTMLElement | null;
      if (!target) return;
      const droppedOnPane =
        target.classList?.contains('react-flow__pane') ||
        (target.closest?.('.react-flow__pane') && !target.closest('.react-flow__handle'));
      if (!droppedOnPane) return;

      const flowPosition = reactFlow.screenToFlowPosition({ x: clientX, y: clientY });

      const connection: PendingConnection = {
        nodeId: pending.nodeId,
        handleName: parsed.name,
        handleType: pending.handleType,
        portType: sourcePort,
        flowPosition,
      };
      openSearch({
        screenPosition: { x: clientX, y: clientY },
        flowPosition,
        connection,
      });
    },
    [openSearch, reactFlow, addRegionPort, addEditorEdge],
  );

  // ─── Spacebar+drag node into / out of a region: re-parent on drop ─────────
  // The user holds Space while dragging. If space is touched at any point
  // during the drag (start, middle, or end) and the cursor is over a
  // region at drop, the node becomes a child of that region. Dropping
  // outside any region detaches the node back to root.
  const duplicateSelection = useEditor((s) => s.duplicateSelection);
  /**
   * Alt-drag state. Set on alt + onNodeDragStart; cleared at drag stop.
   * While set, the `onNodesChange` wrapper below redirects React Flow's
   * position updates from each `originalIds[i]` to `duplicateIds[i]` so
   * the originals literally don't move — their incoming/outgoing wires
   * stay anchored at the start position.
   */
  const altDragRef = useRef<{
    originalIds: string[];
    duplicateIds: string[];
  } | null>(null);
  const onNodeDragStart = useCallback(
    (e: React.MouseEvent | MouseEvent, node: RFNode) => {
      dragSpaceTouchedRef.current = spaceDownRef.current;
      if (e.altKey) {
        const cur = useEditor.getState();
        if (!cur.nodes.some((n) => n.selected)) {
          useEditor.setState((s) => ({
            nodes: s.nodes.map((n) => ({ ...n, selected: n.id === node.id })),
          }));
        }
        const s = useEditor.getState();
        const selected = s.nodes.filter((n) => n.selected);
        const originalIds = selected.map((n) => n.id);
        const duplicateIds = duplicateSelection({ x: 0, y: 0 });
        altDragRef.current = { originalIds, duplicateIds };
      }
    },
    [duplicateSelection],
  );

  // Wrap React Flow's onNodesChange to redirect position updates onto
  // the duplicates during an alt-drag. Net effect: originals stay put
  // with their wires intact; duplicates follow the cursor.
  const onNodesChange = useCallback(
    (changes: import('@xyflow/react').NodeChange[]) => {
      const alt = altDragRef.current;
      if (!alt) {
        onNodesChangeStore(changes);
        return;
      }
      const idxOfOriginal = new Map<string, number>();
      alt.originalIds.forEach((id, i) => idxOfOriginal.set(id, i));
      const remapped = changes.map((c) => {
        if (c.type !== 'position') return c;
        const idx = idxOfOriginal.get(c.id);
        if (idx === undefined) return c;
        const dupId = alt.duplicateIds[idx];
        if (!dupId) return c;
        return { ...c, id: dupId };
      });
      onNodesChangeStore(remapped);
    },
    [onNodesChangeStore],
  );

  const onNodeDrag = useCallback(
    (e: React.MouseEvent | MouseEvent, node: RFNode) => {
      // ─── Auto-grow the parent region as the child reaches its edge ───
      // Runs every onNodeDrag frame. Updates state directly (no history
      // push) so it doesn't spam the undo stack mid-drag.
      //
      // Skipped when the user is space-dragging (reparent mode) — the
      // child is on its way OUT of the region; growing it would fight
      // the user's intent.
      //
      // Right/bottom growth is simple: just expand width/height. Left/top
      // is geometric: extend the parent backwards (`position -= grow`,
      // `width += grow`) AND shift every other child forward by the same
      // amount so they stay at the same world coordinate. The dragged
      // child intentionally stays at its relative position so it moves
      // with the cursor into the newly-created space.
      const reparenting = spaceDownRef.current || dragSpaceTouchedRef.current;
      if (node.parentId && !reparenting && prefsSnapshot.autoGrowRegions) {
        const parent = reactFlow.getNodes().find((n) => n.id === node.parentId);
        if (parent) {
          const { w: pw, h: ph } = regionSize(parent);
          const { w: cw, h: ch } = rfNodeBox(node).size;
          const childRight = node.position.x + cw;
          const childBottom = node.position.y + ch;

          let growLeft = 0;
          let growTop = 0;
          let growRight = 0;
          let growBottom = 0;
          if (node.position.x < REGION_GROW_TRIGGER) {
            growLeft = Math.ceil(REGION_GROW_PADDING - node.position.x);
          }
          if (node.position.y < REGION_GROW_TRIGGER) {
            growTop = Math.ceil(REGION_GROW_PADDING - node.position.y);
          }
          if (childRight + REGION_GROW_TRIGGER > pw) {
            growRight = Math.ceil(childRight + REGION_GROW_PADDING - pw);
          }
          if (childBottom + REGION_GROW_TRIGGER > ph) {
            growBottom = Math.ceil(childBottom + REGION_GROW_PADDING - ph);
          }

          if (growLeft + growTop + growRight + growBottom > 0) {
            const newW = pw + growLeft + growRight;
            const newH = ph + growTop + growBottom;
            const parentId = node.parentId;
            const draggedId = node.id;
            paramStore.setKey(parentId, 'width', newW);
            paramStore.setKey(parentId, 'height', newH);
            useEditor.setState((s) => ({
              nodes: s.nodes.map((n) => {
                if (n.id === parentId) {
                  return {
                    ...n,
                    position: {
                      x: n.position.x - growLeft,
                      y: n.position.y - growTop,
                    },
                    data: { ...n.data, params: { ...n.data.params, width: newW, height: newH } },
                    style: { ...(n.style ?? {}), width: newW, height: newH },
                  };
                }
                // Shift other children so they stay at the same world
                // coords after the parent moves left/up. Only needed when
                // growing on the left or top side; right/bottom growth
                // leaves children untouched.
                if (
                  (growLeft > 0 || growTop > 0) &&
                  n.parentId === parentId &&
                  n.id !== draggedId
                ) {
                  return {
                    ...n,
                    position: { x: n.position.x + growLeft, y: n.position.y + growTop },
                  };
                }
                return n;
              }),
            }));
          }
        }
      }

      if (spaceDownRef.current) dragSpaceTouchedRef.current = true;
      if (!dragSpaceTouchedRef.current) {
        setHoverRegion(null);
        return;
      }
      const { clientX, clientY } = e as MouseEvent;
      const dropFlowPos = reactFlow.screenToFlowPosition({ x: clientX, y: clientY });
      const allNodes = reactFlow.getNodes();
      const target = findRegionContaining(allNodes, dropFlowPos, node.id);
      setHoverRegion(target ? target.id : null);
    },
    [reactFlow],
  );

  const onNodeDragStop = useCallback(
    (e: React.MouseEvent | MouseEvent, node: RFNode) => {
      // ─── Alt-drag finish ───────────────────────────────────────────────
      // The `onNodesChange` wrapper above already routed every position
      // update onto the duplicates, so the originals never moved and
      // the duplicates are at the cursor's end position. All that's
      // left is to hand selection to the duplicates and clear the
      // alt-drag state.
      const alt = altDragRef.current;
      altDragRef.current = null;
      if (alt) {
        const originalSet = new Set(alt.originalIds);
        const duplicateSet = new Set(alt.duplicateIds);
        useEditor.setState((cur) => ({
          nodes: cur.nodes.map((n) => {
            if (originalSet.has(n.id)) return { ...n, selected: false };
            if (duplicateSet.has(n.id)) return { ...n, selected: true };
            return n;
          }),
        }));
        dragSpaceTouchedRef.current = false;
        setHoverRegion(null);
        return;
      }

      const reparent = dragSpaceTouchedRef.current || spaceDownRef.current;
      dragSpaceTouchedRef.current = false;
      setHoverRegion(null);

      // ─── Reparent (only when space-drag) ───────────────────────────────
      if (reparent) {
        const { clientX, clientY } = e as MouseEvent;
        const dropFlowPos = reactFlow.screenToFlowPosition({ x: clientX, y: clientY });
        const allNodes = reactFlow.getNodes();
        const target = findRegionContaining(allNodes, dropFlowPos, node.id);

        const currentParent = node.parentId ?? null;
        const targetParent = target ? target.id : null;
        if (currentParent !== targetParent) {
          // Translate position: current parent's local → absolute → new parent's local.
          let absPos = { ...node.position };
          if (currentParent) {
            const parent = allNodes.find((n) => n.id === currentParent);
            if (parent) {
              absPos = {
                x: absPos.x + parent.position.x,
                y: absPos.y + parent.position.y,
              };
            }
          }
          let newPos = absPos;
          if (target) {
            newPos = {
              x: absPos.x - target.position.x,
              y: absPos.y - target.position.y,
            };
          }
          setNodeParent(node.id, targetParent, newPos);
        }
      }
    },
    [reactFlow, setNodeParent],
  );

  // ─── Right-click on empty pane ─────────────────────────────────────────────
  const onPaneContextMenu = useCallback(
    (e: React.MouseEvent | MouseEvent) => {
      e.preventDefault();
      const flowPos = reactFlow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
      const items: ContextMenuItem[] = [
        {
          label: 'Add node…',
          onSelect: () =>
            openSearch({
              screenPosition: { x: e.clientX, y: e.clientY },
              flowPosition: flowPos,
            }),
          shortcut: 'Dbl-click',
        },
        {
          label: 'Add comment',
          onSelect: () => addNode('meta/comment', flowPos),
        },
        { separator: true, label: '', onSelect: () => {} },
        {
          label: 'Paste',
          shortcut: 'Ctrl+V',
          disabled: !hasClipboard(),
          onSelect: () => pasteAt(flowPos),
        },
      ];
      openContext({ screenPosition: { x: e.clientX, y: e.clientY }, items });
    },
    [addNode, openContext, openSearch, hasClipboard, pasteAt, reactFlow],
  );

  // ─── Right-click on a node ─────────────────────────────────────────────────
  const onNodeContextMenu = useCallback(
    (e: React.MouseEvent, node: RFNode) => {
      e.preventDefault();
      // Make sure the node is selected so cut/copy use the right target.
      if (!node.selected) {
        useEditor.setState((s) => ({
          nodes: s.nodes.map((n) => ({ ...n, selected: n.id === node.id })),
        }));
      }
      const flowPos = reactFlow.screenToFlowPosition({
        x: e.clientX + 24,
        y: e.clientY + 24,
      });

      const items: ContextMenuItem[] = [
        {
          label: 'Duplicate',
          shortcut: 'Ctrl+D',
          onSelect: () => duplicateNode(node.id),
        },
        {
          label: 'Copy',
          shortcut: 'Ctrl+C',
          onSelect: () => copySelected(),
        },
        {
          label: 'Cut',
          shortcut: 'Ctrl+X',
          onSelect: () => {
            copySelected();
            deleteNode(node.id);
          },
        },
        {
          label: 'Paste',
          shortcut: 'Ctrl+V',
          disabled: !hasClipboard(),
          onSelect: () => pasteAt(flowPos),
        },
      ];

      // Region-membership items (skip on region nodes themselves — they
      // can't sit inside other regions in the current model).
      const allNodes = reactFlow.getNodes();
      const nodeDefType = (node.data as { defType?: string } | undefined)?.defType;
      const isRegion = nodeDefType ? !!nodeRegistry.get(nodeDefType)?.regionKind : false;
      if (!isRegion) {
        items.push({ separator: true, label: '', onSelect: () => {} });
        if (node.parentId) {
          items.push({
            label: 'Move out of region',
            shortcut: 'Space+drag',
            onSelect: () => moveNodeToParent(node, null, allNodes, setNodeParent),
          });
        }
        // List all regions that could receive this node.
        const targets = allNodes.filter((n) => {
          const t = (n.data as { defType?: string } | undefined)?.defType;
          return t && !!nodeRegistry.get(t)?.regionKind && n.id !== node.parentId && n.id !== node.id;
        });
        for (const region of targets) {
          const regionLabel =
            ((region.data as { params?: Record<string, unknown> } | undefined)
              ?.params?.label as string | undefined) ?? 'Region';
          items.push({
            label: `Move into ${regionLabel}`,
            onSelect: () => moveNodeToParent(node, region.id, allNodes, setNodeParent),
          });
        }
      }

      // Group-into-subpatch — only if there's a selection (>= 1 selected).
      const selectedCount = reactFlow.getNodes().filter((n) => n.selected).length;
      if (selectedCount >= 1) {
        items.push({ separator: true, label: '', onSelect: () => {} });
        items.push({
          label: selectedCount === 1
            ? 'Wrap in subpatch'
            : `Group ${selectedCount} into subpatch`,
          shortcut: 'Ctrl+G',
          onSelect: () => groupSelectionIntoSubpatch(),
        });
      }

      items.push({ separator: true, label: '', onSelect: () => {} });
      items.push({
        label: 'Delete',
        shortcut: 'Del',
        danger: true,
        onSelect: () => deleteNode(node.id),
      });

      openContext({ screenPosition: { x: e.clientX, y: e.clientY }, items });
    },
    [copySelected, deleteNode, duplicateNode, hasClipboard, openContext, pasteAt, reactFlow, setNodeParent],
  );

  // Block the browser context menu anywhere inside the editor — React Flow's
  // event hooks above handle the pane/node cases; everything else we want
  // simply suppressed.
  const onContainerContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
  }, []);

  // Bridge window-level keyboard events (Tab opens search, Ctrl+V pastes)
  // into React Flow's coordinate projector. The listeners live here because
  // `useReactFlow` only works inside the provider.
  useEffect(() => {
    function onOpenSearch(e: Event) {
      const ev = e as CustomEvent<{ x: number; y: number }>;
      const screen = ev.detail;
      const flow = reactFlow.screenToFlowPosition(screen);
      openSearch({ screenPosition: screen, flowPosition: flow });
    }
    function onPaste(e: Event) {
      const ev = e as CustomEvent<{ x: number; y: number }>;
      const flow = reactFlow.screenToFlowPosition(ev.detail);
      pasteAt(flow);
    }
    function onFocusNode(e: Event) {
      const ev = e as CustomEvent<string>;
      const id = ev.detail;
      if (!id) return;
      // Pan + zoom to the node. fitView accepts node ids via `nodes` filter.
      reactFlow.fitView({ nodes: [{ id }], duration: 300, padding: 0.4 });
    }
    function onFitSelection() {
      // Fit-view to the current selection (or to every node if nothing
      // is selected). Triggered by the global F shortcut.
      const s = useEditor.getState();
      const ids = s.nodes.filter((n) => n.selected).map((n) => ({ id: n.id }));
      if (ids.length > 0) {
        reactFlow.fitView({ nodes: ids, duration: 300, padding: 0.4 });
      } else {
        reactFlow.fitView({ duration: 300, padding: 0.2 });
      }
    }
    window.addEventListener('floret:open-search', onOpenSearch);
    window.addEventListener('floret:paste', onPaste);
    window.addEventListener('floret:focus-node', onFocusNode);
    window.addEventListener('floret:fit-selection', onFitSelection);
    return () => {
      window.removeEventListener('floret:open-search', onOpenSearch);
      window.removeEventListener('floret:paste', onPaste);
      window.removeEventListener('floret:focus-node', onFocusNode);
      window.removeEventListener('floret:fit-selection', onFitSelection);
    };
  }, [openSearch, pasteAt, reactFlow]);

  // ─── Region draw mode mouse + key handlers ──────────────────────────────
  // When `drawingRegion` is set, track cursor moves to update the preview
  // rectangle, and commit on the next click (or cancel with Escape).
  useEffect(() => {
    if (!drawingRegion) return;
    function onMove(e: MouseEvent) {
      const flow = reactFlow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
      setDrawingRegion((prev) => (prev ? { ...prev, currentFlow: flow } : prev));
    }
    function onClick(e: MouseEvent) {
      if (e.button !== 0) return; // left-click only
      const flow = reactFlow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
      const draw = drawingRegion;
      if (!draw) return;
      const x = Math.min(draw.startFlow.x, flow.x);
      const y = Math.min(draw.startFlow.y, flow.y);
      const w = Math.max(60, Math.abs(flow.x - draw.startFlow.x));
      const h = Math.max(60, Math.abs(flow.y - draw.startFlow.y));
      addRegion(draw.kind, { x, y }, { width: w, height: h, autoReparent: true });
      setDrawingRegion(null);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') setDrawingRegion(null);
    }
    window.addEventListener('mousemove', onMove);
    // Use capture so we beat React Flow's own click handlers (which
    // would otherwise deselect / start a panning gesture).
    window.addEventListener('click', onClick, true);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('click', onClick, true);
      window.removeEventListener('keydown', onKey);
    };
  }, [drawingRegion, reactFlow, addRegion]);

  // Spacebar tracking — flips the reparent-on-drop modifier. We listen
  // at window level so it works regardless of focus, but skip when
  // typing in an input/textarea.
  useEffect(() => {
    function isEditing(target: EventTarget | null): boolean {
      const el = target as HTMLElement | null;
      if (!el) return false;
      const tag = el.tagName;
      return tag === 'INPUT' || tag === 'TEXTAREA' || el.isContentEditable;
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.code !== 'Space' || isEditing(e.target)) return;
      // Prevent page scroll on space.
      e.preventDefault();
      if (spaceDownRef.current) return;
      spaceDownRef.current = true;
      setReparentMode(true);
      document.body.setAttribute('data-reparent-mode', '1');
    }
    function onKeyUp(e: KeyboardEvent) {
      if (e.code !== 'Space') return;
      if (!spaceDownRef.current) return;
      spaceDownRef.current = false;
      setReparentMode(false);
      document.body.removeAttribute('data-reparent-mode');
    }
    function onBlur() {
      spaceDownRef.current = false;
      setReparentMode(false);
      document.body.removeAttribute('data-reparent-mode');
    }
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', onBlur);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
    };
  }, []);

  // Type assertion for nodes since React Flow's generic Node type narrows
  // strictly on `data`; our editor store holds a stricter `FloretNodeData`.
  return (
    <div
      ref={wrapperRef}
      className="canvas-wrapper"
      onDrop={onDrop}
      onDragOver={onDragOver}
      onKeyDown={onKeyDown}
      onContextMenu={onContainerContextMenu}
      onDoubleClick={onWrapperDoubleClick}
      tabIndex={0}
    >
      <ReactFlow
        nodes={projectedNodes as never}
        edges={projectedEdges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onConnectStart={onConnectStart}
        onConnectEnd={onConnectEnd}
        onNodeDragStart={onNodeDragStart}
        onNodeDrag={onNodeDrag}
        onNodeDragStop={onNodeDragStop}
        onPaneClick={() => {
          // Single click on empty canvas clears the Port Pill selection
          // alongside React Flow's own clear-of-node-selection behaviour.
          // (Double-click → node search lives on the wrapper's native
          // dblclick — see onWrapperDoubleClick.)
          clearRegionPortSelection();
        }}
        onPaneContextMenu={onPaneContextMenu}
        onNodeContextMenu={onNodeContextMenu}
        onEdgeDoubleClick={onEdgeDoubleClick}
        onEdgeContextMenu={onEdgeContextMenu}
        isValidConnection={isValidConnection}
        proOptions={{ hideAttribution: true }}
        zoomOnDoubleClick={false}
        colorMode="dark"
        deleteKeyCode={null /* handled in onKeyDown to skip during input focus */}
        // Selection-first navigation:
        //   - left-drag on empty pane  → box-select (contain semantics)
        //   - middle-mouse drag        → pan
        //   - Alt + left-drag on pane  → pan (3D-app convention)
        //   - Alt + left-drag on node  → duplicate-drag (onNodeDragStart, unchanged)
        // Space stays reserved for the reparent modifier — we explicitly
        // override RF's default pan-activation key ('Space') with 'Alt'.
        panOnDrag={[1] /* middle button only */}
        selectionOnDrag
        selectionMode={SelectionMode.Full}
        panActivationKeyCode="Alt"
        // React Flow's default minZoom is 0.5; halve it so the user can
        // pull back far enough to see large patches (sprawling sample
        // graphs, subpatch expansions) in their entirety.
        minZoom={0.25}
      >
        <Background color="#2a2f38" gap={16} />
        <Controls />
        <MiniMap
          pannable
          zoomable
          maskColor="rgba(0,0,0,0.6)"
          nodeColor={(n) => {
            const defType = (n.data as { defType?: string } | undefined)?.defType;
            if (!defType) return 'var(--fg-2)';
            return categoryColor(nodeRegistry.get(defType)?.category);
          }}
          nodeStrokeColor="rgba(0,0,0,0.6)"
          nodeStrokeWidth={1}
        />
      </ReactFlow>
      {drawingRegion && (
        <RegionDrawPreview
          startFlow={drawingRegion.startFlow}
          currentFlow={drawingRegion.currentFlow}
          kind={drawingRegion.kind}
        />
      )}
      <SubpatchBreadcrumb
        viewStack={viewStack}
        nodes={nodes}
        onNavigate={(depth) => setViewStack(viewStack.slice(0, depth))}
        onExit={exitSubpatch}
      />
      {nodes.length === 0 && <EmptyCanvasHint />}
      <NodeSearch />
      <ContextMenu />
    </div>
  );
}

/**
 * Top-left breadcrumb. Visible at root view too — there it just shows
 * the label "Project" as a non-clickable header. Each subpatch segment
 * is clickable: clicking depth N navigates back to that level.
 */
function SubpatchBreadcrumb({
  viewStack, nodes, onNavigate, onExit,
}: {
  viewStack: string[];
  nodes: ReadonlyArray<{ id: string; data: { params?: Record<string, unknown>; defType?: string } }>;
  onNavigate: (depth: number) => void;
  onExit: () => void;
}) {
  const segments: { label: string; depth: number }[] = [{ label: 'Project', depth: 0 }];
  for (let i = 0; i < viewStack.length; i++) {
    const id = viewStack[i]!;
    const n = nodes.find((x) => x.id === id);
    const label = (n?.data.params?.label as string | undefined) ?? 'Subpatch';
    segments.push({ label, depth: i + 1 });
  }
  return (
    <div className="subpatch-breadcrumb" data-empty={viewStack.length === 0 ? 1 : 0}>
      {segments.map((s, i) => (
        <span key={i} className="subpatch-crumb-row">
          {i > 0 && <span className="subpatch-crumb-sep">/</span>}
          <button
            className="subpatch-crumb"
            data-current={i === segments.length - 1 ? 1 : 0}
            onClick={() => onNavigate(s.depth)}
            title={i === segments.length - 1 ? 'Current view' : `Go to ${s.label}`}
          >
            {s.label}
          </button>
        </span>
      ))}
      {viewStack.length > 0 && (
        <button
          className="subpatch-crumb-exit"
          onClick={onExit}
          title="Exit one level (O)"
        >
          ↑ O
        </button>
      )}
    </div>
  );
}

/**
 * Muted hint shown when the canvas is empty — the first thing a new
 * user sees, so it has to point at the three main ways to populate
 * the canvas without being noisy.
 */
function EmptyCanvasHint() {
  return (
    <div className="empty-canvas-hint">
      <div className="empty-canvas-line">
        <kbd>Tab</kbd> to search nodes
      </div>
      <div className="empty-canvas-line">
        Drag from the left panel
      </div>
      <div className="empty-canvas-line">
        Or load a <em>Sample</em>
      </div>
    </div>
  );
}

/**
 * Translucent preview rectangle drawn while the user is rubber-banding
 * a new region. Positioned in screen coords by projecting the two
 * flow-coord corners back through `flowToScreenPosition`.
 */
function RegionDrawPreview({
  startFlow, currentFlow, kind,
}: { startFlow: { x: number; y: number }; currentFlow: { x: number; y: number }; kind: 'foreach' | 'if' | 'cache' }) {
  const reactFlow = useReactFlow();
  const a = reactFlow.flowToScreenPosition(startFlow);
  const b = reactFlow.flowToScreenPosition(currentFlow);
  const left = Math.min(a.x, b.x);
  const top = Math.min(a.y, b.y);
  const width = Math.abs(b.x - a.x);
  const height = Math.abs(b.y - a.y);
  return (
    <div
      className={`region-draw-preview region-draw-preview-${kind}`}
      style={{ left, top, width, height }}
    >
      <span className="region-draw-preview-label">{kind}</span>
    </div>
  );
}

export function Canvas() {
  return (
    <ReactFlowProvider>
      <CanvasInner />
    </ReactFlowProvider>
  );
}

/**
 * Find the dragged pin's port type. For region nodes the port type lives
 * on `data.ports`; for normal nodes it lives on the NodeDef.
 */
function resolveDraggedPortType(
  nodeId: string,
  handleName: string,
  handleType: 'source' | 'target',
): import('../runtime/types').PortType | null {
  const defType = getDefTypeFor(nodeId);
  if (!defType) return null;
  const def = nodeRegistry.get(defType);
  if (!def) return null;

  if (def.regionKind) {
    const ports = getRegionPortsFor(nodeId);
    const port = ports?.find((p) => p.id === handleName);
    if (port) return port.type;
    // Fall through to NodeDef static ports (e.g. If.condition).
  }

  if (handleType === 'source') {
    // Source-side type lookup must honour dynamicOutputs so that wires
    // dragged from a polymorphic node (Repeat with a Color upstream, …)
    // carry the resolved type into wire validation and into any region
    // port that gets created on drop. Without this, a Repeat→ForEach
    // drop creates a port typed `any` instead of `color`.
    const params = getParamsFor(nodeId) ?? {};
    const outputs = def.dynamicOutputs
      ? getResolvedOutputs(def, params, getUpstreamTypesFor(nodeId))
      : def.outputs;
    return outputs.find((p) => p.name === handleName)?.type ?? null;
  }

  const port = getResolvedInputs(def, getParamsFor(nodeId) ?? {})
    .find((p) => p.name === handleName);
  return port?.type ?? null;
}

/**
 * Module-local helper: build the upstreamTypes map for a node by reading
 * the editor store's current edge/node lists. Used by
 * `resolveDraggedPortType` (which can't call useEditor() since it's not
 * a hook) so dynamicOutputs sees the same upstream picture the visual
 * layer does.
 */
function getUpstreamTypesFor(nodeId: string) {
  const state = useEditor.getState();
  return getUpstreamTypes(nodeId, state.edges, state.nodes);
}

/**
 * Move a node to a new parent (or detach to root), preserving its
 * visual position. Used by both the shift+drag handler and the
 * right-click "Move into / out of region" menu items.
 */
function moveNodeToParent(
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
 * Walk every handle in the DOM and mark it as compatible / incompatible
 * with the dragged pin. CSS uses the `data-wire-affinity` attribute to
 * brighten the matches and dim the rest, so the user can see at a
 * glance which ports will accept the wire.
 */
function paintWireAffinity(pending: OnConnectStartParams): void {
  if (!pending.nodeId || !pending.handleId || !pending.handleType) return;
  const parsed = parseHandle(pending.handleId);
  if (!parsed) return;
  const draggedType = resolveDraggedPortType(pending.nodeId, parsed.name, pending.handleType);
  if (!draggedType) return;

  // Dragging a source pin → match against TARGET handles. Vice-versa.
  const lookFor: 'source' | 'target' = pending.handleType === 'source' ? 'target' : 'source';

  document.querySelectorAll('.react-flow__handle').forEach((el) => {
    const h = el as HTMLElement;
    const nodeEl = h.closest('.react-flow__node') as HTMLElement | null;
    const nodeId = nodeEl?.dataset.id;
    if (!nodeId) return;
    const handleId = h.dataset.handleid ?? '';
    const role: 'source' | 'target' = h.classList.contains('source') ? 'source' : 'target';
    if (role !== lookFor) return;
    // No self-pin matching.
    if (nodeId === pending.nodeId && handleId === pending.handleId) return;

    const p = parseHandle(handleId);
    if (!p) return;
    const otherType = resolveDraggedPortType(nodeId, p.name, role);
    if (!otherType) return;
    const compat = arePortsCompatible(draggedType, otherType);
    h.setAttribute('data-wire-affinity', compat ? 'match' : 'miss');
  });
  document.body.setAttribute('data-wire-dragging', '1');
}

function clearWireAffinity(): void {
  document.body.removeAttribute('data-wire-dragging');
  document.querySelectorAll('[data-wire-affinity]').forEach((el) => {
    (el as HTMLElement).removeAttribute('data-wire-affinity');
  });
}

/**
 * Find a region whose bounds contain the given flow-coord point.
 * Used for shift+drag reparenting. Skips the node being dragged (you
 * can't parent a node to itself) and any descendant of it.
 */
function findRegionContaining(
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
function findRegionEdgeAtFlow(
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
