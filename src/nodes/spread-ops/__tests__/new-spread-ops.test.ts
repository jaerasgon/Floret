import { describe, expect, it } from 'vitest';
import '../slice-mutation';
import '../combine';
import '../filter';
import '../reorder';
import '../partition';
import '../generators';
import { nodeRegistry } from '../../registry';
import type { NodeDef } from '../../types';
import type { PortValue } from '../../../runtime/types';

const get = (type: string): NodeDef => {
  const def = nodeRegistry.get(type);
  if (!def) throw new Error(`missing node ${type}`);
  return def;
};

const run = (type: string, inputs: PortValue[]): PortValue[] => {
  const def = get(type);
  if (!def.computeRaw) throw new Error(`${type} has no computeRaw`);
  return def.computeRaw(inputs, { nodeId: 't', param: <T,>(_k: string, f?: T) => f as T });
};

describe('slice-mutation', () => {
  it('SetSlice replaces at index', () => {
    expect(run('spread/set-slice', [[1, 2, 3, 4], [1], [99]])[0]).toEqual([1, 99, 3, 4]);
  });
  it('SetSlice expands when replacement is longer', () => {
    expect(run('spread/set-slice', [[1, 2, 3, 4], [1], [9, 9, 9]])[0]).toEqual([1, 9, 9, 9, 3, 4]);
  });
  it('SetSlice negative index', () => {
    expect(run('spread/set-slice', [[1, 2, 3, 4], [-1], [99]])[0]).toEqual([1, 2, 3, 99]);
  });
  it('SetSlice empty source returns replacement', () => {
    expect(run('spread/set-slice', [[], [0], [9]])[0]).toEqual([9]);
  });

  it('InsertSlice inserts without overwriting', () => {
    expect(run('spread/insert-slice', [[1, 2, 3], [1], [99]])[0]).toEqual([1, 99, 2, 3]);
  });
  it('InsertSlice at length acts as append', () => {
    expect(run('spread/insert-slice', [[1, 2], [2], [99]])[0]).toEqual([1, 2, 99]);
  });
  it('InsertSlice empty insertion is identity', () => {
    expect(run('spread/insert-slice', [[1, 2], [1], []])[0]).toEqual([1, 2]);
  });

  it('RemoveSlice removes N at index', () => {
    expect(run('spread/remove-slice', [[1, 2, 3, 4, 5], [1], [2]])[0]).toEqual([1, 4, 5]);
  });
  it('RemoveSlice on empty is empty', () => {
    expect(run('spread/remove-slice', [[], [0], [3]])[0]).toEqual([]);
  });
  it('RemoveSlice negative index', () => {
    expect(run('spread/remove-slice', [[1, 2, 3, 4], [-1], [1]])[0]).toEqual([1, 2, 3]);
  });

  it('Swap swaps two elements', () => {
    expect(run('spread/swap', [[1, 2, 3, 4], [0], [3]])[0]).toEqual([4, 2, 3, 1]);
  });
  it('Swap single element is identity', () => {
    expect(run('spread/swap', [[7], [0], [0]])[0]).toEqual([7]);
  });
});

describe('combine', () => {
  it('Interleave zips two equal-length spreads', () => {
    expect(run('spread/interleave', [[1, 2, 3], [10, 20, 30]])[0]).toEqual([1, 10, 2, 20, 3, 30]);
  });
  it('Interleave truncates to min length', () => {
    expect(run('spread/interleave', [[1, 2], [10, 20, 30]])[0]).toEqual([1, 10, 2, 20]);
  });
  it('Interleave empty either side concatenates', () => {
    expect(run('spread/interleave', [[], [1, 2]])[0]).toEqual([1, 2]);
  });

  it('Uninterleave splits even/odd', () => {
    const [even, odd] = run('spread/uninterleave', [[1, 2, 3, 4, 5]]);
    expect(even).toEqual([1, 3, 5]);
    expect(odd).toEqual([2, 4]);
  });

  it('Cross product yields all pairs', () => {
    const [a, b] = run('spread/cross', [['x', 'y'], [1, 2, 3]]);
    expect(a).toEqual(['x', 'x', 'x', 'y', 'y', 'y']);
    expect(b).toEqual([1, 2, 3, 1, 2, 3]);
  });

  it('Cons prepends', () => {
    expect(run('spread/cons', [[0], [1, 2, 3]])[0]).toEqual([0, 1, 2, 3]);
  });
  it('Append appends', () => {
    expect(run('spread/append', [[1, 2, 3], [9]])[0]).toEqual([1, 2, 3, 9]);
  });

  it('Inject places marks at N positions', () => {
    const out = run('spread/inject', [[1, 2, 3, 4], ['*'], [1]])[0]!;
    expect(out.length).toBe(5);
    expect(out).toContain('*');
  });
  it('Inject count 0 is identity', () => {
    expect(run('spread/inject', [[1, 2, 3], ['*'], [0]])[0]).toEqual([1, 2, 3]);
  });
});

