/**
 * ParamStore — mutable IO-box values, OUTSIDE React's reconciliation.
 *
 * Critical architectural piece (per the plan):
 *
 *   - Live slider/colorpicker/xypad scrubs write here on every pointer event.
 *     The runtime reads here every tick. ZERO React reconcile during drag.
 *
 *   - On *commit* (pointerup, blur, change-event for non-drag controls), the
 *     value is also persisted into the editor's `node.data.params` via
 *     `setNodeParam`, so save/load and (future) undo see it.
 *
 *   - IO-box React components subscribe to *their own* paramStore key via
 *     `useSyncExternalStore`. They re-render themselves without touching
 *     React Flow's node array.
 *
 * Shape: nodeId → { paramKey → value }. Per-node and global subscription.
 */

type Listener = () => void;
type NodeId = string;
type Params = Record<string, unknown>;

class ParamStore {
  private byNode = new Map<NodeId, Params>();
  private listenersByNode = new Map<NodeId, Set<Listener>>();
  private globalListeners = new Set<Listener>();

  /** Whole-record read. Mutating the returned object is undefined behaviour. */
  getNode(id: NodeId): Params | undefined {
    return this.byNode.get(id);
  }

  /** Single key read. Returns `fallback` when unset. */
  getKey<T = unknown>(id: NodeId, key: string, fallback?: T): T | undefined {
    const params = this.byNode.get(id);
    if (!params) return fallback;
    const v = params[key];
    return v !== undefined ? (v as T) : fallback;
  }

  /** Single-key write. Triggers per-node and global listeners. No-op if unchanged. */
  setKey(id: NodeId, key: string, value: unknown): void {
    let p = this.byNode.get(id);
    if (!p) {
      p = {};
      this.byNode.set(id, p);
    }
    if (p[key] === value) return;
    p[key] = value;
    this._notify(id);
  }

  /** Replace a node's entire param record. */
  setNode(id: NodeId, params: Params): void {
    this.byNode.set(id, { ...params });
    this._notify(id);
  }

  /** Drop a node — call when a node is deleted in the editor. */
  removeNode(id: NodeId): void {
    if (!this.byNode.has(id)) return;
    this.byNode.delete(id);
    this._notify(id);
  }

  /** Wholesale replace, e.g. when loading a patch. */
  replaceAll(entries: Iterable<[NodeId, Params]>): void {
    this.byNode.clear();
    for (const [id, v] of entries) this.byNode.set(id, { ...v });
    for (const ls of this.listenersByNode.values()) for (const l of ls) l();
    for (const l of this.globalListeners) l();
  }

  /** Subscribe to changes for a specific node. */
  subscribe(id: NodeId, listener: Listener): () => void {
    let set = this.listenersByNode.get(id);
    if (!set) {
      set = new Set();
      this.listenersByNode.set(id, set);
    }
    set.add(listener);
    return () => {
      set!.delete(listener);
      if (set!.size === 0) this.listenersByNode.delete(id);
    };
  }

  /** Subscribe to any change. */
  subscribeAll(listener: Listener): () => void {
    this.globalListeners.add(listener);
    return () => {
      this.globalListeners.delete(listener);
    };
  }

  /** Snapshot for persistence. Returns a plain object — safe to JSON.stringify. */
  snapshot(): Record<NodeId, Params> {
    const out: Record<NodeId, Params> = {};
    for (const [id, p] of this.byNode) out[id] = { ...p };
    return out;
  }

  private _notify(id: NodeId): void {
    const ls = this.listenersByNode.get(id);
    if (ls) for (const l of ls) l();
    for (const l of this.globalListeners) l();
  }
}

export const paramStore = new ParamStore();
export type { ParamStore };

// Wire every paramStore write to the cook-on-change dirty bit. Covers
// slider drags, color picker edits, every inline param tweak — anything
// that flows through paramStore now wakes the render loop for exactly
// one tick. Imported here (not at the top of the file) to avoid the
// cycle param-store ↔ dirty if dirty ever needs to read params.
import { markDirty } from './dirty';
paramStore.subscribeAll(markDirty);
