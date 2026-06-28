import { create } from 'zustand';
import { useEditor } from '../editor/editor-state';

// Preview panels are no longer tracked here — they're derived 1:1 from
// `render/renderer` nodes in the editor's patch (see `App.tsx`). The
// store only carries global runtime state.

interface AppState {
  running: boolean;
  fps: number;
  frameCount: number;
  nodeCount: number;
  /** Incremented by `refresh()` — Preview watches this to remount the renderer. */
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
  /**
   * Reset the running scene — discard every node's hidden runtime state
   * (particle buffers, trail ring buffers, stateful values, memo caches)
   * by rebuilding the runtime graph from scratch. The PATCH itself
   * (nodes + edges) stays untouched; this is "reboot the scene", not
   * "reload the file".
   *
   * 1. `useEditor.recompile()` swaps in a fresh RuntimeNode set, so all
   *    of those hidden Float32Arrays / Maps come back empty.
   * 2. `refreshKey` ticks so every Preview remounts its render driver, which
   *    resets `time` to 0 and clears the canvas.
   * 3. Counters reset so the status bar reflects the new run.
   * 4. `running: true` un-pauses if the user was paused.
   */
  refresh: () => {
    useEditor.getState().recompile();
    set((s) => ({
      refreshKey: s.refreshKey + 1,
      running: true,
      fps: 0,
      frameCount: 0,
    }));
  },
  setFps: (fps) => set({ fps }),
  setFrameCount: (n) => set({ frameCount: n }),
  setNodeCount: (n) => set({ nodeCount: n }),
}));
