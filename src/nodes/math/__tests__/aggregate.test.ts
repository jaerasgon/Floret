import { describe, expect, it } from 'vitest';
import '../aggregate';
import { nodeRegistry } from '../../registry';
import type { PortValue } from '../../../runtime/types';

const run = (type: string, inputs: PortValue[]): PortValue[] => {
  const def = nodeRegistry.get(type);
  if (!def?.computeRaw) throw new Error(`missing ${type}`);
  return def.computeRaw(inputs, { nodeId: 't', param: <T,>(_k: string, f?: T) => f as T });
};

describe('math/aggregate', () => {
  it('Sum sums', () => {
    expect(run('math/sum', [[1, 2, 3, 4]])[0]).toEqual([10]);
  });
  it('Sum empty is 0', () => {
    expect(run('math/sum', [[]])[0]).toEqual([0]);
  });

  it('Mean averages', () => {
    expect(run('math/mean', [[2, 4, 6]])[0]).toEqual([4]);
  });
  it('Mean empty is 0', () => {
    expect(run('math/mean', [[]])[0]).toEqual([0]);
  });

  it('MinSpread / MaxSpread', () => {
    expect(run('math/min-spread', [[3, 1, 4, 1, 5]])[0]).toEqual([1]);
    expect(run('math/max-spread', [[3, 1, 4, 1, 5]])[0]).toEqual([5]);
  });
  it('MinSpread empty defaults to 0', () => {
    expect(run('math/min-spread', [[]])[0]).toEqual([0]);
  });

  it('Median odd length', () => {
    expect(run('math/median', [[3, 1, 2]])[0]).toEqual([2]);
  });
  it('Median even length averages middles', () => {
    expect(run('math/median', [[1, 2, 3, 4]])[0]).toEqual([2.5]);
  });

  it('StdDev of constant is 0', () => {
    expect(run('math/stddev', [[5, 5, 5]])[0]).toEqual([0]);
  });
  it('StdDev of [1,2,3] = sqrt(2/3)', () => {
    const [v] = run('math/stddev', [[1, 2, 3]])[0]! as number[];
    expect(v).toBeCloseTo(Math.sqrt(2 / 3), 10);
  });

  it('Cumulative running sum', () => {
    expect(run('math/cumulative', [[1, 2, 3, 4]])[0]).toEqual([1, 3, 6, 10]);
  });
  it('Cumulative empty is empty', () => {
    expect(run('math/cumulative', [[]])[0]).toEqual([]);
  });

  it('Delta differences', () => {
    expect(run('math/delta', [[1, 3, 7, 12]])[0]).toEqual([2, 4, 5]);
  });
  it('Delta single element is empty', () => {
    expect(run('math/delta', [[5]])[0]).toEqual([]);
  });

  it('Histogram bins', () => {
    expect(run('math/histogram', [[0, 0.1, 0.4, 0.6, 0.9, 0.99], [4], [0], [1]])[0]).toEqual([
      2, 1, 1, 2,
    ]);
  });
  it('Histogram empty has zero counts', () => {
    expect(run('math/histogram', [[], [3], [0], [1]])[0]).toEqual([0, 0, 0]);
  });
});
