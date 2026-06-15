# Adaptive (polymorphic) arithmetic nodes

**Date:** 2026-06-15
**Status:** Approved

## Problem

Arithmetic exists twice — once for `number` (`math/add`, …) and once for `vec2`
(`vec2/add`, …). The user wants a single adaptive node per operation: untyped
until something is wired, then both operands and the result take the wired type.

## Scope

Adaptive ops, each over **number | vec2 | color**:
`add, sub, mul, div, min, max` (binops), `lerp` (map), `neg, abs` (unary).

Out of scope (unchanged): number-only (`mod, pow, atan2, sin…, floor…`),
vec2-only (`dot, mag, normalize, heading, set-mag, limit, rotate, from-angle,
make, split, dist, angle-between`), and `vec2/scale` (vec2×number — mixed-type).
Color arithmetic is **new** capability (no prior color add/sub/mul nodes).

## Type latch

Each adaptive node gains a param `valueType: 'any' | 'number' | 'vec2' |
'color'` (default `'any'`).

- `dynamicInputs(params)` / `dynamicOutputs(params)` render **every** pin
  (operands + result) as `valueType` — or `any` when unset.
- **First wins:** the editor maintains the latch on connect/disconnect. When a
  node is `'any'` and the first wire attaches (to an operand OR the result), the
  peer port's concrete type is inferred and stored. Later wires can't change it
  — they're constrained to the latched type by the connection rules.
- **Reset:** when the last wire is removed and none remain, `valueType` reverts
  to `'any'` so the node can be re-typed.

## Compute

A single `computeRaw` branches on `valueType`:
- `number` → scalar op (current behaviour).
- `vec2` → component-wise on `{x,y}`.
- `color` → component-wise on `{r,g,b,a}`.

`lerp`'s `t` operand is **always number**, regardless of `valueType` (only
`a`/`b` switch). `min/max/abs/neg` on vec2/color are component-wise.

Binops keep their existing variable-arity (`slotCount`) behaviour; arity and
type are independent params.

## Untyped (`'any'`) behaviour

Before any wire: pins render neutral ("any"), and the node computes as
**number** so inline literals and preview keep working. The first typed wire
latches it; pins then take the type colour.

## Connection rules

- `valueType === 'any'`: any supported type connects; connecting sets the latch.
- Latched: pins act as that concrete type (existing `arePortsCompatible`;
  number↔boolean coercion still allowed for `number`).

## Consolidation + migration

- Palette shows **one** of each adaptive op (the `math/*` node, now adaptive).
- `vec2/add, vec2/sub, vec2/mul, vec2/lerp` become `hidden: true` and are
  migrated on load by `migrate.ts`: rewrite the node `defType` → the `math/*`
  equivalent with `params.valueType='vec2'`, and remap connected edge handles
  (vec2 output handle `vector` → the adaptive output handle; operand handles
  `a`/`b`/`t` already match). vec2-only ops and `vec2/scale` are untouched.
- Built-in samples using vec2 arithmetic flow through the same migration path
  (or have their builds updated).

## Implementation approach

Store the resolved type in the `valueType` param (chosen over continuous
graph-inference, which fights "first wins" and needs conflict resolution).
Reuses the existing param-driven `dynamicInputs` / `dynamicOutputs(params,
upstreamTypes)` resolution; no per-render graph walk.

Editor latch lives in `editor-state.ts` (connect/disconnect handlers) +
`connection-rules.ts` (type acceptance while `any`, enforcement once latched).

## Testing

- Per-type compute correctness for each op (number/vec2/color), incl. lerp's
  number `t`.
- Latch: operand-sets-type, result-sets-type, first-wins, disconnect-all-resets.
- Connection rules reject a mismatched type once latched.
- Migration roundtrip: a `vec2/add` patch → adaptive node `valueType=vec2`,
  edges intact.
