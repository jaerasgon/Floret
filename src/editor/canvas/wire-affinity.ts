import type { OnConnectStartParams } from '@xyflow/react';
import { nodeRegistry } from '../../nodes/registry';
import { getResolvedInputs, getResolvedOutputs } from '../../nodes/types';
import type { PortType } from '../../runtime/types';
import { useEditor, getDefTypeFor, getRegionPortsFor, getParamsFor } from '../editor-state';
import { arePortsCompatible, getUpstreamTypes, parseHandle } from '../connection-rules';

/**
 * Find the dragged pin's port type. For region nodes the port type lives
 * on `data.ports`; for normal nodes it lives on the NodeDef.
 */
export function resolveDraggedPortType(
  nodeId: string,
  handleName: string,
  handleType: 'source' | 'target',
): PortType | null {
  const defType = getDefTypeFor(nodeId);
  if (!defType) return null;
  const def = nodeRegistry.get(defType);
  if (!def) return null;

  if (def.regionKind) {
    const ports = getRegionPortsFor(nodeId);
    const port = ports?.find((p) => p.id === handleName);
    if (port) return port.type;
    // Fall through to NodeDef static ports (e.g. If.condition).
  }

  if (handleType === 'source') {
    // Source-side type lookup must honour dynamicOutputs so that wires
    // dragged from a polymorphic node (Repeat with a Color upstream, …)
    // carry the resolved type into wire validation and into any region
    // port that gets created on drop. Without this, a Repeat→ForEach
    // drop creates a port typed `any` instead of `color`.
    const params = getParamsFor(nodeId) ?? {};
    const outputs = def.dynamicOutputs
      ? getResolvedOutputs(def, params, getUpstreamTypesFor(nodeId))
      : def.outputs;
    return outputs.find((p) => p.name === handleName)?.type ?? null;
  }

  const port = getResolvedInputs(def, getParamsFor(nodeId) ?? {})
    .find((p) => p.name === handleName);
  return port?.type ?? null;
}

/**
 * Module-local helper: build the upstreamTypes map for a node by reading
 * the editor store's current edge/node lists. Used by
 * `resolveDraggedPortType` (which can't call useEditor() since it's not
 * a hook) so dynamicOutputs sees the same upstream picture the visual
 * layer does.
 */
function getUpstreamTypesFor(nodeId: string) {
  const state = useEditor.getState();
  return getUpstreamTypes(nodeId, state.edges, state.nodes);
}

/**
 * Walk every handle in the DOM and mark it as compatible / incompatible
 * with the dragged pin. CSS uses the `data-wire-affinity` attribute to
 * brighten the matches and dim the rest, so the user can see at a
 * glance which ports will accept the wire.
 */
export function paintWireAffinity(pending: OnConnectStartParams): void {
  if (!pending.nodeId || !pending.handleId || !pending.handleType) return;
  const parsed = parseHandle(pending.handleId);
  if (!parsed) return;
  const draggedType = resolveDraggedPortType(pending.nodeId, parsed.name, pending.handleType);
  if (!draggedType) return;

  // Dragging a source pin → match against TARGET handles. Vice-versa.
  const lookFor: 'source' | 'target' = pending.handleType === 'source' ? 'target' : 'source';

  document.querySelectorAll('.react-flow__handle').forEach((el) => {
    const h = el as HTMLElement;
    const nodeEl = h.closest('.react-flow__node') as HTMLElement | null;
    const nodeId = nodeEl?.dataset.id;
    if (!nodeId) return;
    const handleId = h.dataset.handleid ?? '';
    const role: 'source' | 'target' = h.classList.contains('source') ? 'source' : 'target';
    if (role !== lookFor) return;
    // No self-pin matching.
    if (nodeId === pending.nodeId && handleId === pending.handleId) return;

    const p = parseHandle(handleId);
    if (!p) return;
    const otherType = resolveDraggedPortType(nodeId, p.name, role);
    if (!otherType) return;
    const compat = arePortsCompatible(draggedType, otherType);
    h.setAttribute('data-wire-affinity', compat ? 'match' : 'miss');
  });
  document.body.setAttribute('data-wire-dragging', '1');
}

export function clearWireAffinity(): void {
  document.body.removeAttribute('data-wire-dragging');
  document.querySelectorAll('[data-wire-affinity]').forEach((el) => {
    (el as HTMLElement).removeAttribute('data-wire-affinity');
  });
}
