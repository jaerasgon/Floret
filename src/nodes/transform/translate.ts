import { registerNode } from '../registry';
import type { Layer } from '../../runtime/types';
import { translateMat, rotateMat, scaleMat } from '../../runtime/scene';

/**
 * Transform nodes wrap their inner Layer in a `group` SceneNode with
 * the appropriate affine. The scene renderer applies the transform
 * under `save()/restore()` so siblings are unaffected.
 */

export const translateNode = registerNode({
  type: 'transform/translate',
  category: 'Transform',
  label: 'Translate',
  description: 'Translate a layer by (x, y).',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
    { name: 'layer', type: 'layer' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  compute: (x, y, layer) => {
    const inner = layer as Layer;
    const node: Layer = {
      kind: 'group',
      children: [inner],
      transform: translateMat(x as number, y as number),
    };
    return [node];
  },
});

export const rotateNode = registerNode({
  type: 'transform/rotate',
  category: 'Transform',
  label: 'Rotate',
  description: 'Rotate a layer by `angle` radians around the origin.',
  inputs: [
    { name: 'angle', type: 'number', default: 0 },
    { name: 'layer', type: 'layer' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  compute: (angle, layer) => {
    const inner = layer as Layer;
    const node: Layer = {
      kind: 'group',
      children: [inner],
      transform: rotateMat(angle as number),
    };
    return [node];
  },
});

export const scaleNode = registerNode({
  type: 'transform/scale',
  category: 'Transform',
  label: 'Scale',
  description: 'Scale a layer by (sx, sy).',
  inputs: [
    { name: 'sx', type: 'number', default: 1 },
    { name: 'sy', type: 'number', default: 1 },
    { name: 'layer', type: 'layer' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  compute: (sx, sy, layer) => {
    const inner = layer as Layer;
    const node: Layer = {
      kind: 'group',
      children: [inner],
      transform: scaleMat(sx as number, sy as number),
    };
    return [node];
  },
});
