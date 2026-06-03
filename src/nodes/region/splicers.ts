import { registerNode } from '../registry';
import { SplicerInNode, SplicerOutNode } from '../../editor/nodes/SplicerNode';
import type { PortType } from '../../runtime/types';

/**
 * Splicer-in: cross-boundary "spread → per-iteration slot" entry pin.
 *   Outer port: `outer` (kind: spread)
 *   Inner port: `inner` (kind: single)
 *
 * Splicer-out: cross-boundary "per-iteration slot → spread" exit pin.
 *   Inner port: `inner` (kind: single)
 *   Outer port: `outer` (kind: spread)
 *
 * One NodeDef per (role × port type) — number splicers are the common
 * case for indices and counts; layer splicers collect inner-drawn
 * Layers back to a Spread<Layer> for the Renderer.
 */

function makeSplicerIn(portType: PortType) {
  return registerNode({
    type: `region/splicer-in/${portType}`,
    category: 'Region',
    label: `SplicerIn:${portType}`,
    description: `Receive a Spread<${portType}> from outside; deliver one slot per iteration.`,
    inputs: [{ name: 'outer', type: portType, kind: 'spread' }],
    outputs: [{ name: 'inner', type: portType, kind: 'single' }],
    regionRole: 'splicer-in',
    component: SplicerInNode,
    // Compile synthesises these from region frame ports — the user
    // never needs to drop one manually.
    hidden: true,
  });
}

function makeSplicerOut(portType: PortType) {
  return registerNode({
    type: `region/splicer-out/${portType}`,
    category: 'Region',
    label: `SplicerOut:${portType}`,
    description: `Collect one value per iteration into an outer Spread<${portType}>.`,
    inputs: [{ name: 'inner', type: portType, kind: 'single' }],
    outputs: [{ name: 'outer', type: portType, kind: 'spread' }],
    regionRole: 'splicer-out',
    component: SplicerOutNode,
    hidden: true,
  });
}

/**
 * Single source of truth for which PortTypes get splicers. The
 * `satisfies Record<PortType, boolean>` is the future-proofing trick:
 * if a new variant is added to `PortType`, TypeScript refuses to
 * compile this file until it's listed here. No silent gaps. Set the
 * value to `false` for types that intentionally don't get splicers.
 *
 * Every `true` entry produces both an `-in` and an `-out` splicer
 * registered under `region/splicer-in/<type>` / `region/splicer-out/
 * <type>`, which compile.ts looks up by string when expanding region
 * ports.
 *
 * `audio` is intentionally `false` — splicing audio handles through a
 * ForEach has no clear meaning (each iteration would create or alias
 * persistent audio nodes), and the value flowing through is a handle
 * to a long-lived object, not a per-frame snapshot.
 */
const SPLICEABLE_TYPES = {
  number: true,
  boolean: true,
  string: true,
  vec2: true,
  color: true,
  mat3: true,
  layer: true,
  style: true,
  audio: false,
  image: true,
  any: true,
} satisfies Record<PortType, boolean>;

for (const [t, enabled] of Object.entries(SPLICEABLE_TYPES) as [PortType, boolean][]) {
  if (!enabled) continue;
  makeSplicerIn(t);
  makeSplicerOut(t);
}
