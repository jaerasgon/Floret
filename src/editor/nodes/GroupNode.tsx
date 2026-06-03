import { memo, useCallback } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { useEditor, type NoduleNodeData } from '../editor-state';
import { nodeRegistry } from '../../nodes/registry';
import { getResolvedInputs } from '../../nodes/types';
import { portColorVar } from '../connection-rules';

/**
 * Group node — N regular slot pins on the left, one output on the right,
 * with `+` and `−` buttons in the header to grow/shrink the slot count.
 * Each pin accepts ONE wire of either `layer` or `Spread<layer>`.
 *
 * Removing a slot drops the edge that was wired to its handle (handled
 * automatically by `setNodeParam` in the editor store — it filters out
 * edges whose target handle no longer resolves to an active port).
 */
const MIN_SLOTS = 1;
const MAX_SLOTS = 32;

export const GroupNode = memo(function GroupNode(props: NodeProps) {
  const { id, data, selected } = props;
  const nodeData = data as unknown as NoduleNodeData;
  const def = nodeRegistry.get(nodeData.defType);
  const setNodeParam = useEditor((s) => s.setNodeParam);
  const warningText = useEditor((s) =>
    s.warnings
      .filter((w) => w.nodeIds?.includes(id))
      .map((w) => w.message)
      .join('\n'),
  );

  const slotCount = Math.max(
    MIN_SLOTS,
    Math.min(MAX_SLOTS, Math.floor(Number(nodeData.params.slotCount) || 2)),
  );

  const addSlot = useCallback(() => {
    if (slotCount >= MAX_SLOTS) return;
    setNodeParam(id, 'slotCount', slotCount + 1);
  }, [id, slotCount, setNodeParam]);

  const removeSlot = useCallback(() => {
    if (slotCount <= MIN_SLOTS) return;
    setNodeParam(id, 'slotCount', slotCount - 1);
  }, [id, slotCount, setNodeParam]);

  if (!def) return null;

  const inputs = getResolvedInputs(def, nodeData.params);
  const output = def.outputs[0]!;
  const rows = Math.max(inputs.length, 1);

  return (
    <div
      className="node node-grid node-group"
      data-selected={selected ? 1 : 0}
      data-category={def.category.toLowerCase()}
      data-warning={warningText ? 1 : 0}
    >
      <div className="node-header group-header" title={def.description ?? def.type}>
        <span className="node-label">{def.label}</span>
        <span className="group-slot-controls nodrag">
          <button
            type="button"
            className="group-slot-btn"
            onClick={(e) => { e.stopPropagation(); removeSlot(); }}
            disabled={slotCount <= MIN_SLOTS}
            title="Remove last slot"
          >
            −
          </button>
          <span className="group-slot-count" title={`${slotCount} slots`}>
            {slotCount}
          </span>
          <button
            type="button"
            className="group-slot-btn"
            onClick={(e) => { e.stopPropagation(); addSlot(); }}
            disabled={slotCount >= MAX_SLOTS}
            title="Add a slot"
          >
            +
          </button>
        </span>
        {warningText && (
          <span className="node-warning-chip" title={warningText}>⚠</span>
        )}
        <span className="node-type">{def.type}</span>
      </div>
      <div className="node-rows">
        {Array.from({ length: rows }, (_, i) => {
          const p = inputs[i];
          const showOutput = i === 0;
          return (
            <div key={i} className="node-row">
              {p && (
                <>
                  <Handle
                    id={`in-${p.name}`}
                    type="target"
                    position={Position.Left}
                    className="node-row-handle node-row-handle-left"
                    style={{
                      background: portColorVar(p.type),
                      borderColor: portColorVar(p.type),
                      color: portColorVar(p.type),
                    }}
                  />
                  <span className="node-row-label node-row-label-left group-slot-label">
                    {p.label ?? p.name}
                  </span>
                </>
              )}
              <span className="node-row-spacer" />
              {showOutput && output && (
                <>
                  <span className="node-row-label node-row-label-right">{output.label ?? output.name}</span>
                  <Handle
                    id={`out-${output.name}`}
                    type="source"
                    position={Position.Right}
                    className="node-row-handle node-row-handle-right"
                    style={{
                      background: portColorVar(output.type),
                      borderColor: portColorVar(output.type),
                      color: portColorVar(output.type),
                    }}
                  />
                </>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
});
