import { create } from 'zustand';
import { getPanel } from '../editor/panel/panelRegistry';
import { loadJSON, saveJSON } from '../persistence/local-store';

/**
 * Library panel UI state — separate from `library.ts` (persistence) and
 * from the editor store. Holds three things:
 *
 *   - `visible`: the Library button in the toolbar toggles this. Drives
 *     whether <LibraryPanel> mounts at all. (We render-or-not rather than
 *     `dock: 'hidden'` so the panel disappears completely when off —
 *     hidden tabs would clutter the rail edge with a "Library" stub.)
 *   - `viewMode`: which of the three Finder-style views to render.
 *   - `selection`: { folder, library } used by grid + column views to
 *     track drill-down location. List view ignores it.
 *
 * Persisted to localStorage so the user's last view/selection sticks.
 */
export type LibraryViewMode = 'list' | 'grid' | 'column';

interface Selection {
  /** null = root; otherwise the folder name (e.g. "Samples"). */
  folder: string | null;
  /** null = no library focused; otherwise a library slug. */
  library: string | null;
}

interface LibraryUIState {
  visible: boolean;
  viewMode: LibraryViewMode;
  selection: Selection;
  toggleVisible: () => void;
  setVisible: (v: boolean) => void;
  setViewMode: (m: LibraryViewMode) => void;
  setSelection: (s: Partial<Selection>) => void;
  resetSelection: () => void;
}

const LS_KEY = 'floret.libraryUI.v1';

interface Persisted {
  visible: boolean;
  viewMode: LibraryViewMode;
  selection: Selection;
}

function load(): Persisted {
  const parsed = loadJSON<Partial<Persisted>>(LS_KEY, {});
  return {
    visible: typeof parsed.visible === 'boolean' ? parsed.visible : false,
    viewMode: parsed.viewMode === 'grid' || parsed.viewMode === 'column' || parsed.viewMode === 'list'
      ? parsed.viewMode : 'list',
    selection: {
      folder: typeof parsed.selection?.folder === 'string' ? parsed.selection.folder : null,
      library: typeof parsed.selection?.library === 'string' ? parsed.selection.library : null,
    },
  };
}

function persist(s: Persisted): void {
  saveJSON(LS_KEY, s);
}

const initial = load();

/**
 * When the toolbar makes the Library panel visible, peek at the panel's
 * current dock state. If it was previously 'hidden' (user clicked × at
 * some point — though we now hide × on this panel, persisted state from
 * an earlier session can still hold 'hidden'), snap back to 'left' so
 * the panel actually appears somewhere instead of just rendering its
 * sideways restore-tab. The panel doesn't register until after mount,
 * so we wait a tick.
 */
function ensureLibraryDocked(): void {
  // Two rAFs: one for React to mount the panel, one for usePanelState's
  // post-mount registry sync. Enough in practice; harmless if late.
  requestAnimationFrame(() => requestAnimationFrame(() => {
    const p = getPanel('library');
    if (p && p.dock === 'hidden') p.setDock('left');
  }));
}

export const useLibraryUI = create<LibraryUIState>((set, get) => ({
  visible: initial.visible,
  viewMode: initial.viewMode,
  selection: initial.selection,
  toggleVisible: () => {
    const next = !get().visible;
    set({ visible: next });
    if (next) ensureLibraryDocked();
    const s = get();
    persist({ visible: s.visible, viewMode: s.viewMode, selection: s.selection });
  },
  setVisible: (visible) => {
    set({ visible });
    if (visible) ensureLibraryDocked();
    const s = get();
    persist({ visible: s.visible, viewMode: s.viewMode, selection: s.selection });
  },
  setViewMode: (viewMode) => {
    set({ viewMode });
    const s = get();
    persist({ visible: s.visible, viewMode: s.viewMode, selection: s.selection });
  },
  setSelection: (sel) => {
    set((s) => ({ selection: { ...s.selection, ...sel } }));
    const s = get();
    persist({ visible: s.visible, viewMode: s.viewMode, selection: s.selection });
  },
  resetSelection: () => {
    set({ selection: { folder: null, library: null } });
    const s = get();
    persist({ visible: s.visible, viewMode: s.viewMode, selection: s.selection });
  },
}));
