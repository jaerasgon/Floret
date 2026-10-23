import { describe, expect, it } from 'vitest';
import { compileGraph, type RegistryLike } from '../compile';
import { tick } from '../tick';
import type { NodeDef } from '../../nodes/types';

function makeRegistry(defs: NodeDef[]): RegistryLike {
  const map = new Map(defs.map((d) => [d.type, d] as const));
  return {
    get: (t) => map.get(t),
    require: (t) => {
      const d = map.get(t);
      if (!d) throw new Error(`unknown: ${t}`);
      return d;
    },
  };
}

const frameZero = {
  t: 0, dt: 0, frameCount: 0, width: 400, height: 300,
  mouseX: 0, mouseY: 0, mouseIsPressed: false, key: '', keyIsPressed: false,
};

describe('compileGraph (scalar / kinded compute)', () => {
  it('computes a scalar chain via applyKinded', () => {
    const defs: NodeDef[] = [
      {
        type: 'val/constant',
        category: 't',
        label: 'C',
        inputs: [],
        outputs: [{ name: 'out', type: 'number' }],
        readFromContext: (_f, params) => [[params.param<number>('v', 0)]],
        defaultParams: { v: 0 },
      },
      {
        type: 'math/add',
        category: 't',
        label: 'Add',
        inputs: [
          { name: 'a', type: 'number', default: 0 },
          { name: 'b', type: 'number', default: 0 },
        ],
        outputs: [{ name: 'sum', type: 'number' }],
        compute: (a, b) => [(a as number) + (b as number)],
      },
    ];

    const result = compileGraph(
      [
        { id: 'a', defType: 'val/constant', params: { v: 7 } },
        { id: 'b', defType: 'val/constant', params: { v: 35 } },
        { id: 'sum', defType: 'math/add', params: {} },
      ],
      [
        { fromNode: 'a', fromPort: 'out', toNode: 'sum', toPort: 'a' },
        { fromNode: 'b', fromPort: 'out', toNode: 'sum', toPort: 'b' },
      ],
      {
        registry: makeRegistry(defs),
        paramReader: (id) =>
          id === 'a' ? { v: 7 } : id === 'b' ? { v: 35 } : {},
      },
    );

    expect(result.warnings).toEqual([]);
    const t = tick(result.graph, frameZero);
    expect(t.outputs.get('sum')).toEqual([[42]]);
  });

  it('passes spread-kind inputs through whole', () => {
    const defs: NodeDef[] = [
      {
        type: 'gen/range',
        category: 't',
        label: 'Range',
        inputs: [],
        outputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
        computeRaw: () => [[0, 1, 2, 3]],
      },
      {
        type: 'op/count',
        category: 't',
        label: 'Count',
        inputs: [{ name: 'spread', type: 'number', kind: 'spread' }],
        outputs: [{ name: 'n', type: 'number' }],
        compute: (s) => [(s as ReadonlyArray<unknown>).length],
      },
    ];

    const result = compileGraph(
      [
        { id: 'r', defType: 'gen/range', params: {} },
        { id: 'c', defType: 'op/count', params: {} },
      ],
      [{ fromNode: 'r', fromPort: 'spread', toNode: 'c', toPort: 'spread' }],
      { registry: makeRegistry(defs), paramReader: () => ({}) },
    );

    expect(result.warnings).toEqual([]);
    const t = tick(result.graph, frameZero);
    expect(t.outputs.get('c')).toEqual([[4]]);
  });

  it('port-value overrides feed kinded inputs', () => {
    const defs: NodeDef[] = [
      {
        type: 'math/dbl',
        category: 't',
        label: 'Double',
        inputs: [{ name: 'x', type: 'number', default: 0 }],
        outputs: [{ name: 'out', type: 'number' }],
        compute: (x) => [(x as number) * 2],
      },
    ];

    const result = compileGraph(
      [{ id: 'd', defType: 'math/dbl', params: {} }],
      [],
      {
        registry: makeRegistry(defs),
        paramReader: () => ({}),
        portValueReader: (_id, port) => (port === 'x' ? 21 : undefined),
      },
    );
    expect(tick(result.graph, frameZero).outputs.get('d')).toEqual([[42]]);
  });
});
