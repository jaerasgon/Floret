import { describe, expect, it } from 'vitest';
import {
  buildNodeIndex,
  absolutePosition,
  findAncestor,
  isDescendantOf,
  type GeoNode,
} from '../canvas-geometry';

const n = (id: string, x: number, y: number, parentId?: string): GeoNode => ({
  id, parentId, position: { x, y },
});

// root → region(r) → child(c) → grandchild(g)
const nodes: GeoNode[] = [
  n('root', 10, 10),
  n('r', 100, 100),
  n('c', 5, 5, 'r'),
  n('g', 2, 2, 'c'),
  n('orphan', 0, 0, 'missing'), // dangling parent ref
];

describe('canvas-geometry', () => {
  it('buildNodeIndex maps every node by id', () => {
    const idx = buildNodeIndex(nodes);
    expect(idx.size).toBe(nodes.length);
    expect(idx.get('g')?.position).toEqual({ x: 2, y: 2 });
  });

  describe('absolutePosition', () => {
    const byId = buildNodeIndex(nodes);
    it('returns own position for a root node', () => {
      expect(absolutePosition('r', byId)).toEqual({ x: 100, y: 100 });
    });
    it('sums one level of nesting', () => {
      expect(absolutePosition('c', byId)).toEqual({ x: 105, y: 105 });
    });
    it('sums a deep chain', () => {
      expect(absolutePosition('g', byId)).toEqual({ x: 107, y: 107 });
    });
    it('returns {0,0} for an unknown id', () => {
      expect(absolutePosition('nope', byId)).toEqual({ x: 0, y: 0 });
    });
    it('treats a dangling parent as origin', () => {
      // orphan.parent = 'missing' (not indexed) → contributes {0,0}.
      expect(absolutePosition('orphan', byId)).toEqual({ x: 0, y: 0 });
    });
  });

  describe('findAncestor', () => {
    const byId = buildNodeIndex(nodes);
    it('finds the nearest ancestor matching the predicate', () => {
      expect(findAncestor('g', byId, (x) => x.id === 'r')?.id).toBe('r');
    });
    it('excludes the node itself', () => {
      expect(findAncestor('r', byId, (x) => x.id === 'r')).toBeNull();
    });
    it('returns null when no ancestor matches', () => {
      expect(findAncestor('g', byId, (x) => x.id === 'zzz')).toBeNull();
    });
    it('returns the first (nearest) match walking up', () => {
      // both 'c' and 'r' are ancestors of 'g'; nearest is 'c'.
      expect(findAncestor('g', byId, (x) => x.id === 'c' || x.id === 'r')?.id).toBe('c');
    });
  });

  describe('isDescendantOf', () => {
    const byId = buildNodeIndex(nodes);
    it('is true for the node itself', () => {
      expect(isDescendantOf('r', 'r', byId)).toBe(true);
    });
    it('is true for a deep descendant', () => {
      expect(isDescendantOf('g', 'r', byId)).toBe(true);
    });
    it('is false for an unrelated node', () => {
      expect(isDescendantOf('root', 'r', byId)).toBe(false);
    });
    it('is false in the wrong direction', () => {
      expect(isDescendantOf('r', 'g', byId)).toBe(false);
    });
  });

  it('findAncestor is cycle-safe', () => {
    // a → b → a (pathological): must terminate, not stack-overflow.
    const cyclic = buildNodeIndex([n('a', 0, 0, 'b'), n('b', 0, 0, 'a')]);
    expect(findAncestor('a', cyclic, (x) => x.id === 'zzz')).toBeNull();
  });
});
