import { registerNode } from '../registry';
import type { NodeParamContext } from '../types';

/**
 * Inline CSV → number spread.
 *
 * The CSV text lives in the `data` param. Cells are separated by
 * commas, newlines, spaces, or tabs (any run of those). Each cell is
 * parsed as a number; non-numeric cells become `NaN` and are dropped
 * before the spread is emitted.
 *
 * Use case: small static data sets baked into a patch — bar charts,
 * scatter plots, sequencer steps. For real CSV-from-file we'd need a
 * loader node hitting `fetch`; deferred until needed.
 */
export const csvInlineNode = registerNode({
  type: 'data/csv-inline',
  category: 'Data',
  label: 'CSVInline',
  description: 'Parse inline CSV data into a number spread.',
  defaultParams: {
    data: '12, 45, 22, 67, 38, 91, 54, 73, 29, 60, 84, 19, 41, 77, 33, 55',
  },
  inputs: [],
  outputs: [{ name: 'out', type: 'number', kind: 'spread' }],
  readFromContext: (_frame, params: NodeParamContext) => {
    const text = params.param<string>('data', '');
    const parts = text.split(/[,\s\n\t]+/).map((s) => s.trim()).filter(Boolean);
    const nums: number[] = [];
    for (const p of parts) {
      const n = Number(p);
      if (Number.isFinite(n)) nums.push(n);
    }
    return [nums];
  },
});
