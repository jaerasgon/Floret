import { nodeRegistry } from '../../nodes/registry';
import { paramStore } from '../../runtime/param-store';
import {
  copySelectionToClipboard,
  hasClipboard,
  pasteFromClipboard,
} from '../clipboard';
import type { EditorGet, EditorSet, EditorState, FloretEdge, FloretNode, FloretNodeData } from './types';
import { newId, sortNodesParentFirst } from './util';
import { pushHistory } from './history-bridge';

type ClipboardSlice = Pick<
  EditorState,
  'copySelected' | 'pasteAt' | 'hasClipboard' | 'duplicateNode' | 'duplicateSelection'
>;

/** Copy/paste + in-place / selection duplication. */
export function createClipboardSlice(
  set: EditorSet,
  get: EditorGet,
  deps: { compile: () => void },
): ClipboardSlice {
  const { compile } = deps;
  return {
    copySelected: () => {
      const s = get();
      return copySelectionToClipboard(s.nodes, s.edges);
    },

    pasteAt: (anchor) => {
      const { nodes: newNodes, edges: newEdges, ids } = pasteFromClipboard(anchor);
      if (newNodes.length === 0) return [];
      for (const n of newNodes) paramStore.setNode(n.id, n.data.params);
      set((s) => ({
        // Deselect everything previously selected so the paste is the new selection.
        nodes: [
          ...s.nodes.map((n) => (n.selected ? { ...n, selected: false } : n)),
          ...newNodes,
        ],
        edges: [...s.edges, ...newEdges],
      }));
      compile();
      pushHistory();
      return ids;
    },

    hasClipboard: () => hasClipboard(),

    duplicateNode: (nodeId, offset = { x: 24, y: 24 }) => {
      const src = get().nodes.find((n) => n.id === nodeId);
      if (!src) return null;
      const def = nodeRegistry.get(src.data.defType);
      if (!def) return null;
      const id = newId(src.data.defType.replace('/', '_'));
      const params = { ...src.data.params };
      const node: FloretNode = {
        id,
        type: 'floret',
        position: { x: src.position.x + offset.x, y: src.position.y + offset.y },
        data: { defType: src.data.defType, params },
        selected: true,
      };
      paramStore.setNode(id, params);
      set((s) => ({
        nodes: [...s.nodes.map((n) => (n.selected ? { ...n, selected: false } : n)), node],
      }));
      compile();
      pushHistory();
      return id;
    },

    duplicateSelection: (offset = { x: 0, y: 0 }) => {
      const state = get();
      const selected = state.nodes.filter((n) => n.selected);
      if (selected.length === 0) return [];

      // Build new ids and a map from old → new.
      const idMap = new Map<string, string>();
      for (const n of selected) {
        idMap.set(n.id, newId(n.data.defType.replace('/', '_')));
      }

      const newNodes: FloretNode[] = selected.map((n) => {
        const newNodeId = idMap.get(n.id)!;
        // Clone region ports too — they carry edge connections.
        const dataClone: FloretNodeData = {
          defType: n.data.defType,
          params: { ...n.data.params },
          ...(n.data.portValues ? { portValues: { ...n.data.portValues } } : {}),
          ...(n.data.ports ? { ports: n.data.ports.map((p) => ({ ...p })) } : {}),
        };
        paramStore.setNode(newNodeId, dataClone.params);
        const copy: FloretNode = {
          id: newNodeId,
          type: 'floret',
          position: { x: n.position.x + offset.x, y: n.position.y + offset.y },
          data: dataClone,
          selected: false, // originals stay selected so the active drag follows them
        };
        // If the original lived inside a region AND the parent region
        // was also part of the selection, the duplicate goes into the
        // duplicated parent. Otherwise it inherits the original parent.
        const remappedParent = n.parentId ? idMap.get(n.parentId) ?? n.parentId : undefined;
        if (remappedParent) {
          copy.parentId = remappedParent;
          copy.extent = 'parent';
        }
        return copy;
      });

      // Duplicate edges where BOTH ends were in the selection.
      const newEdges: FloretEdge[] = [];
      for (const e of state.edges) {
        const src = idMap.get(e.source);
        const dst = idMap.get(e.target);
        if (src && dst) {
          newEdges.push({
            id: `e_dup_${newId('').slice(1)}`,
            source: src,
            target: dst,
            sourceHandle: e.sourceHandle,
            targetHandle: e.targetHandle,
          });
        }
      }

      set((s) => ({
        nodes: sortNodesParentFirst([...s.nodes, ...newNodes]),
        edges: [...s.edges, ...newEdges],
      }));
      compile();
      pushHistory();
      return [...idMap.values()];
    },
  };
}
