import { memo, useCallback, useMemo, useState } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { useEditor, type FloretNodeData } from '../editor-state';
import { nodeRegistry } from '../../nodes/registry';
import { getResolvedInputs } from '../../nodes/types';
import { portColorVar, parseHandle } from '../connection-rules';
import { PortValueInlineEditor, supportsInlineEditor, formatValue, InputPin } from './GenericNode';
import { openPortValueEditor } from '../PortValueEditor';

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
  const nodeData = data as unknown as FloretNodeData;
  const def = nodeRegistry.get(nodeData.defType);
  const setNodeParam = useEditor((s) => s.setNodeParam);
  const warningText = useEditor((s) =>
    s.warnings
      .filter((w) => w.nodeIds?.includes(id))
      .map((w) => w.message)
      .join('\n'),
  );

  // Which slots have an incoming wire — those show their pin only; the rest
  // get an inline editor so the user can type a constant (e.g. type 3 into
  // a Mul slot instead of wiring a Number node).
  const connectedInputPorts = useEditor((s) => {
    const names: string[] = [];
    for (const e of s.edges) {
      if (e.target !== id) continue;
      const parsed = parseHandle(e.targetHandle);
      if (parsed?.kind === 'in') names.push(parsed.name);
    }
    return names.sort().join(',');
  });
  const connectedSet = useMemo(
    () => new Set(connectedInputPorts.split(',').filter(Boolean)),
    [connectedInputPorts],
  );

  // Per-slot editor-expanded state — labels are clickable toggles, collapsed
  // by default, click to reveal the inline editor (matches GenericNode).
  const [expandedPorts, setExpandedPorts] = useState<Set<string>>(() => new Set());
  const togglePort = useCallback((portName: string) => {
    setExpandedPorts((prev) => {
      const next = new Set(prev);
      if (next.has(portName)) next.delete(portName);
      else next.add(portName);
      return next;
    });
  }, []);

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
          // Inline editor mirrors GenericNode: only on unconnected editable
          // slots, revealed by clicking the label; otherwise a value readout.
          const editable = !!p && !connectedSet.has(p.name) && supportsInlineEditor(p.type);
          const override = p ? nodeData.portValues?.[p.name] : undefined;
          const expanded = !!p && expandedPorts.has(p.name);
          const showEditor = editable && expanded;
          const showReadout = editable && override !== undefined && !showEditor;
          return (
            <div
              key={i}
              className="node-row"
              // Right-click an editable slot to open the value editor — parity
              // with GenericNode's input rows.
              onContextMenu={editable && p ? (e) => {
                e.preventDefault();
                e.stopPropagation();
                openPortValueEditor({
                  nodeId: id,
                  portName: p.name,
                  portType: p.type,
                  initialValue: override !== undefined ? override : p.default,
                  screenPosition: { x: e.clientX, y: e.clientY },
                });
              } : undefined}
            >
              {p && (
                <>
                  {/* InputPin gives the hover-to-inspect tooltip (same handle
                      styling as before). */}
                  <InputPin
                    nodeId={id}
                    port={p}
                    wired={connectedSet.has(p.name)}
                    override={override}
                  />
                  <span
                    className={`node-row-label node-row-label-left group-slot-label${editable ? ' clickable' : ''}${showEditor ? ' expanded' : ''}`}
                    onClick={editable ? (e) => { e.stopPropagation(); togglePort(p.name); } : undefined}
                  >
                    {p.label ?? p.name}
                  </span>
                  {showEditor && (
                    <PortValueInlineEditor
                      nodeId={id}
                      portName={p.name}
                      type={p.type}
                      defaultVal={override ?? p.default}
                    />
                  )}
                  {showReadout && (
                    <span className="port-value-readout">{formatValue(override)}</span>
                  )}
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
