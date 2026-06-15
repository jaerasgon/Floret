import { registerNode } from '../registry';
import { RegionNode } from '../../editor/nodes/RegionNode';

/**
 * ForEach Region — a visual rectangle that scopes a sub-patch.
 *
 * Inner nodes execute once per iteration. Splicer-in children deliver
 * outer spread slots to the inside; splicer-out children collect inner
 * values back into outer spreads. Iteration count N = min over input
 * splicer-in spread lengths; if there are no splicer-ins, the region
 * runs zero times.
 *
 * Splicer inputs/outputs are dynamic — derived from the splicer children
 * at compile time. The NodeDef declares one STATIC input, `delay`.
 *
 * Built-in loop variables: every ForEach exposes `index`, `count`, and
 * `fraction` (0→1) inner-source pins — wire them for per-item stagger or
 * gradients without deriving anything from the data.
 *
 * `delay` (seconds): rewinds each iteration's TIME SOURCE by
 * `index * delay`, so `sin(time)` inside the loop ripples across the
 * spread. Scoped side effect: only nodes that read time from the frame
 * (e.g. `time/seconds`) shift. Stateful inner nodes (accumulators, audio,
 * particles) keep their single real-time state and won't stagger — and a
 * node that both holds state and reads offset time may read inconsistently.
 */
export const foreachRegionNode = registerNode({
  type: 'region/foreach',
  category: 'Region',
  label: 'ForEach',
  description: 'Iterate inner nodes once per element of the input spread. Built-in index/count/fraction pins; delay staggers each iteration\'s time source.',
  inputs: [{ name: 'delay', type: 'number', kind: 'single', default: 0 }],
  outputs: [],
  defaultParams: { width: 480, height: 280, label: 'ForEach' },
  regionKind: 'foreach',
  component: RegionNode,
});
