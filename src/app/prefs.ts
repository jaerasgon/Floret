import { create } from 'zustand';

/**
 * User-facing preferences. Persisted to localStorage so they survive
 * across reloads, and exposed both via Zustand selectors (for UI that
 * needs to re-render on change) and a plain module-level snapshot
 * (`prefsSnapshot`) for code paths that must read the value without a
 * hook — e.g. the p5 driver's per-frame hook and the Mouse node's
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
}

const LS_KEY = 'nodule.prefs.v1';

const DEFAULTS: Prefs = {
  wireFlowAnimation: true,
  autoGrowRegions: true,
  proMode: false,
};

function loadFromStorage(): Prefs {
  if (typeof window === 'undefined') return DEFAULTS;
  try {
    const raw = window.localStorage.getItem(LS_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<Prefs>;
    // Spread DEFAULTS last so missing keys in older payloads fall back
    // to the current default rather than `undefined`.
    return { ...DEFAULTS, ...parsed };
  } catch {
    return DEFAULTS;
  }
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
  // Persist whole shape on every change. Small payload, infrequent
  // writes — no batching required.
  if (typeof window !== 'undefined') {
    try {
      window.localStorage.setItem(
        LS_KEY,
        JSON.stringify({
          wireFlowAnimation: s.wireFlowAnimation,
          autoGrowRegions: s.autoGrowRegions,
          proMode: s.proMode,
        }),
      );
    } catch {
      // Quota or private mode — silently ignore.
    }
  }
});
