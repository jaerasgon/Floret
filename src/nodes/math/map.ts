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

export const wrapNode = registerNode({
  type: 'math/wrap',
  category: 'Math',
  label: 'Wrap',
  description: 'Wrap x into the half-open range [min, max) (modulo that keeps wrapping seamless).',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'min', type: 'number', default: 0 },
    { name: 'max', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  compute: (x, min, max) => {
    const lo = min as number;
    const hi = max as number;
    const range = hi - lo;
    if (range <= 0) return [lo];
    let r = ((x as number) - lo) % range;
    if (r < 0) r += range;
    return [lo + r];
  },
});

export const smoothstepNode = registerNode({
  type: 'math/smoothstep',
  category: 'Math',
  label: 'Smoothstep',
  description: 'Smooth Hermite 0→1 ramp as x moves across [edge0, edge1]; flat outside.',
  inputs: [
    { name: 'edge0', type: 'number', default: 0 },
    { name: 'edge1', type: 'number', default: 1 },
    { name: 'x', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  compute: (edge0, edge1, x) => {
    const e0 = edge0 as number;
    const e1 = edge1 as number;
    if (e0 === e1) return [(x as number) < e0 ? 0 : 1];
    let t = ((x as number) - e0) / (e1 - e0);
    t = t < 0 ? 0 : t > 1 ? 1 : t;
    return [t * t * (3 - 2 * t)];
  },
});

export const stepNode = registerNode({
  type: 'math/step',
  category: 'Math',
  label: 'Step',
  description: 'Returns 0 when x < edge, else 1 — a hard threshold.',
  inputs: [
    { name: 'edge', type: 'number', default: 0 },
    { name: 'x', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  compute: (edge, x) => [(x as number) < (edge as number) ? 0 : 1],
});
