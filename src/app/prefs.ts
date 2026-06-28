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
  /** Pro mode — collapses nodes to title + pins only. */
  proMode: boolean;
  /** Help panel auto-tracks the currently selected node. When off, the
   *  panel is "pinned" to whatever was last picked via the search box. */
  autoJumpHelp: boolean;
  /** Nodes (palette) panel visible. Toggled from the toolbar; when off the
   *  panel is dropped from the dock layout (re-docks on show). */
  paletteVisible: boolean;
  /** Help panel visible. Same toolbar-toggle treatment as the palette. */
  helpVisible: boolean;
}

const LS_KEY = 'floret.prefs.v1';

const DEFAULTS: Prefs = {
  wireFlowAnimation: true,
  autoGrowRegions: true,
  proMode: false,
  autoJumpHelp: true,
  paletteVisible: true,
  helpVisible: true,
};

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

usePrefs.subscribe((s) => {
  prefsSnapshot.wireFlowAnimation = s.wireFlowAnimation;
  prefsSnapshot.autoGrowRegions = s.autoGrowRegions;
  prefsSnapshot.proMode = s.proMode;
  prefsSnapshot.autoJumpHelp = s.autoJumpHelp;
  prefsSnapshot.paletteVisible = s.paletteVisible;
  prefsSnapshot.helpVisible = s.helpVisible;
  // Persist whole shape on every change. Small payload, infrequent
  // writes — no batching required.
  saveJSON(LS_KEY, {
    wireFlowAnimation: s.wireFlowAnimation,
    autoGrowRegions: s.autoGrowRegions,
    proMode: s.proMode,
    autoJumpHelp: s.autoJumpHelp,
    paletteVisible: s.paletteVisible,
    helpVisible: s.helpVisible,
  });
});
