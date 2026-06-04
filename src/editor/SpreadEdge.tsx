import { BaseEdge, getBezierPath, type EdgeProps } from '@xyflow/react';
import { useShallow } from 'zustand/react/shallow';
import { nodeRegistry } from '../nodes/registry';
import { parseHandle, portColorVar } from './connection-rules';
import { useEditor, type FloretNodeData } from './editor-state';
import type { PortType } from '../runtime/types';

/**
 * Default edge. Wire colour is set as a CSS custom property
 * (`--edge-stroke`) derived from the source port's type — styles.css
 * mixes that with `--fg-2` for a subtle tint and brightens to full on
 * hover. Spread length is no longer displayed here; the same info is
 * available via the value tooltip on the source pin (which shows the
 * head value plus the "1 of N" badge).
 */
export function SpreadEdge(props: EdgeProps) {
  const {
    id,
    sourceX, sourceY, targetX, targetY,
    sourcePosition, targetPosition,
    source, sourceHandleId,
    style, markerEnd,
  } = props;

  const [edgePath] = getBezierPath({
    sourceX, sourceY, sourcePosition,
    targetX, targetY, targetPosition,
  });

  // Resolve the source pin's port type so we can tint the wire. We only
  // need the type — not the runtime id — since the spread badge that
  // used those is gone. `useShallow` keeps the selector stable.
  const portType = useEditor(
    useShallow((s) => {
      const node = s.nodes.find((n) => n.id === source);
      if (!node) return null as PortType | null;
      const def = nodeRegistry.get((node.data as FloretNodeData).defType);
      if (!def) return null;
      const parsed = sourceHandleId ? parseHandle(sourceHandleId) : null;
      if (!parsed) return null;
      if (def.regionKind) {
        const ports = (node.data as FloretNodeData).ports ?? [];
        const matching = ports.find((p) => p.id === parsed.name);
        return matching?.type ?? null;
      }
      const port = def.outputs.find((p) => p.name === parsed.name);
      return port?.type ?? null;
    }),
  );

  const stroke = portType && portType !== 'any' ? portColorVar(portType) : undefined;
  const wireStyle = stroke
    ? ({ ...style, '--edge-stroke': stroke } as React.CSSProperties)
    : style;

  return <BaseEdge id={id} path={edgePath} style={wireStyle} markerEnd={markerEnd} />;
}
