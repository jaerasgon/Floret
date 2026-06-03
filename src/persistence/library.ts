import type { NoduleEdge, NoduleNode } from '../editor/editor-state';
import type { PatchFile } from './schema';
import { deserializePatch } from './serialize';

/**
 * Patch Library — a JSON bundle holding multiple full patches plus a
 * little metadata. Imported libraries live in localStorage so they
 * persist across reloads; users browse them from a toolbar dropdown
 * and click a patch to load it like a sample.
 *
 * The format is intentionally close to a `PatchFile`-of-`PatchFile`s
 * so a future "save patch as library entry" gesture composes cleanly.
 */
export interface LibraryEntry {
  id: string;
  name: string;
  description?: string;
  /** A complete PatchFile — same shape File → Open consumes. */
  patch: PatchFile;
}

export interface PatchLibrary {
  kind: 'nodule.library';
  version: number;
  name: string;
  description?: string;
  author?: string;
  patches: LibraryEntry[];
}

const LIBRARY_VERSION = 1;
const LS_KEY = 'nodule.libraries.v1';

interface StoredLibrary {
  /** Stable id derived from the library's name; lets us upsert on re-import. */
  slug: string;
  library: PatchLibrary;
  importedAt: string;
}

function slugify(s: string): string {
  return s
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '') || `lib-${Date.now()}`;
}

function readStore(): StoredLibrary[] {
  if (typeof window === 'undefined') return [];
  try {
    const raw = window.localStorage.getItem(LS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed as StoredLibrary[];
  } catch {
    return [];
  }
}

function writeStore(libs: StoredLibrary[]): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(LS_KEY, JSON.stringify(libs));
  } catch {
    // ignore quota errors — library data is recoverable from the source JSON.
  }
}

/**
 * Validate a raw JSON value as a PatchLibrary. Returns the library on
 * success or an error message on failure. We don't deep-validate each
 * patch's shape (deserializePatch surfaces malformed cases at load time).
 */
export function parseLibrary(raw: unknown): { ok: true; library: PatchLibrary } | { ok: false; error: string } {
  if (!raw || typeof raw !== 'object') return { ok: false, error: 'expected a JSON object' };
  const obj = raw as Record<string, unknown>;
  if (obj.kind !== 'nodule.library') return { ok: false, error: `kind must be "nodule.library", got ${JSON.stringify(obj.kind)}` };
  if (typeof obj.name !== 'string' || obj.name.trim() === '') return { ok: false, error: 'name is required' };
  if (!Array.isArray(obj.patches)) return { ok: false, error: 'patches must be an array' };
  for (const p of obj.patches as unknown[]) {
    if (!p || typeof p !== 'object') return { ok: false, error: 'each patch must be an object' };
    const e = p as Record<string, unknown>;
    if (typeof e.id !== 'string') return { ok: false, error: 'each patch needs an id' };
    if (typeof e.name !== 'string') return { ok: false, error: 'each patch needs a name' };
    if (!e.patch || typeof e.patch !== 'object') return { ok: false, error: 'each entry needs a patch (PatchFile)' };
  }
  return {
    ok: true,
    library: {
      kind: 'nodule.library',
      version: typeof obj.version === 'number' ? obj.version : LIBRARY_VERSION,
      name: obj.name as string,
      description: typeof obj.description === 'string' ? obj.description : undefined,
      author: typeof obj.author === 'string' ? obj.author : undefined,
      patches: obj.patches as LibraryEntry[],
    },
  };
}

export function listLibraries(): StoredLibrary[] {
  return readStore();
}

export function importLibrary(library: PatchLibrary): StoredLibrary {
  const all = readStore();
  const slug = slugify(library.name);
  const entry: StoredLibrary = {
    slug,
    library,
    importedAt: new Date().toISOString(),
  };
  const idx = all.findIndex((l) => l.slug === slug);
  if (idx >= 0) all[idx] = entry;
  else all.push(entry);
  writeStore(all);
  return entry;
}

export function removeLibrary(slug: string): void {
  writeStore(readStore().filter((l) => l.slug !== slug));
}

/**
 * Resolve a (librarySlug, patchId) pair to editor-ready nodes+edges.
 * Throws if either is missing — caller handles UI fallback.
 */
export function loadLibraryPatch(slug: string, patchId: string): { nodes: NoduleNode[]; edges: NoduleEdge[]; name: string } {
  const lib = readStore().find((l) => l.slug === slug);
  if (!lib) throw new Error(`library not found: ${slug}`);
  const entry = lib.library.patches.find((p) => p.id === patchId);
  if (!entry) throw new Error(`patch not found in library: ${patchId}`);
  const { nodes, edges } = deserializePatch(entry.patch);
  return { nodes, edges, name: entry.name };
}

/**
 * Create an empty library. Returns the new stored entry so callers can
 * immediately add a patch to it.
 */
export function createLibrary(name: string, description?: string): StoredLibrary {
  const library: PatchLibrary = {
    kind: 'nodule.library',
    version: LIBRARY_VERSION,
    name: name.trim(),
    description: description?.trim() || undefined,
    patches: [],
  };
  return importLibrary(library);
}

/**
 * Append (or replace by id) a single patch entry in a library. The
 * `entry.patch` should already be a valid PatchFile — typically built
 * by `serializePatch(...)` against the current editor state. Returns
 * the updated stored library.
 */
export function upsertLibraryEntry(slug: string, entry: LibraryEntry): StoredLibrary {
  const all = readStore();
  const idx = all.findIndex((l) => l.slug === slug);
  if (idx < 0) throw new Error(`library not found: ${slug}`);
  const lib = all[idx]!;
  const patches = [...lib.library.patches];
  const ei = patches.findIndex((p) => p.id === entry.id);
  if (ei >= 0) patches[ei] = entry;
  else patches.push(entry);
  const next: StoredLibrary = {
    ...lib,
    library: { ...lib.library, patches },
  };
  all[idx] = next;
  writeStore(all);
  return next;
}

/** Remove one patch entry from a library by id. Library stays in store even if empty. */
export function removeLibraryEntry(slug: string, patchId: string): void {
  const all = readStore();
  const idx = all.findIndex((l) => l.slug === slug);
  if (idx < 0) return;
  const lib = all[idx]!;
  const patches = lib.library.patches.filter((p) => p.id !== patchId);
  if (patches.length === lib.library.patches.length) return;
  all[idx] = { ...lib, library: { ...lib.library, patches } };
  writeStore(all);
}

/**
 * Trigger a browser download of a library as JSON. Filename is the
 * library's slug + `.library.json`. Caller decides how to surface the
 * success — typically a one-shot toast.
 */
export function downloadLibrary(slug: string): void {
  if (typeof window === 'undefined') return;
  const lib = readStore().find((l) => l.slug === slug);
  if (!lib) throw new Error(`library not found: ${slug}`);
  const json = JSON.stringify(lib.library, null, 2);
  const blob = new Blob([json], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `${lib.slug}.library.json`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  // Defer revoke so the click handler can finish.
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
