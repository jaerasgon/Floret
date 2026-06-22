import { nodeRegistry } from '../../nodes/registry';
import type { EditorGet, EditorSet, EditorState } from './types';

type ViewSlice = Pick<
  EditorState,
  'enterSubpatch' | 'exitSubpatch' | 'setViewStack' | 'selectRegionPort' | 'clearRegionPortSelection'
>;

/** Subpatch view-stack navigation + region port-pill selection. */
export function createViewSlice(set: EditorSet, get: EditorGet): ViewSlice {
  return {
    selectRegionPort: (regionId, portId, additive) => {
      const key = `${regionId}#${portId}`;
      set((s) => {
        if (additive) {
          const has = s.selectedPortKeys.includes(key);
          const next = has
            ? s.selectedPortKeys.filter((k) => k !== key)
            : [...s.selectedPortKeys, key];
          return { selectedPortKeys: next };
        }
        // Replace selection. Also clear React Flow node/edge selection
        // so the user has a single clear focus on the pill they clicked.
        return {
          selectedPortKeys: [key],
          nodes: s.nodes.map((n) => (n.selected ? { ...n, selected: false } : n)),
          edges: s.edges.map((e) => (e.selected ? { ...e, selected: false } : e)),
        };
      });
    },

    clearRegionPortSelection: () => {
      set((s) => (s.selectedPortKeys.length === 0 ? {} : { selectedPortKeys: [] }));
    },

    enterSubpatch: (nodeId) => {
      const n = get().nodes.find((x) => x.id === nodeId);
      if (!n) return;
      const def = nodeRegistry.get(n.data.defType);
      if (def?.regionKind !== 'subpatch') return;
      // Skip if already at top of stack.
      const stack = get().viewStack;
      if (stack[stack.length - 1] === nodeId) return;
      set({ viewStack: [...stack, nodeId] });
    },

    exitSubpatch: () => {
      const stack = get().viewStack;
      if (stack.length === 0) return;
      set({ viewStack: stack.slice(0, -1) });
    },

    setViewStack: (stack) => {
      // Validate each id still refers to an existing subpatch ancestor chain.
      const nodes = get().nodes;
      const valid: string[] = [];
      let expectedParent: string | undefined = undefined;
      for (const id of stack) {
        const n = nodes.find((x) => x.id === id);
        if (!n) break;
        const def = nodeRegistry.get(n.data.defType);
        if (def?.regionKind !== 'subpatch') break;
        if ((n.parentId ?? undefined) !== expectedParent) break;
        valid.push(id);
        expectedParent = id;
      }
      set({ viewStack: valid });
    },
  };
}