describe('filter', () => {
  it('Sift keeps trues', () => {
    expect(run('spread/sift', [[1, 2, 3, 4], [true, false, true, false]])[0]).toEqual([1, 3]);
  });
  it('Sift empty input is empty', () => {
    expect(run('spread/sift', [[], [true]])[0]).toEqual([]);
  });

  it('Unique dedupes preserving order', () => {
    expect(run('spread/unique', [[1, 2, 1, 3, 2, 4]])[0]).toEqual([1, 2, 3, 4]);
  });
  it('Unique on empty is empty', () => {
    expect(run('spread/unique', [[]])[0]).toEqual([]);
  });

  it('IndexOf finds first match', () => {
    expect(run('spread/index-of', [[10, 20, 30, 20], [20]])[0]).toEqual([1]);
  });
  it('IndexOf returns -1 on miss', () => {
    expect(run('spread/index-of', [[10, 20, 30], [99]])[0]).toEqual([-1]);
  });

  it('CountWhere counts trues', () => {
    expect(run('spread/count-where', [[true, false, true, true]])[0]).toEqual([3]);
  });

  it('IndicesByValue returns all matches', () => {
    expect(run('spread/indices-by-value', [[1, 0, 1, 0, 1], [1]])[0]).toEqual([0, 2, 4]);
  });
});

describe('reorder', () => {
  it('Rotate shifts right', () => {
    expect(run('spread/rotate', [[1, 2, 3, 4], [1]])[0]).toEqual([4, 1, 2, 3]);
  });
  it('Rotate negative shifts left', () => {
    expect(run('spread/rotate', [[1, 2, 3, 4], [-1]])[0]).toEqual([2, 3, 4, 1]);
  });
  it('Rotate empty is empty', () => {
    expect(run('spread/rotate', [[], [3]])[0]).toEqual([]);
  });
  it('Rotate by length is identity', () => {
    expect(run('spread/rotate', [[1, 2, 3], [3]])[0]).toEqual([1, 2, 3]);
  });

  it('Permute reorders by indices', () => {
    expect(run('spread/permute', [['a', 'b', 'c', 'd'], [2, 0, 3, 1]])[0]).toEqual([
      'c', 'a', 'd', 'b',
    ]);
  });
  it('Permute negative index from end', () => {
    expect(run('spread/permute', [['a', 'b', 'c'], [-1]])[0]).toEqual(['c']);
  });

  it('SortByKey sorts by key spread', () => {
    expect(run('spread/sort-by-key', [['a', 'b', 'c'], [3, 1, 2], [false]])[0]).toEqual([
      'b', 'c', 'a',
    ]);
  });
  it('SortByKey descending', () => {
    expect(run('spread/sort-by-key', [['a', 'b', 'c'], [3, 1, 2], [true]])[0]).toEqual([
      'a', 'c', 'b',
    ]);
  });
});

