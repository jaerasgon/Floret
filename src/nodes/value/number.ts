import { registerNode } from '../registry';

/**
 * A constant number node whose value lives in `node.data.params.value`.
 * The editor's NumberNode component renders an inline input that writes
 * the field; the runtime reads it every tick via `readFromContext` so a
 * value change is picked up without recompiling the graph.
 */
export const numberNode = registerNode({
  type: 'value/number',
  category: 'IO',
  label: 'Number',
  description: 'Hold a constant number.',
  inputs: [],
  outputs: [{ name: 'number', type: 'number' }],
  defaultParams: { value: 0 },
  readFromContext: (_frame, params) => {
    const v = params.param<number>('value', 0);
    return [[v]];
  },
});
