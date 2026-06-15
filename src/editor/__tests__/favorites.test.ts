import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { useFavorites } from '../favorites';
import { loadJSON } from '../../persistence/local-store';

const KEY = 'floret.favorites.v1';

// Tests run in the node env (no jsdom). The persistence helpers are SSR-
// guarded on `window`, so stub a minimal localStorage to exercise them.
const store = new Map<string, string>();
const fakeLocalStorage = {
  getItem: (k: string) => (store.has(k) ? store.get(k)! : null),
  setItem: (k: string, v: string) => { store.set(k, String(v)); },
  removeItem: (k: string) => { store.delete(k); },
  clear: () => store.clear(),
};

beforeEach(() => {
  store.clear();
  vi.stubGlobal('window', { localStorage: fakeLocalStorage });
  useFavorites.setState({ types: [] });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('favorites store', () => {
  it('toggles a type on and off', () => {
    const { toggleFavorite } = useFavorites.getState();
    toggleFavorite('math/add');
    expect(useFavorites.getState().types).toEqual(['math/add']);
    expect(useFavorites.getState().isFavorite('math/add')).toBe(true);

    toggleFavorite('math/add');
    expect(useFavorites.getState().types).toEqual([]);
    expect(useFavorites.getState().isFavorite('math/add')).toBe(false);
  });

  it('preserves insertion order', () => {
    const { toggleFavorite } = useFavorites.getState();
    toggleFavorite('a');
    toggleFavorite('b');
    toggleFavorite('c');
    expect(useFavorites.getState().types).toEqual(['a', 'b', 'c']);
  });

  it('persists to localStorage on toggle', () => {
    useFavorites.getState().toggleFavorite('draw/rect');
    expect(loadJSON<string[]>(KEY, [])).toEqual(['draw/rect']);
    useFavorites.getState().toggleFavorite('draw/rect');
    expect(loadJSON<string[]>(KEY, [])).toEqual([]);
  });
});
