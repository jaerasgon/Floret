/**
 * Recursive dock layout — a tree of splits and tabbed groups.
 *
 *   - A `split` lays its children out in a row or column, each child
 *     taking a fraction of the space (`sizes`, summing to 1).
 *   - A `group` is a tabbed container of one or more panel ids.
 *
 * Every docked panel is a tab in exactly one group; the canvas is just
 * another (pinned) panel. Dropping a panel on a group's edge splits that
 * region recursively; dropping on its centre adds a tab. All operations
 * here are pure — they take a tree and return a new one — so the store
 * stays a thin shell and the gnarly bits (pruning, same-axis flattening)
 * are unit-testable in isolation.
 */

export type DockNode = DockSplit | DockGroup;

export interface DockSplit {
  kind: 'split';
  id: string;
  dir: 'row' | 'col';
  children: DockNode[];
  /** Fractional sizes, parallel to `children`, summing to ~1. */
  sizes: number[];
}

export interface DockGroup {
  kind: 'group';
  id: string;
  panels: string[];
  active: number;
}

export type Side = 'left' | 'right' | 'top' | 'bottom' | 'center';

let _seq = 0;
/** Monotonic id for new splits/groups. (App runtime, not a workflow —
 *  a simple counter is fine and keeps trees diffable in tests.) */
export function dockId(prefix = 'd'): string {
  _seq += 1;
  return `${prefix}${_seq}`;
}

export function group(panels: string[], active = 0, id = dockId('g')): DockGroup {
  return { kind: 'group', id, panels, active: Math.max(0, Math.min(active, panels.length - 1)) };
}

export function split(dir: 'row' | 'col', children: DockNode[], sizes?: number[], id = dockId('s')): DockSplit {
  const n = children.length;
  const s = sizes && sizes.length === n ? sizes : children.map(() => 1 / n);
  return { kind: 'split', id, dir, children, sizes: normalise(s) };
}

function normalise(sizes: number[]): number[] {
  const sum = sizes.reduce((a, b) => a + b, 0) || 1;
  return sizes.map((x) => x / sum);
}

/** All panel ids anywhere in the tree, in document order. */
export function collectPanels(node: DockNode | null): string[] {
  if (!node) return [];
  if (node.kind === 'group') return [...node.panels];
  return node.children.flatMap(collectPanels);
}

export function findGroupOf(node: DockNode | null, panelId: string): DockGroup | null {
  if (!node) return null;
  if (node.kind === 'group') return node.panels.includes(panelId) ? node : null;
  for (const c of node.children) {
    const found = findGroupOf(c, panelId);
    if (found) return found;
  }
  return null;
}

export function findGroup(node: DockNode | null, groupId: string): DockGroup | null {
  if (!node) return null;
  if (node.kind === 'group') return node.id === groupId ? node : null;
  for (const c of node.children) {
    const found = findGroup(c, groupId);
    if (found) return found;
  }
  return null;
}

/**
 * Remove a panel from the tree. Empties → drop the group; a split left
 * with one child collapses into that child; same-axis nesting is left
 * alone (it re-flattens naturally on the next insert). Returns the new
 * root, or null if the tree is now empty.
 */
export function removePanel(node: DockNode | null, panelId: string): DockNode | null {
  if (!node) return null;
  if (node.kind === 'group') {
    if (!node.panels.includes(panelId)) return node;
    const panels = node.panels.filter((p) => p !== panelId);
    if (panels.length === 0) return null;
    return { ...node, panels, active: Math.min(node.active, panels.length - 1) };
  }
  const children: DockNode[] = [];
  const sizes: number[] = [];
  node.children.forEach((c, i) => {
    const next = removePanel(c, panelId);
    if (next) { children.push(next); sizes.push(node.sizes[i]!); }
  });
  if (children.length === 0) return null;
  if (children.length === 1) return children[0]!;
  return { ...node, children, sizes: normalise(sizes) };
}

/** Add a panel as a new tab in an existing group, made active. */
export function addTab(node: DockNode, groupId: string, panelId: string): DockNode {
  if (node.kind === 'group') {
    if (node.id !== groupId) return node;
    const panels = [...node.panels, panelId];
    return { ...node, panels, active: panels.length - 1 };
  }
  return { ...node, children: node.children.map((c) => addTab(c, groupId, panelId)) };
}

/**
 * Split a target group, inserting `panelId` as a new sibling group on the
 * given side. If the target's parent split already runs along the needed
 * axis, the new group is inserted as a sibling there (flattened) rather
 * than wrapping — so repeated drops on one axis stay shallow, like a real
 * docker. Otherwise the target is replaced by a fresh 2-child split.
 */
export function splitInsert(
  root: DockNode,
  targetGroupId: string,
  panelId: string,
  side: Exclude<Side, 'center'>,
): DockNode {
  const dir: 'row' | 'col' = side === 'left' || side === 'right' ? 'row' : 'col';
  const before = side === 'left' || side === 'top';
  const newGroup = group([panelId]);

  // Rebuild the tree, handling the target wherever it sits. We need parent
  // context to flatten, so recurse over splits and look at their children.
  function rebuild(node: DockNode): DockNode {
    if (node.kind === 'group') {
      // A group can only be split by wrapping (no parent context here);
      // the parent-split case is handled below before we recurse into it.
      if (node.id !== targetGroupId) return node;
      return split(dir, before ? [newGroup, node] : [node, newGroup], [0.5, 0.5]);
    }
    // Look for the target directly among this split's children so we can
    // flatten when axes match.
    const idx = node.children.findIndex((c) => c.kind === 'group' && c.id === targetGroupId);
    if (idx >= 0 && node.dir === dir) {
      const children = [...node.children];
      const sizes = [...node.sizes];
      const half = sizes[idx]! / 2;
      sizes[idx] = half;
      const insertAt = before ? idx : idx + 1;
      children.splice(insertAt, 0, newGroup);
      sizes.splice(insertAt, 0, half);
      return { ...node, children, sizes: normalise(sizes) };
    }
    // Otherwise recurse; the target (if a child group on the cross axis,
    // or deeper) gets wrapped by the group-case above.
    return { ...node, children: node.children.map(rebuild) };
  }

  return rebuild(root);
}

/** Split at the outermost level — dock to a viewport edge. */
export function splitRoot(root: DockNode, panelId: string, side: Exclude<Side, 'center'>): DockNode {
  const dir: 'row' | 'col' = side === 'left' || side === 'right' ? 'row' : 'col';
  const before = side === 'left' || side === 'top';
  const newGroup = group([panelId]);
  if (root.kind === 'split' && root.dir === dir) {
    const children = before ? [newGroup, ...root.children] : [...root.children, newGroup];
    const share = 1 / children.length;
    return { ...root, children, sizes: children.map(() => share) };
  }
  return split(dir, before ? [newGroup, root] : [root, newGroup], [0.3, 0.7]);
}

export function setSizes(node: DockNode, splitId: string, sizes: number[]): DockNode {
  if (node.kind === 'group') return node;
  if (node.id === splitId) return { ...node, sizes: normalise(sizes) };
  return { ...node, children: node.children.map((c) => setSizes(c, splitId, sizes)) };
}

export function setActive(node: DockNode, groupId: string, active: number): DockNode {
  if (node.kind === 'group') {
    if (node.id !== groupId) return node;
    return { ...node, active: Math.max(0, Math.min(active, node.panels.length - 1)) };
  }
  return { ...node, children: node.children.map((c) => setActive(c, groupId, active)) };
}
