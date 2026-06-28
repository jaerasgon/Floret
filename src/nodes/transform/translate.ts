import { registerNode } from '../registry';
import type { Layer } from '../../runtime/types';
import { translateMat, rotateMat, scaleMat } from '../../runtime/scene';
import { toVec2 } from '../_shared/coerce';

/**
 * Transform nodes wrap their inner Layer in a `group` SceneNode with
 * the appropriate affine. The renderer composes that affine onto each
 * child as it walks the group — the matrix is scoped to the subtree, so
 * siblings outside the group are unaffected.
 */

export const translateNode = registerNode({
  type: 'transform/translate',
  category: 'Transform',
  label: 'Translate',
  description: 'Translate a layer by the vector `by`.',
  inputs: [
    { name: 'by', type: 'vec2', default: { x: 0, y: 0 } },
    { name: 'layer', type: 'layer' },
  ],
  outputs: [{ name: 'translated', type: 'layer' }],
  compute: (by, layer) => {
    const v = toVec2(by);
    const inner = layer as Layer;
    const node: Layer = {
      kind: 'group',
      children: [inner],
      transform: translateMat(v.x, v.y),
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
  outputs: [{ name: 'rotated', type: 'layer' }],
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
  outputs: [{ name: 'scaled', type: 'layer' }],
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
