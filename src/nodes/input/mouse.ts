import { registerNode } from '../registry';

/**
 * Mouse — source node reading from FrameCtx. Outputs:
 *   - x (number, canvas px)
 *   - y (number, canvas px)
 *   - pressed (number 0/1 — until we have first-class boolean wiring)
 */
export const mouseNode = registerNode({
  type: 'input/mouse',
  category: 'Input',
  label: 'Mouse',
  description: 'Read mouse position and button state from the preview.',
  inputs: [],
  outputs: [
    { name: 'x', type: 'number' },
    { name: 'y', type: 'number' },
    { name: 'pressed', type: 'boolean' },
  ],
  readFromContext: (frame) => [
    [frame.mouseX],
    [frame.mouseY],
    [frame.mouseIsPressed],
  ],
});
