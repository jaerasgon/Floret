import type { Edge, Node as RFNode } from '@xyflow/react';
import { nodeRegistry } from '../../nodes/registry';
import {
  useEditor,
  getDefTypeFor,
  getRegionPortsFor,
  getParamsFor,
} from '../editor-state';
import { isMultiTargetPort } from '../connection-rules';
import type { ContextMenuItem } from '../overlay-store';
import { moveNodeToParent } from './region-hit';

const SEP: ContextMenuItem = { separator: true, label: '', onSelect: () => {} };

/** Items for a right-click on empty canvas: add node / comment / paste. */
export function buildPaneMenu(
  flowPos: { x: number; y: number },
  screenPos: { x: number; y: number },
  deps: {
    openSearch: (o: { screenPosition: { x: number; y: number }; flowPosition: { x: number; y: number } }) => void;
    addNode: (defType: string, pos: { x: number; y: number }) => string;
    hasClipboard: () => boolean;
    pasteAt: (pos: { x: number; y: number }) => string[];
  },
): ContextMenuItem[] {
  return [
    {
      label: 'Add node…',
      onSelect: () => deps.openSearch({ screenPosition: screenPos, flowPosition: flowPos }),
      shortcut: 'Dbl-click',
    },
    {
      label: 'Add comment',
      onSelect: () => deps.addNode('meta/comment', flowPos),
    },
    SEP,
    {
      label: 'Paste',
      shortcut: 'Ctrl+V',
      disabled: !deps.hasClipboard(),
      onSelect: () => deps.pasteAt(flowPos),
    },
  ];
}

/** Items for a right-click on an edge: reorder on a multi pin, or delete. */
export function buildEdgeMenu(
  edge: Edge,
  deps: {
    reorderMultiEdge: (edgeId: string, delta: number) => void;
    removeEdge: (edgeId: string) => void;
  },
): ContextMenuItem[] {
  const isMulti =
    edge.target && edge.targetHandle
      ? isMultiTargetPort(
          edge.target, edge.targetHandle,
          getDefTypeFor, getRegionPortsFor, getParamsFor,
        )
      : false;
  const items: ContextMenuItem[] = [];
  if (isMulti) {
    // Compute this edge's current slot among siblings sharing the
    // target multi pin — so the menu can show "(1/N)" and disable
    // moves that would no-op.
    const s = useEditor.getState();
    const subset = s.edges.filter(
      (x) => x.target === edge.target && x.targetHandle === edge.targetHandle,
    );
    const idx = subset.findIndex((x) => x.id === edge.id);
    items.push({
      label: `Move up · (${idx + 1}/${subset.length})`,
      disabled: idx <= 0,
      shortcut: '[',
      onSelect: () => deps.reorderMultiEdge(edge.id, -1),
    });
    items.push({
      label: 'Move down',
      disabled: idx >= subset.length - 1,
      shortcut: ']',
      onSelect: () => deps.reorderMultiEdge(edge.id, +1),
    });
    items.push({
      label: 'Move to top',
      disabled: idx <= 0,
      onSelect: () => deps.reorderMultiEdge(edge.id, -subset.length),
    });
    items.push({
      label: 'Move to bottom',
      disabled: idx >= subset.length - 1,
      onSelect: () => deps.reorderMultiEdge(edge.id, +subset.length),
    });
    items.push(SEP);
  }
  items.push({
    label: 'Delete wire',
    shortcut: 'Del',
    danger: true,
    onSelect: () => deps.removeEdge(edge.id),
  });
  return items;
}

/** Items for a right-click on a node: clipboard, region membership, group, delete. */
export function buildNodeMenu(
  node: RFNode,
  flowPos: { x: number; y: number },
  allNodes: ReadonlyArray<RFNode>,
  selectedCount: number,
  deps: {
    duplicateNode: (id: string) => string | null;
    copySelected: () => number;
    deleteNode: (id: string) => void;
    hasClipboard: () => boolean;
    pasteAt: (pos: { x: number; y: number }) => string[];
    setNodeParent: (id: string, parentId: string | null, position?: { x: number; y: number }) => void;
    groupSelectionIntoSubpatch: () => string | null;
  },
): ContextMenuItem[] {
  const items: ContextMenuItem[] = [
    {
      label: 'Duplicate',
      shortcut: 'Ctrl+D',
      onSelect: () => deps.duplicateNode(node.id),
    },
    {
      label: 'Copy',
      shortcut: 'Ctrl+C',
      onSelect: () => deps.copySelected(),
    },
    {
      label: 'Cut',
      shortcut: 'Ctrl+X',
      onSelect: () => {
        deps.copySelected();
        deps.deleteNode(node.id);
      },
    },
    {
      label: 'Paste',
      shortcut: 'Ctrl+V',
      disabled: !deps.hasClipboard(),
      onSelect: () => deps.pasteAt(flowPos),
    },
  ];

  // Region-membership items (skip on region nodes themselves — they
  // can't sit inside other regions in the current model).
  const nodeDefType = (node.data as { defType?: string } | undefined)?.defType;
  const isRegion = nodeDefType ? !!nodeRegistry.get(nodeDefType)?.regionKind : false;
  if (!isRegion) {
    items.push(SEP);
    if (node.parentId) {
      items.push({
        label: 'Move out of region',
        shortcut: 'Space+drag',
        onSelect: () => moveNodeToParent(node, null, allNodes, deps.setNodeParent),
      });
    }
    // List all regions that could receive this node.
    const targets = allNodes.filter((n) => {
      const t = (n.data as { defType?: string } | undefined)?.defType;
      return t && !!nodeRegistry.get(t)?.regionKind && n.id !== node.parentId && n.id !== node.id;
    });
    for (const region of targets) {
      const regionLabel =
        ((region.data as { params?: Record<string, unknown> } | undefined)
          ?.params?.label as string | undefined) ?? 'Region';
      items.push({
        label: `Move into ${regionLabel}`,
        onSelect: () => moveNodeToParent(node, region.id, allNodes, deps.setNodeParent),
      });
    }
  }

  // Group-into-subpatch — only if there's a selection (>= 1 selected).
  if (selectedCount >= 1) {
    items.push(SEP);
    items.push({
      label: selectedCount === 1
        ? 'Wrap in subpatch'
        : `Group ${selectedCount} into subpatch`,
      shortcut: 'Ctrl+G',
      onSelect: () => deps.groupSelectionIntoSubpatch(),
    });
  }

  items.push(SEP);
  items.push({
    label: 'Delete',
    shortcut: 'Del',
    danger: true,
    onSelect: () => deps.deleteNode(node.id),
  });

  return items;
}
