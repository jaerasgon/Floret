import { registerNode } from '../registry';

/**
 * A constant string node. Edit inline; outputs the value as a string
 * port (suitable for Text.text, Fill.hex, etc.).
 */
export const stringNode = registerNode({
  type: 'value/string',
  category: 'IO',
  label: 'String',
  description: 'Hold a constant string.',
  inputs: [],
  outputs: [{ name: 'text', type: 'string' }],
  defaultParams: { value: '' },
  readFromContext: (_frame, params) => {
    const v = params.param<string>('value', '');
    return [[v]];
  },
});
