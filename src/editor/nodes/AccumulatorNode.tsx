import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { nodeRegistry } from '../../nodes/registry';
import { portColorVar } from '../connection-rules';
import type { NoduleNodeData } from '../editor-state';

/**
 * Accumulator visual. Two handles on each side:
 *   Left  (outer):  `in-init` (top),  `out-final` (bottom)
 *   Right (inner):  `out-read` (top), `in-write` (bottom)
 *
 * The styling makes the role clear: a wider pill with a divider, init on
 * the top-left and final on the bottom-left, read on the top-right and
 * write on the bottom-right.
 */
export const AccumulatorNode = memo(function AccumulatorNode(props: NodeProps) {
  const { data, selected } = props;
  const nodeData = data as unknown as NoduleNodeData;
  const def = nodeRegistry.get(nodeData.defType);
  if (!def) return null;
  const portType = def.inputs[0]!.type;
  const color = portColorVar(portType);

  return (
    <div className="accumulator-node" data-selected={selected ? 1 : 0}>
      <div className="accumulator-row">
        <Handle
          id="in-init"
          type="target"
          position={Position.Left}
          style={{ top: 14, background: color, borderColor: color }}
        />
        <span className="accumulator-label">init</span>
        <span className="accumulator-label" style={{ marginLeft: 'auto' }}>read</span>
        <Handle
          id="out-read"
          type="source"
          position={Position.Right}
          style={{ top: 14, background: color, borderColor: color }}
        />
      </div>
      <div className="accumulator-divider" />
      <div className="accumulator-row">
        <Handle
          id="out-final"
          type="source"
          position={Position.Left}
          style={{ top: 'calc(100% - 14px)', background: color, borderColor: color }}
        />
        <span className="accumulator-label">final</span>
        <span className="accumulator-label" style={{ marginLeft: 'auto' }}>write</span>
        <Handle
          id="in-write"
          type="target"
          position={Position.Right}
          style={{ top: 'calc(100% - 14px)', background: color, borderColor: color }}
        />
      </div>
    </div>
  );
});
