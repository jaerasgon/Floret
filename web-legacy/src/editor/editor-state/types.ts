import type { Connection, Edge, EdgeChange, Node, NodeChange } from '@xyflow/react';
import type { StoreApi } from 'zustand';
import type { PortType } from '../../runtime/types';
import type { RuntimeGraph } from '../../runtime/graph';
import type { CompileWarning } from '../../runtime/compile';

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
  type: PortType;
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

export interface EditorState {
  nodes: FloretNode[];
  edges: FloretEdge[];
  runtime: RuntimeGraph | null;
  /** Stable subscription bumped whenever the runtime graph rebuilds. */
  runtimeVersion: number;
  warnings: CompileWarning[];

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
    spec: { name?: string; type: PortType; direction: 'input' | 'output'; y?: number },
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

/** Zustand store accessors, shared by the editor-state slice factories. */
export type EditorSet = StoreApi<EditorState>['setState'];
export type EditorGet = () => EditorState;
