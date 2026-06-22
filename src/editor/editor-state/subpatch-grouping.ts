import { nodeRegistry } from '../../nodes/registry';
import type { PortType } from '../../runtime/types';
import { parseHandle } from '../connection-rules';
import type { FloretEdge, FloretNode, RegionPortDescriptor } from './types';
import { newId, sortNodesParentFirst } from './util';

export interface SubpatchGrouping {
  nodes: FloretNode[];
  edges: FloretEdge[];
  subId: string;
  subParams: Record<string, unknown>;
}

/**
 * Pure transform behind `groupSelectionIntoSubpatch`: given the current
 * nodes/edges, wrap the selected nodes in a new meta/subpatch node. The
 * selected nodes become children of the subpatch; edges crossing the
 * selection boundary become subpatch ports (one per crossing), with the
 * outer wires re-routed to the subpatch's outer pins and inner wires to its
 * inner pins. Returns the new graph + the subpatch id/params (the caller
 * seeds paramStore and commits), or null when the selection can't be grouped
 * (empty, mixed parents, or contains a non-subpatch region).
 */
export function buildSubpatchGrouping(
  allNodes: FloretNode[],
  allEdges: FloretEdge[],
): SubpatchGrouping | null {
  const selected = allNodes.filter((n) => n.selected);
  if (selected.length === 0) return null;
  // All selected nodes must share the same parent (we can only group
  // siblings into one subpatch). Otherwise it's ambiguous.
  const parentIds = new Set(selected.map((n) => n.parentId ?? null));
  if (parentIds.size > 1) return null;
  const sharedParent = selected[0]!.parentId;
  // Reject if anything selected is itself a region (regions can't
  // currently sit inside subpatches in the simplified model).
  for (const n of selected) {
    const def = nodeRegistry.get(n.data.defType);
    if (def?.regionKind && def.regionKind !== 'subpatch') return null;
  }
  const selectedIds = new Set(selected.map((n) => n.id));

  // Centroid (in shared-parent local coords) → position the subpatch.
  let sumX = 0, sumY = 0;
  for (const n of selected) { sumX += n.position.x; sumY += n.position.y; }
  const cx = sumX / selected.length;
  const cy = sumY / selected.length;

  const subId = newId('region_subpatch');
  const subDef = nodeRegistry.require('meta/subpatch');
  const subParams: Record<string, unknown> = { ...(subDef.defaultParams ?? {}) };
  subParams.label = subParams.label ?? 'Subpatch';

  type CrossingIn = {
    portId: string;
    portName: string;
    portType: PortType;
    outerSource: string;
    outerSourceHandle: string;
    innerTarget: string;
    innerTargetHandle: string;
    originalEdgeId: string;
  };
  type CrossingOut = {
    portId: string;
    portName: string;
    portType: PortType;
    innerSource: string;
    innerSourceHandle: string;
    outerTarget: string;
    outerTargetHandle: string;
    originalEdgeId: string;
  };
  const crossingsIn: CrossingIn[] = [];
  const crossingsOut: CrossingOut[] = [];
  let portCounter = 0;
  const nextPortId = () => `p_subport_${++portCounter}_${subId}`;

  const portTypeOf = (
    nodeId: string,
    side: 'in' | 'out',
    handleName: string,
  ): PortType | null => {
    const node = allNodes.find((x) => x.id === nodeId);
    if (!node) return null;
    const d = nodeRegistry.get(node.data.defType);
    if (!d) return null;
    if (side === 'out') {
      const p = d.outputs.find((p) => p.name === handleName);
      return p?.type ?? null;
    }
    const inputs = d.dynamicInputs ? d.dynamicInputs(node.data.params) : d.inputs;
    const p = inputs.find((p) => p.name === handleName);
    return p?.type ?? null;
  };

  for (const e of allEdges) {
    const srcIn = selectedIds.has(e.source);
    const dstIn = selectedIds.has(e.target);
    if (srcIn === dstIn) continue;
    const srcParsed = parseHandle(e.sourceHandle);
    const dstParsed = parseHandle(e.targetHandle);
    if (!srcParsed || !dstParsed) continue;
    if (!srcIn && dstIn) {
      const t = portTypeOf(e.target, 'in', dstParsed.name);
      if (!t) continue;
      crossingsIn.push({
        portId: nextPortId(),
        portName: `in${crossingsIn.length + 1}`,
        portType: t,
        outerSource: e.source,
        outerSourceHandle: e.sourceHandle!,
        innerTarget: e.target,
        innerTargetHandle: e.targetHandle!,
        originalEdgeId: e.id,
      });
    } else if (srcIn && !dstIn) {
      const t = portTypeOf(e.source, 'out', srcParsed.name);
      if (!t) continue;
      crossingsOut.push({
        portId: nextPortId(),
        portName: `out${crossingsOut.length + 1}`,
        portType: t,
        innerSource: e.source,
        innerSourceHandle: e.sourceHandle!,
        outerTarget: e.target,
        outerTargetHandle: e.targetHandle!,
        originalEdgeId: e.id,
      });
    }
  }

  const ports: RegionPortDescriptor[] = [
    ...crossingsIn.map((c) => ({
      id: c.portId,
      name: c.portName,
      type: c.portType,
      direction: 'input' as const,
    })),
    ...crossingsOut.map((c) => ({
      id: c.portId,
      name: c.portName,
      type: c.portType,
      direction: 'output' as const,
    })),
  ];

  const sub: FloretNode = {
    id: subId,
    type: 'floret',
    position: { x: cx, y: cy },
    data: { defType: 'meta/subpatch', params: subParams, ports },
  };
  if (sharedParent) { sub.parentId = sharedParent; sub.extent = 'parent'; }

  // Drop the original crossing edges; add the new outer / inner pairs.
  const droppedIds = new Set<string>([
    ...crossingsIn.map((c) => c.originalEdgeId),
    ...crossingsOut.map((c) => c.originalEdgeId),
  ]);

  const newNodes: FloretNode[] = allNodes.map((n) => {
    if (!selectedIds.has(n.id)) return n;
    // Reparent selected node into the subpatch; convert its
    // position from shared-parent local to subpatch-local.
    return {
      ...n,
      parentId: subId,
      extent: 'parent' as const,
      position: { x: n.position.x - cx, y: n.position.y - cy },
      selected: false,
    };
  });
  newNodes.push({ ...sub, selected: true });

  const edges = allEdges.filter((e) => !droppedIds.has(e.id));
  for (const c of crossingsIn) {
    edges.push({
      id: `e_${newId('subedge')}`,
      source: c.outerSource,
      target: subId,
      sourceHandle: c.outerSourceHandle,
      targetHandle: `in-${c.portId}`,
    });
    edges.push({
      id: `e_${newId('subedge')}`,
      source: subId,
      target: c.innerTarget,
      sourceHandle: `out-${c.portId}`,
      targetHandle: c.innerTargetHandle,
    });
  }
  for (const c of crossingsOut) {
    edges.push({
      id: `e_${newId('subedge')}`,
      source: c.innerSource,
      target: subId,
      sourceHandle: c.innerSourceHandle,
      targetHandle: `in-${c.portId}`,
    });
    edges.push({
      id: `e_${newId('subedge')}`,
      source: subId,
      target: c.outerTarget,
      sourceHandle: `out-${c.portId}`,
      targetHandle: c.outerTargetHandle,
    });
  }

  return { nodes: sortNodesParentFirst(newNodes), edges, subId, subParams };
}
