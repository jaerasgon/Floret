import { memo, useCallback, useRef } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { nodeRegistry } from '../../nodes/registry';
import { portColorVar } from '../connection-rules';
import { useNodeParam } from '../useNodeParam';
import type { NoduleNodeData } from '../editor-state';

/**
 * XY Pad — a 2D draggable area. Outputs `x` and `y` in [0, 1].
 * (Range customization can come later via inline params.)
 */
export const XYPadNode = memo(function XYPadNode(props: NodeProps) {
  const { id, data, selected } = props;
  const nodeData = data as unknown as NoduleNodeData;
  const def = nodeRegistry.get(nodeData.defType)!;
  const [x, setX, commitX] = useNodeParam<number>(id, 'x', 0.5);
  const [y, setY, commitY] = useNodeParam<number>(id, 'y', 0.5);
  const padRef = useRef<HTMLDivElement | null>(null);

  const drag = useCallback(
    (e: React.PointerEvent) => {
      if (e.buttons === 0) return; // not actively dragging
      const el = padRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const nx = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
      const ny = Math.max(0, Math.min(1, (e.clientY - rect.top) / rect.height));
      setX(nx);
      setY(ny);
    },
    [setX, setY],
  );

  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
      drag(e);
    },
    [drag],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent) => {
      (e.target as HTMLElement).releasePointerCapture(e.pointerId);
      commitX();
      commitY();
    },
    [commitX, commitY],
  );

  const numColor = portColorVar('number');

  return (
    <div className="node iobox" data-selected={selected ? 1 : 0} data-category="io">
      <div className="node-header">
        <span className="node-label">{def.label}</span>
        <span className="node-type">
          {x.toFixed(2)} · {y.toFixed(2)}
        </span>
      </div>
      <div className="node-iobox-body">
        <div
          ref={padRef}
          className="iobox-xypad nodrag"
          onPointerDown={onPointerDown}
          onPointerMove={drag}
          onPointerUp={onPointerUp}
        >
          <div
            className="iobox-xypad-dot"
            style={{ left: `${x * 100}%`, top: `${y * 100}%` }}
          />
        </div>
      </div>
      <div className="node-ports">
        <div className="port-col port-col-out">
          {(['x', 'y'] as const).map((n) => (
            <div className="port-row right" key={n}>
              <span className="port-name">{n}</span>
              <Handle
                id={`out-${n}`}
                type="source"
                position={Position.Right}
                style={{ background: numColor, borderColor: numColor }}
              />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
});
