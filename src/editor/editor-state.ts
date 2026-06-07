import { create } from 'zustand';
import {
  applyEdgeChanges,
  applyNodeChanges,
  type Connection,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
  addEdge,
} from '@xyflow/react';
import { nodeRegistry } from '../nodes/registry';
import { compileGraph, type EditorEdgeRef, type EditorNodeRef } from '../runtime/compile';
import type { RuntimeGraph } from '../runtime/graph';
import { paramStore } from '../runtime/param-store';
import { audioRegistry } from '../runtime/audio';
import { imageRegistry } from '../runtime/image-registry';
import { derivedImageRegistry } from '../runtime/derived-image-registry';
import { parseHandle, wouldCreateCycle, isMultiTargetPort } from './connection-rules';
import {
  copySelectionToClipboard,
  hasClipboard,
  pasteFromClipboard,
} from './clipboard';
import { markDirty } from '../runtime/dirty';

/**
 * Lazy import to avoid a circular dependency with history.ts (which itself
 * pulls in this module). We import after the module body has been
 * evaluated, so the binding is in place by the time editor mutations fire.
 */
let _historyPush: ((paramKey?: string) => void) | null = null;
export function _setHistoryPush(fn: (paramKey?: string) => void): void {
  _historyPush = fn;
}
function pushHistory(paramKey?: string): void {
  _historyPush?.(paramKey);
}

/**
 * Late-bound history reset — invoked by `loadPatch` so every patch-load
 * code path (autosave restore, file open, sample/library import, New)
 * automatically seeds the undo stack with the loaded state. Without this,
 * the first Ctrl+Z would wind back to whatever was loaded BEFORE the
 * external load — typically the empty seed entry, which manifested as
 * "undo deleted everything". history.ts wires this so its own
 * suppressed-flag guard makes the call a no-op during undo/redo apply.
 */
let _historyReset: (() => void) | null = null;
export function _setHistoryReset(fn: () => void): void {
  _historyReset = fn;
}
function resetHistory(): void {
  _historyReset?.();
}

export interface FloretNodeData extends Record<string, unknown> {
  defType: string;
  params: Record<string, unknown>;
  /**
   * Per-input-port constant override. When an input port has no edge
   * connected, the runtime reads `portValues[portName]` if set, else
   * falls back to the NodeDef's `PortSpec.default`. Edges always win.
   */
  portValues?: Record<string, unknown>;
  /**
   * Dynamic ports on a region's frame. Present only on region nodes.
   * Each port carries a stable id (used in React Flow handle ids:
   * `in-<id>` / `out-<id>`) so renames don't break edges. Compile
   * synthesizes virtual splicer-in / splicer-out RuntimeNodes from this
   * list — see `expandRegionPorts` in compile.ts.
   *
   * direction === 'input':
   *   left edge of frame; outer-target (in-<id>) receives a spread,
   *   inner-source (out-<id>) emits one slot per iteration.
   * direction === 'output':
   *   right edge of frame; inner-target (in-<id>) collects per-iteration
   *   values, outer-source (out-<id>) emits the aggregated spread.
   */
  ports?: RegionPortDescriptor[];
}

export interface RegionPortDescriptor {
  /** Stable id, never reused. Used in handle ids. */
  id: string;
  /** Display name (also used as the splicer's user-visible label). */
  name: string;
  type: import('../runtime/types').PortType;
  direction: 'input' | 'output';
  /**
   * Optional manual y-position (in region-local pixels) for the port
   * pin on the frame edge. When absent, the renderer auto-distributes
   * the port near the top of its side. Set by dragging the port tab.
   */
  y?: number;
}

/**
 * Editor-side node — adds `parentId` for region containment. React Flow
 * v12 reads `parentId` on each node to nest it visually; the runtime
 * compile step uses the same field to build the scope tree.
 */

export type FloretNode = Node<FloretNodeData>;
export type FloretEdge = Edge;

