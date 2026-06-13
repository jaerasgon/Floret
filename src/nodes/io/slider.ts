import { registerNode } from '../registry';
import { SliderNode } from '../../editor/nodes/SliderNode';

export const sliderNode = registerNode({
  type: 'io/slider',
  category: 'IO',
  label: 'Slider',
  description: 'Drag to set a number.',
  inputs: [],
  outputs: [{ name: 'number', type: 'number' }],
  defaultParams: { value: 0.5, min: 0, max: 1 },
  readFromContext: (_frame, params) => [[params.param<number>('value', 0)]],
  component: SliderNode,
});
