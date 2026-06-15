import { registerNode } from '../registry';
import { GroupNode } from '../../editor/nodes/GroupNode';
import {
  adaptiveSlots, adaptiveOutputs, adaptiveReduce,
  reduceAdd, reduceSub, reduceMul, reduceDiv, reduceMin, reduceMax,
} from './_adaptive';

/**
 * Binary math ops, generalised to N inputs.
 *
 * For the associative ones — add, sub, mul, div, min, max — the node
 * grows extra input slots via the same +/− chrome that `transform/group`
 * uses. Default slotCount is 2, so existing patches behave identically
 * to the old strictly-binary form.
 *
 * These six are also ADAPTIVE (polymorphic): a `valueType` param latches to
 * the type of the first wire — number, vec2, or color — and every pin +
 * the result follow it (component-wise arithmetic). 'any' until wired; the
 * editor maintains the latch. See `_adaptive.ts`. Slot labels are alphabetic
 * (a, b, c, …) so a 4-input Add reads as `a + b + c + d`.
 *
 * Non-associative ops (pow, mod, atan2) stay strictly binary and number-only —
 * there's no single sensible reading of `pow(a, b, c)` or `pow(vec2, vec2)`.
 */

export const addNode = registerNode({
  type: 'math/add',
  category: 'Math',
  label: 'Add',
  description: 'Sum all wired inputs (number, vec2, or color — adapts to what you plug in).',
  inputs: [
    { name: 'a', type: 'number', default: 0 },
    { name: 'b', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  defaultParams: { slotCount: 2, valueType: 'any' },
  adaptiveType: true,
  dynamicInputs: adaptiveSlots(0),
  dynamicOutputs: adaptiveOutputs,
  component: GroupNode,
  compute: adaptiveReduce(reduceAdd),
});

export const subNode = registerNode({
  type: 'math/sub',
  category: 'Math',
  label: 'Sub',
  description: 'Subtract subsequent inputs from the first (number, vec2, or color).',
  inputs: [
    { name: 'a', type: 'number', default: 0 },
    { name: 'b', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  defaultParams: { slotCount: 2, valueType: 'any' },
  adaptiveType: true,
  dynamicInputs: adaptiveSlots(0),
  dynamicOutputs: adaptiveOutputs,
  component: GroupNode,
  compute: adaptiveReduce(reduceSub),
});

export const mulNode = registerNode({
  type: 'math/mul',
  category: 'Math',
  label: 'Mul',
  description: 'Multiply all wired inputs (number, vec2, or color — component-wise).',
  inputs: [
    { name: 'a', type: 'number', default: 1 },
    { name: 'b', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  defaultParams: { slotCount: 2, valueType: 'any' },
  adaptiveType: true,
  dynamicInputs: adaptiveSlots(1),
  dynamicOutputs: adaptiveOutputs,
  component: GroupNode,
  compute: adaptiveReduce(reduceMul),
});

export const divNode = registerNode({
  type: 'math/div',
  category: 'Math',
  label: 'Div',
  description: 'Divide the first input by subsequent ones (component-wise). Division by zero yields 0.',
  inputs: [
    { name: 'a', type: 'number', default: 1 },
    { name: 'b', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  defaultParams: { slotCount: 2, valueType: 'any' },
  adaptiveType: true,
  dynamicInputs: adaptiveSlots(1),
  dynamicOutputs: adaptiveOutputs,
  component: GroupNode,
  compute: adaptiveReduce(reduceDiv),
});

export const modNode = registerNode({
  type: 'math/mod',
  category: 'Math',
  label: 'Mod',
  description: 'Euclidean modulo — a mod b, always non-negative.',
  inputs: [
    { name: 'a', type: 'number', default: 0 },
    { name: 'b', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  compute: (a, b) => {
    const bv = b as number;
    if (bv === 0) return [0];
    const av = a as number;
    return [((av % bv) + bv) % bv]; // Euclidean mod, like vvvv's
  },
});

export const powNode = registerNode({
  type: 'math/pow',
  category: 'Math',
  label: 'Pow',
  description: 'Raise a to the power b.',
  inputs: [
    { name: 'a', type: 'number', default: 1 },
    { name: 'b', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  compute: (a, b) => [Math.pow(a as number, b as number)],
});

export const minNode = registerNode({
  type: 'math/min',
  category: 'Math',
  label: 'Min',
  description: 'Smallest of all wired inputs (component-wise for vec2 / color).',
  inputs: [
    { name: 'a', type: 'number', default: Infinity },
    { name: 'b', type: 'number', default: Infinity },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  defaultParams: { slotCount: 2, valueType: 'any' },
  adaptiveType: true,
  dynamicInputs: adaptiveSlots(Infinity),
  dynamicOutputs: adaptiveOutputs,
  component: GroupNode,
  compute: adaptiveReduce(reduceMin),
});

export const maxNode = registerNode({
  type: 'math/max',
  category: 'Math',
  label: 'Max',
  description: 'Largest of all wired inputs (component-wise for vec2 / color).',
  inputs: [
    { name: 'a', type: 'number', default: -Infinity },
    { name: 'b', type: 'number', default: -Infinity },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  defaultParams: { slotCount: 2, valueType: 'any' },
  adaptiveType: true,
  dynamicInputs: adaptiveSlots(-Infinity),
  dynamicOutputs: adaptiveOutputs,
  component: GroupNode,
  compute: adaptiveReduce(reduceMax),
});

export const atan2Node = registerNode({
  type: 'math/atan2',
  category: 'Math',
  label: 'Atan2',
  description: 'Angle of (x, y) in radians, full [-π, π] range.',
  inputs: [
    { name: 'y', type: 'number', default: 0 },
    { name: 'x', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  compute: (y, x) => [Math.atan2(y as number, x as number)],
});
