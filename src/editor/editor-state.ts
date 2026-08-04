import { create } from 'zustand';
import { addEdge, applyEdgeChanges, applyNodeChanges } from '@xyflow/react';
import { nodeRegistry } from '../nodes/registry';
import { paramStore } from '../runtime/param-store';
import { prefsSnapshot } from '../app/prefs';
import { parseHandle, wouldCreateCycle, isMultiTargetPort } from './connection-rules';

import type { EditorState, FloretEdge, FloretNode, RegionPortDescriptor } from './editor-state/types';
import { newId, sortNodesParentFirst, isStructuralNodeChange, isStructuralEdgeChange } from './editor-state/util';
import { pushHistory, resetHistory, _setHistoryPush, _setHistoryReset } from './editor-state/history-bridge';
import { resetRegistries } from './editor-state/registry-sync';
import { createCompilePipeline } from './editor-state/compile-pipeline';
import { buildSubpatchGrouping } from './editor-state/subpatch-grouping';
import { createViewSlice } from './editor-state/view-slice';
import { createRegionPortSlice } from './editor-state/region-port-slice';
import { createClipboardSlice } from './editor-state/clipboard-slice';

// Re-export the editor-state types + history bridge setters so existing
// importers (`./editor-state`) keep working after the slice split.
export type { FloretNodeData, RegionPortDescriptor, FloretNode, FloretEdge, EditorState } from './editor-state/types';
export { sortNodesParentFirst } from './editor-state/util';
export { _setHistoryPush, _setHistoryReset };

// Dev-only: expose the editor store on window for browser-side debugging.
// Stripped by Vite's NODE_ENV=production build.
const _exposeStore = (s: unknown): void => {
  if (typeof window !== 'undefined') {
    (window as unknown as { __editor?: unknown }).__editor = s;
  }
};

