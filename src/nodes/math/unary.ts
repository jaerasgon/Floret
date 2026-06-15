import { registerNode } from '../registry';

const x = [{ name: 'x', type: 'number' as const, default: 0 }];

export const sinNode = registerNode({
  type: 'math/sin',
  category: 'Math',
  label: 'Sin',
  description: 'Sine of x (radians).',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => [Math.sin(v as number)],
});

export const cosNode = registerNode({
  type: 'math/cos',
  category: 'Math',
  label: 'Cos',
  description: 'Cosine of x (radians).',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => [Math.cos(v as number)],
});

export const tanNode = registerNode({
  type: 'math/tan',
  category: 'Math',
  label: 'Tan',
  description: 'Tangent of x (radians).',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => [Math.tan(v as number)],
});

export const absNode = registerNode({
  type: 'math/abs',
  category: 'Math',
  label: 'Abs',
  description: 'Absolute value of x.',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => [Math.abs(v as number)],
});

export const negNode = registerNode({
  type: 'math/neg',
  category: 'Math',
  label: 'Neg',
  description: 'Negate x.',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => [-(v as number)],
});

export const floorNode = registerNode({
  type: 'math/floor',
  category: 'Math',
  label: 'Floor',
  description: 'Round x down to the nearest integer.',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => [Math.floor(v as number)],
});

export const roundNode = registerNode({
  type: 'math/round',
  category: 'Math',
  label: 'Round',
  description: 'Round x to the nearest integer.',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => [Math.round(v as number)],
});

export const sqrtNode = registerNode({
  type: 'math/sqrt',
  category: 'Math',
  label: 'Sqrt',
  description: 'Square root of x. Negatives yield 0.',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => {
    const n = v as number;
    return [n < 0 ? 0 : Math.sqrt(n)];
  },
});

export const sqNode = registerNode({
  type: 'math/sq',
  category: 'Math',
  label: 'Sq',
  description: 'Square of x (x²).',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => {
    const n = v as number;
    return [n * n];
  },
});

export const ceilNode = registerNode({
  type: 'math/ceil',
  category: 'Math',
  label: 'Ceil',
  description: 'Round x up to the nearest integer.',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => [Math.ceil(v as number)],
});

export const fractNode = registerNode({
  type: 'math/fract',
  category: 'Math',
  label: 'Fract',
  description: 'Fractional part of x — always non-negative.',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => {
    const n = v as number;
    return [n - Math.floor(n)];
  },
});

export const expNode = registerNode({
  type: 'math/exp',
  category: 'Math',
  label: 'Exp',
  description: 'Exponential — eˣ.',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => [Math.exp(v as number)],
});

export const logNode = registerNode({
  type: 'math/log',
  category: 'Math',
  label: 'Log',
  description: 'Natural logarithm. log(x ≤ 0) = 0.',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => {
    const n = v as number;
    return [n <= 0 ? 0 : Math.log(n)];
  },
});

export const asinNode = registerNode({
  type: 'math/asin',
  category: 'Math',
  label: 'Asin',
  description: 'Arcsine in radians. Input clamped to [-1, 1].',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => {
    const n = v as number;
    return [Math.asin(n < -1 ? -1 : n > 1 ? 1 : n)];
  },
});

export const acosNode = registerNode({
  type: 'math/acos',
  category: 'Math',
  label: 'Acos',
  description: 'Arccosine in radians. Input clamped to [-1, 1].',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => {
    const n = v as number;
    return [Math.acos(n < -1 ? -1 : n > 1 ? 1 : n)];
  },
});

export const atanNode = registerNode({
  type: 'math/atan',
  category: 'Math',
  label: 'Atan',
  description: 'Arctangent in radians.',
  inputs: x,
  outputs: [{ name: 'result', type: 'number' }],
  compute: (v) => [Math.atan(v as number)],
});
