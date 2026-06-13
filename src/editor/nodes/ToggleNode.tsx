import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { nodeRegistry } from '../../nodes/registry';
import { portColorVar } from '../connection-rules';
import { useNodeParam } from '../useNodeParam';
import type { FloretNodeData } from '../editor-state';

export const ToggleNode = memo(function ToggleNode(props: NodeProps) {
  const { id, data, selected } = props;
  const nodeData = data as unknown as FloretNodeData;
  const def = nodeRegistry.get(nodeData.defType)!;
  const [value, setLive, commit] = useNodeParam<boolean>(id, 'value', false);

  return (
    <div className="node iobox" data-selected={selected ? 1 : 0} data-category="io">
      <div className="node-header">
        <span className="node-label">{def.label}</span>
        <span className="node-type">{value ? 'on' : 'off'}</span>
      </div>
      <div className="node-iobox-body" style={{ alignItems: 'center' }}>
        <button
          className={`iobox-toggle nodrag ${value ? 'on' : 'off'}`}
          onClick={() => {
            setLive(!value);
            commit();
          }}
        >
          {value ? '● ON' : '○ OFF'}
        </button>
      </div>
      <div className="node-ports">
        <div className="port-col port-col-out">
          <div className="port-row right">
            <span className="port-name">bool</span>
            <Handle
              id="out-bool"
              type="source"
              position={Position.Right}
              style={{
                background: portColorVar('boolean'),
                borderColor: portColorVar('boolean'),
              }}
            />
          </div>
        </div>
      </div>
    </div>
  );
});
