import { create } from 'zustand';
import { loadJSON, saveJSON } from '../persistence/local-store';

/**
 * Favorite node types — the small set of nodes the user reaches for most.
 * Starred from the palette or the search popover, surfaced in a pinned
 * palette section and the Q quick-menu, and persisted locally so they
 * survive reloads.
 *
 * Stored as an ordered array of node-type strings (insertion order = display
 * order). Stale entries (a type that no longer exists in the registry) are
 * filtered lazily by consumers at render time, not here — the registry may
 * not be fully populated when this module initializes.
 */

const KEY = 'floret.favorites.v1';

interface FavoritesState {
  /** Favorited node types, in display order. */
  types: string[];
  isFavorite: (type: string) => boolean;
  /** Add the type if absent, remove it if present. Persists immediately. */
  toggleFavorite: (type: string) => void;
}

export const useFavorites = create<FavoritesState>((set, get) => ({
  types: loadJSON<string[]>(KEY, []),
  isFavorite: (type) => get().types.includes(type),
  toggleFavorite: (type) => {
    const cur = get().types;
    const next = cur.includes(type)
      ? cur.filter((t) => t !== type)
      : [...cur, type];
    saveJSON(KEY, next);
    set({ types: next });
  },
}));
