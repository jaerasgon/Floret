import { registerNode } from '../registry';
import { ProbeNode } from '../../editor/nodes/ProbeNode';

/**
 * Probe — a persistent data inspector that also reroutes the wire.
 *
 * Drag a wire out of any pin and Alt-click empty space to drop one: it
 * shows the data flowing through that wire (structured, scrollable for
 * spreads) and passes it straight through, so it doubles as a reroute
 * point for tidying wires. Works for every data type.
 *
 * Pass-through reroute, kind-transparent:
 *   - `in` is `multi`, so a single OR spread source wire is accepted and
 *     arrives verbatim (gather concatenates a one-wire spread unchanged).
 *   - `out` is `single`-kind. The runtime is unaffected by this (computeRaw
 *     re-emits the gathered PortValue verbatim — gatherInputs hands it to any
 *     downstream target as-is), but a `single` source kind is editor-
 *     compatible with EVERY target kind (single / spread / multi). A `spread`
 *     output would be blocked from `single` inputs, so this is what makes the
 *     probe rewire ALL data types. `dynamicOutputs` echoes the upstream type
 *     so the output pin keeps the wire's colour.
 *
 * The custom component reads this node's own live output
 * (inspector.getLatestOutputs) and renders it.
 */
export const probeNode = registerNode({
  type: 'meta/probe',
  category: 'Meta',
  label: 'Probe',
  description: 'Inspect the data on a wire — a persistent, scrollable readout that also reroutes the wire. Drag a wire out and Alt-click empty space to drop one.',
  inputs: [{ name: 'in', type: 'any', kind: 'spread' }],
  outputs: [{ name: 'out', type: 'any', kind: 'single' }],
  dynamicOutputs: (_params, up) => [{ name: 'out', type: up.in ?? 'any', kind: 'single' }],
  computeRaw: (inputs) => [inputs[0] ?? []],
  component: ProbeNode,
});
