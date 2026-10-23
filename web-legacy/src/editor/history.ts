import { paramStore } from '../runtime/param-store';
import { _setHistoryPush, _setHistoryReset, useEditor, type FloretEdge, type FloretNode } from './editor-state';

/**
 * Undo / redo.
 *
 * A history entry is a snapshot of `(nodes, edges, paramStoreSnapshot)`.
 * The Zustand store calls `pushHistory()` on every *committed* mutation:
 *
 *   - Structural: add/remove node, add/remove edge        → 1 entry
 *   - IO-box commit (pointerup, blur):  setNodeParam(...) → 1 entry
 *   - Mid-drag scrubs (ParamStore-only): NO entry         → not in history
 *
 * That's the architectural promise from the plan: live scrubs don't pollute
 * the history; only the final value at pointerup does.
 *
 * Coalescing: a setNodeParam on the same (nodeId, key) within 250ms of the
 * previous entry replaces that entry instead of adding a new one. Lets
 * keyboard typing in a Number node collapse into a single history step.
 */

const MAX_HISTORY = 100;
const COALESCE_MS = 250;

interface Snapshot {
  nodes: FloretNode[];
  edges: FloretEdge[];
  params: Record<string, Record<string, unknown>>;
}

interface Entry extends Snapshot {
  at: number;
  /** For coalescing: the (nodeId, key) of the param mutation, if any. */
  paramKey?: string;
}

class History {
  private past: Entry[] = [];
  private future: Entry[] = [];
  private suppressed = false;
  private listeners = new Set<() => void>();

  /** Snapshot the current editor + paramStore state. */
  private snapshot(): Snapshot {
    const s = useEditor.getState();
    return {
      nodes: s.nodes.map((n) => ({ ...n, data: { ...n.data, params: { ...n.data.params } } })),
      edges: s.edges.map((e) => ({ ...e })),
      params: paramStore.snapshot(),
    };
  }

  /** Push a new entry. `paramKey` lets us coalesce rapid commits on the same key. */
  push(paramKey?: string): void {
    if (this.suppressed) return;
    const now = performance.now();
    const last = this.past[this.past.length - 1];
    const next: Entry = { ...this.snapshot(), at: now, paramKey };

    if (
      last &&
      paramKey &&
      last.paramKey === paramKey &&
      now - last.at < COALESCE_MS
    ) {
      // Coalesce: replace the last entry rather than stacking.
      this.past[this.past.length - 1] = next;
    } else {
      this.past.push(next);
      if (this.past.length > MAX_HISTORY) this.past.shift();
    }
    this.future = [];
    this.notify();
  }

  canUndo(): boolean { return this.past.length > 1; }
  canRedo(): boolean { return this.future.length > 0; }

  undo(): void {
    if (!this.canUndo()) return;
    const current = this.past.pop()!;
    this.future.push(current);
    const target = this.past[this.past.length - 1]!;
    this.apply(target);
    this.notify();
  }

  redo(): void {
    if (!this.canRedo()) return;
    const target = this.future.pop()!;
    this.past.push(target);
    this.apply(target);
    this.notify();
  }

  /** Drop everything (e.g. on patch load / new). */
  reset(): void {
    // Suppressed during undo/redo apply — `apply()` calls loadPatch which
    // ends in resetHistory(); without this guard, undo would wipe its own
    // stack and the user would lose redo (and effectively be stuck at
    // whatever state apply just produced).
    if (this.suppressed) return;
    this.past = [this.snapshot() as Entry];
    this.past[0]!.at = performance.now();
    this.future = [];
    this.notify();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  }

  private apply(s: Snapshot): void {
    this.suppressed = true;
    try {
      // Replay into ParamStore first so the editor's compile picks up live
      // values when it reactively rebuilds the graph.
      paramStore.replaceAll(Object.entries(s.params));
      useEditor.getState().loadPatch(
        s.nodes.map((n) => ({ ...n, data: { ...n.data, params: { ...n.data.params } } })),
        s.edges.map((e) => ({ ...e })),
      );
    } finally {
      this.suppressed = false;
    }
  }

  private notify(): void {
    for (const l of this.listeners) l();
  }
}

export const history = new History();

/** Seed the first entry on app start. */
history.reset();

// Wire history into the editor store's mutation callbacks. We use the
// late-binding `_setHistoryPush` to avoid a circular import.
_setHistoryPush((paramKey) => history.push(paramKey));
// Same late-binding pattern for resetHistory — called by loadPatch on
// every fresh patch load (file open, sample import, autosave restore,
// New). The reset() suppressed-flag guard above makes this a no-op when
// the load came from history.apply itself, so undo/redo don't wipe the
// stack mid-replay.
_setHistoryReset(() => history.reset());
