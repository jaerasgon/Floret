/**
 * Patch I/O across runtimes.
 *
 *   - Tauri build (M6+):       window.__TAURI__ is present → use native dialogs (TODO).
 *   - Modern Chromium browser: File System Access API (showSaveFilePicker / showOpenFilePicker).
 *   - Fallback (any browser):  download via <a> + upload via <input type="file">.
 *
 * For M6 we ship the browser paths; the Tauri branch is wired but inert
 * until the shell is added.
 */

import { isPatchFile, type PatchFile } from './schema';
import { migratePatch } from './migrate';
import { loadJSON, saveJSON, removeKey } from './local-store';

declare global {
  interface Window {
    __TAURI__?: unknown;
    showSaveFilePicker?: (options?: SaveFilePickerOptions) => Promise<FileSystemFileHandle>;
    showOpenFilePicker?: (options?: OpenFilePickerOptions) => Promise<FileSystemFileHandle[]>;
  }
  interface SaveFilePickerOptions {
    suggestedName?: string;
    types?: { description?: string; accept: Record<string, string[]> }[];
  }
  interface OpenFilePickerOptions {
    multiple?: boolean;
    types?: { description?: string; accept: Record<string, string[]> }[];
  }
  interface FileSystemFileHandle {
    getFile(): Promise<File>;
    createWritable(): Promise<FileSystemWritableFileStream>;
    name: string;
    queryPermission?(opts?: { mode?: 'read' | 'readwrite' }): Promise<PermissionState>;
    requestPermission?(opts?: { mode?: 'read' | 'readwrite' }): Promise<PermissionState>;
  }
  interface FileSystemWritableFileStream extends WritableStream {
    write(data: string | BufferSource | Blob): Promise<void>;
    close(): Promise<void>;
  }
}

const PATCH_FILE_TYPES = [
  {
    description: 'Floret patch',
    // .flora is our extension (the content is still JSON). The legacy
    // .floret.json / .json are kept so older saves still open.
    accept: { 'application/json': ['.flora', '.floret.json', '.json'] },
  },
];

export interface SavedHandle {
  /** Opaque handle for the FS Access API; null in fallback mode. */
  handle: FileSystemFileHandle | null;
  /** Filename shown in UI. */
  name: string;
}

/** Save to disk. Returns a handle so subsequent saves can write to the same file. */
export async function savePatch(
  patch: PatchFile,
  reuseHandle?: FileSystemFileHandle | null,
): Promise<SavedHandle> {
  const json = JSON.stringify(patch, null, 2);

  // Path 1: File System Access API (Chromium-based browsers).
  if (typeof window.showSaveFilePicker === 'function') {
    // Reuse the current file's handle when we have one — this is what makes
    // Ctrl+S save in place instead of re-opening the save dialog. A handle
    // restored from a previous session may have lost write permission; ask
    // for it back (the save keystroke is a user gesture, so this is allowed).
    let handle = reuseHandle ?? null;
    if (handle && handle.queryPermission) {
      let perm = await handle.queryPermission({ mode: 'readwrite' });
      if (perm !== 'granted' && handle.requestPermission) {
        perm = await handle.requestPermission({ mode: 'readwrite' });
      }
      if (perm !== 'granted') handle = null; // fall back to the picker below
    }
    if (!handle) {
      handle = await window.showSaveFilePicker({
        suggestedName: suggestedFilename(patch),
        types: PATCH_FILE_TYPES,
      });
    }
    const writable = await handle.createWritable();
    await writable.write(json);
    await writable.close();
    return { handle, name: handle.name };
  }

  // Path 2: download fallback.
  const blob = new Blob([json], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = suggestedFilename(patch);
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
  return { handle: null, name: a.download };
}

/** Load a patch from disk. Returns the migrated patch + handle (where available). */
export async function loadPatch(): Promise<{ patch: PatchFile; handle: FileSystemFileHandle | null; name: string }> {
  if (typeof window.showOpenFilePicker === 'function') {
    const [handle] = await window.showOpenFilePicker({
      multiple: false,
      types: PATCH_FILE_TYPES,
    });
    if (!handle) throw new Error('no file selected');
    const file = await handle.getFile();
    const text = await file.text();
    const patch = parseAndMigrate(text);
    return { patch, handle, name: handle.name };
  }

  // Path 2: <input type="file"> fallback.
  const file = await pickFileViaInput();
  const text = await file.text();
  const patch = parseAndMigrate(text);
  return { patch, handle: null, name: file.name };
}

function pickFileViaInput(): Promise<File> {
  return new Promise((resolve, reject) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.flora,.floret.json,.json,application/json';
    input.onchange = () => {
      const f = input.files?.[0];
      if (f) resolve(f);
      else reject(new Error('no file selected'));
    };
    input.click();
  });
}

function parseAndMigrate(text: string): PatchFile {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch (e) {
    throw new Error(`patch is not valid JSON: ${(e as Error).message}`);
  }
  if (!isPatchFile(raw)) {
    throw new Error('file is not a Floret patch');
  }
  return migratePatch(raw).patch;
}

function suggestedFilename(patch: PatchFile): string {
  const name = patch.meta?.name ?? 'untitled';
  const safe = name.replace(/[^a-z0-9._-]+/gi, '-').toLowerCase();
  return `${safe}.flora`;
}

// ─── localStorage autosave (small patches; not for large work) ──────────────

const AUTOSAVE_KEY = 'floret:autosave';

export function autosave(patch: PatchFile): void {
  saveJSON(AUTOSAVE_KEY, patch);
}

export function loadAutosave(): PatchFile | null {
  const raw = loadJSON<unknown>(AUTOSAVE_KEY, null);
  if (!raw || !isPatchFile(raw)) return null;
  return migratePatch(raw).patch;
}

export function clearAutosave(): void {
  removeKey(AUTOSAVE_KEY);
}

// ─── File-handle persistence (so the open file survives a reload) ────────────
//
// localStorage can't hold a FileSystemFileHandle, but IndexedDB can (handles
// are structured-cloneable). Stashing it lets Ctrl+S keep saving to the same
// file after a reload — the runtime just re-requests write permission on the
// next save gesture (see `savePatch`). Falls back to no-op where IndexedDB or
// the File System Access API isn't available.

const HANDLE_DB = 'floret-fs';
const HANDLE_STORE = 'handles';
const HANDLE_KEY = 'current';

function openHandleDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(HANDLE_DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(HANDLE_STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function persistFileHandle(handle: FileSystemFileHandle | null): Promise<void> {
  if (typeof indexedDB === 'undefined') return;
  try {
    const db = await openHandleDB();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(HANDLE_STORE, 'readwrite');
      const store = tx.objectStore(HANDLE_STORE);
      if (handle) store.put(handle, HANDLE_KEY);
      else store.delete(HANDLE_KEY);
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
    db.close();
  } catch {
    // Private mode / storage disabled — persistence is a convenience.
  }
}

export async function loadFileHandle(): Promise<FileSystemFileHandle | null> {
  if (typeof indexedDB === 'undefined') return null;
  try {
    const db = await openHandleDB();
    const handle = await new Promise<FileSystemFileHandle | null>((resolve) => {
      const tx = db.transaction(HANDLE_STORE, 'readonly');
      const req = tx.objectStore(HANDLE_STORE).get(HANDLE_KEY);
      req.onsuccess = () => resolve((req.result as FileSystemFileHandle | undefined) ?? null);
      req.onerror = () => resolve(null);
    });
    db.close();
    return handle;
  } catch {
    return null;
  }
}

export function clearFileHandle(): Promise<void> {
  return persistFileHandle(null);
}
