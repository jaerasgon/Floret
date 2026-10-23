import { registerNode } from '../registry';
import { SubpatchNode } from '../../editor/nodes/SubpatchNode';

/**
 * Subpatch — a collapsible container that groups a set of inner nodes
 * behind a manually-defined set of input/output ports. From the outside
 * a subpatch looks like a normal node; the user "enters" it (shortcut
 * `I`, or click the breadcrumb / subpatch label) to edit its contents,
 * and exits with `O`.
 *
 * Ports live on `data.ports` (same `RegionPortDescriptor` shape regions
 * use), so the existing wire-drop-on-frame-edge path adds new exposed
 * ports as the user wires inner nodes to the boundary.
 *
 * `regionKind: 'subpatch'` differentiates it from iterating regions
 * (foreach / if / cache) — compile treats it as a transparent scope
 * boundary: outer→port→inner-consumer edges are flattened straight
 * through, no iteration, no spread wrapping.
 */
export const subpatchNode = registerNode({
  type: 'meta/subpatch',
  category: 'Meta',
  label: 'Subpatch',
  description: 'Group nodes behind a single boundary with custom ports.',
  inputs: [],
  outputs: [],
  defaultParams: { label: 'Subpatch', width: 480, height: 280 },
  regionKind: 'subpatch',
  component: SubpatchNode,
});
