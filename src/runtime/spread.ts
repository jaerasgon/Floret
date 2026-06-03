import type { PortValue } from './types';

/**
 * After the regions refactor, `Spread<T>` is no longer the universal wire
 * type — most wires carry single values now. This file kept the
 * `liftCyclic` machinery before; that's gone. What remains:
 *
 *   - `ScalarFn` / `RawComputeFn` type aliases (still the shape every
 *     NodeDef's `compute` and `computeRaw` follow).
 *   - `applyKinded` — the replacement for `liftCyclic`. Given a node's
 *     input/output kinds, it unwraps single inputs to scalars, passes
 *     spread inputs through whole, calls the author's scalar `compute`
 *     once (no iteration), and re-wraps single outputs.
 *   - `asNumber` — kept as a coercion helper for `number ↔ boolean`.
 */

/** A node author's compute fn — single-call, mixed scalar/spread args. */
export type ScalarFn = (...inputs: unknown[]) => unknown[];

/** A "raw" compute fn that already speaks the runtime's PortValue (spread) shape. */
export type RawComputeFn = (inputs: PortValue[]) => PortValue[];

/** Wire-shape on the compute boundary.
 *  - 'single': unwrapped slot-0 scalar
 *  - 'spread': whole list
 *  - 'multi':  whole list (identical shape; the runtime gathers many
 *               wires into one spread BEFORE compute sees it)
 *  multi only appears on INPUT ports — outputs are single or spread.
 */
export type PortKind = 'single' | 'spread' | 'multi';

/**
 * Wrap a scalar compute fn into a RawComputeFn that honours per-port
 * `kind`. Each input arrives as a `PortValue` (always a spread on the
 * wire); the wrapper unwraps `single`-kind ports to their slot-0 scalar
 * and passes `spread`-kind ports through whole. Outputs go the other way.
 *
 * This is the new "compute glue" — no iteration, no broadcasting. Anything
 * that needs to iterate must live inside a ForEach region.
 */
export function applyKinded(
  fn: ScalarFn,
  arity: { in: number; out: number },
  inputKinds: ReadonlyArray<PortKind>,
  outputKinds: ReadonlyArray<PortKind>,
): RawComputeFn {
  return (inputs: PortValue[]) => {
    if (inputs.length !== arity.in) {
      throw new Error(
        `applyKinded arity mismatch: expected ${arity.in} inputs, got ${inputs.length}`,
      );
    }
    const args: unknown[] = new Array(inputs.length);
    for (let i = 0; i < inputs.length; i++) {
      const k = inputKinds[i] ?? 'single';
      const v = inputs[i]!;
      // multi behaves exactly like spread on the compute boundary —
      // it's just gathered differently upstream (N wires → one spread).
      args[i] = (k === 'spread' || k === 'multi') ? v : v[0];
    }
    const results = fn(...args);
    if (results.length !== arity.out) {
      throw new Error(
        `applyKinded output mismatch: expected ${arity.out}, got ${results.length}`,
      );
    }
    const outs: PortValue[] = new Array(results.length);
    for (let i = 0; i < results.length; i++) {
      const k = outputKinds[i] ?? 'single';
      outs[i] = k === 'spread'
        ? ((results[i] ?? []) as PortValue)
        : [results[i]];
    }
    return outs;
  };
}

/**
 * Coerce an unknown value to a number (for the strict-types + narrow-coercion rule).
 * Used at port boundaries when number↔boolean is allowed.
 */
export function asNumber(v: unknown): number {
  if (typeof v === 'number') return v;
  if (typeof v === 'boolean') return v ? 1 : 0;
  return Number(v) || 0;
}
