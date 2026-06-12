import { describe, expect, it } from 'vitest';
import {
  group, split, collectPanels, findGroupOf, findGroup,
  removePanel, addTab, splitInsert, splitRoot, setSizes, setActive,
  maxIdSeq, sanitize,
  type DockNode, type DockGroup,
} from '../dockTree';

// A small fixed tree:  row[ A | col[ B / C ] ]
function tree(): DockNode {
  return split('row', [
    group(['A'], 0, 'gA'),
    split('col', [group(['B'], 0, 'gB'), group(['C'], 0, 'gC')], [0.5, 0.5], 'sCol'),
  ], [0.3, 0.7], 'sRow');
}

describe('dockTree — queries', () => {
  it('collects all panels in order', () => {
    expect(collectPanels(tree())).toEqual(['A', 'B', 'C']);
  });
  it('finds the group containing a panel', () => {
    expect(findGroupOf(tree(), 'B')?.id).toBe('gB');
    expect(findGroupOf(tree(), 'Z')).toBeNull();
  });
  it('finds a group by id', () => {
    expect(findGroup(tree(), 'gC')?.panels).toEqual(['C']);
  });
});

describe('dockTree — addTab', () => {
  it('appends a panel as the active tab', () => {
    const t = addTab(tree(), 'gA', 'X') as DockNode;
    const g = findGroup(t, 'gA')!;
    expect(g.panels).toEqual(['A', 'X']);
    expect(g.active).toBe(1);
  });
});

describe('dockTree — removePanel', () => {
  it('removes a panel from a multi-tab group, keeping the group', () => {
    const withTab = addTab(tree(), 'gA', 'X');
    const t = removePanel(withTab, 'X')!;
    expect(findGroup(t, 'gA')!.panels).toEqual(['A']);
  });
  it('drops an emptied group and collapses the single-child split', () => {
    // Removing B leaves col[ C ] → collapses to group C; then row[ A | C ].
    const t = removePanel(tree(), 'B')!;
    expect(collectPanels(t)).toEqual(['A', 'C']);
    // The col split collapsed — root is still a row with two groups.
    expect(t.kind).toBe('split');
    if (t.kind === 'split') {
      expect(t.children.every((c) => c.kind === 'group')).toBe(true);
    }
  });
  it('returns null when the last panel is removed', () => {
    expect(removePanel(group(['only']), 'only')).toBeNull();
  });
});

describe('dockTree — splitInsert', () => {
  it('wraps a group in a new split on the cross axis', () => {
    // gA is in a row; splitting it top/bottom needs a col wrap.
    const t = splitInsert(tree(), 'gA', 'X', 'top');
    const node = (t as Extract<DockNode, { kind: 'split' }>).children[0]!;
    expect(node.kind).toBe('split');
    if (node.kind === 'split') {
      expect(node.dir).toBe('col');
      expect(collectPanels(node)).toEqual(['X', 'A']); // before=true (top)
    }
  });

  it('flattens into the parent split when axes match', () => {
    // gA sits in a row; dropping X to its right is the same axis → insert
    // as a sibling in the existing row, NOT a nested split.
    const t = splitInsert(tree(), 'gA', 'X', 'right') as Extract<DockNode, { kind: 'split' }>;
    expect(t.dir).toBe('row');
    expect(t.children).toHaveLength(3); // gA, X-group, col-split
    expect(t.children[0]!.kind).toBe('group');
    expect(t.children[1]!.kind).toBe('group');
    expect(collectPanels(t.children[1]!)).toEqual(['X']);
    // sizes still sum to 1
    expect(t.sizes.reduce((a, b) => a + b, 0)).toBeCloseTo(1, 6);
  });

  it('places the new panel before/after according to side', () => {
    const left = splitInsert(tree(), 'gA', 'X', 'left') as Extract<DockNode, { kind: 'split' }>;
    expect(collectPanels(left.children[0]!)).toEqual(['X']); // X inserted before A
    expect(collectPanels(left.children[1]!)).toEqual(['A']);
  });
});

describe('dockTree — splitRoot', () => {
  it('docks to a root edge, flattening same-axis', () => {
    const t = splitRoot(tree(), 'X', 'left') as Extract<DockNode, { kind: 'split' }>;
    expect(t.dir).toBe('row');
    expect(collectPanels(t.children[0]!)).toEqual(['X']);
    expect(t.children).toHaveLength(3);
  });
  it('wraps when the root axis differs', () => {
    const t = splitRoot(tree(), 'X', 'bottom') as Extract<DockNode, { kind: 'split' }>;
    expect(t.dir).toBe('col');
    expect(t.children).toHaveLength(2);
    expect(collectPanels(t.children[1]!)).toEqual(['X']); // bottom = after
  });
});

describe('dockTree — sanitize / id seq', () => {
  it('reports the highest id sequence', () => {
    expect(maxIdSeq(tree())).toBe(0); // ids are non-numeric in this fixture (gA…)
    expect(maxIdSeq(split('row', [group(['A'], 0, 'g1'), group(['B'], 0, 'g2')], [0.5, 0.5], 's7'))).toBe(7);
  });

  it('dedupes a panel that appears in two groups (keeps the first)', () => {
    const corrupt = split('row', [group(['A'], 0, 'g1'), group(['A', 'B'], 0, 'g2')], [0.5, 0.5], 's1');
    const fixed = sanitize(corrupt)!;
    // 'A' kept only in the first group; second group keeps just 'B'.
    expect(collectPanels(fixed)).toEqual(['A', 'B']);
  });

  it('regenerates duplicate node ids', () => {
    // Two groups sharing id 'g1' — the second must be re-keyed.
    const corrupt = split('row', [group(['A'], 0, 'g1'), group(['B'], 0, 'g1')], [0.5, 0.5], 's1');
    const fixed = sanitize(corrupt) as Extract<DockNode, { kind: 'split' }>;
    const ids = fixed.children.map((c) => c.id);
    expect(new Set(ids).size).toBe(2); // no collision
  });

  it('collapses a split left with one child after dedup', () => {
    const corrupt = split('row', [group(['A'], 0, 'g1'), group(['A'], 0, 'g2')], [0.5, 0.5], 's1');
    const fixed = sanitize(corrupt)!;
    expect(fixed.kind).toBe('group'); // second group emptied → split collapses
    expect(collectPanels(fixed)).toEqual(['A']);
  });
});

describe('dockTree — sizes & tabs', () => {
  it('sets and re-normalises split sizes', () => {
    const t = setSizes(tree(), 'sRow', [2, 8]) as Extract<DockNode, { kind: 'split' }>;
    expect(t.sizes[0]).toBeCloseTo(0.2, 6);
    expect(t.sizes[1]).toBeCloseTo(0.8, 6);
  });
  it('sets the active tab, clamped', () => {
    const withTab = addTab(tree(), 'gA', 'X');
    const t = setActive(withTab, 'gA', 9);
    expect((findGroup(t, 'gA') as DockGroup).active).toBe(1);
  });
});
