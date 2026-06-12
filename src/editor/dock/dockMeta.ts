import type { ReactNode } from 'react';

/**
 * Live panel metadata — title, header actions, and flags — kept out of
 * the persisted layout store because it carries React nodes. Panels
 * register here on mount; the DockArea reads it to paint tab labels and
 * per-panel header controls. A tiny subscribe model lets the renderer
 * re-render when metadata changes (e.g. a Preview's title updates).
 */
export interface DockPanelMeta {
  id: string;
  title: string;
  headerActions?: ReactNode;
  /** Pinned panels (the canvas) can't be hidden or floated. */
  pinned?: boolean;
  /** Hide the × close affordance (still hideable via other means). */
  hideClose?: boolean;
}

const meta = new Map<string, DockPanelMeta>();
const listeners = new Set<() => void>();

function emit(): void { for (const l of listeners) l(); }

export function registerDockMeta(m: DockPanelMeta): () => void {
  meta.set(m.id, m);
  emit();
  return () => { meta.delete(m.id); emit(); };
}

/** Update an already-registered panel's metadata (title / actions). */
export function updateDockMeta(id: string, patch: Partial<DockPanelMeta>): void {
  const cur = meta.get(id);
  if (!cur) return;
  meta.set(id, { ...cur, ...patch });
  emit();
}

export function getDockMeta(id: string): DockPanelMeta | undefined {
  return meta.get(id);
}

export function subscribeDockMeta(fn: () => void): () => void {
  listeners.add(fn);
  return () => { listeners.delete(fn); };
}
