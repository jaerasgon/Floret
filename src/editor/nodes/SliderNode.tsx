import { memo, useEffect } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { nodeRegistry } from '../../nodes/registry';
import { portColorVar } from '../connection-rules';
import { useNodeParam } from '../useNodeParam';
import type { NoduleNodeData } from '../editor-state';

/**
 * Slider IO box. Three params:
 *   - `value` (number)
 *   - `min`   (number, default 0)
 *   - `max`   (number, default 1)
 *
 * During drag we only call `setLive` (ParamStore write, no React reconcile).
 * On pointerup we `commit()` (push the value into `node.data.params` for
 * persistence). Matches the architecture in the plan §pitfall 1 + 2.
 *
 * If `value` falls outside `[min, max]` (e.g. a sample patch saved
 * `value: 50` with `min: 0, max: 1`), the slider auto-clamps on mount so
 * the thumb position and the wire-out value agree. HTML range inputs
 * already clamp drag input — this effect catches the load case.
 */
export const SliderNode = memo(function SliderNode(props: NodeProps) {
  const { id, data, selected } = props;
  const nodeData = data as unknown as NoduleNodeData;
  const def = nodeRegistry.get(nodeData.defType)!;
  const [value, setLive, commit] = useNodeParam<number>(id, 'value', 0);
  const [min] = useNodeParam<number>(id, 'min', 0);
  const [max] = useNodeParam<number>(id, 'max', 1);
  const span = max - min;
  const safeMax = span === 0 ? min + 1 : max;
  const range = safeMax - min;
  const step = range > 100 ? 1 : range > 10 ? 0.1 : 0.01;
  const portColor = portColorVar('number');

  useEffect(() => {
    const clamped = value < min ? min : value > safeMax ? safeMax : value;
    if (clamped !== value) {
      setLive(clamped);
      commit();
    }
  }, [value, min, safeMax, setLive, commit]);

  return (
    <div className="node iobox" data-selected={selected ? 1 : 0} data-category="io">
      <div className="node-header">
        <span className="node-label">{def.label}</span>
        <span className="node-type">{value.toFixed(2)}</span>
      </div>
      <div className="node-iobox-body">
        <input
          type="range"
          className="iobox-slider nodrag"
          min={min}
          max={safeMax}
          step={step}
          value={value}
          onChange={(e) => setLive(Number(e.target.value))}
          onPointerUp={commit}
          onBlur={commit}
        />
        <div className="iobox-range-labels">
          <span>{min}</span>
          <span>{safeMax}</span>
        </div>
      </div>
      <div className="node-ports">
        <div className="port-col port-col-out">
          <div className="port-row right">
            <span className="port-name">out</span>
            <Handle
              id="out-out"
              type="source"
              position={Position.Right}
              style={{ background: portColor, borderColor: portColor }}
            />
          </div>
        </div>
      </div>
    </div>
  );
});
