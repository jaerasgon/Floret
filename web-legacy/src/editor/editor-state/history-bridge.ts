/**
 * Late-bound bridge to history.ts. Both modules need each other (history
 * pushes editor snapshots; the editor pushes history on every mutation), so
 * history.ts injects its push/reset here after this module body evaluates,
 * avoiding a circular import.
 */

let _historyPush: ((paramKey?: string) => void) | null = null;
export function _setHistoryPush(fn: (paramKey?: string) => void): void {
  _historyPush = fn;
}
export function pushHistory(paramKey?: string): void {
  _historyPush?.(paramKey);
}

/**
 * Late-bound history reset — invoked by `loadPatch` so every patch-load
 * code path (autosave restore, file open, sample/library import, New)
 * automatically seeds the undo stack with the loaded state. Without this,
 * the first Ctrl+Z would wind back to whatever was loaded BEFORE the
 * external load — typically the empty seed entry, which manifested as
 * "undo deleted everything". history.ts wires this so its own
 * suppressed-flag guard makes the call a no-op during undo/redo apply.
 */
let _historyReset: (() => void) | null = null;
export function _setHistoryReset(fn: () => void): void {
  _historyReset = fn;
}
export function resetHistory(): void {
  _historyReset?.();
}