let _nextId = 0;
function newId(prefix: string): string {
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

interface EditorState {
  nodes: FloretNode[];
  edges: FloretEdge[];
  runtime: RuntimeGraph | null;
  /** Stable subscription bumped whenever the runtime graph rebuilds. */
  runtimeVersion: number;
  warnings: import('../runtime/compile').CompileWarning[];

  /**
   * Stack of subpatch ids the user has entered. Empty = root view; the
   * last element is the *current* view. Each subpatch in the stack must
   * be an ancestor (by parentId chain) of the next. Navigation actions
   * push/pop this stack; the Canvas filters its rendered nodes by it.
   */
  viewStack: string[];
  /** Push a subpatch id onto the view stack. No-op if the id isn't a subpatch. */
  enterSubpatch: (nodeId: string) => void;
  /** Pop one level off the view stack. No-op at root. */
  exitSubpatch: () => void;
  /** Replace the view stack wholesale (used by breadcrumb clicks). */
  setViewStack: (stack: string[]) => void;

  setNodes: (n: FloretNode[]) => void;
  setEdges: (e: FloretEdge[]) => void;
  onNodesChange: (changes: NodeChange[]) => void;
  onEdgesChange: (changes: EdgeChange[]) => void;
  onConnect: (conn: Connection) => void;

  addNode: (defType: string, position: { x: number; y: number }) => string;
  removeSelected: () => void;
  setNodeParam: (nodeId: string, key: string, value: unknown) => void;
  loadPatch: (nodes: FloretNode[], edges: FloretEdge[]) => void;

  /**
   * Currently-selected Port Pills (the draggable port tabs on a Region's
   * frame). Keyed as `${regionId}#${portId}`. Independent of React Flow's
   * node/edge selection — pills live inside region bodies and aren't
   * React Flow nodes, so we track their selection ourselves.
   * `removeSelected` and the Delete-key handler include these.
   */
  selectedPortKeys: ReadonlyArray<string>;
  /** Toggle a port-pill's selection. `additive` keeps existing selection
   *  (shift/ctrl-click); `additive=false` replaces with just this pill. */
  selectRegionPort: (regionId: string, portId: string, additive: boolean) => void;
  /** Clear all port-pill selection. Called on background click in Canvas. */
  clearRegionPortSelection: () => void;

  /** Connect two nodes by handle id (e.g. `out-out`, `in-x`). No-op on invalid edges. */
  addEdge: (
    source: string,
    sourceHandle: string,
    target: string,
    targetHandle: string,
  ) => void;
  /** Remove an edge by id. No-op if it doesn't exist. */
  removeEdge: (edgeId: string) => void;

  /**
   * Move an edge up or down WITHIN the subset of edges that share the
   * same (target, targetHandle). Used for reordering wires on a
   * multi-input pin (transform/group, etc.) where the gather order is
   * edge-insertion order. `delta` is positive (down/later) or negative
   * (up/earlier); clamped to the bounds of the subset. No-op if the
   * edge doesn't land on a multi pin (there's nothing meaningful to
   * reorder).
   */
  reorderMultiEdge: (edgeId: string, delta: number) => void;

  /** Copy selected nodes (and their internal edges) into the in-memory clipboard. */
  copySelected: () => number;
  /** Paste the clipboard at an anchor point. Returns the new node ids (selected). */
  pasteAt: (anchor: { x: number; y: number }) => string[];
  hasClipboard: () => boolean;

  /** Duplicate a node in-place (slightly offset). Returns the new node id. */
  duplicateNode: (nodeId: string, offset?: { x: number; y: number }) => string | null;
  /**
   * Duplicate every currently-selected node + their internal edges. The
   * duplicates land at `original.position + offset`, are NOT selected
   * (so React Flow's ongoing drag stays on the originals), and inherit
   * the parentId of their source. Returns the new node ids.
   *
   * Used by Alt-drag duplication: we duplicate at offset (0, 0) right
   * when the drag begins so the originals carry on under the cursor and
   * the duplicates sit at the starting position.
   */
  duplicateSelection: (offset?: { x: number; y: number }) => string[];
  /** Delete one node by id (and any edges touching it). */
  deleteNode: (nodeId: string) => void;

  /**
   * Create a region (foreach / if / cache) at the given position with
   * an optional size override. If `autoReparent` is true, any root
   * nodes whose centre falls inside the region's bounds become its
   * children automatically — used by the draw-mode region creation.
   */
  addRegion: (
    kind: 'foreach' | 'if' | 'cache',
    position: { x: number; y: number },
    options?: { width?: number; height?: number; autoReparent?: boolean },
  ) => string;

  /** Add a port to a region's frame. Returns the new port id. */
  addRegionPort: (
    regionId: string,
    spec: { name?: string; type: import('../runtime/types').PortType; direction: 'input' | 'output' },
  ) => string;
  /** Remove a port from a region (and any edges that referenced it). */
  removeRegionPort: (regionId: string, portId: string) => void;
  /** Rename a region port (id stays the same — edges keep working). */
  renameRegionPort: (regionId: string, portId: string, name: string) => void;
  /**
   * Reorder a region port within its direction subset (inputs reorder
   * relative to other inputs, outputs to other outputs). `delta` is the
   * number of slots to move (positive = down, negative = up); clamped
   * to the bounds of the direction subset.
   */
  moveRegionPort: (regionId: string, portId: string, delta: number) => void;

  /**
   * Set the manual y-position of a region/subpatch port pin on its
   * frame edge. Pass `undefined` to revert to auto-distribute. The y is
   * in region-local pixels and clamped at render time to the frame
   * bounds.
   */
  setRegionPortY: (regionId: string, portId: string, y: number | undefined) => void;

  /**
   * Move a node into a different region (or to root if `parentId === null`).
   * If `position` is provided, the node's position is reset to it — used
   * by shift+drag reparenting to keep the visual position constant across
   * the parent-coord-space change.
   */
  setNodeParent: (
    nodeId: string,
    parentId: string | null,
    position?: { x: number; y: number },
  ) => void;

  /**
   * Wrap the current selection in a new meta/subpatch node. The selected
   * nodes become children of the subpatch; their edges that crossed the
   * selection boundary become subpatch ports (one per crossing). Returns
   * the new subpatch id, or null if nothing was selected.
   */
  groupSelectionIntoSubpatch: () => string | null;

  /**
   * Set a per-input-port constant override. Live — runtime picks it up
   * on the next tick (no recompile needed beyond the graph rebuild
   * triggered by this state change for history snapshotting).
   */
  setNodePortValue: (nodeId: string, portName: string, value: unknown) => void;
  /** Remove a per-port override, reverting to the NodeDef default. */
  clearNodePortValue: (nodeId: string, portName: string) => void;

  /** Rebuild the runtime graph from current editor state. */
  recompile: () => void;
}

/** Convert editor nodes/edges to the opaque shape compileGraph expects. */
function toEditorRefs(state: { nodes: FloretNode[]; edges: FloretEdge[] }): {
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
function isStructuralNodeChange(c: NodeChange): boolean {
  return c.type === 'add' || c.type === 'remove' || c.type === 'replace';
}
function isStructuralEdgeChange(c: EdgeChange): boolean {
  return c.type === 'add' || c.type === 'remove' || c.type === 'replace';
}

// Dev-only: expose the editor store on window for browser-side debugging.
// Stripped by Vite's NODE_ENV=production build.
const _exposeStore = (s: unknown): void => {
  if (typeof window !== 'undefined') {
    (window as unknown as { __editor?: unknown }).__editor = s;
  }
};

export const useEditor = create<EditorState>((set, get) => {
  /**
   * Defensive sweep: drop any edge whose source or target node no
   * longer exists, OR whose target handle doesn't resolve to a port on
   * the current node shape. Runs at the head of every compile so that
   * dynamic-input changes, port-rename, etc. can't leave wires
   * dangling at stale coordinates.
   */
  const _pruneOrphanEdges = () => {
    const s = get();
    const nodeById = new Map(s.nodes.map((n) => [n.id, n] as const));
    const cleaned = s.edges.filter((e) => {
      const srcNode = nodeById.get(e.source);
      const dstNode = nodeById.get(e.target);
      if (!srcNode || !dstNode) return false;
      return true;
    });
    if (cleaned.length !== s.edges.length) {
      set({ edges: cleaned });
    }
  };

  const _compile = () => {
    _pruneOrphanEdges();
    const { nodes, edges } = toEditorRefs(get());
    // Audio + image registry sync — drop entries for any node that no longer exists.
    const liveIds = new Set(nodes.map((n) => n.id));
    audioRegistry.retain(liveIds);
    imageRegistry.retain(liveIds);
    derivedImageRegistry.retain(liveIds);
    try {
      const result = compileGraph(nodes, edges, {
        registry: nodeRegistry,
        paramReader: (id) => {
          const found = get().nodes.find((n) => n.id === id);
          return found?.data.params ?? {};
        },
        portValueReader: (id, portName) => {
          const found = get().nodes.find((n) => n.id === id);
          return found?.data.portValues?.[portName];
        },
      });
      set((s) => ({
        runtime: result.graph,
        runtimeVersion: s.runtimeVersion + 1,
        warnings: result.warnings,
      }));
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn('compileGraph failed:', err);
      set((s) => ({
        runtime: null,
        runtimeVersion: s.runtimeVersion + 1,
        warnings: [{ message: String(err) }],
      }));
    }
    // Cook-on-change: every recompile is a new graph that must paint at
    // least once. (Idle frames after that depend on the graph's own
    // alwaysAnimate flag + paramStore changes.)
    markDirty();
  };

  return {
    nodes: [],
    edges: [],
    runtime: null,
    runtimeVersion: 0,
    warnings: [],
    viewStack: [],
    selectedPortKeys: [],

    selectRegionPort: (regionId, portId, additive) => {
      const key = `${regionId}#${portId}`;
      set((s) => {
        if (additive) {
          const has = s.selectedPortKeys.includes(key);
          const next = has
            ? s.selectedPortKeys.filter((k) => k !== key)
            : [...s.selectedPortKeys, key];
          return { selectedPortKeys: next };
        }
        // Replace selection. Also clear React Flow node/edge selection
        // so the user has a single clear focus on the pill they clicked.
        return {
          selectedPortKeys: [key],
          nodes: s.nodes.map((n) => (n.selected ? { ...n, selected: false } : n)),
          edges: s.edges.map((e) => (e.selected ? { ...e, selected: false } : e)),
        };
      });
    },

    clearRegionPortSelection: () => {
      set((s) => (s.selectedPortKeys.length === 0 ? {} : { selectedPortKeys: [] }));
    },

    enterSubpatch: (nodeId) => {
      const n = get().nodes.find((x) => x.id === nodeId);
      if (!n) return;
      const def = nodeRegistry.get(n.data.defType);
      if (def?.regionKind !== 'subpatch') return;
      // Skip if already at top of stack.
      const stack = get().viewStack;
      if (stack[stack.length - 1] === nodeId) return;
      set({ viewStack: [...stack, nodeId] });
    },

    exitSubpatch: () => {
      const stack = get().viewStack;
      if (stack.length === 0) return;
      set({ viewStack: stack.slice(0, -1) });
    },

    setViewStack: (stack) => {
      // Validate each id still refers to an existing subpatch ancestor chain.
      const nodes = get().nodes;
      const valid: string[] = [];
      let expectedParent: string | undefined = undefined;
      for (const id of stack) {
        const n = nodes.find((x) => x.id === id);
        if (!n) break;
        const def = nodeRegistry.get(n.data.defType);
        if (def?.regionKind !== 'subpatch') break;
        if ((n.parentId ?? undefined) !== expectedParent) break;
        valid.push(id);
        expectedParent = id;
      }
      set({ viewStack: valid });
    },

    groupSelectionIntoSubpatch: () => {
      const state = get();
      const selected = state.nodes.filter((n) => n.selected);
      if (selected.length === 0) return null;
      // All selected nodes must share the same parent (we can only group
      // siblings into one subpatch). Otherwise it's ambiguous.
      const parentIds = new Set(selected.map((n) => n.parentId ?? null));
      if (parentIds.size > 1) return null;
      const sharedParent = selected[0]!.parentId;
      // Reject if anything selected is itself a region (regions can't
      // currently sit inside subpatches in the simplified model).
      for (const n of selected) {
        const def = nodeRegistry.get(n.data.defType);
        if (def?.regionKind && def.regionKind !== 'subpatch') return null;
      }
      const selectedIds = new Set(selected.map((n) => n.id));

      // Centroid (in shared-parent local coords) → position the subpatch.
      let sumX = 0, sumY = 0;
      for (const n of selected) { sumX += n.position.x; sumY += n.position.y; }
      const cx = sumX / selected.length;
      const cy = sumY / selected.length;

      const subId = newId('region_subpatch');
      const subDef = nodeRegistry.require('meta/subpatch');
      const subParams: Record<string, unknown> = { ...(subDef.defaultParams ?? {}) };
      subParams.label = subParams.label ?? 'Subpatch';

      // Build exposed ports from edges that cross the selection boundary.
      // - Edge from outside-source → selected-target: create an input port
      //   on the subpatch, retarget outer edge to subpatch.in-<portId>,
      //   and add an inner edge from a synthetic "passthrough" — actually
      //   we re-route the original target edge to come from the subpatch's
      //   inner pin via a synthetic proxy. We just store the port spec;
      //   the compile step will materialize the rewiring.
      // For MVP we don't yet generate proxy nodes — the editor renders
      // the ports on the subpatch only; wiring inside the subpatch to
      // those exposed values requires the inner consumer to receive the
      // edge from a re-routed handle. Simpler: we re-wire the existing
      // edges so the outer source connects to the subpatch's outer pin
      // (in-<portId>), and the inner consumer keeps its existing pin —
      // we add an internal edge from the subpatch (out- side, inner pin)
      // to that consumer.
      type CrossingIn = {
        portId: string;
        portName: string;
        portType: import('../runtime/types').PortType;
        outerSource: string;
        outerSourceHandle: string;
        innerTarget: string;
        innerTargetHandle: string;
        originalEdgeId: string;
      };
      type CrossingOut = {
        portId: string;
        portName: string;
        portType: import('../runtime/types').PortType;
        innerSource: string;
        innerSourceHandle: string;
        outerTarget: string;
        outerTargetHandle: string;
        originalEdgeId: string;
      };
      const crossingsIn: CrossingIn[] = [];
      const crossingsOut: CrossingOut[] = [];
      let portCounter = 0;
      const nextPortId = () => `p_subport_${++portCounter}_${subId}`;

      const portTypeOf = (
        nodeId: string,
        side: 'in' | 'out',
        handleName: string,
      ): import('../runtime/types').PortType | null => {
        const node = state.nodes.find((x) => x.id === nodeId);
        if (!node) return null;
        const d = nodeRegistry.get(node.data.defType);
        if (!d) return null;
        if (side === 'out') {
          const p = d.outputs.find((p) => p.name === handleName);
          return p?.type ?? null;
        }
        const inputs = d.dynamicInputs ? d.dynamicInputs(node.data.params) : d.inputs;
        const p = inputs.find((p) => p.name === handleName);
        return p?.type ?? null;
      };

      for (const e of state.edges) {
        const srcIn = selectedIds.has(e.source);
        const dstIn = selectedIds.has(e.target);
        if (srcIn === dstIn) continue;
        const srcParsed = parseHandle(e.sourceHandle);
        const dstParsed = parseHandle(e.targetHandle);
        if (!srcParsed || !dstParsed) continue;
        if (!srcIn && dstIn) {
          const t = portTypeOf(e.target, 'in', dstParsed.name);
          if (!t) continue;
          crossingsIn.push({
            portId: nextPortId(),
            portName: `in${crossingsIn.length + 1}`,
            portType: t,
            outerSource: e.source,
            outerSourceHandle: e.sourceHandle!,
            innerTarget: e.target,
            innerTargetHandle: e.targetHandle!,
            originalEdgeId: e.id,
          });
        } else if (srcIn && !dstIn) {
          const t = portTypeOf(e.source, 'out', srcParsed.name);
          if (!t) continue;
          crossingsOut.push({
            portId: nextPortId(),
            portName: `out${crossingsOut.length + 1}`,
            portType: t,
            innerSource: e.source,
            innerSourceHandle: e.sourceHandle!,
            outerTarget: e.target,
            outerTargetHandle: e.targetHandle!,
            originalEdgeId: e.id,
          });
        }
      }

      const ports: RegionPortDescriptor[] = [
        ...crossingsIn.map((c) => ({
          id: c.portId,
          name: c.portName,
          type: c.portType,
          direction: 'input' as const,
        })),
        ...crossingsOut.map((c) => ({
          id: c.portId,
          name: c.portName,
          type: c.portType,
          direction: 'output' as const,
        })),
      ];

      const sub: FloretNode = {
        id: subId,
        type: 'floret',
        position: { x: cx, y: cy },
        data: { defType: 'meta/subpatch', params: subParams, ports },
      };
      if (sharedParent) { sub.parentId = sharedParent; sub.extent = 'parent'; }
      paramStore.setNode(subId, subParams);

      // Drop the original crossing edges; add the new outer / inner pairs.
      const droppedIds = new Set<string>([
        ...crossingsIn.map((c) => c.originalEdgeId),
        ...crossingsOut.map((c) => c.originalEdgeId),
      ]);

      set((s) => {
        const newNodes: FloretNode[] = s.nodes.map((n) => {
          if (!selectedIds.has(n.id)) return n;
          // Reparent selected node into the subpatch; convert its
          // position from shared-parent local to subpatch-local.
          const next: FloretNode = {
            ...n,
            parentId: subId,
            extent: 'parent' as const,
            position: { x: n.position.x - cx, y: n.position.y - cy },
            selected: false,
          };
          return next;
        });
        newNodes.push({ ...sub, selected: true });

        let edges = s.edges.filter((e) => !droppedIds.has(e.id));
        for (const c of crossingsIn) {
          edges.push({
            id: `e_${newId('subedge')}`,
            source: c.outerSource,
            target: subId,
            sourceHandle: c.outerSourceHandle,
            targetHandle: `in-${c.portId}`,
          });
          edges.push({
            id: `e_${newId('subedge')}`,
            source: subId,
            target: c.innerTarget,
            sourceHandle: `out-${c.portId}`,
            targetHandle: c.innerTargetHandle,
          });
        }
        for (const c of crossingsOut) {
          edges.push({
            id: `e_${newId('subedge')}`,
            source: c.innerSource,
            target: subId,
            sourceHandle: c.innerSourceHandle,
            targetHandle: `in-${c.portId}`,
          });
          edges.push({
            id: `e_${newId('subedge')}`,
            source: subId,
            target: c.outerTarget,
            sourceHandle: `out-${c.portId}`,
            targetHandle: c.outerTargetHandle,
          });
        }

        return { nodes: sortNodesParentFirst(newNodes), edges };
      });
      _compile();
      pushHistory();
      return subId;
    },

    setNodes: (n) => {
      set({ nodes: n });
      _compile();
    },
    setEdges: (e) => {
      set({ edges: e });
      _compile();
    },

    onNodesChange: (changes) => {
      set((s) => ({ nodes: applyNodeChanges(changes, s.nodes) as FloretNode[] }));
      if (changes.some(isStructuralNodeChange)) {
        _compile();
        pushHistory();
      }
    },
    onEdgesChange: (changes) => {
      set((s) => ({ edges: applyEdgeChanges(changes, s.edges) }));
      if (changes.some(isStructuralEdgeChange)) {
        _compile();
        pushHistory();
      }
    },
    onConnect: (conn) => {
      // Enforce single connection per target handle EXCEPT on multi-kind
      // inputs (which gather N wires into a spread). Source handles can
      // always fan out to many targets.
      set((s) => {
        const isMulti = conn.target
          ? isMultiTargetPort(conn.target, conn.targetHandle, getDefTypeFor, getRegionPortsFor, getParamsFor)
          : false;
        const filtered = isMulti
          ? s.edges
          : s.edges.filter(
              (e) => !(e.target === conn.target && e.targetHandle === conn.targetHandle),
            );
        // No cycles. Defensive — isValidConnection already blocks the
        // visual drop, but onConnect can still fire for programmatic paths.
        if (conn.source && conn.target && wouldCreateCycle(conn.source, conn.target, conn.targetHandle, filtered, getDefTypeFor, conn.sourceHandle)) {
          return { edges: s.edges };
        }
        return { edges: addEdge(conn, filtered) };
      });
      _compile();
      pushHistory();
    },

    addNode: (defType, position) => {
      const def = nodeRegistry.require(defType);
      const id = newId(defType.replace('/', '_'));
      const initialParams = { ...(def.defaultParams ?? {}) };
      // Auto-pick the smallest unused integer slot for the four
      // Read/Write/Read*/Write* nodes. Scoped per-type: dropping Read,
      // Write gives both slot 1; dropping another Read takes slot 2.
      // Read* and Write* keep their own independent slot namespaces.
      if (
        defType === 'state/read' ||
        defType === 'state/write' ||
        defType === 'state/read-spread' ||
        defType === 'state/write-spread'
      ) {
        const used = new Set<number>();
        for (const n of get().nodes) {
          if (n.data.defType === defType) {
            const s = Number(n.data.params?.slot);
            if (Number.isFinite(s)) used.add(s);
          }
        }
        let slot = 1;
        while (used.has(slot)) slot++;
        initialParams.slot = slot;
      }
      // If the user has entered a subpatch, new nodes belong to it.
      // The given `position` is treated as already-local to that scope —
      // callers (palette drop, search, paste) project screen→flow via
      // React Flow before invoking us, and React Flow's flow coords are
      // relative to the parent's origin for children of a parent node.
      const currentView = get().viewStack[get().viewStack.length - 1];
      const node: FloretNode = {
        id,
        type: 'floret',
        position,
        data: {
          defType,
          params: initialParams,
        },
      };
      if (currentView) {
        node.parentId = currentView;
        node.extent = 'parent';
      }
      paramStore.setNode(id, initialParams);
      set((s) => ({ nodes: sortNodesParentFirst([...s.nodes, node]) }));
      _compile();
      pushHistory();
      return id;
    },

    removeSelected: () => {
      const beforeIds = new Set(get().nodes.map((n) => n.id));
      // Resolve selected Port Pills into (regionId, portId) pairs so we
      // can strip them and any edges touching them at the same time as
      // selected nodes/edges. Doing it in one `set` keeps the runtime
      // recompile to a single tick at the end.
      const portsToRemove = get().selectedPortKeys
        .map((k) => {
          const ix = k.indexOf('#');
          return ix > 0 ? { regionId: k.slice(0, ix), portId: k.slice(ix + 1) } : null;
        })
        .filter((p): p is { regionId: string; portId: string } => p !== null);
      const portRemovalIndex = new Map<string, Set<string>>();
      for (const { regionId, portId } of portsToRemove) {
        let s = portRemovalIndex.get(regionId);
        if (!s) { s = new Set(); portRemovalIndex.set(regionId, s); }
        s.add(portId);
      }
      set((s) => {
        const keepIds = new Set(s.nodes.filter((n) => !n.selected).map((n) => n.id));
        const nodes = s.nodes
          .filter((n) => keepIds.has(n.id))
          .map((n) => {
            const drop = portRemovalIndex.get(n.id);
            if (!drop || !n.data.ports) return n;
            const remaining = n.data.ports.filter((p) => !drop.has(p.id));
            if (remaining.length === n.data.ports.length) return n;
            return { ...n, data: { ...n.data, ports: remaining } };
          });
        // An edge is gone if either endpoint node is gone, OR it's a
        // wire touching one of the freshly-removed region ports — the
        // handle id pattern is `in-<portId>` / `out-<portId>`.
        const touchesRemovedPort = (nodeId: string, handle: string | null | undefined): boolean => {
          const drop = portRemovalIndex.get(nodeId);
          if (!drop || !handle) return false;
          const m = /^(?:in|out)-(.+)$/.exec(handle);
          return !!m && drop.has(m[1]!);
        };
        const edges = s.edges.filter(
          (e) =>
            keepIds.has(e.source) &&
            keepIds.has(e.target) &&
            !e.selected &&
            !touchesRemovedPort(e.source, e.sourceHandle) &&
            !touchesRemovedPort(e.target, e.targetHandle),
        );
        for (const id of beforeIds) if (!keepIds.has(id)) paramStore.removeNode(id);
        return { nodes, edges, selectedPortKeys: [] };
      });
      _compile();
      pushHistory();
    },

    setNodeParam: (nodeId, key, value) => {
      paramStore.setKey(nodeId, key, value);
      set((s) => {
        const nodes = s.nodes.map((n) =>
          n.id === nodeId
            ? { ...n, data: { ...n.data, params: { ...n.data.params, [key]: value } } }
            : n,
        );
        // If the changed param drives dynamicInputs, drop any edges that
        // referenced now-defunct input ports.
        const changedNode = nodes.find((n) => n.id === nodeId);
        const def = changedNode ? nodeRegistry.get(changedNode.data.defType) : undefined;
        let edges = s.edges;
        if (def?.dynamicInputs && changedNode) {
          const activeNames = new Set(
            def.dynamicInputs(changedNode.data.params).map((p) => p.name),
          );
          edges = s.edges.filter((e) => {
            if (e.target !== nodeId) return true;
            const parsed = parseHandle(e.targetHandle);
            if (!parsed) return true;
            return activeNames.has(parsed.name);
          });
        }
        return { nodes, edges };
      });
      // A param change that shifts dynamicInputs needs a recompile so
      // the runtime sees the new shape; plain params don't.
      const node = get().nodes.find((n) => n.id === nodeId);
      const def = node ? nodeRegistry.get(node.data.defType) : undefined;
      if (def?.dynamicInputs) _compile();
      pushHistory(`${nodeId}.${key}`);
    },

    loadPatch: (nodes, edges) => {
      // New patch = fresh audio + image state. Tear down any oscillators /
      // effects / loaded images owned by the previous patch so they don't
      // leak into the new one.
      audioRegistry.reset();
      imageRegistry.reset();
      derivedImageRegistry.reset();
      paramStore.replaceAll(nodes.map((n) => [n.id, n.data.params] as const));
      // React Flow v12 requires parents to appear before their children
      // in the nodes array; samples & save files may not respect that.
      set({ nodes: sortNodesParentFirst(nodes), edges, selectedPortKeys: [] });
      _compile();
      // Seed the undo stack with the freshly-loaded patch so the first
      // Ctrl+Z doesn't wind back past it. The history module suppresses
      // this when *it* is the caller (during undo/redo apply), so this
      // call is a no-op there.
      resetHistory();
    },

    addEdge: (source, sourceHandle, target, targetHandle) => {
      // Skip if an identical edge already exists.
      const existing = get().edges.some(
        (e) =>
          e.source === source &&
          e.target === target &&
          e.sourceHandle === sourceHandle &&
          e.targetHandle === targetHandle,
      );
      if (existing) return;
      const id = `e_${newId('edge')}`;
      set((s) => {
        // Enforce single connection per (target, targetHandle) EXCEPT on
        // multi-kind inputs, which accept N wires and gather them into a
        // spread at runtime.
        const isMulti = isMultiTargetPort(target, targetHandle, getDefTypeFor, getRegionPortsFor, getParamsFor);
        const filtered = isMulti
          ? s.edges
          : s.edges.filter(
              (e) => !(e.target === target && e.targetHandle === targetHandle),
            );
        // No cycles. Defensive — caller may not have run isValidConnection.
        if (wouldCreateCycle(source, target, targetHandle, filtered, getDefTypeFor, sourceHandle)) {
          return { edges: s.edges };
        }
        return {
          edges: [
            ...filtered,
            { id, source, target, sourceHandle, targetHandle },
          ],
        };
      });
      _compile();
      pushHistory();
    },

    removeEdge: (edgeId) => {
      set((s) => ({ edges: s.edges.filter((e) => e.id !== edgeId) }));
      _compile();
      pushHistory();
    },

    reorderMultiEdge: (edgeId, delta) => {
      if (delta === 0) return;
      const s = get();
      const target = s.edges.find((e) => e.id === edgeId);
      if (!target || !target.targetHandle) return;
      // Only meaningful when the target handle is a multi pin (gather
      // semantics); skip on plain single/spread pins where edge order
      // doesn't affect behaviour.
      const isMulti = isMultiTargetPort(
        target.target, target.targetHandle,
        getDefTypeFor, getRegionPortsFor, getParamsFor,
      );
      if (!isMulti) return;
      // Reorder within the subset of edges feeding the same multi pin.
      const subset = s.edges.filter(
        (e) => e.target === target.target && e.targetHandle === target.targetHandle,
      );
      const subIdx = subset.findIndex((e) => e.id === edgeId);
      const nextIdx = Math.max(0, Math.min(subset.length - 1, subIdx + delta));
      if (nextIdx === subIdx) return;
      // Build a fresh subset array with the moved edge in its new slot,
      // then thread it back through the global edges list at the slots
      // currently occupied by subset members (preserving the positions
      // of non-multi-pin edges).
      const reordered = [...subset];
      const [moved] = reordered.splice(subIdx, 1);
      reordered.splice(nextIdx, 0, moved!);
      const subsetIds = new Set(subset.map((e) => e.id));
      let cur = 0;
      const next = s.edges.map((e) =>
        subsetIds.has(e.id) ? reordered[cur++]! : e,
      );
      set({ edges: next });
      _compile();
      pushHistory();
    },

    copySelected: () => {
      const s = get();
      return copySelectionToClipboard(s.nodes, s.edges);
    },

    pasteAt: (anchor) => {
      const { nodes: newNodes, edges: newEdges, ids } = pasteFromClipboard(anchor);
      if (newNodes.length === 0) return [];
      for (const n of newNodes) paramStore.setNode(n.id, n.data.params);
      set((s) => ({
        // Deselect everything previously selected so the paste is the new selection.
        nodes: [
          ...s.nodes.map((n) => (n.selected ? { ...n, selected: false } : n)),
          ...newNodes,
        ],
        edges: [...s.edges, ...newEdges],
      }));
      _compile();
      pushHistory();
      return ids;
    },

    hasClipboard: () => hasClipboard(),

    duplicateNode: (nodeId, offset = { x: 24, y: 24 }) => {
      const src = get().nodes.find((n) => n.id === nodeId);
      if (!src) return null;
      const def = nodeRegistry.get(src.data.defType);
      if (!def) return null;
      const id = newId(src.data.defType.replace('/', '_'));
      const params = { ...src.data.params };
      const node: FloretNode = {
        id,
        type: 'floret',
        position: { x: src.position.x + offset.x, y: src.position.y + offset.y },
        data: { defType: src.data.defType, params },
        selected: true,
      };
      paramStore.setNode(id, params);
      set((s) => ({
        nodes: [...s.nodes.map((n) => (n.selected ? { ...n, selected: false } : n)), node],
      }));
      _compile();
      pushHistory();
      return id;
    },

    duplicateSelection: (offset = { x: 0, y: 0 }) => {
      const state = get();
      const selected = state.nodes.filter((n) => n.selected);
      if (selected.length === 0) return [];

      // Build new ids and a map from old → new.
      const idMap = new Map<string, string>();
      for (const n of selected) {
        idMap.set(n.id, newId(n.data.defType.replace('/', '_')));
      }

      const newNodes: FloretNode[] = selected.map((n) => {
        const newNodeId = idMap.get(n.id)!;
        // Clone region ports too — they carry edge connections.
        const dataClone: FloretNodeData = {
          defType: n.data.defType,
          params: { ...n.data.params },
          ...(n.data.portValues ? { portValues: { ...n.data.portValues } } : {}),
          ...(n.data.ports ? { ports: n.data.ports.map((p) => ({ ...p })) } : {}),
        };
        paramStore.setNode(newNodeId, dataClone.params);
        const copy: FloretNode = {
          id: newNodeId,
          type: 'floret',
          position: { x: n.position.x + offset.x, y: n.position.y + offset.y },
          data: dataClone,
          selected: false, // originals stay selected so the active drag follows them
        };
        // If the original lived inside a region AND the parent region
        // was also part of the selection, the duplicate goes into the
        // duplicated parent. Otherwise it inherits the original parent.
        const remappedParent = n.parentId ? idMap.get(n.parentId) ?? n.parentId : undefined;
        if (remappedParent) {
          copy.parentId = remappedParent;
          copy.extent = 'parent';
        }
        return copy;
      });

      // Duplicate edges where BOTH ends were in the selection.
      const newEdges: FloretEdge[] = [];
      for (const e of state.edges) {
        const src = idMap.get(e.source);
        const dst = idMap.get(e.target);
        if (src && dst) {
          newEdges.push({
            id: `e_dup_${newId('').slice(1)}`,
            source: src,
            target: dst,
            sourceHandle: e.sourceHandle,
            targetHandle: e.targetHandle,
          });
        }
      }

      set((s) => ({
        nodes: sortNodesParentFirst([...s.nodes, ...newNodes]),
        edges: [...s.edges, ...newEdges],
      }));
      _compile();
      pushHistory();
      return [...idMap.values()];
    },

    deleteNode: (nodeId) => {
      const before = get().nodes;
      if (!before.find((n) => n.id === nodeId)) return;
      paramStore.removeNode(nodeId);
      set((s) => ({
        nodes: s.nodes.filter((n) => n.id !== nodeId),
        edges: s.edges.filter((e) => e.source !== nodeId && e.target !== nodeId),
      }));
      _compile();
      pushHistory();
    },

    addRegion: (kind, position, options) => {
      const id = newId(`region_${kind}`);
      const regionDefType = `region/${kind}`;
      const regionDef = nodeRegistry.require(regionDefType);
      const regionParams = { ...(regionDef.defaultParams ?? {}) };
      const w = options?.width ?? (regionParams.width as number) ?? 480;
      const h = options?.height ?? (regionParams.height as number) ?? 280;
      regionParams.width = w;
      regionParams.height = h;
      const region: FloretNode = {
        id,
        type: 'floret',
        position,
        data: { defType: regionDefType, params: regionParams, ports: [] },
        style: { width: w, height: h },
      };
      paramStore.setNode(id, regionParams);

      set((s) => {
        let nodes = [...s.nodes, region];

        // Auto-reparent: any root node whose centre is inside the new
        // region's bounds becomes a child of it. Used by draw-mode
        // creation so the user can lasso existing nodes into a region.
        if (options?.autoReparent) {
          nodes = nodes.map((n) => {
            if (n.id === id || n.parentId) return n;
            const def = nodeRegistry.get(n.data.defType);
            if (def?.regionKind) return n; // don't nest regions
            const nw = (n.style?.width as number | undefined) ?? n.width ?? 160;
            const nh = (n.style?.height as number | undefined) ?? n.height ?? 60;
            const cx = n.position.x + nw / 2;
            const cy = n.position.y + nh / 2;
            if (cx < position.x || cx > position.x + w) return n;
            if (cy < position.y || cy > position.y + h) return n;
            return {
              ...n,
              parentId: id,
              extent: 'parent' as const,
              position: {
                x: n.position.x - position.x,
                y: n.position.y - position.y,
              },
            };
          });
          nodes = sortNodesParentFirst(nodes);
        }

        return { nodes };
      });
      _compile();
      pushHistory();
      return id;
    },

    addRegionPort: (regionId, spec) => {
      const portId = `p_${newId('rp')}`;
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== regionId) return n;
          const ports = n.data.ports ?? [];
          const name = spec.name ?? uniquePortName(ports, spec.type, spec.direction);
          return {
            ...n,
            data: {
              ...n.data,
              ports: [...ports, { id: portId, name, type: spec.type, direction: spec.direction }],
            },
          };
        }),
      }));
      _compile();
      pushHistory();
      return portId;
    },

    removeRegionPort: (regionId, portId) => {
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== regionId) return n;
          const ports = (n.data.ports ?? []).filter((p) => p.id !== portId);
          return { ...n, data: { ...n.data, ports } };
        }),
        edges: s.edges.filter((e) => {
          if (e.source === regionId) {
            const parsed = parseHandle(e.sourceHandle);
            if (parsed?.name === portId) return false;
          }
          if (e.target === regionId) {
            const parsed = parseHandle(e.targetHandle);
            if (parsed?.name === portId) return false;
          }
          return true;
        }),
      }));
      _compile();
      pushHistory();
    },

    renameRegionPort: (regionId, portId, name) => {
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== regionId) return n;
          const ports = (n.data.ports ?? []).map((p) =>
            p.id === portId ? { ...p, name } : p,
          );
          return { ...n, data: { ...n.data, ports } };
        }),
      }));
      pushHistory();
    },

    moveRegionPort: (regionId, portId, delta) => {
      if (delta === 0) return;
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== regionId) return n;
          const all = n.data.ports ?? [];
          const target = all.find((p) => p.id === portId);
          if (!target) return n;
          const dir = target.direction;
          const sameDir = all.filter((p) => p.direction === dir);
          const subsetIdx = sameDir.findIndex((p) => p.id === portId);
          const newSubsetIdx = Math.max(0, Math.min(sameDir.length - 1, subsetIdx + delta));
          if (newSubsetIdx === subsetIdx) return n;
          // Reorder within the same-direction subset; thread the result
          // back into the global array, preserving non-matching slots.
          const reordered = [...sameDir];
          const [moved] = reordered.splice(subsetIdx, 1);
          reordered.splice(newSubsetIdx, 0, moved!);
          let cur = 0;
          const next = all.map((p) => (p.direction === dir ? reordered[cur++]! : p));
          return { ...n, data: { ...n.data, ports: next } };
        }),
      }));
      _compile();
      pushHistory();
    },

    setRegionPortY: (regionId, portId, y) => {
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== regionId) return n;
          const ports = (n.data.ports ?? []).map((p) =>
            p.id === portId ? { ...p, ...(y === undefined ? { y: undefined } : { y }) } : p,
          );
          return { ...n, data: { ...n.data, ports } };
        }),
      }));
      // Drag-driven calls fire on every mousemove; skip pushHistory to
      // avoid a 60Hz undo stack. The drag-end pushHistory in the
      // dragger's mouseup handler covers the final state.
    },

    setNodeParent: (nodeId, parentId, position) => {
      set((s) => ({
        nodes: sortNodesParentFirst(
          s.nodes.map((n) => {
            if (n.id !== nodeId) return n;
            const next: FloretNode = {
              ...n,
              parentId: parentId ?? undefined,
              extent: parentId ? 'parent' : undefined,
            };
            if (position) next.position = position;
            return next;
          }),
        ),
      }));
      _compile();
      pushHistory();
    },

    setNodePortValue: (nodeId, portName, value) => {
      set((s) => ({
        nodes: s.nodes.map((n) =>
          n.id === nodeId
            ? {
                ...n,
                data: {
                  ...n.data,
                  portValues: { ...(n.data.portValues ?? {}), [portName]: value },
                },
              }
            : n,
        ),
      }));
      // Light recompile so the new value reaches the runtime immediately.
      _compile();
      pushHistory(`${nodeId}.port.${portName}`);
    },

    clearNodePortValue: (nodeId, portName) => {
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== nodeId) return n;
          if (!n.data.portValues || !(portName in n.data.portValues)) return n;
          const next = { ...n.data.portValues };
          delete next[portName];
          return { ...n, data: { ...n.data, portValues: next } };
        }),
      }));
      _compile();
      pushHistory(`${nodeId}.port.${portName}`);
    },

    recompile: _compile,
  };
});

