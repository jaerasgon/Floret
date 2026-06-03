import { registerNode } from '../registry';
import { GroupNode } from '../../editor/nodes/GroupNode';
import type { Layer } from '../../runtime/types';
import type { PortSpec } from '../types';
import type { SceneNode } from '../../runtime/scene';

/**
 * Group — collapse N Layer wires into a single Layer.
 *
 * Each slot is `kind: 'spread'`, accepting one wire of either a single
 * Layer or a `Spread<Layer>` (from a ForEach). computeRaw concatenates
 * every slot's array, wraps the result in a `group` SceneNode. Draw
 * order = top-to-bottom by slot index.
 */
function buildLayerSlots(params: Record<string, unknown>): ReadonlyArray<PortSpec> {
  const n = Math.max(1, Math.floor(Number(params.slotCount) || 2));
  const slots: PortSpec[] = [];
  for (let i = 0; i < n; i++) {
    slots.push({
      name: `s${i + 1}`,
      label: `${i + 1}`,
      type: 'layer',
      kind: 'spread',
    });
  }
  return slots;
}

export const groupNode = registerNode({
  type: 'transform/group',
  category: 'Transform',
  label: 'Group',
  description: 'Combine multiple layer wires into one layer.',
  inputs: [
    { name: 's1', label: '1', type: 'layer', kind: 'spread' },
    { name: 's2', label: '2', type: 'layer', kind: 'spread' },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  defaultParams: { slotCount: 2 },
  dynamicInputs: buildLayerSlots,
  component: GroupNode,
  computeRaw: (inputs) => {
    const children: SceneNode[] = [];
    for (const slot of inputs) {
      const arr = (slot ?? []) as ReadonlyArray<SceneNode>;
      for (const node of arr) children.push(node);
    }
    const grouped: Layer = { kind: 'group', children };
    return [[grouped]];
  },
});
