import { create } from 'zustand';
import type { NodeId } from '../runtime/graph';

interface PreviewInstance {
  panelId: string;
  rendererId: NodeId | null;
}

interface AppState {
  running: boolean;
  fps: number;
  frameCount: number;
  nodeCount: number;
  /** Incremented by `refresh()` — Preview watches this to remount p5. */
  refreshKey: number;
  /** Mounted Preview panels. The first entry is the default preview;
   *  the user can spawn / dismiss further previews. */
  previews: PreviewInstance[];
  toggleRunning: () => void;
  refresh: () => void;
  setFps: (fps: number) => void;
  setFrameCount: (n: number) => void;
  setNodeCount: (n: number) => void;
  addPreview: () => void;
  removePreview: (panelId: string) => void;
  setPreviewRenderer: (panelId: string, rendererId: NodeId | null) => void;
}

const PREVIEWS_LS_KEY = 'floret.previews.v1';

function loadPreviews(): PreviewInstance[] {
  try {
    const raw = localStorage.getItem(PREVIEWS_LS_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as PreviewInstance[];
      if (Array.isArray(parsed) && parsed.length > 0) return parsed;
    }
  } catch { /* ignore */ }
  return [{ panelId: 'preview', rendererId: null }];
}

function savePreviews(list: PreviewInstance[]): void {
  try { localStorage.setItem(PREVIEWS_LS_KEY, JSON.stringify(list)); } catch { /* ignore */ }
}

export const useAppStore = create<AppState>((set, get) => ({
  running: true,
  fps: 0,
  frameCount: 0,
  nodeCount: 0,
  refreshKey: 0,
  previews: loadPreviews(),
  toggleRunning: () => set((s) => ({ running: !s.running })),
  refresh: () => set((s) => ({ refreshKey: s.refreshKey + 1, running: true })),
  setFps: (fps) => set({ fps }),
  setFrameCount: (n) => set({ frameCount: n }),
  setNodeCount: (n) => set({ nodeCount: n }),
  addPreview: () => {
    const list = get().previews;
    // Pick the next unused suffix so dismissed panel ids don't conflict
    // with the localStorage-persisted layouts of previous spawns.
    const used = new Set(list.map((p) => p.panelId));
    let n = 2;
    while (used.has(`preview-${n}`)) n++;
    const next = [...list, { panelId: `preview-${n}`, rendererId: null }];
    savePreviews(next);
    set({ previews: next });
  },
  removePreview: (panelId) => {
    const next = get().previews.filter((p) => p.panelId !== panelId);
    // Always keep at least one preview to avoid losing access to the
    // canvas — the default 'preview' panel is recreated if dismissed.
    const safe = next.length > 0 ? next : [{ panelId: 'preview', rendererId: null }];
    savePreviews(safe);
    set({ previews: safe });
  },
  setPreviewRenderer: (panelId, rendererId) => {
    const next = get().previews.map((p) => p.panelId === panelId ? { ...p, rendererId } : p);
    savePreviews(next);
    set({ previews: next });
  },
}));
