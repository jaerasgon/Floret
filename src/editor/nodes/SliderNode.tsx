import { memo, useEffect } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { nodeRegistry } from '../../nodes/registry';
import { portColorVar } from '../connection-rules';
import { useNodeParam } from '../useNodeParam';
import { ScrubInput } from '../ScrubInput';
import type { FloretNodeData } from '../editor-state';

/**
 * Slider IO box. Three params:
 *   - `value` (number)
 *   - `min`   (number, default 0) — editable inline
 *   - `max`   (number, default 1) — editable inline
 *
 * During drag we only call `setLive` (ParamStore write, no React reconcile).
 * On pointerup we `commit()` (push the value into `node.data.params` for
 * persistence). Matches the architecture in the plan §pitfall 1 + 2.
 *
 * Min / max use the same ScrubInput as inline node params — click to type
 * a new value, or drag horizontally on the field to scrub. When the user
 * edits min/max we also pull the slider value back into range so the
 * thumb position never disagrees with the wired-out scalar.
 *
 * If `value` falls outside `[min, max]` (e.g. a sample patch saved
 * `value: 50` with `min: 0, max: 1`), the slider auto-clamps on mount so
 * the thumb position and the wire-out value agree. HTML range inputs
 * already clamp drag input — this effect catches the load case.
 */
export const SliderNode = memo(function SliderNode(props: NodeProps) {
  const { id, data, selected } = props;
  const nodeData = data as unknown as FloretNodeData;
  const def = nodeRegistry.get(nodeData.defType)!;
  const [value, setLiveValue, commitValue] = useNodeParam<number>(id, 'value', 0);
  const [min, setLiveMin, commitMin] = useNodeParam<number>(id, 'min', 0);
  const [max, setLiveMax, commitMax] = useNodeParam<number>(id, 'max', 1);
  // Keep the slider usable when the user enters an inverted or empty
  // range — we never want the HTML range input to receive max <= min.
  const safeMax = max > min ? max : min + 1;
  const range = safeMax - min;
  // Pick a step that feels right for the active range. The slider's drag
  // resolution scales with the range so 0..1 stays smooth and 0..1000
  // doesn't take forever to traverse.
  const sliderStep = range > 100 ? 1 : range > 10 ? 0.1 : 0.01;
  // ScrubInput uses its own step; pick something proportional but with a
  // floor so typing in tiny numbers still moves on each drag tick.
  const scrubStep = Math.max(0.01, range / 100);
  const portColor = portColorVar('number');

  useEffect(() => {
    const clamped = value < min ? min : value > safeMax ? safeMax : value;
    if (clamped !== value) {
      setLiveValue(clamped);
      commitValue();
    }
  }, [value, min, safeMax, setLiveValue, commitValue]);

  return (
    <div className="node iobox iobox-slider-node" data-selected={selected ? 1 : 0} data-category="io">
      <div className="node-header">
        <span className="node-label">{def.label}</span>
        <span className="node-type">{value.toFixed(2)}</span>
      </div>
      <div className="node-iobox-body">
        {/* Slider lives on the same row as the output pin so the node
            stays vertically compact — no dedicated ports row below. */}
        <div className="iobox-slider-row">
          <input
            type="range"
            className="iobox-slider nodrag"
            min={min}
            max={safeMax}
            step={sliderStep}
            value={value}
            onChange={(e) => setLiveValue(Number(e.target.value))}
            onPointerUp={commitValue}
            onBlur={commitValue}
          />
          <Handle
            id="out-number"
            type="source"
            position={Position.Right}
            className="iobox-slider-handle"
            style={{ background: portColor, borderColor: portColor }}
          />
        </div>
        <div className="iobox-range-labels">
          <ScrubInput
            value={min}
            step={scrubStep}
            className="iobox-range-input iobox-range-input-min"
            onLive={setLiveMin}
            onCommit={commitMin}
          />
          <ScrubInput
            value={max}
            step={scrubStep}
            className="iobox-range-input iobox-range-input-max"
            onLive={setLiveMax}
            onCommit={commitMax}
          />
        </div>
      </div>
    </div>
  );
});
