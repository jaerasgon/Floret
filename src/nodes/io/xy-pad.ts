import { registerNode } from '../registry';
import { XYPadNode } from '../../editor/nodes/XYPadNode';

export const xyPadNode = registerNode({
  type: 'io/xy-pad',
  category: 'IO',
  label: 'XYPad',
  description: 'Drag a 2D point — outputs x, y in [0, 1].',
  inputs: [],
  outputs: [
    { name: 'x', type: 'number' },
    { name: 'y', type: 'number' },
  ],
  defaultParams: { x: 0.5, y: 0.5 },
  readFromContext: (_frame, params) => [
    [params.param<number>('x', 0.5)],
    [params.param<number>('y', 0.5)],
  ],
  component: XYPadNode,
});