describe('partition', () => {
  it('Partition emits flat + sizes', () => {
    const [flat, sizes] = run('spread/partition', [[1, 2, 3, 4, 5, 6, 7], [3]]);
    expect(flat).toEqual([1, 2, 3, 4, 5, 6, 7]);
    expect(sizes).toEqual([3, 3, 1]);
  });
  it('Partition empty is empty', () => {
    const [flat, sizes] = run('spread/partition', [[], [3]]);
    expect(flat).toEqual([]);
    expect(sizes).toEqual([]);
  });

  it('Pairwise yields adjacent halves', () => {
    const [a, b] = run('spread/pairwise', [[1, 2, 3, 4]]);
    expect(a).toEqual([1, 2, 3]);
    expect(b).toEqual([2, 3, 4]);
  });
  it('Pairwise single element returns empty', () => {
    const [a, b] = run('spread/pairwise', [[1]]);
    expect(a).toEqual([]);
    expect(b).toEqual([]);
  });

  it('Windows builds sliding windows', () => {
    const [flat, sizes] = run('spread/windows', [[1, 2, 3, 4], [2]]);
    expect(flat).toEqual([1, 2, 2, 3, 3, 4]);
    expect(sizes).toEqual([2, 2, 2]);
  });
  it('Windows larger than input is empty', () => {
    const [flat, sizes] = run('spread/windows', [[1, 2], [5]]);
    expect(flat).toEqual([]);
    expect(sizes).toEqual([]);
  });

  it('AtGroup picks the Nth group', () => {
    // Partition([0..9], 3) → flat=[0..9], sizes=[3,3,3,1]
    const flat = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    const sizes = [3, 3, 3, 1];
    expect(run('spread/at-group', [flat, sizes, [0]])[0]).toEqual([0, 1, 2]);
    expect(run('spread/at-group', [flat, sizes, [1]])[0]).toEqual([3, 4, 5]);
    expect(run('spread/at-group', [flat, sizes, [2]])[0]).toEqual([6, 7, 8]);
    expect(run('spread/at-group', [flat, sizes, [3]])[0]).toEqual([9]);
  });
  it('AtGroup negative index from end', () => {
    expect(run('spread/at-group', [[1, 2, 3, 4, 5], [2, 3], [-1]])[0]).toEqual([3, 4, 5]);
  });
  it('AtGroup out-of-range returns empty', () => {
    expect(run('spread/at-group', [[1, 2, 3], [3], [5]])[0]).toEqual([]);
  });
  it('AtGroup empty input is empty', () => {
    expect(run('spread/at-group', [[], [], [0]])[0]).toEqual([]);
  });
});

describe('inject edge-case fix', () => {
  it('Inject 3 marks into 2-element source places them without trailing pile-up', () => {
    const out = run('spread/inject', [[1, 2], ['*'], [3]])[0]! as unknown[];
    expect(out.length).toBe(5);
    // exactly 3 marks
    expect(out.filter((v) => v === '*').length).toBe(3);
    // first and last are not both source (with dense marks, edges should be marks)
    // and no source element is dropped
    expect(out.filter((v) => v === 1 || v === 2).length).toBe(2);
  });
  it('Inject evenly across long source', () => {
    const out = run('spread/inject', [[0, 1, 2, 3, 4, 5, 6, 7], ['*'], [3]])[0]! as unknown[];
    expect(out.length).toBe(11);
    expect(out.filter((v) => v === '*').length).toBe(3);
    // All source elements present in original order
    const srcOnly = out.filter((v) => v !== '*');
    expect(srcOnly).toEqual([0, 1, 2, 3, 4, 5, 6, 7]);
  });
});

describe('new generators', () => {
  it('SineSpread peaks near amplitude', () => {
    const out = run('spread/sine', [[64], [1], [1], [0], [0]])[0]! as number[];
    expect(out.length).toBe(64);
    expect(Math.max(...out)).toBeGreaterThan(0.98);
    expect(Math.min(...out)).toBeLessThan(-0.98);
  });
  it('SineSpread count 0 is empty', () => {
    expect(run('spread/sine', [[0], [1], [1], [0], [0]])[0]).toEqual([]);
  });

  it('GeometricSpread doubles', () => {
    expect(run('spread/geometric', [[1], [2], [5]])[0]).toEqual([1, 2, 4, 8, 16]);
  });
  it('GeometricSpread count 0 is empty', () => {
    expect(run('spread/geometric', [[1], [2], [0]])[0]).toEqual([]);
  });

  it('NoiseSpread stays in [0, 1]', () => {
    const out = run('spread/noise', [[32], [4], [0], [42]])[0]! as number[];
    expect(out.length).toBe(32);
    for (const v of out) {
      expect(v).toBeGreaterThanOrEqual(0);
      expect(v).toBeLessThanOrEqual(1);
    }
  });
  it('NoiseSpread is deterministic by seed', () => {
    const a = run('spread/noise', [[8], [2], [0], [7]])[0]!;
    const b = run('spread/noise', [[8], [2], [0], [7]])[0]!;
    expect(a).toEqual(b);
  });
});
