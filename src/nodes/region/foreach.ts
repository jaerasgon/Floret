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
 * Inputs/outputs are dynamic — derived from the splicer children at
 * compile time. The NodeDef itself declares no ports; compile expands.
 */
export const foreachRegionNode = registerNode({
  type: 'region/foreach',
  category: 'Region',
  label: 'ForEach',
  description: 'Iterate inner nodes once per element of the input spread.',
  inputs: [],
  outputs: [],
  defaultParams: { width: 480, height: 280, label: 'ForEach' },
  regionKind: 'foreach',
  component: RegionNode,
});
