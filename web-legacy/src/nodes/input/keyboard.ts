import { registerNode } from '../registry';

export const keyboardNode = registerNode({
  type: 'input/keyboard',
  category: 'Input',
  label: 'Keyboard',
  description: 'Read the last key pressed in the preview.',
  inputs: [],
  outputs: [
    { name: 'pressed', type: 'boolean' },
    { name: 'key', type: 'string' },
  ],
  readFromContext: (frame) => [[frame.keyIsPressed], [frame.key]],
});
