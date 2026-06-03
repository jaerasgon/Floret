import { registerNode } from '../registry';
import { AccumulatorNode } from '../../editor/nodes/AccumulatorNode';
import type { PortType } from '../../runtime/types';

/**
 * Accumulator pin — threads a stateful value across the iterations of a
 * ForEach region.
 *
 *   Outer side:
 *     `init`  (input)  — value seeded at iteration 0
 *     `final` (output) — value after the last iteration
 *
 *   Inner side:
 *     `read`  (output) — the value for the current iteration
 *     `write` (input)  — the value to carry into the next iteration
 *
 * runRegion seeds `init` → first `read`, captures `write` after each
 * iteration to become the next iteration's `read`, and exposes the last
 * `write` as `final` on the outer side.
 */
function makeAccumulator(portType: PortType) {
  return registerNode({
    type: `region/accumulator/${portType}`,
    category: 'Region',
    label: `Accumulator:${portType}`,
    description: `Thread a ${portType} value across region iterations.`,
    inputs: [
      { name: 'init', type: portType, kind: 'single' },
      { name: 'write', type: portType, kind: 'single' },
    ],
    outputs: [
      { name: 'read', type: portType, kind: 'single' },
      { name: 'final', type: portType, kind: 'single' },
    ],
    regionRole: 'accumulator',
    component: AccumulatorNode,
  });
}

export const accumulatorNumber = makeAccumulator('number');
export const accumulatorBool = makeAccumulator('boolean');
export const accumulatorImage = makeAccumulator('image');