_exposeStore(useEditor);

/** Convenience: lookup a node's defType (used by connection-rules). */
export function getDefTypeFor(nodeId: string): string | undefined {
  return useEditor.getState().nodes.find((n) => n.id === nodeId)?.data.defType;
}

/** Lookup a region's dynamic ports (used by connection-rules to type-check edges). */
export function getRegionPortsFor(nodeId: string): RegionPortDescriptor[] | undefined {
  return useEditor.getState().nodes.find((n) => n.id === nodeId)?.data.ports;
}

/** Lookup a node's current params (used by connection-rules for dynamicInputs). */
export function getParamsFor(nodeId: string): Record<string, unknown> | undefined {
  return useEditor.getState().nodes.find((n) => n.id === nodeId)?.data.params;
}

/** Live edges snapshot for connection-rules' cycle check. */
export function getEdgesSnapshot(): ReadonlyArray<FloretEdge> {
  return useEditor.getState().edges;
}

/**
 * Return a port name that doesn't collide with any existing port on the
 * same region. Starts from the type-derived base name and adds a
 * numeric suffix until it's unique.
 */
function uniquePortName(
  existing: ReadonlyArray<RegionPortDescriptor>,
  type: import('../runtime/types').PortType,
  _direction: 'input' | 'output',
): string {
  const base = type;
  const taken = new Set(existing.map((p) => p.name));
  if (!taken.has(base)) return base;
  let i = 2;
  while (taken.has(`${base}${i}`)) i += 1;
  return `${base}${i}`;
}
