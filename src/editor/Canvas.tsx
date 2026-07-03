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
import { buildIsValidConnection, categoryColor, parseHandle } from './connection-rules';
import { nodeRegistry } from '../nodes/registry';
import { type PendingConnection } from './overlay-store';
import { useOverlay } from './overlay-store';
import { NodeSearch } from './NodeSearch';
import { FavoritesMenu } from './FavoritesMenu';
import { ContextMenu } from './ContextMenu';
import { prefsSnapshot, usePrefs } from '../app/prefs';
import { paramStore } from '../runtime/param-store';

import {
  rfNodeBox,
  regionSize,
  REGION_GROW_TRIGGER,
  REGION_GROW_PADDING,
  findRegionContaining,
  findRegionEdgeAtFlow,
} from './canvas/region-hit';
import { resolveDraggedPortType, paintWireAffinity, clearWireAffinity } from './canvas/wire-affinity';
import { SubpatchBreadcrumb, EmptyCanvasHint, RegionDrawPreview } from './canvas/parts';
import { useProjectedGraph } from './canvas/use-projected-graph';
import { buildPaneMenu, buildEdgeMenu, buildNodeMenu } from './canvas/context-menus';
import { useCanvasEventBus, useRegionDrawTool, useSpacebarReparent } from './canvas/use-canvas-effects';

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
  const openFavorites = useOverlay((s) => s.openFavorites);
  const openContext = useOverlay((s) => s.openContext);
  const armedRegionKind = useOverlay((s) => s.armedRegionKind);
  const armRegionDraw = useOverlay((s) => s.armRegionDraw);
  const disarmRegionDraw = useOverlay((s) => s.disarmRegionDraw);

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

  // Subpatch view filtering + reparent extent-stripping + region drag-handles.
  const { projectedNodes, projectedEdges } = useProjectedGraph(
    nodes, edges, currentViewId, viewStack, reparentMode,
  );

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
      // Dragging a region from the palette arms the crosshair draw tool
      // rather than dropping a fixed-size box — the user sizes it with a
      // click-drag on the canvas, the same as every other region entry point.
      const def = nodeRegistry.get(defType);
      if (def?.regionKind && def.regionKind !== 'subpatch') {
        armRegionDraw(def.regionKind);
        return;
      }
      const position = reactFlow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
      addNode(defType, position);
    },
    [reactFlow, addNode, armRegionDraw],
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
      const items = buildEdgeMenu(edge, { reorderMultiEdge, removeEdge });
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

      // ─── Alt-drop on empty pane → drop a Probe (inspect + reroute) ────
      // Instead of the node-search popover, wire a Probe to the dragged pin
      // so you immediately see the data and can keep routing from it.
      const alt = 'altKey' in e && e.altKey;
      if (alt) {
        const probeId = addNode('meta/probe', flowPosition);
        if (pending.handleType === 'source') {
          addEditorEdge(pending.nodeId, pending.handleId, probeId, 'in-in');
        } else {
          addEditorEdge(probeId, 'out-out', pending.nodeId, pending.handleId);
        }
        return;
      }

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
    [openSearch, reactFlow, addRegionPort, addEditorEdge, addNode],
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
      const items = buildPaneMenu(
        flowPos,
        { x: e.clientX, y: e.clientY },
        { openSearch, addNode, hasClipboard, pasteAt },
      );
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
      const allNodes = reactFlow.getNodes();
      const selectedCount = allNodes.filter((n) => n.selected).length;
      const items = buildNodeMenu(node, flowPos, allNodes, selectedCount, {
        duplicateNode, copySelected, deleteNode, hasClipboard, pasteAt,
        setNodeParent, groupSelectionIntoSubpatch,
      });
      openContext({ screenPosition: { x: e.clientX, y: e.clientY }, items });
    },
    [copySelected, deleteNode, duplicateNode, groupSelectionIntoSubpatch, hasClipboard, openContext, pasteAt, reactFlow, setNodeParent],
  );

  // Block the browser context menu anywhere inside the editor — React Flow's
  // event hooks above handle the pane/node cases; everything else we want
  // simply suppressed.
  const onContainerContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
  }, []);

  // Window-level keyboard/event bridges that need React Flow's projector.
  useCanvasEventBus({ openSearch, openFavorites, pasteAt });
  useRegionDrawTool(armedRegionKind, setDrawingRegion, { addRegion, disarmRegionDraw });
  useSpacebarReparent(spaceDownRef, setReparentMode);

  // React Flow's colorMode drives its own control/minimap/handle theming.
  const theme = usePrefs((s) => s.theme);

  // Type assertion for nodes since React Flow's generic Node type narrows
  // strictly on `data`; our editor store holds a stricter `FloretNodeData`.
  return (
    <div
      ref={wrapperRef}
      className={`canvas-wrapper${armedRegionKind ? ' region-arming' : ''}`}
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
        colorMode={theme}
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
        // Trackpad navigation (design-tool feel):
        //   - two-finger move → pan (panOnScroll)
        //   - pinch          → zoom (zoomOnPinch, RF default)
        //   - ⌘/Ctrl + scroll → zoom (zoomActivationKeyCode)
        // zoomOnScroll is turned off so a plain two-finger scroll pans
        // instead of zooming.
        panOnScroll
        zoomOnScroll={false}
        zoomActivationKeyCode={['Meta', 'Control']}
        // React Flow's default minZoom is 0.5. Pull it way back so even large,
        // wide patches (long region chains, subpatch expansions, an auto-arranged
        // sprawl) fit entirely in view.
        minZoom={0.08}
      >
        {/* grid colour comes from --grid-line (themable) via a CSS override below */}
        <Background gap={22} size={1} />
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
      <FavoritesMenu />
      <ContextMenu />
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
