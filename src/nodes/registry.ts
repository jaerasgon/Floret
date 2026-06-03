import type { NodeDef } from './types';

/**
 * The single source of truth that both editor (palette, port colors,
 * connection rules) and runtime (compile) consult.
 *
 * Mutating the registry bumps a version counter; downstream subscribers
 * (compile-on-registry-change for hot reload, palette UI) watch this
 * to know when to refresh. In M3 we use it just for the palette; in
 * a future milestone it lets us hot-reload a single node definition
 * without restarting the p5 instance.
 */

class NodeRegistry {
  private defs = new Map<string, NodeDef>();
  private byCategory = new Map<string, NodeDef[]>();
  private _version = 0;
  private listeners = new Set<() => void>();

  register(def: NodeDef): void {
    if (this.defs.has(def.type)) {
      // Re-registration replaces (intentional for hot reload).
      const old = this.defs.get(def.type)!;
      const oldList = this.byCategory.get(old.category);
      if (oldList) {
        const idx = oldList.indexOf(old);
        if (idx >= 0) oldList.splice(idx, 1);
      }
    }
    this.defs.set(def.type, def);
    let list = this.byCategory.get(def.category);
    if (!list) {
      list = [];
      this.byCategory.set(def.category, list);
    }
    list.push(def);
    this._version++;
    for (const l of this.listeners) l();
  }

  get(type: string): NodeDef | undefined {
    return this.defs.get(type);
  }

  /** Throws if missing — for code paths where the def must exist. */
  require(type: string): NodeDef {
    const def = this.defs.get(type);
    if (!def) throw new Error(`unknown node type: ${type}`);
    return def;
  }

  all(): ReadonlyArray<NodeDef> {
    return [...this.defs.values()];
  }

  categories(): ReadonlyMap<string, ReadonlyArray<NodeDef>> {
    return this.byCategory;
  }

  version(): number {
    return this._version;
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }
}

export const nodeRegistry = new NodeRegistry();

/** Sugar for module-level registration in node definition files. */
export function registerNode(def: NodeDef): NodeDef {
  nodeRegistry.register(def);
  return def;
}
