import { nodeRegistry } from '../../nodes/registry';
import { getResolvedInputs } from '../../nodes/types';
import { compileGraph, parseLoopVarHandle } from '../../runtime/compile';
import type { PortType } from '../../runtime/types';
import { paramStore } from '../../runtime/param-store';
import { markDirty } from '../../runtime/dirty';
import { parseHandle } from '../connection-rules';
import type { EditorGet, EditorSet } from './types';
import { toEditorRefs } from './util';
import { retainRegistries } from './registry-sync';

/**
 * The editor → runtime compile pipeline. Returns a single `compile()` the
 * store calls after every structural mutation. Bundles the three pre-passes
 * that run ahead of `compileGraph`:
 *   - orphan-edge pruning (drop wires whose endpoints vanished),
 *   - adaptive-type reconciliation (the `valueType` latch on arithmetic
 *     nodes),
 *   - registry retain (free per-node runtime resources for dead nodes).
 */
export function createCompilePipeline(get: EditorGet, set: EditorSet): { compile: () => void } {
  /**
   * Defensive sweep: drop any edge whose source or target node no
   * longer exists, OR whose target handle doesn't resolve to a port on
   * the current node shape. Runs at the head of every compile so that
   * dynamic-input changes, port-rename, etc. can't leave wires
   * dangling at stale coordinates.
   */
  const pruneOrphanEdges = () => {
    const s = get();
    const nodeById = new Map(s.nodes.map((n) => [n.id, n] as const));
    const cleaned = s.edges.filter((e) => {
      const srcNode = nodeById.get(e.source);
      const dstNode = nodeById.get(e.target);
      if (!srcNode || !dstNode) return false;
      return true;
    });
    if (cleaned.length !== s.edges.length) {
      set({ edges: cleaned });
    }
  };

  /**
   * Concrete value type of a node's port (or null when it's `any` / unknown).
   * Adaptive peers report their latched `valueType`; region ports report their
   * declared type; loop-var pins are number; everything else uses its def.
   */
  const portConcreteType = (nodeId: string, handle: string | null | undefined): PortType | null => {
    const parsed = parseHandle(handle);
    if (!parsed) return null;
    const n = get().nodes.find((x) => x.id === nodeId);
    const def = n ? nodeRegistry.get(n.data.defType) : undefined;
    if (!n || !def) return null;
    let t: PortType | undefined;
    if (def.adaptiveType) {
      const vt = n.data.params.valueType;
      t = vt === 'vec2' || vt === 'color' || vt === 'number' ? vt : undefined;
    } else if (def.regionKind) {
      if (parseLoopVarHandle(parsed.name)) t = 'number';
      else t = n.data.ports?.find((p) => p.id === parsed.name)?.type
        ?? def.inputs.find((p) => p.name === parsed.name)?.type;
    } else if (parsed.kind === 'out') {
      t = def.outputs.find((p) => p.name === parsed.name)?.type;
    } else {
      t = getResolvedInputs(def, n.data.params).find((p) => p.name === parsed.name)?.type;
    }
    if (!t || t === 'any') return null;
    if (t === 'boolean') return 'number'; // number↔boolean coercion
    return t === 'number' || t === 'vec2' || t === 'color' ? t : null;
  };

  /**
   * Maintain the `valueType` latch on adaptive arithmetic nodes. First wire
   * wins: while a node is `any`, the first edge that yields a concrete peer
   * type (number / vec2 / color) latches it; later wires can't change it.
   * When the last wire is removed, it resets to `any`. Idempotent — safe to
   * run on every compile. Returns true if anything changed.
   */
  const reconcileAdaptiveTypes = (): boolean => {
    const s = get();
    let changed = false;
    const nextNodes = s.nodes.map((n) => {
      const def = nodeRegistry.get(n.data.defType);
      if (!def?.adaptiveType) return n;
      const cur = (n.data.params.valueType as string | undefined) ?? 'any';
      // Only the ADAPTIVE ports drive the latch — fixed-number controls
      // (lerp's `t`, motion's `rate`/`stiffness`/`damping`) must not. Detect
      // them by diffing the resolved inputs across two value-types: an input
      // that changes type is adaptive. The single `result` output is adaptive.
      const insA = getResolvedInputs(def, { ...n.data.params, valueType: 'number' });
      const insB = getResolvedInputs(def, { ...n.data.params, valueType: 'vec2' });
      const adaptiveIns = new Set<string>();
      insA.forEach((p, i) => { if (insB[i] && insB[i]!.type !== p.type) adaptiveIns.add(p.name); });
      const touching = s.edges.filter((e) => {
        if (e.source === n.id) return true; // result output follows valueType
        if (e.target === n.id) {
          const p = parseHandle(e.targetHandle);
          return !!p && adaptiveIns.has(p.name);
        }
        return false;
      });
      let next = cur;
      if (touching.length === 0) {
        next = 'any';
      } else if (cur === 'any') {
        for (const e of touching) {
          const peer = e.source === n.id
            ? portConcreteType(e.target, e.targetHandle)   // our result → their input
            : portConcreteType(e.source, e.sourceHandle);  // their output → our operand
          if (peer) { next = peer; break; }
        }
      }
      if (next === cur) return n;
      changed = true;
      paramStore.setKey(n.id, 'valueType', next);
      return { ...n, data: { ...n.data, params: { ...n.data.params, valueType: next } } };
    });
    if (changed) set({ nodes: nextNodes });
    return changed;
  };

  const compile = () => {
    pruneOrphanEdges();
    reconcileAdaptiveTypes();
    const { nodes, edges } = toEditorRefs(get());
    // Audio + image registry sync — drop entries for any node that no longer exists.
    const liveIds = new Set(nodes.map((n) => n.id));
    retainRegistries(liveIds);
    try {
      const result = compileGraph(nodes, edges, {
        registry: nodeRegistry,
        paramReader: (id) => {
          const found = get().nodes.find((n) => n.id === id);
          return found?.data.params ?? {};
        },
        portValueReader: (id, portName) => {
          const found = get().nodes.find((n) => n.id === id);
          return found?.data.portValues?.[portName];
        },
      });
      set((s) => ({
        runtime: result.graph,
        runtimeVersion: s.runtimeVersion + 1,
        warnings: result.warnings,
      }));
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn('compileGraph failed:', err);
      set((s) => ({
        runtime: null,
        runtimeVersion: s.runtimeVersion + 1,
        warnings: [{ message: String(err) }],
      }));
    }
    // Cook-on-change: every recompile is a new graph that must paint at
    // least once. (Idle frames after that depend on the graph's own
    // alwaysAnimate flag + paramStore changes.)
    markDirty();
  };

  return { compile };
}