export const useEditor = create<EditorState>((set, get) => {
  const { compile: _compile } = createCompilePipeline(get, set);

  return {
    nodes: [],
    edges: [],
    runtime: null,
    runtimeVersion: 0,
    warnings: [],
    viewStack: [],
    selectedPortKeys: [],

    ...createViewSlice(set, get),
    ...createRegionPortSlice(set, get, { compile: _compile }),
    ...createClipboardSlice(set, get, { compile: _compile }),

    groupSelectionIntoSubpatch: () => {
      const result = buildSubpatchGrouping(get().nodes, get().edges);
      if (!result) return null;
      paramStore.setNode(result.subId, result.subParams);
      set({ nodes: result.nodes, edges: result.edges });
      _compile();
      pushHistory();
      return result.subId;
    },

    setNodes: (n) => {
      set({ nodes: n });
      _compile();
    },
    setEdges: (e) => {
      set({ edges: e });
      _compile();
    },

    onNodesChange: (changes) => {
      set((s) => ({ nodes: applyNodeChanges(changes, s.nodes) as FloretNode[] }));
      if (changes.some(isStructuralNodeChange)) {
        _compile();
        pushHistory();
      }
    },
    onEdgesChange: (changes) => {
      set((s) => ({ edges: applyEdgeChanges(changes, s.edges) }));
      if (changes.some(isStructuralEdgeChange)) {
        _compile();
        pushHistory();
      }
    },
    onConnect: (conn) => {
      // Enforce single connection per target handle EXCEPT on multi-kind
      // inputs (which gather N wires into a spread). Source handles can
      // always fan out to many targets.
      set((s) => {
        const isMulti = conn.target
          ? isMultiTargetPort(conn.target, conn.targetHandle, getDefTypeFor, getRegionPortsFor, getParamsFor)
          : false;
        const filtered = isMulti
          ? s.edges
          : s.edges.filter(
              (e) => !(e.target === conn.target && e.targetHandle === conn.targetHandle),
            );
        // No cycles. Defensive — isValidConnection already blocks the
        // visual drop, but onConnect can still fire for programmatic paths.
        if (conn.source && conn.target && wouldCreateCycle(conn.source, conn.target, conn.targetHandle, filtered, getDefTypeFor, conn.sourceHandle)) {
          return { edges: s.edges };
        }
        return { edges: addEdge(conn, filtered) };
      });
      _compile();
      pushHistory();
    },

    addNode: (defType, position) => {
      const def = nodeRegistry.require(defType);
      const id = newId(defType.replace('/', '_'));
      const initialParams = { ...(def.defaultParams ?? {}) };
      // A freshly-dropped Renderer inherits the user's default resolution
      // (Preferences → Canvas) rather than the static NodeDef fallback.
      if (defType === 'render/renderer') {
        initialParams.width = prefsSnapshot.defaultRenderWidth;
        initialParams.height = prefsSnapshot.defaultRenderHeight;
      }
      // Auto-pick the smallest unused integer slot for the four
      // Read/Write/Read*/Write* nodes. Scoped per-type: dropping Read,
      // Write gives both slot 1; dropping another Read takes slot 2.
      // Read* and Write* keep their own independent slot namespaces.
      if (
        defType === 'state/read' ||
        defType === 'state/write' ||
        defType === 'state/read-spread' ||
        defType === 'state/write-spread'
      ) {
        const used = new Set<number>();
        for (const n of get().nodes) {
          if (n.data.defType === defType) {
            const s = Number(n.data.params?.slot);
            if (Number.isFinite(s)) used.add(s);
          }
        }
        let slot = 1;
        while (used.has(slot)) slot++;
        initialParams.slot = slot;
      }
      // If the user has entered a subpatch, new nodes belong to it.
      // The given `position` is treated as already-local to that scope —
      // callers (palette drop, search, paste) project screen→flow via
      // React Flow before invoking us, and React Flow's flow coords are
      // relative to the parent's origin for children of a parent node.
      const currentView = get().viewStack[get().viewStack.length - 1];
      const node: FloretNode = {
        id,
        type: 'floret',
        position,
        data: {
          defType,
          params: initialParams,
        },
      };
      if (currentView) {
        node.parentId = currentView;
        node.extent = 'parent';
      }
      paramStore.setNode(id, initialParams);
      set((s) => ({ nodes: sortNodesParentFirst([...s.nodes, node]) }));
      _compile();
      pushHistory();
      return id;
    },

    removeSelected: () => {
      const beforeIds = new Set(get().nodes.map((n) => n.id));
      // Resolve selected Port Pills into (regionId, portId) pairs so we
      // can strip them and any edges touching them at the same time as
      // selected nodes/edges. Doing it in one `set` keeps the runtime
      // recompile to a single tick at the end.
      const portsToRemove = get().selectedPortKeys
        .map((k) => {
          const ix = k.indexOf('#');
          return ix > 0 ? { regionId: k.slice(0, ix), portId: k.slice(ix + 1) } : null;
        })
        .filter((p): p is { regionId: string; portId: string } => p !== null);
      const portRemovalIndex = new Map<string, Set<string>>();
      for (const { regionId, portId } of portsToRemove) {
        let s = portRemovalIndex.get(regionId);
        if (!s) { s = new Set(); portRemovalIndex.set(regionId, s); }
        s.add(portId);
      }
      set((s) => {
        const keepIds = new Set(s.nodes.filter((n) => !n.selected).map((n) => n.id));
        const nodes = s.nodes
          .filter((n) => keepIds.has(n.id))
          .map((n) => {
            const drop = portRemovalIndex.get(n.id);
            if (!drop || !n.data.ports) return n;
            const remaining = n.data.ports.filter((p) => !drop.has(p.id));
            if (remaining.length === n.data.ports.length) return n;
            return { ...n, data: { ...n.data, ports: remaining } };
          });
        // An edge is gone if either endpoint node is gone, OR it's a
        // wire touching one of the freshly-removed region ports — the
        // handle id pattern is `in-<portId>` / `out-<portId>`.
        const touchesRemovedPort = (nodeId: string, handle: string | null | undefined): boolean => {
          const drop = portRemovalIndex.get(nodeId);
          if (!drop || !handle) return false;
          const m = /^(?:in|out)-(.+)$/.exec(handle);
          return !!m && drop.has(m[1]!);
        };
        const edges = s.edges.filter(
          (e) =>
            keepIds.has(e.source) &&
            keepIds.has(e.target) &&
            !e.selected &&
            !touchesRemovedPort(e.source, e.sourceHandle) &&
            !touchesRemovedPort(e.target, e.targetHandle),
        );
        for (const id of beforeIds) if (!keepIds.has(id)) paramStore.removeNode(id);
        return { nodes, edges, selectedPortKeys: [] };
      });
      _compile();
      pushHistory();
    },

    setNodeParam: (nodeId, key, value) => {
      paramStore.setKey(nodeId, key, value);
      set((s) => {
        const nodes = s.nodes.map((n) =>
          n.id === nodeId
            ? { ...n, data: { ...n.data, params: { ...n.data.params, [key]: value } } }
            : n,
        );
        // If the changed param drives dynamicInputs, drop any edges that
        // referenced now-defunct input ports.
        const changedNode = nodes.find((n) => n.id === nodeId);
        const def = changedNode ? nodeRegistry.get(changedNode.data.defType) : undefined;
        let edges = s.edges;
        if (def?.dynamicInputs && changedNode) {
          const activeNames = new Set(
            def.dynamicInputs(changedNode.data.params).map((p) => p.name),
          );
          edges = s.edges.filter((e) => {
            if (e.target !== nodeId) return true;
            const parsed = parseHandle(e.targetHandle);
            if (!parsed) return true;
            return activeNames.has(parsed.name);
          });
        }
        return { nodes, edges };
      });
      // A param change that shifts dynamicInputs needs a recompile so
      // the runtime sees the new shape; plain params don't.
      const node = get().nodes.find((n) => n.id === nodeId);
      const def = node ? nodeRegistry.get(node.data.defType) : undefined;
      if (def?.dynamicInputs) _compile();
      pushHistory(`${nodeId}.${key}`);
    },

    loadPatch: (nodes, edges) => {
      // New patch = fresh audio + image state. Tear down any oscillators /
      // effects / loaded images owned by the previous patch so they don't
      // leak into the new one.
      resetRegistries();
      paramStore.replaceAll(nodes.map((n) => [n.id, n.data.params] as const));
      // React Flow v12 requires parents to appear before their children
      // in the nodes array; samples & save files may not respect that.
      set({ nodes: sortNodesParentFirst(nodes), edges, selectedPortKeys: [] });
      _compile();
      // Seed the undo stack with the freshly-loaded patch so the first
      // Ctrl+Z doesn't wind back past it. The history module suppresses
      // this when *it* is the caller (during undo/redo apply), so this
      // call is a no-op there.
      resetHistory();
    },

    addEdge: (source, sourceHandle, target, targetHandle) => {
      // Skip if an identical edge already exists.
      const existing = get().edges.some(
        (e) =>
          e.source === source &&
          e.target === target &&
          e.sourceHandle === sourceHandle &&
          e.targetHandle === targetHandle,
      );
      if (existing) return;
      const id = `e_${newId('edge')}`;
      set((s) => {
        // Enforce single connection per (target, targetHandle) EXCEPT on
        // multi-kind inputs, which accept N wires and gather them into a
        // spread at runtime.
        const isMulti = isMultiTargetPort(target, targetHandle, getDefTypeFor, getRegionPortsFor, getParamsFor);
        const filtered = isMulti
          ? s.edges
          : s.edges.filter(
              (e) => !(e.target === target && e.targetHandle === targetHandle),
            );
        // No cycles. Defensive — caller may not have run isValidConnection.
        if (wouldCreateCycle(source, target, targetHandle, filtered, getDefTypeFor, sourceHandle)) {
          return { edges: s.edges };
        }
        return {
          edges: [
            ...filtered,
            { id, source, target, sourceHandle, targetHandle },
          ],
        };
      });
      _compile();
      pushHistory();
    },

    removeEdge: (edgeId) => {
      set((s) => ({ edges: s.edges.filter((e) => e.id !== edgeId) }));
      _compile();
      pushHistory();
    },

    reorderMultiEdge: (edgeId, delta) => {
      if (delta === 0) return;
      const s = get();
      const target = s.edges.find((e) => e.id === edgeId);
      if (!target || !target.targetHandle) return;
      // Only meaningful when the target handle is a multi pin (gather
      // semantics); skip on plain single/spread pins where edge order
      // doesn't affect behaviour.
      const isMulti = isMultiTargetPort(
        target.target, target.targetHandle,
        getDefTypeFor, getRegionPortsFor, getParamsFor,
      );
      if (!isMulti) return;
      // Reorder within the subset of edges feeding the same multi pin.
      const subset = s.edges.filter(
        (e) => e.target === target.target && e.targetHandle === target.targetHandle,
      );
      const subIdx = subset.findIndex((e) => e.id === edgeId);
      const nextIdx = Math.max(0, Math.min(subset.length - 1, subIdx + delta));
      if (nextIdx === subIdx) return;
      // Build a fresh subset array with the moved edge in its new slot,
      // then thread it back through the global edges list at the slots
      // currently occupied by subset members (preserving the positions
      // of non-multi-pin edges).
      const reordered = [...subset];
      const [moved] = reordered.splice(subIdx, 1);
      reordered.splice(nextIdx, 0, moved!);
      const subsetIds = new Set(subset.map((e) => e.id));
      let cur = 0;
      const next = s.edges.map((e) =>
        subsetIds.has(e.id) ? reordered[cur++]! : e,
      );
      set({ edges: next });
      _compile();
      pushHistory();
    },

    deleteNode: (nodeId) => {
      const before = get().nodes;
      if (!before.find((n) => n.id === nodeId)) return;
      paramStore.removeNode(nodeId);
      set((s) => ({
        nodes: s.nodes.filter((n) => n.id !== nodeId),
        edges: s.edges.filter((e) => e.source !== nodeId && e.target !== nodeId),
      }));
      _compile();
      pushHistory();
    },

    setNodeParent: (nodeId, parentId, position) => {
      set((s) => ({
        nodes: sortNodesParentFirst(
          s.nodes.map((n) => {
            if (n.id !== nodeId) return n;
            const next: FloretNode = {
              ...n,
              parentId: parentId ?? undefined,
              extent: parentId ? 'parent' : undefined,
            };
            if (position) next.position = position;
            return next;
          }),
        ),
      }));
      _compile();
      pushHistory();
    },

    setNodePortValue: (nodeId, portName, value) => {
      set((s) => ({
        nodes: s.nodes.map((n) =>
          n.id === nodeId
            ? {
                ...n,
                data: {
                  ...n.data,
                  portValues: { ...(n.data.portValues ?? {}), [portName]: value },
                },
              }
            : n,
        ),
      }));
      // Light recompile so the new value reaches the runtime immediately.
      _compile();
      pushHistory(`${nodeId}.port.${portName}`);
    },

    clearNodePortValue: (nodeId, portName) => {
      set((s) => ({
        nodes: s.nodes.map((n) => {
          if (n.id !== nodeId) return n;
          if (!n.data.portValues || !(portName in n.data.portValues)) return n;
          const next = { ...n.data.portValues };
          delete next[portName];
          return { ...n, data: { ...n.data, portValues: next } };
        }),
      }));
      _compile();
      pushHistory(`${nodeId}.port.${portName}`);
    },

    recompile: _compile,
  };
});

_exposeStore(useEditor);

/** Convenience: lookup a node's defType (used by connection-rules). */
export function getDefTypeFor(nodeId: string): string | undefined {
  return useEditor.getState().nodes.find((n) => n.id === nodeId)?.data.defType;
}

/** Lookup a region's dynamic ports (used by connection-rules to type-check edges). */
export function getRegionPortsFor(nodeId: string): RegionPortDescriptor[] | undefined {
  return useEditor.getState().nodes.find((n) => n.id === nodeId)?.data.ports;
}

/** Lookup a node's current params (used by connection-rules for dynamicInputs). */
export function getParamsFor(nodeId: string): Record<string, unknown> | undefined {
  return useEditor.getState().nodes.find((n) => n.id === nodeId)?.data.params;
}

/** Live edges snapshot for connection-rules' cycle check. */
export function getEdgesSnapshot(): ReadonlyArray<FloretEdge> {
  return useEditor.getState().edges;
}
