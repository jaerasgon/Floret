/**
 * Cross-panel coordination registry.
 *
 * Every <Panel> registers itself at mount so the drop overlay can find
 * sibling panels and compute per-panel drop zones (top/bottom/left/right
 * quadrants). The registry is intentionally tiny: it just exposes
 * panels' current dock side, in-rail order, a way to read their
 * bounding rect, and mutators so the overlay can commit a drop without
 * threading callbacks through the React tree.
 */
import type { PanelDock } from './usePanelState';

export interface PanelEntry {
  id: string;
  /** Live snapshot of which side the panel currently lives in. */
  dock: PanelDock;
  /** Column within the dock side. 0 = nearest the canvas. */
  column: number;
  /** In-rail order; lower = closer to the top of the rail. */
  order: number;
  /** Returns the panel's current screen rect, or null when it's not
   *  currently rendered (e.g. hidden state). */
  getRect: () => DOMRect | null;
  /** Set the panel's dock. */
  setDock: (dock: PanelDock) => void;
  /** Set the panel's column. */
  setColumn: (column: number) => void;
  /** Set the panel's in-rail order. */
  setOrder: (order: number) => void;
  /** Set float position (used when the drop target is "free space"). */
  setFloatPos: (x: number, y: number) => void;
}

const entries = new Map<string, PanelEntry>();
const listeners = new Set<() => void>();

function emit(): void {
  for (const l of listeners) l();
}

export function registerPanel(entry: PanelEntry): () => void {
  entries.set(entry.id, entry);
  emit();
  return () => {
    entries.delete(entry.id);
    emit();
  };
}

/** Update mutable fields of an already-registered panel without
 *  triggering re-registration. Called whenever React-side panel state
 *  changes so the overlay reads fresh values. */
export function updatePanel(id: string, patch: Partial<PanelEntry>): void {
  const cur = entries.get(id);
  if (!cur) return;
  entries.set(id, { ...cur, ...patch });
}

export function getPanel(id: string): PanelEntry | undefined {
  return entries.get(id);
}

export function listPanels(): PanelEntry[] {
  return Array.from(entries.values());
}

export function subscribePanels(fn: () => void): () => void {
  listeners.add(fn);
  return () => { listeners.delete(fn); };
}
