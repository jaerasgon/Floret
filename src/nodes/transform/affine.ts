import { registerNode } from '../registry';
import type { Layer } from '../../runtime/types';
import { shearXMat, shearYMat, affine } from '../../runtime/scene';

/**
 * Shear and arbitrary-matrix transforms. Like Translate/Rotate/Scale,
 * these emit a `group` SceneNode with the affine attached.
 */

export const shearXNode = registerNode({
  type: 'transform/shear-x',
  category: 'Transform',
  label: 'ShearX',
  description: 'Shear a layer along the X axis by `angle` radians.',
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
      transform: shearXMat(angle as number),
    };
    return [node];
  },
});

export const shearYNode = registerNode({
  type: 'transform/shear-y',
  category: 'Transform',
  label: 'ShearY',
  description: 'Shear a layer along the Y axis by `angle` radians.',
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
      transform: shearYMat(angle as number),
    };
    return [node];
  },
});

/**
 * Multiply the current transform by an arbitrary 2D affine
 * `[[a, c, e], [b, d, f], [0, 0, 1]]`. Defaults = identity.
 */
export const applyMatrixNode = registerNode({
  type: 'transform/apply-matrix',
  category: 'Transform',
  label: 'ApplyMatrix',
  description: 'Apply an arbitrary 2D affine transform to a layer.',
  inputs: [
    { name: 'a', type: 'number', default: 1 },
    { name: 'b', type: 'number', default: 0 },
    { name: 'c', type: 'number', default: 0 },
    { name: 'd', type: 'number', default: 1 },
    { name: 'e', type: 'number', default: 0 },
    { name: 'f', type: 'number', default: 0 },
    { name: 'layer', type: 'layer' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  compute: (a, b, c, d, e, f, layer) => {
    const inner = layer as Layer;
    const node: Layer = {
      kind: 'group',
      children: [inner],
      transform: affine(
        a as number, b as number, c as number,
        d as number, e as number, f as number,
      ),
    };
    return [node];
  },
});
