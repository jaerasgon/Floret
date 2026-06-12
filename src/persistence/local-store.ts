/**
 * Tiny localStorage JSON helpers.
 *
 * Five call sites (prefs, panel state, library UI, the library store, and
 * file autosave) had independently hand-rolled the same SSR-guarded
 * try/catch around `JSON.parse` / `JSON.stringify`. This consolidates that
 * boilerplate without taking an opinion on shape: `loadJSON` returns the
 * parsed value (or the fallback on a missing key / parse error), and any
 * caller that needs schema validation or default-merging does it on the
 * result. Writes are best-effort — quota errors and private-mode
 * exceptions are swallowed, since every caller treats persistence as a
 * convenience, never a source of truth.
 */

export function loadJSON<T>(key: string, fallback: T): T {
  if (typeof window === 'undefined') return fallback;
  try {
    const raw = window.localStorage.getItem(key);
    if (raw == null) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

export function saveJSON(key: string, value: unknown): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Quota exceeded or storage disabled — drop silently.
  }
}

export function removeKey(key: string): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.removeItem(key);
  } catch {
    // ignore
  }
}
