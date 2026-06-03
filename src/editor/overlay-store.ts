import { create } from 'zustand';

/**
 * Floating overlays in the editor — the node-search popover and the
 * right-click context menu. Both are dispatched from React Flow event
 * callbacks (or keyboard shortcuts) and rendered as siblings of the
 * canvas, so we keep their state in one tiny shared store.
 *
 * - `search` opens when the user double-clicks empty space, drags a
 *   wire to empty space, or presses Tab/`/`. If it was opened from a
 *   wire drop, `connection` carries the source node + handle and the
 *   *type* (so we can filter compatible nodes); on commit, the caller
 *   reads `connection` to also create the edge.
 *
 * - `context` is the right-click menu. Items are passed in at open
 *   time — the same component handles "pane" and "node" right-clicks
 *   with different item lists.
 */

export interface PendingConnection {
  /** Node id at the other end of the dragged wire. */
  nodeId: string;
  /** Handle name (without the `out-` / `in-` prefix). */
  handleName: string;
  /** Whether the drag originated at a source (output) or target (input) handle. */
  handleType: 'source' | 'target';
  /** Port type of the originating handle — for filtering compatible nodes. */
  portType: string;
  /** Approximate flow-coordinate position where the user released. */
  flowPosition: { x: number; y: number };
}

/**
 * "Insert into edge" mode — the user double-clicked a wire. When they
 * pick a node, the original edge is split: the picked node sits
 * in-between, with the edge's source feeding its first compatible
 * input and its first compatible output feeding the original target.
 */
export interface PendingInsertion {
  edgeId: string;
  source: string;
  sourceHandle: string;
  sourcePortType: string;
  target: string;
  targetHandle: string;
  targetPortType: string;
  /** Where to place the new node (midpoint of the wire in flow coords). */
  flowPosition: { x: number; y: number };
}

export interface ContextMenuItem {
  /** Display label. */
  label: string;
  /** Action; the store closes the menu before invoking. */
  onSelect: () => void;
  /** Optional keyboard hint shown on the right. */
  shortcut?: string;
  /** If true, render as a separator and ignore everything else. */
  separator?: boolean;
  /** If true, item is greyed out and not clickable. */
  disabled?: boolean;
  /** If true, give the item the destructive (red) styling. */
  danger?: boolean;
}

interface OverlayState {
  searchOpen: boolean;
  searchScreenPosition: { x: number; y: number };
  searchFlowPosition: { x: number; y: number };
  searchConnection: PendingConnection | null;
  searchInsertion: PendingInsertion | null;

  contextOpen: boolean;
  contextScreenPosition: { x: number; y: number };
  contextItems: ContextMenuItem[];

  openSearch: (args: {
    screenPosition: { x: number; y: number };
    flowPosition: { x: number; y: number };
    connection?: PendingConnection | null;
    insertion?: PendingInsertion | null;
  }) => void;
  closeSearch: () => void;

  openContext: (args: {
    screenPosition: { x: number; y: number };
    items: ContextMenuItem[];
  }) => void;
  closeContext: () => void;
}

export const useOverlay = create<OverlayState>((set) => ({
  searchOpen: false,
  searchScreenPosition: { x: 0, y: 0 },
  searchFlowPosition: { x: 0, y: 0 },
  searchConnection: null,
  searchInsertion: null,

  contextOpen: false,
  contextScreenPosition: { x: 0, y: 0 },
  contextItems: [],

  openSearch: ({ screenPosition, flowPosition, connection, insertion }) => {
    set({
      searchOpen: true,
      searchScreenPosition: screenPosition,
      searchFlowPosition: flowPosition,
      searchConnection: connection ?? null,
      searchInsertion: insertion ?? null,
      contextOpen: false,
    });
  },
  closeSearch: () => set({ searchOpen: false, searchConnection: null, searchInsertion: null }),

  openContext: ({ screenPosition, items }) => {
    set({
      contextOpen: true,
      contextScreenPosition: screenPosition,
      contextItems: items,
      searchOpen: false,
    });
  },
  closeContext: () => set({ contextOpen: false, contextItems: [] }),
}));
