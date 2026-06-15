import { registerNode } from '../registry';
import { adaptiveLerpInputs, adaptiveLerpCompute, adaptiveOutputs } from './_adaptive';

export const mapNode = registerNode({
  type: 'math/map',
  category: 'Math',
  label: 'Map',
  description: 'Remap x from [inMin, inMax] to [outMin, outMax].',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'inMin', type: 'number', default: 0 },
    { name: 'inMax', type: 'number', default: 1 },
    { name: 'outMin', type: 'number', default: 0 },
    { name: 'outMax', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  compute: (x, im, iM, om, oM) => {
    const xv = x as number;
    const imv = im as number;
    const iMv = iM as number;
    const omv = om as number;
    const oMv = oM as number;
    if (iMv === imv) return [omv];
    return [omv + ((xv - imv) / (iMv - imv)) * (oMv - omv)];
  },
});

export const lerpNode = registerNode({
  type: 'math/lerp',
  category: 'Math',
  label: 'Lerp',
  description: 'Linear interpolate between a and b at t ∈ [0, 1]. Adapts a/b to number, vec2, or color; t is always a number.',
  inputs: [
    { name: 'a', type: 'number', default: 0 },
    { name: 'b', type: 'number', default: 1 },
    { name: 't', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  defaultParams: { valueType: 'any' },
  adaptiveType: true,
  dynamicInputs: adaptiveLerpInputs,
  dynamicOutputs: adaptiveOutputs,
  compute: adaptiveLerpCompute,
});

export const clampNode = registerNode({
  type: 'math/clamp',
  category: 'Math',
  label: 'Clamp',
  description: 'Constrain x to the range [min, max].',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'min', type: 'number', default: 0 },
    { name: 'max', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  compute: (x, mn, mx) => {
    const v = x as number;
    return [Math.max(mn as number, Math.min(mx as number, v))];
  },
});

export const normNode = registerNode({
  type: 'math/norm',
  category: 'Math',
  label: 'Norm',
  description: 'Map x from [start, stop] to [0, 1] (inverse of Lerp).',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'start', type: 'number', default: 0 },
    { name: 'stop', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  compute: (x, start, stop) => {
    const xv = x as number;
    const a = start as number;
    const b = stop as number;
    if (a === b) return [0];
    return [(xv - a) / (b - a)];
  },
});

export const magNode = registerNode({
  type: 'math/mag',
  category: 'Math',
  label: 'Mag',
  description: 'Distance from the origin to (x, y).',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  compute: (x, y) => [Math.hypot(x as number, y as number)],
});

export const distNode = registerNode({
  type: 'math/dist',
  category: 'Math',
  label: 'Dist',
  description: 'Distance between (x1, y1) and (x2, y2).',
  inputs: [
    { name: 'x1', type: 'number', default: 0 },
    { name: 'y1', type: 'number', default: 0 },
    { name: 'x2', type: 'number', default: 0 },
    { name: 'y2', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  compute: (x1, y1, x2, y2) => [Math.hypot((x2 as number) - (x1 as number), (y2 as number) - (y1 as number))],
});
