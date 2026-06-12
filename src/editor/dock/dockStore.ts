import { create } from 'zustand';
import { loadJSON, saveJSON } from '../../persistence/local-store';
import {
  type DockNode, type Side,
  group, split, collectPanels, findGroupOf, findGroup, removePanel as treeRemove,
  addTab, splitInsert, splitRoot, setSizes as treeSetSizes, setActive as treeSetActive,
} from './dockTree';

/**
 * The dock layout store — one recursive split-tree of docked panels plus
 * a flat list of floating panels and a set of hidden ones. Panels manage
 * their own membership: a panel ensures itself into the layout on mount
 * (defaulting to a sensible spot) and is removed explicitly when it's
 * genuinely gone (a Preview's node deleted, the Library toggled off).
 *
 * The serialisable layout (tree + floats + hidden) persists to
 * localStorage; live panel metadata (title, header actions, pinned) lives
 * in a separate module map since it carries React nodes.
 */

export interface FloatRect { id: string; x: number; y: number; w: number; h: number }

export interface DockPersisted {
  root: DockNode | null;
  floating: FloatRect[];
  hidden: string[];
}

const LS_KEY = 'floret.dock.v1';

export interface DefaultPlacement {
  /** Where a freshly-mounted panel lands if it's not already in the
   *  layout. 'root-edge' docks to a viewport edge; 'tab' joins an
   *  existing group; 'float' opens detached. */
  kind: 'root-edge' | 'tab' | 'float';
  side?: Exclude<Side, 'center'>;
  intoGroupOf?: string;   // for 'tab': share the group that holds this panel id
  floatRect?: Omit<FloatRect, 'id'>;
}

interface DockState extends DockPersisted {
  /** Insert a panel if it isn't anywhere yet. No-op when already placed
   *  (so a reload that remounts every panel preserves the saved tree). */
  ensurePanel: (id: string, place: DefaultPlacement) => void;
  /** Remove a panel from wherever it lives (tree / floating / hidden). */
  removePanel: (id: string) => void;
  /** Drop `id` onto a group: centre → tab, edge → recursive split. */
  dropOnGroup: (id: string, groupId: string, side: Side) => void;
  /** Drop `id` onto a viewport edge of the whole dock area. */
  dropOnRootEdge: (id: string, side: Exclude<Side, 'center'>) => void;
  floatPanel: (id: string, rect: Omit<FloatRect, 'id'>) => void;
  moveFloat: (id: string, x: number, y: number) => void;
  resizeFloat: (id: string, w: number, h: number) => void;
  hidePanel: (id: string) => void;
  setSizes: (splitId: string, sizes: number[]) => void;
  setActive: (groupId: string, active: number) => void;
  /** Drop any preview-* panels whose node no longer exists. */
  reconcile: (liveIds: ReadonlySet<string>, prunablePrefix: string) => void;
}

function load(): DockPersisted {
  const p = loadJSON<Partial<DockPersisted>>(LS_KEY, {});
  return {
    root: p.root ?? null,
    floating: Array.isArray(p.floating) ? p.floating : [],
    hidden: Array.isArray(p.hidden) ? p.hidden : [],
  };
}

function everywhere(s: DockPersisted): Set<string> {
  return new Set([...collectPanels(s.root), ...s.floating.map((f) => f.id), ...s.hidden]);
}

