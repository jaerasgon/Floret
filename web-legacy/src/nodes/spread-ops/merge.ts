import { registerNode } from '../registry';
import { GroupNode } from '../../editor/nodes/GroupNode';
import type { PortSpec } from '../types';

/**
 * Merge — combine multiple wires into one spread.
 *
 * Every input pin in Floret takes exactly ONE wire. When you need to feed
 * several sources into a single input (several forces into a World, several
 * layers into the Renderer, several strings into Concat, …), route them
 * through Merge: each numbered slot takes one wire — a single value OR a
 * spread — and the output concatenates them all, in slot order, into one
 * `spread`. `+/−` on the node adds/removes slots.
 *
 * Type-neutral (`any`) so it works for forces, colliders, layers, vec2,
 * strings — anything. For layers specifically, `transform/group` is the
 * richer option (it wraps the result in a transformable group).
 */
function buildSlots(params: Record<string, unknown>): ReadonlyArray<PortSpec> {
  const n = Math.max(1, Math.floor(Number(params.slotCount) || 2));
  const slots: PortSpec[] = [];
  for (let i = 0; i < n; i++) {
    slots.push({ name: `s${i + 1}`, label: `${i + 1}`, type: 'any', kind: 'spread' });
  }
  return slots;
}

export const mergeNode = registerNode({
  type: 'spread/merge',
  category: 'Spread',
  label: 'Merge',
  description: 'Combine multiple wires into one spread (each numbered slot takes one wire). Use it to feed several sources into a single input pin.',
  inputs: [
    { name: 's1', label: '1', type: 'any', kind: 'spread' },
    { name: 's2', label: '2', type: 'any', kind: 'spread' },
  ],
  outputs: [{ name: 'merged', type: 'any', kind: 'spread' }],
  defaultParams: { slotCount: 2 },
  dynamicInputs: buildSlots,
  component: GroupNode,
  computeRaw: (inputs) => {
    const out: unknown[] = [];
    for (const slot of inputs) {
      const arr = (slot ?? []) as ReadonlyArray<unknown>;
      for (const v of arr) out.push(v);
    }
    return [out];
  },
});
