import { registerNode } from '../registry';
import { ToggleNode } from '../../editor/nodes/ToggleNode';

export const toggleNode = registerNode({
  type: 'io/toggle',
  category: 'IO',
  label: 'Toggle',
  description: 'Click to flip a boolean on or off.',
  inputs: [],
  outputs: [{ name: 'out', type: 'boolean' }],
  defaultParams: { value: false },
  readFromContext: (_frame, params) => [[params.param<boolean>('value', false)]],
  component: ToggleNode,
});
