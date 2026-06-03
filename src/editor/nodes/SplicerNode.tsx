import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { nodeRegistry } from '../../nodes/registry';
import { portColorVar } from '../connection-rules';
import type { NoduleNodeData } from '../editor-state';

/**
 * Splicer In / Out visuals — small pill-shaped child nodes anchored to a
 * region's border. They carry `parentId: regionId` so they move with the
 * region. The "outer" handle faces outside the region; the "inner"
 * handle faces inside.
 */

export const SplicerInNode = memo(function SplicerInNode(props: NodeProps) {
  const { data, selected } = props;
  const nodeData = data as unknown as NoduleNodeData;
  const def = nodeRegistry.get(nodeData.defType);
  if (!def) return null;
  const portType = def.inputs[0]!.type;
  return (
    <div className={`splicer-node splicer-in`} data-selected={selected ? 1 : 0}>
      <Handle
        id="in-outer"
        type="target"
        position={Position.Left}
        style={{ background: portColorVar(portType), borderColor: portColorVar(portType) }}
      />
      <span className="splicer-arrow">▸</span>
      <Handle
        id="out-inner"
        type="source"
        position={Position.Right}
        style={{ background: portColorVar(portType), borderColor: portColorVar(portType) }}
      />
    </div>
  );
});

export const SplicerOutNode = memo(function SplicerOutNode(props: NodeProps) {
  const { data, selected } = props;
  const nodeData = data as unknown as NoduleNodeData;
  const def = nodeRegistry.get(nodeData.defType);
  if (!def) return null;
  const portType = def.outputs[0]!.type;
  return (
    <div className={`splicer-node splicer-out`} data-selected={selected ? 1 : 0}>
      <Handle
        id="in-inner"
        type="target"
        position={Position.Left}
        style={{ background: portColorVar(portType), borderColor: portColorVar(portType) }}
      />
      <span className="splicer-arrow">▸</span>
      <Handle
        id="out-outer"
        type="source"
        position={Position.Right}
        style={{ background: portColorVar(portType), borderColor: portColorVar(portType) }}
      />
    </div>
  );
});