export const useDock = create<DockState>((set, get) => {
  const persist = () => {
    const { root, floating, hidden } = get();
    saveJSON(LS_KEY, { root, floating, hidden });
  };
  const commit = (patch: Partial<DockPersisted>) => { set(patch); persist(); };

  return {
    ...load(),

    ensurePanel: (id, place) => {
      const s = get();
      if (everywhere(s).has(id)) return;
      if (place.kind === 'float') {
        const r = place.floatRect ?? { x: 120, y: 120, w: 360, h: 300 };
        commit({ floating: [...s.floating, { id, ...r }] });
        return;
      }
      if (place.kind === 'tab' && place.intoGroupOf) {
        const g = findGroupOf(s.root, place.intoGroupOf);
        if (g && s.root) { commit({ root: addTab(s.root, g.id, id) }); return; }
      }
      // root-edge (or tab fallback): seed the tree or split a root edge.
      if (!s.root) { commit({ root: group([id]) }); return; }
      commit({ root: splitRoot(s.root, id, place.side ?? 'right') });
    },

    removePanel: (id) => {
      const s = get();
      commit({
        root: treeRemove(s.root, id),
        floating: s.floating.filter((f) => f.id !== id),
        hidden: s.hidden.filter((h) => h !== id),
      });
    },

    dropOnGroup: (id, groupId, side) => {
      const s = get();
      // Pull the panel out of its current home first so a move within the
      // tree doesn't duplicate it.
      const root0 = treeRemove(s.root, id);
      const floating = s.floating.filter((f) => f.id !== id);
      const hidden = s.hidden.filter((h) => h !== id);
      if (!root0) { commit({ root: group([id]), floating, hidden }); return; }
      // The target group may have been pruned if it only held the moved
      // panel; fall back to a root-edge dock in that case. (Look up by
      // GROUP id — not panel id.)
      const g = findGroup(root0, groupId) ? groupId : null;
      let root: DockNode;
      if (!g) root = splitRoot(root0, id, side === 'center' ? 'right' : side);
      else if (side === 'center') root = addTab(root0, g, id);
      else root = splitInsert(root0, g, id, side);
      commit({ root, floating, hidden });
    },

    dropOnRootEdge: (id, side) => {
      const s = get();
      const root0 = treeRemove(s.root, id);
      const floating = s.floating.filter((f) => f.id !== id);
      const hidden = s.hidden.filter((h) => h !== id);
      const root = root0 ? splitRoot(root0, id, side) : group([id]);
      commit({ root, floating, hidden });
    },

    floatPanel: (id, rect) => {
      const s = get();
      const floating = [...s.floating.filter((f) => f.id !== id), { id, ...rect }];
      commit({ root: treeRemove(s.root, id), floating, hidden: s.hidden.filter((h) => h !== id) });
    },

    moveFloat: (id, x, y) => {
      const s = get();
      commit({ floating: s.floating.map((f) => (f.id === id ? { ...f, x, y } : f)) });
    },
    resizeFloat: (id, w, h) => {
      const s = get();
      commit({ floating: s.floating.map((f) => (f.id === id ? { ...f, w: Math.max(160, w), h: Math.max(120, h) } : f)) });
    },

    hidePanel: (id) => {
      const s = get();
      if (s.hidden.includes(id)) return;
      commit({
        root: treeRemove(s.root, id),
        floating: s.floating.filter((f) => f.id !== id),
        hidden: [...s.hidden, id],
      });
    },

    setSizes: (splitId, sizes) => {
      const s = get();
      if (!s.root) return;
      commit({ root: treeSetSizes(s.root, splitId, sizes) });
    },
    setActive: (groupId, active) => {
      const s = get();
      if (!s.root) return;
      commit({ root: treeSetActive(s.root, groupId, active) });
    },

    reconcile: (liveIds, prunablePrefix) => {
      const s = get();
      const stale = [...everywhere(s)].filter((id) => id.startsWith(prunablePrefix) && !liveIds.has(id));
      if (stale.length === 0) return;
      let root = s.root;
      for (const id of stale) root = treeRemove(root, id);
      commit({
        root,
        floating: s.floating.filter((f) => !stale.includes(f.id)),
        hidden: s.hidden.filter((h) => !stale.includes(h)),
      });
    },
  };
});

/** Seed a sensible first-run layout if nothing is persisted yet. Called
 *  once at boot with the always-present panel ids. */
export function seedDefaultLayout(): void {
  const s = useDock.getState();
  if (s.root || s.floating.length) return;
  // row[ palette(.18) | canvas(.57) | help(.25) ]. Previews and the
  // library dock themselves dynamically when they mount.
  const root = split('row', [
    group(['palette']),
    group(['canvas']),
    group(['help']),
  ], [0.18, 0.57, 0.25]);
  useDock.setState({ root });
  saveJSON(LS_KEY, { root, floating: [], hidden: [] });
}
