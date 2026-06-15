import type { PortSpec, PortType } from '../types';
import type { Color, Vec2 } from '../../runtime/types';

/**
 * Shared machinery for the ADAPTIVE arithmetic nodes (add, sub, mul, div,
 * min, max, lerp, neg, abs). One node per op, polymorphic over
 * `number | vec2 | color`:
 *
 *   - The node carries a `valueType` param: 'any' | 'number' | 'vec2' | 'color'
 *     (default 'any'). The editor latches it to the type of the FIRST wire
 *     attached (to an operand or the result) and resets to 'any' when the
 *     last wire is removed. See editor-state.ts.
 *   - `adaptiveSlots` / `adaptiveOutputs` render every pin as that type
 *     (neutral `any` until latched), with type-appropriate identity defaults
 *     so unwired operands don't corrupt the result.
 *   - The compute reducers dispatch on the runtime value shape and operate
 *     component-wise (x,y for vec2; r,g,b,a for color; scalar for number).
 */

export type ValueKind = 'number' | 'vec2' | 'color';
export type ValueType = 'any' | ValueKind;
export const VALUE_TYPE_OPTIONS: readonly ValueType[] = ['any', 'number', 'vec2', 'color'];

/** The concrete kind a node computes in for a given `valueType` param —
 *  `any` behaves as number (so inline literals / preview still work). */
export function kindForValueType(valueType: unknown): ValueKind {
  return valueType === 'vec2' || valueType === 'color' ? valueType : 'number';
}

/** The PortType to render pins with — `any` stays neutral until latched. */
function portTypeForValueType(valueType: unknown): PortType {
  return valueType === 'vec2' || valueType === 'color' || valueType === 'number'
    ? (valueType as PortType)
    : 'any';
}

/** Detect the value kind from a runtime value (null when unknown). */
export function kindOf(v: unknown): ValueKind | null {
  if (typeof v === 'number') return 'number';
  if (v && typeof v === 'object') {
    const o = v as Record<string, unknown>;
    if (typeof o.r === 'number' && typeof o.g === 'number' && typeof o.b === 'number') return 'color';
    if (typeof o.x === 'number' && typeof o.y === 'number') return 'vec2';
  }
  return null;
}

const COMPONENT_COUNT: Record<ValueKind, number> = { number: 1, vec2: 2, color: 4 };

/** Break a value into its numeric components, coercing to the given kind. */
export function componentsOf(v: unknown, kind: ValueKind): number[] {
  switch (kind) {
    case 'number':
      return [typeof v === 'number' ? v : Number(v) || 0];
    case 'vec2': {
      const o = (v && typeof v === 'object' ? v : {}) as Partial<Vec2>;
      return [typeof o.x === 'number' ? o.x : 0, typeof o.y === 'number' ? o.y : 0];
    }
    case 'color': {
      const o = (v && typeof v === 'object' ? v : {}) as Partial<Color>;
      return [
        typeof o.r === 'number' ? o.r : 0,
        typeof o.g === 'number' ? o.g : 0,
        typeof o.b === 'number' ? o.b : 0,
        typeof o.a === 'number' ? o.a : 1,
      ];
    }
  }
}

/** Reassemble numeric components back into a value of the given kind. */
export function fromComponents(c: number[], kind: ValueKind): number | Vec2 | Color {
  switch (kind) {
    case 'number': return c[0] ?? 0;
    case 'vec2': return { x: c[0] ?? 0, y: c[1] ?? 0 };
    case 'color': return { r: c[0] ?? 0, g: c[1] ?? 0, b: c[2] ?? 0, a: c[3] ?? 1 };
  }
}

/** An identity value of `kind` with every component set to `scalar` —
 *  the per-op unwired-operand default (0 for add, 1 for mul, ±Infinity for
 *  min/max). */
export function identityValue(kind: ValueKind, scalar: number): number | Vec2 | Color {
  return fromComponents(new Array(COMPONENT_COUNT[kind]).fill(scalar), kind);
}

/**
 * Build a `dynamicInputs(params)` for a variable-arity adaptive op: `n` slots
 * (a, b, c, …) typed by `valueType`, each defaulting to the op's identity in
 * the resolved kind. `identityScalar` is the per-op identity (0 / 1 / ±Inf).
 */
