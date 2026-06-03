import { create } from 'zustand';

interface AppState {
  running: boolean;
  fps: number;
  frameCount: number;
  nodeCount: number;
  /** Incremented by `refresh()` — Preview watches this to remount p5. */
  refreshKey: number;
  toggleRunning: () => void;
  refresh: () => void;
  setFps: (fps: number) => void;
  setFrameCount: (n: number) => void;
  setNodeCount: (n: number) => void;
}

export const useAppStore = create<AppState>((set) => ({
  running: true,
  fps: 0,
  frameCount: 0,
  nodeCount: 0,
  refreshKey: 0,
  toggleRunning: () => set((s) => ({ running: !s.running })),
  refresh: () => set((s) => ({ refreshKey: s.refreshKey + 1, running: true })),
  setFps: (fps) => set({ fps }),
  setFrameCount: (n) => set({ frameCount: n }),
  setNodeCount: (n) => set({ nodeCount: n }),
}));
