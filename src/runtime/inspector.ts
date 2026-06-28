import type { NodeId } from './graph';
import type { PortValue } from './types';
import type { RegionTickStats } from './tick';

/**
 * A snapshot of the most recent tick's outputs. Written by the render
 * driver every frame; read on-demand by hover tooltips that want to
 * show live values. Module-level state on purpose — we don't want
 * Zustand re-renders firing for every node, every frame.
 */
let latest: Map<NodeId, PortValue[]> = new Map();
let latestRegionStats: Map<NodeId, RegionTickStats> = new Map();

export function publishTickOutputs(outputs: Map<NodeId, PortValue[]>): void {
  latest = outputs;
}

export function publishRegionStats(stats: Map<NodeId, RegionTickStats>): void {
  latestRegionStats = stats;
}

export function getLatestOutputs(): Map<NodeId, PortValue[]> {
  return latest;
}

export function getRegionStats(nodeId: NodeId): RegionTickStats | undefined {
  return latestRegionStats.get(nodeId);
}

/**
 * Look up a single port's current value. Returns undefined when the
 * node hasn't ticked yet (or has no output for that port).
 */
export function getRuntimeValue(nodeId: NodeId, portIdx: number): unknown {
  const outs = latest.get(nodeId);
  if (!outs) return undefined;
  const port = outs[portIdx];
  if (!port) return undefined;
  // Spreads of length 1 unwrap; longer spreads return the array.
  return port.length === 1 ? port[0] : port;
}