export function adaptiveSlots(identityScalar: number, maxSlots = 26) {
  return (params: Record<string, unknown>): ReadonlyArray<PortSpec> => {
    const valueType = params.valueType;
    const kind = kindForValueType(valueType);
    const type = portTypeForValueType(valueType);
    const def = identityValue(kind, identityScalar);
    const n = Math.max(1, Math.min(maxSlots, Math.floor(Number(params.slotCount) || 2)));
    const slots: PortSpec[] = [];
    for (let i = 0; i < n; i++) {
      const name = String.fromCharCode(97 + i);
      slots.push({ name, label: name, type, default: def });
    }
    return slots;
  };
}

/** The `result` output pin, typed by `valueType` (neutral `any` until latched). */
export function adaptiveOutputs(params: Record<string, unknown>): ReadonlyArray<PortSpec> {
  return [{ name: 'result', type: portTypeForValueType(params.valueType) }];
}

/**
 * Wrap a per-component scalar reducer into a node `compute`. The reducer takes
 * the list of that component across all operands and returns one number. The
 * value kind is detected from the operands (defaulting to number), then the
 * reducer runs once per component and the result is reassembled.
 */
export function adaptiveReduce(perComponent: (xs: number[]) => number) {
  return (...vals: unknown[]): unknown[] => {
    if (vals.length === 0) return [0];
    let kind: ValueKind = 'number';
    for (const v of vals) { const k = kindOf(v); if (k) { kind = k; break; } }
    const cols = vals.map((v) => componentsOf(v, kind));
    const n = COMPONENT_COUNT[kind];
    const out = new Array<number>(n);
    for (let ci = 0; ci < n; ci++) out[ci] = perComponent(cols.map((c) => c[ci] ?? 0));
    return [fromComponents(out, kind)];
  };
}

// ── Per-op component reducers (match the scalar binop semantics) ──
export const reduceAdd = (xs: number[]): number => xs.reduce((a, b) => a + b, 0);
export const reduceMul = (xs: number[]): number => xs.reduce((a, b) => a * b, 1);
export const reduceSub = (xs: number[]): number =>
  xs.length === 0 ? 0 : xs.slice(1).reduce((a, b) => a - b, xs[0]!);
export const reduceDiv = (xs: number[]): number => {
  if (xs.length === 0) return 0;
  let r = xs[0]!;
  for (let i = 1; i < xs.length; i++) { const d = xs[i]!; if (d === 0) return 0; r /= d; }
  return r;
};
export const reduceMin = (xs: number[]): number => Math.min(...xs);
export const reduceMax = (xs: number[]): number => Math.max(...xs);

/** Map a single value component-wise (for neg/abs). */
export function adaptiveMap(perComponent: (x: number) => number) {
  return (v: unknown): unknown[] => {
    const kind = kindOf(v) ?? 'number';
    return [fromComponents(componentsOf(v, kind).map(perComponent), kind)];
  };
}

/** `dynamicInputs` for a single-operand adaptive op (neg / abs). */
export function adaptiveUnaryInputs(name: string, identityScalar = 0) {
  return (params: Record<string, unknown>): ReadonlyArray<PortSpec> => {
    const kind = kindForValueType(params.valueType);
    return [{
      name, label: name,
      type: portTypeForValueType(params.valueType),
      default: identityValue(kind, identityScalar),
    }];
  };
}

/** `dynamicInputs` for Lerp: `a`/`b` follow `valueType`; `t` is always number. */
export function adaptiveLerpInputs(params: Record<string, unknown>): ReadonlyArray<PortSpec> {
  const kind = kindForValueType(params.valueType);
  const type = portTypeForValueType(params.valueType);
  return [
    { name: 'a', type, default: identityValue(kind, 0) },
    { name: 'b', type, default: identityValue(kind, 1) },
    { name: 't', type: 'number', default: 0 },
  ];
}

/** Component-wise lerp; `t` is a scalar regardless of value kind. */
export function adaptiveLerpCompute(a: unknown, b: unknown, t: unknown): unknown[] {
  const kind = kindOf(a) ?? kindOf(b) ?? 'number';
  const ca = componentsOf(a, kind);
  const cb = componentsOf(b, kind);
  const tv = typeof t === 'number' ? t : Number(t) || 0;
  return [fromComponents(ca.map((av, i) => av + ((cb[i] ?? 0) - av) * tv), kind)];
}
