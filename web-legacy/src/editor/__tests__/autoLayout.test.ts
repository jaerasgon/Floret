import { describe, it, expect } from 'vitest';
import { autoLayoutPatch } from '../autoLayout';
import type { FloretNode, FloretEdge } from '../editor-state';

// Headless: nodes have no `measured` size, so rfNodeBox falls back to 200×100.
const n = (id: string, parentId?: string): FloretNode => ({
  id, type: 'floret', position: { x: 0, y: 0 },
  data: { defType: 'math/add', params: {} }, parentId,
} as unknown as FloretNode);
const e = (s: string, t: string): FloretEdge => ({
  id: `${s}-${t}`, source: s, target: t, sourceHandle: 'a', targetHandle: 'b',
} as unknown as FloretEdge);
const at = (out: FloretNode[], id: string) => out.find((o) => o.id === id)!.position;

describe('autoLayoutPatch', () => {
  it('places a dependency chain strictly left-to-right', () => {
    const out = autoLayoutPatch([n('a'), n('b'), n('c')], [e('a', 'b'), e('b', 'c')]);
    expect(at(out, 'a').x).toBeLessThan(at(out, 'b').x);
    expect(at(out, 'b').x).toBeLessThan(at(out, 'c').x);
  });

  it('stacks same-layer nodes with no overlap', () => {
    // a and b both feed c → a,b share a column, must not overlap (height 100).
    const out = autoLayoutPatch([n('a'), n('b'), n('c')], [e('a', 'c'), e('b', 'c')]);
    const a = at(out, 'a'), b = at(out, 'b');
    expect(Math.abs(a.x - b.x)).toBeLessThan(1);              // same column
    expect(Math.abs(a.y - b.y)).toBeGreaterThanOrEqual(100);  // ≥ node height apart
  });

  it('centres a fan-in target near the barycentre of its inputs', () => {
    const out = autoLayoutPatch([n('a'), n('b'), n('c')], [e('a', 'c'), e('b', 'c')]);
    const a = at(out, 'a'), b = at(out, 'b'), c = at(out, 'c');
    expect(c.y).toBeGreaterThan(Math.min(a.y, b.y) - 1);
    expect(c.y).toBeLessThan(Math.max(a.y, b.y) + 1);
  });

  it('resizes a region to fit its tidied children', () => {
    const region = {
      id: 'r', type: 'floret', position: { x: 0, y: 0 },
      data: { defType: 'region/foreach', params: {} }, style: { width: 100, height: 100 },
    } as unknown as FloretNode;
    const out = autoLayoutPatch([region, n('c1', 'r'), n('c2', 'r')], []);
    const r = out.find((o) => o.id === 'r')!;
    const style = r.style as { width: number; height: number };
    expect(style.width).toBeGreaterThan(100);   // grew past the seed size to hold 200-wide children
    expect(style.height).toBeGreaterThan(100);
  });

  it('parks comment nodes to the far right', () => {
    const comment = {
      id: 'cm', type: 'floret', position: { x: 0, y: 0 }, data: { defType: 'meta/comment', params: {} },
    } as unknown as FloretNode;
    const out = autoLayoutPatch([n('a'), n('b'), comment], [e('a', 'b')]);
    expect(at(out, 'cm').x).toBeGreaterThan(at(out, 'b').x);
  });
});
