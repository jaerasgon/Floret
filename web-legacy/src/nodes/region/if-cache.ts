import { registerNode } from '../registry';
import { RegionNode } from '../../editor/nodes/RegionNode';

/**
 * If Region — evaluates inner scope ONCE when `condition` is true. When
 * false, the inner scope is skipped and splicer-outs emit empty spreads.
 * Accumulators forward their init value unchanged when condition is
 * false (no iteration, no write — so final = init).
 *
 * Like ForEach, but with a fixed N=1 (when true) or N=0 (when false),
 * plus a static `condition` input on the region itself (not a splicer).
 */
export const ifRegionNode = registerNode({
  type: 'region/if',
  category: 'Region',
  label: 'If',
  description: 'Run inner nodes only when `condition` is true.',
  inputs: [{ name: 'condition', type: 'boolean', kind: 'single' }],
  outputs: [],
  defaultParams: { width: 480, height: 240, label: 'If' },
  regionKind: 'if',
  component: RegionNode,
});

/**
 * Cache Region — runs the inner scope only when its outer inputs differ
 * from the last frame. Otherwise returns the cached outputs verbatim.
 * Same iteration semantics as ForEach (N = min splicer-in length).
 *
 * Useful for wrapping expensive generators that only need to recompute
 * when their inputs change.
 */
export const cacheRegionNode = registerNode({
  type: 'region/cache',
  category: 'Region',
  label: 'Cache',
  description: 'Cache inner-scope outputs; re-run only when inputs change.',
  inputs: [],
  outputs: [],
  defaultParams: { width: 480, height: 240, label: 'Cache' },
  regionKind: 'cache',
  component: RegionNode,
});
