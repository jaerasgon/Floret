import { create } from 'zustand';
import { loadJSON, saveJSON } from '../persistence/local-store';

/**
 * User-facing preferences. Persisted to localStorage so they survive
 * across reloads, and exposed both via Zustand selectors (for UI that
 * needs to re-render on change) and a plain module-level snapshot
 * (`prefsSnapshot`) for code paths that must read the value without a
 * hook — e.g. the render driver's per-frame hook and the Mouse node's
 * `readFromContext`. The snapshot stays in sync via a subscription set
 * up below.
 *
 * Keys deliberately default to "v1 sensible" rather than "matches old
 * behaviour" — the user explicitly asked for center origin to ship as
 * the new default. The old behaviour is one toggle away.
 */
export interface Prefs {
  /** Flow-direction wire animation (the dashed marching ants on spread edges). */
  wireFlowAnimation: boolean;
  /** Region frames auto-grow when a child is dragged near the edge. */
  autoGrowRegions: boolean;
  /** Help panel auto-tracks the currently selected node. When off, the
   *  panel is "pinned" to whatever was last picked via the search box. */
  autoJumpHelp: boolean;
  /** Nodes (palette) panel visible. Toggled from the toolbar; when off the
   *  panel is dropped from the dock layout (re-docks on show). */
  paletteVisible: boolean;
  /** Help panel visible. Same toolbar-toggle treatment as the palette. */
  helpVisible: boolean;
  /** UI colour theme. 'system' follows the OS. Resolved to data-theme on <html>. */
  theme: 'dark' | 'light' | 'system';
  /** Generative "living canvas" backdrop behind the graph. */
  ambientMotion: boolean;
  /** Show the minimap in the canvas corner. */
  showMinimap: boolean;
  /** Show the dotted background grid. */
  showGrid: boolean;
  /** Snap node positions to a grid while dragging. */
  snapToGrid: boolean;
  /** Grid step (px) used when snapToGrid is on. */
  gridSize: number;
  /** What loads on launch: blank canvas, the starter showcase, or last session. */
  startupPatch: 'blank' | 'starter' | 'last';
  /** Debounced autosave of the working patch to localStorage. */
  autosaveEnabled: boolean;
  /** Resolution seeded into a newly-created Renderer node. */
  defaultRenderWidth: number;
  defaultRenderHeight: number;
}

const LS_KEY = 'floret.prefs.v1';

const DEFAULTS: Prefs = {
  wireFlowAnimation: true,
  autoGrowRegions: true,
  autoJumpHelp: true,
  paletteVisible: true,
  // Hidden by default — the canvas is the star of a fresh layout. The toolbar
  // toggle brings Help back, and it auto-tracks selection exactly as before.
  helpVisible: false,
  theme: 'dark',
  ambientMotion: true,
  showMinimap: true,
  showGrid: true,
  snapToGrid: false,
  gridSize: 16,
  startupPatch: 'last',
  autosaveEnabled: true,
  defaultRenderWidth: 1080,
  defaultRenderHeight: 1920,
};

/** Resolve the theme pref to a concrete mode, following the OS for 'system'. */
export function resolveTheme(t: Prefs['theme']): 'dark' | 'light' {
  if (t === 'system') {
    return typeof window !== 'undefined' &&
      window.matchMedia('(prefers-color-scheme: light)').matches
      ? 'light'
      : 'dark';
  }
  return t;
}

function loadFromStorage(): Prefs {
  // Spread DEFAULTS first so missing keys in older payloads fall back to
  // the current default rather than `undefined`.
  return { ...DEFAULTS, ...loadJSON<Partial<Prefs>>(LS_KEY, {}) };
}

interface PrefsState extends Prefs {
  set: <K extends keyof Prefs>(key: K, value: Prefs[K]) => void;
  reset: () => void;
}

export const usePrefs = create<PrefsState>((set) => ({
  ...loadFromStorage(),
  set: (key, value) => set((s) => ({ ...s, [key]: value })),
  reset: () => set(() => ({ ...DEFAULTS })),
}));

/**
 * Module-level snapshot for non-React readers (frame-hook, node
 * compute functions). Stays in sync via a Zustand subscription — no
 * polling, no per-frame hook calls.
 */
export const prefsSnapshot: Prefs = { ...usePrefs.getState() };

const PREF_KEYS: (keyof Prefs)[] = [
  'wireFlowAnimation', 'autoGrowRegions', 'autoJumpHelp', 'paletteVisible',
  'helpVisible', 'theme', 'ambientMotion', 'showMinimap', 'showGrid',
  'snapToGrid', 'gridSize', 'startupPatch', 'autosaveEnabled',
  'defaultRenderWidth', 'defaultRenderHeight',
];

usePrefs.subscribe((s) => {
  const snap = prefsSnapshot as unknown as Record<string, unknown>;
  const payload: Record<string, unknown> = {};
  for (const k of PREF_KEYS) {
    // Sync the module-level snapshot and build the persisted payload in one
    // pass so non-React readers (frame hook, addNode) and localStorage stay
    // in lock-step with the store.
    snap[k] = s[k];
    payload[k] = s[k];
  }
  saveJSON(LS_KEY, payload);
});
