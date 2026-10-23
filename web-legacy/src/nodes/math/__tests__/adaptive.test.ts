import { describe, expect, it } from 'vitest';
import {
  adaptiveReduce, adaptiveMap, adaptiveSlots, adaptiveOutputs,
  reduceAdd, reduceSub, reduceMul, reduceDiv, reduceMin, reduceMax,
  identityValue, kindForValueType, kindOf, componentsOf, fromComponents,
} from '../_adaptive';

describe('adaptive value helpers', () => {
  it('kindOf detects number / vec2 / color', () => {
    expect(kindOf(3)).toBe('number');
    expect(kindOf({ x: 1, y: 2 })).toBe('vec2');
    expect(kindOf({ r: 1, g: 0, b: 0, a: 1 })).toBe('color');
    expect(kindOf('nope')).toBeNull();
  });

  it('kindForValueType maps any → number', () => {
    expect(kindForValueType('any')).toBe('number');
    expect(kindForValueType(undefined)).toBe('number');
    expect(kindForValueType('vec2')).toBe('vec2');
    expect(kindForValueType('color')).toBe('color');
  });

  it('identityValue fills components', () => {
    expect(identityValue('number', 1)).toBe(1);
    expect(identityValue('vec2', 0)).toEqual({ x: 0, y: 0 });
    expect(identityValue('color', 1)).toEqual({ r: 1, g: 1, b: 1, a: 1 });
  });

  it('components round-trip', () => {
    expect(fromComponents(componentsOf({ x: 3, y: 4 }, 'vec2'), 'vec2')).toEqual({ x: 3, y: 4 });
  });
});

describe('adaptiveReduce dispatches on operand shape', () => {
  it('add — number / vec2 / color', () => {
    expect(adaptiveReduce(reduceAdd)(2, 3)).toEqual([5]);
    expect(adaptiveReduce(reduceAdd)({ x: 1, y: 2 }, { x: 3, y: 4 })).toEqual([{ x: 4, y: 6 }]);
    expect(adaptiveReduce(reduceAdd)({ r: 0.2, g: 0.1, b: 0, a: 1 }, { r: 0.3, g: 0, b: 0.5, a: 0 }))
      .toEqual([{ r: 0.5, g: 0.1, b: 0.5, a: 1 }]);
  });

  it('sub / mul / div component-wise on vec2', () => {
    expect(adaptiveReduce(reduceSub)({ x: 5, y: 9 }, { x: 1, y: 2 })).toEqual([{ x: 4, y: 7 }]);
    expect(adaptiveReduce(reduceMul)({ x: 2, y: 3 }, { x: 4, y: 5 })).toEqual([{ x: 8, y: 15 }]);
    expect(adaptiveReduce(reduceDiv)({ x: 8, y: 9 }, { x: 2, y: 3 })).toEqual([{ x: 4, y: 3 }]);
  });

  it('div by zero → 0 for that component', () => {
    expect(adaptiveReduce(reduceDiv)({ x: 8, y: 9 }, { x: 0, y: 3 })).toEqual([{ x: 0, y: 3 }]);
    expect(adaptiveReduce(reduceDiv)(6, 0)).toEqual([0]);
  });

  it('min / max component-wise', () => {
    expect(adaptiveReduce(reduceMin)({ x: 1, y: 9 }, { x: 4, y: 2 })).toEqual([{ x: 1, y: 2 }]);
    expect(adaptiveReduce(reduceMax)({ x: 1, y: 9 }, { x: 4, y: 2 })).toEqual([{ x: 4, y: 9 }]);
  });

  it('N-ary add (variable slots)', () => {
    expect(adaptiveReduce(reduceAdd)(1, 2, 3, 4)).toEqual([10]);
  });

  it('an unwired number identity does not corrupt a wired vec2 operand', () => {
    // mul: vec2 × identity(1,1) stays the vec2 (defaults are typed in real use,
    // but even a stray scalar coerces into the detected kind).
    expect(adaptiveReduce(reduceMul)({ x: 2, y: 3 }, { x: 1, y: 1 })).toEqual([{ x: 2, y: 3 }]);
  });
});

describe('adaptiveMap (neg / abs)', () => {
  it('neg vec2 / color', () => {
    expect(adaptiveMap((x) => -x)({ x: 1, y: -2 })).toEqual([{ x: -1, y: 2 }]);
  });
  it('abs number / vec2', () => {
    expect(adaptiveMap(Math.abs)(-5)).toEqual([5]);
    expect(adaptiveMap(Math.abs)({ x: -1, y: -2 })).toEqual([{ x: 1, y: 2 }]);
  });
});

describe('adaptiveSlots / adaptiveOutputs reflect valueType', () => {
  it('any → neutral pins, number identity default', () => {
    const slots = adaptiveSlots(0)({ valueType: 'any', slotCount: 2 });
    expect(slots).toHaveLength(2);
    expect(slots[0]).toMatchObject({ name: 'a', type: 'any', default: 0 });
    expect(adaptiveOutputs({ valueType: 'any' })).toEqual([{ name: 'result', type: 'any' }]);
  });

  it('vec2 → vec2 pins, vec2 identity default', () => {
    const slots = adaptiveSlots(1)({ valueType: 'vec2', slotCount: 3 });
    expect(slots).toHaveLength(3);
    expect(slots[0]).toMatchObject({ name: 'a', type: 'vec2', default: { x: 1, y: 1 } });
    expect(adaptiveOutputs({ valueType: 'vec2' })).toEqual([{ name: 'result', type: 'vec2' }]);
  });
});
