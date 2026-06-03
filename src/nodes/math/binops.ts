import { registerNode } from '../registry';
import { GroupNode } from '../../editor/nodes/GroupNode';
import type { PortSpec } from '../types';

/**
 * Binary math ops, generalised to N inputs.
 *
 * For the associative ones — add, sub, mul, div, min, max — the node
 * grows extra input slots via the same +/− chrome that `transform/group`
 * uses. Default slotCount is 2, so existing patches behave identically
 * to the old strictly-binary form.
 *
 * Slot labels are alphabetic (a, b, c, …) so a 4-input Add reads as
 * `a + b + c + d`. Slots beyond `z` (slotCount > 26) are unlikely but
 * fall back to numeric labels.
 *
 * Non-associative ops (pow, mod, atan2) stay strictly binary — there's
 * no single sensible reading of `pow(a, b, c)`.
 */

// Slot ports are named (and labelled) a, b, c, …, capping at 26 to keep
// the naming letter-based. Existing patches that reference `a`/`b` on
// the old strictly-binary form continue to work — slot 0 = `a`, slot 1 = `b`.
const MAX_SLOTS = 26;

function slotName(i: number): string {
  return String.fromCharCode(97 + i);
}

function buildSlots(defaultValue: number) {
  return (params: Record<string, unknown>): ReadonlyArray<PortSpec> => {
    const n = Math.max(1, Math.min(MAX_SLOTS, Math.floor(Number(params.slotCount) || 2)));
    const slots: PortSpec[] = [];
    for (let i = 0; i < n; i++) {
      const name = slotName(i);
      slots.push({
        name,
        label: name,
        type: 'number',
        default: defaultValue,
      });
    }
    return slots;
  };
}

const out = [{ name: 'out', type: 'number' as const }];

export const addNode = registerNode({
  type: 'math/add',
  category: 'Math',
  label: 'Add',
  description: 'Sum all wired inputs.',
  inputs: [
    { name: 'a', type: 'number', default: 0 },
    { name: 'b', type: 'number', default: 0 },
  ],
  outputs: out,
  defaultParams: { slotCount: 2 },
  dynamicInputs: buildSlots(0),
  component: GroupNode,
  compute: (...vals) => {
    let s = 0;
    for (const v of vals) s += (v as number);
    return [s];
  },
});

export const subNode = registerNode({
  type: 'math/sub',
  category: 'Math',
  label: 'Sub',
  description: 'Subtract subsequent inputs from the first.',
  inputs: [
    { name: 'a', type: 'number', default: 0 },
    { name: 'b', type: 'number', default: 0 },
  ],
  outputs: out,
  defaultParams: { slotCount: 2 },
  dynamicInputs: buildSlots(0),
  component: GroupNode,
  compute: (...vals) => {
    if (vals.length === 0) return [0];
    let r = vals[0] as number;
    for (let i = 1; i < vals.length; i++) r -= vals[i] as number;
    return [r];
  },
});

export const mulNode = registerNode({
  type: 'math/mul',
  category: 'Math',
  label: 'Mul',
  description: 'Multiply all wired inputs.',
  inputs: [
    { name: 'a', type: 'number', default: 1 },
    { name: 'b', type: 'number', default: 1 },
  ],
  outputs: out,
  defaultParams: { slotCount: 2 },
  dynamicInputs: buildSlots(1),
  component: GroupNode,
  compute: (...vals) => {
    let p = 1;
    for (const v of vals) p *= (v as number);
    return [p];
  },
});

export const divNode = registerNode({
  type: 'math/div',
  category: 'Math',
  label: 'Div',
  description: 'Divide the first input by subsequent ones. Division by zero yields 0.',
  inputs: [
    { name: 'a', type: 'number', default: 1 },
    { name: 'b', type: 'number', default: 1 },
  ],
  outputs: out,
  defaultParams: { slotCount: 2 },
  dynamicInputs: buildSlots(1),
  component: GroupNode,
  compute: (...vals) => {
    if (vals.length === 0) return [0];
    let r = vals[0] as number;
    for (let i = 1; i < vals.length; i++) {
      const d = vals[i] as number;
      if (d === 0) return [0];
      r /= d;
    }
    return [r];
  },
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
  outputs: out,
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
  outputs: out,
  compute: (a, b) => [Math.pow(a as number, b as number)],
});

export const minNode = registerNode({
  type: 'math/min',
  category: 'Math',
  label: 'Min',
  description: 'Smallest of all wired inputs.',
  inputs: [
    { name: 'a', type: 'number', default: Infinity },
    { name: 'b', type: 'number', default: Infinity },
  ],
  outputs: out,
  defaultParams: { slotCount: 2 },
  dynamicInputs: buildSlots(Infinity),
  component: GroupNode,
  compute: (...vals) => {
    if (vals.length === 0) return [0];
    let m = vals[0] as number;
    for (let i = 1; i < vals.length; i++) {
      const v = vals[i] as number;
      if (v < m) m = v;
    }
    return [m];
  },
});

export const maxNode = registerNode({
  type: 'math/max',
  category: 'Math',
  label: 'Max',
  description: 'Largest of all wired inputs.',
  inputs: [
    { name: 'a', type: 'number', default: -Infinity },
    { name: 'b', type: 'number', default: -Infinity },
  ],
  outputs: out,
  defaultParams: { slotCount: 2 },
  dynamicInputs: buildSlots(-Infinity),
  component: GroupNode,
  compute: (...vals) => {
    if (vals.length === 0) return [0];
    let m = vals[0] as number;
    for (let i = 1; i < vals.length; i++) {
      const v = vals[i] as number;
      if (v > m) m = v;
    }
    return [m];
  },
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
  outputs: out,
  compute: (y, x) => [Math.atan2(y as number, x as number)],
});
