import { create } from 'zustand';

/**
 * Focus (Presentation) mode — promotes a single Preview to full-screen art and
 * fades the app chrome. Transient by design: it is NOT persisted, so a reload
 * never traps the user in a chrome-less state.
 *
 * The heavy lifting is CSS (see `body[data-focus]` + `.focus-featured` rules).
 * This store only holds the on/off flag + which renderer's preview is featured.
 */
interface FocusState {
  on: boolean;
  /** Renderer node id whose preview is shown full-screen. */
  featured: string | null;
  enter: (featured: string) => void;
  exit: () => void;
  toggle: (featured: string | null) => void;
}

export const useFocusMode = create<FocusState>((set, get) => ({
  on: false,
  featured: null,
  enter: (featured) => set({ on: true, featured }),
  exit: () => set({ on: false, featured: null }),
  toggle: (featured) => {
    if (get().on) set({ on: false, featured: null });
    else if (featured) set({ on: true, featured });
  },
}));
