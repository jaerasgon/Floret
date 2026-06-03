import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Handle, NodeResizer, Position, useUpdateNodeInternals, type NodeProps } from '@xyflow/react';
import { useEditor, type NoduleNodeData, type RegionPortDescriptor } from '../editor-state';
import { nodeRegistry } from '../../nodes/registry';
import { portColorVar } from '../connection-rules';

/**
 * Subpatch container — has two distinct visual modes:
 *
 *   1) Collapsed (default — when this subpatch is NOT the current view):
 *      Renders as a normal-sized node. Input ports on the left (target
 *      pins, id `in-<portId>`); output ports on the right (source pins,
 *      id `out-<portId>`). Click the title or press `I` while selected
 *      to enter it.
 *
 *   2) Expanded (when `viewStack` ends with this subpatch's id):
 *      Renders as a region-style frame holding the inner nodes. The
 *      OUTER pins (which still exist so cross-scope edges resolve)
 *      anchor to the frame's edges; INNER pins face inward so inner
 *      nodes can wire to/from them.
 *
 * Handle id scheme matches RegionNode so the existing connection-rules
 * and wire-drop machinery work without changes:
 *   - input port `p.id`:  outer `in-<id>`  (target, left)
 *                         inner `out-<id>` (source, just inside left)
 *   - output port `p.id`: outer `out-<id>` (source, right)
 *                         inner `in-<id>`  (target, just inside right)
 */
const PORT_TAB_WIDTH = 56;
const PORT_TAB_HEIGHT = 14;
const PORT_ROW_PITCH = 20;
const PORT_TOP_OFFSET = 14;
const PORT_TAB_INSET = 4;
// (Inner pin position is computed inline — see SubpatchFramePort.)

export const SubpatchNode = memo(function SubpatchNode(props: NodeProps) {
  const { id, data, selected } = props;
  const nodeData = data as unknown as NoduleNodeData;
  const def = nodeRegistry.get(nodeData.defType);
  const isCurrentView = useEditor((s) => s.viewStack[s.viewStack.length - 1] === id);
  const enterSubpatch = useEditor((s) => s.enterSubpatch);
  const setNodeParam = useEditor((s) => s.setNodeParam);
  const removeRegionPort = useEditor((s) => s.removeRegionPort);
  const renameRegionPort = useEditor((s) => s.renameRegionPort);
  const setRegionPortY = useEditor((s) => s.setRegionPortY);
  const selectRegionPort = useEditor((s) => s.selectRegionPort);
  // Port-pill selection set, filtered to this subpatch (primitive
  // string keeps the zustand selector stable).
  const selectedPortSig = useEditor((s) =>
    s.selectedPortKeys.filter((k) => k.startsWith(`${id}#`)).join('|'),
  );
  const selectedPortKeys = useMemo(
    () => new Set(selectedPortSig ? selectedPortSig.split('|') : []),
    [selectedPortSig],
  );
  const warningText = useEditor((s) =>
    s.warnings
      .filter((w) => w.nodeIds?.includes(id))
      .map((w) => w.message)
      .join('\n'),
  );

  const ports: RegionPortDescriptor[] = nodeData.ports ?? [];
  const inputPorts = ports.filter((p) => p.direction === 'input');
  const outputPorts = ports.filter((p) => p.direction === 'output');
  const label = (nodeData.params.label as string | undefined) ?? def?.label ?? 'Subpatch';

  // Re-measure handles when the port list or width changes, so wires
  // follow port pins to their new positions.
  const updateNodeInternals = useUpdateNodeInternals();
  // Live size from RF (updated every frame by NodeResizer during a
  // drag), with persisted params as the fallback for the first render.
  // See RegionNode for the rationale — without this the visible frame
  // lags the resize until onResizeEnd commits.
  const expandedW = (props.width as number | undefined) ?? (nodeData.params.width as number | undefined) ?? 480;
  const expandedH = (props.height as number | undefined) ?? (nodeData.params.height as number | undefined) ?? 280;
  // Include each port's `y` so dragging the port tab triggers
  // updateNodeInternals — without it, React Flow keeps the handle's
  // cached position and connected wires stay anchored to the old y.
  const portSig = `${isCurrentView ? 1 : 0}|${expandedW}|${expandedH}|${ports.map((p) => `${p.id}.${p.direction}.${p.y ?? ''}`).join(',')}`;
  useEffect(() => {
    updateNodeInternals(id);
  }, [id, portSig, updateNodeInternals]);

  const onResizeEnd = useCallback(
    (_e: unknown, params: { width: number; height: number }) => {
      setNodeParam(id, 'width', params.width);
      setNodeParam(id, 'height', params.height);
    },
    [id, setNodeParam],
  );

  // ─── Expanded view: full region-style frame holding inner children ─────
  if (isCurrentView) {
    return (
      <div
        className="region-node region-subpatch"
        data-selected={selected ? 1 : 0}
        style={{ width: expandedW, height: expandedH }}
      >
        <NodeResizer
          minWidth={200}
          minHeight={140}
          isVisible
          onResizeEnd={onResizeEnd}
          lineClassName="region-resize-line"
          handleClassName="region-resize-handle"
        />
        <div className="region-header">
          <span className="region-label">{label}</span>
          {warningText && (
            <span className="node-warning-chip" title={warningText}>⚠</span>
          )}
          <span className="region-type">subpatch</span>
        </div>

        {inputPorts.map((p, i) => (
          <SubpatchFramePort
            key={p.id}
            port={p}
            side="left"
            top={p.y ?? PORT_TOP_OFFSET + i * PORT_ROW_PITCH}
            regionWidth={expandedW}
            regionHeight={expandedH}
            selected={selectedPortKeys.has(`${id}#${p.id}`)}
            onRemove={() => removeRegionPort(id, p.id)}
            onRename={(name) => renameRegionPort(id, p.id, name)}
            onDragY={(y) => setRegionPortY(id, p.id, y)}
            onSelect={(additive) => selectRegionPort(id, p.id, additive)}
          />
        ))}
        {outputPorts.map((p, i) => (
          <SubpatchFramePort
            key={p.id}
            port={p}
            side="right"
            top={p.y ?? PORT_TOP_OFFSET + i * PORT_ROW_PITCH}
            regionWidth={expandedW}
            regionHeight={expandedH}
            selected={selectedPortKeys.has(`${id}#${p.id}`)}
            onRemove={() => removeRegionPort(id, p.id)}
            onRename={(name) => renameRegionPort(id, p.id, name)}
            onDragY={(y) => setRegionPortY(id, p.id, y)}
            onSelect={(additive) => selectRegionPort(id, p.id, additive)}
          />
        ))}
      </div>
    );
  }

  // ─── Collapsed view: normal-shaped node with one row per port ─────────
  const numRows = Math.max(inputPorts.length, outputPorts.length, 1);
  return (
    <div
      className="node node-grid node-subpatch"
      data-selected={selected ? 1 : 0}
      data-category="meta"
      data-warning={warningText ? 1 : 0}
    >
      <div
        className="node-header subpatch-header"
        title="Click or press I to enter this subpatch"
        onDoubleClick={(e) => {
          e.stopPropagation();
          enterSubpatch(id);
        }}
      >
        <span className="node-label">▸ {label}</span>
        {warningText && (
          <span className="node-warning-chip" title={warningText}>⚠</span>
        )}
        <span className="node-type">subpatch</span>
      </div>
      <div className="node-rows">
        {Array.from({ length: numRows }, (_, i) => {
          const left = inputPorts[i];
          const right = outputPorts[i];
          return (
            <div key={i} className="node-row">
              {left ? (
                <>
                  <Handle
                    id={`in-${left.id}`}
                    type="target"
                    position={Position.Left}
                    className="node-row-handle node-row-handle-left"
                    style={{
                      background: portColorVar(left.type),
                      borderColor: portColorVar(left.type),
                      color: portColorVar(left.type),
                    }}
                  />
                  <span className="node-row-label node-row-label-left">{left.name}</span>
                </>
              ) : (
                <span className="node-row-label node-row-label-left node-row-label-empty" />
              )}
              <span className="node-row-spacer" />
              {right ? (
                <>
                  <span className="node-row-label node-row-label-right">{right.name}</span>
                  <Handle
                    id={`out-${right.id}`}
                    type="source"
                    position={Position.Right}
                    className="node-row-handle node-row-handle-right"
                    style={{
                      background: portColorVar(right.type),
                      borderColor: portColorVar(right.type),
                      color: portColorVar(right.type),
                    }}
                  />
                </>
              ) : null}
            </div>
          );
        })}
      </div>
    </div>
  );
});

interface SubpatchFramePortProps {
  port: RegionPortDescriptor;
  side: 'left' | 'right';
  top: number;
  regionWidth: number;
  regionHeight: number;
  /** Whether this Port Pill is in the editor's port-selection set. */
  selected: boolean;
  onRemove: () => void;
  onRename: (name: string) => void;
  onDragY: (y: number) => void;
  /** Click handler — additive=true for shift/ctrl/meta-click. */
  onSelect: (additive: boolean) => void;
}

/**
 * Frame port renderer for the expanded subpatch view. Mirrors
 * RegionFramePort but used for subpatches. The outer/inner pin roles
 * match the region scheme so the existing connection-rules and the
 * compile-side port lookup keep working.
 */
function SubpatchFramePort({ port, side, top, regionWidth, regionHeight, selected, onRemove, onRename, onDragY, onSelect }: SubpatchFramePortProps) {
  const color = portColorVar(port.type);
  const isInput = port.direction === 'input';
  const outerHandleId = isInput ? `in-${port.id}` : `out-${port.id}`;
  const outerHandleType: 'target' | 'source' = isInput ? 'target' : 'source';
  const innerHandleId = isInput ? `out-${port.id}` : `in-${port.id}`;
  const innerHandleType: 'target' | 'source' = isInput ? 'source' : 'target';

  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(port.name);
  const inputRef = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);
  const commit = useCallback(() => {
    setEditing(false);
    if (draft.trim() && draft !== port.name) onRename(draft.trim());
  }, [draft, port.name, onRename]);

  // Pins centred on the Port Pill's rounded ends — inset by half the
  // pill's height so the pin lands on the apex of the semicircle. See
  // RegionFramePort for the equivalent geometry note.
  const halfPill = PORT_TAB_HEIGHT / 2;
  const outerPinLeft = side === 'left'
    ? PORT_TAB_INSET + halfPill
    : regionWidth - PORT_TAB_INSET - halfPill;
  const innerPinLeft = side === 'left'
    ? PORT_TAB_INSET + PORT_TAB_WIDTH - halfPill
    : regionWidth - PORT_TAB_INSET - PORT_TAB_WIDTH + halfPill;
  const tabLeft = side === 'left' ? PORT_TAB_INSET : regionWidth - PORT_TAB_WIDTH - PORT_TAB_INSET;

  // Right-click suppresses the browser menu but doesn't delete — use
  // Del/Backspace on a selected pill instead. See RegionFramePort.
  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };
  void onRemove;

  // Click (no movement) → select / toggle selection. Drag past the
  // threshold → reorder via y. Same scheme as RegionFramePort so the
  // two pill flavours behave identically.
  const DRAG_THRESHOLD_PX = 3;
  const [dragging, setDragging] = useState(false);
  const handleReorderMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    if (editing) return;
    e.stopPropagation();
    e.preventDefault();
    const startClientY = e.clientY;
    const startClientX = e.clientX;
    const startTop = top;
    const additive = e.shiftKey || e.ctrlKey || e.metaKey;
    const vp = (e.currentTarget as HTMLElement).closest('.react-flow__viewport') as HTMLElement | null;
    const m = vp ? new DOMMatrixReadOnly(getComputedStyle(vp).transform) : null;
    const zoom = m && m.a ? m.a : 1;
    let crossed = false;
    const onMouseMove = (ev: MouseEvent) => {
      const rawDy = ev.clientY - startClientY;
      const rawDx = ev.clientX - startClientX;
      if (!crossed && Math.hypot(rawDx, rawDy) >= DRAG_THRESHOLD_PX) {
        crossed = true;
        setDragging(true);
      }
      if (!crossed) return;
      const dy = rawDy / zoom;
      const next = Math.max(8, Math.min(regionHeight - 8, startTop + dy));
      onDragY(next);
    };
    const onMouseUp = () => {
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
      if (!crossed) onSelect(additive);
      setDragging(false);
    };
    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
  };

  return (
    <>
      <div
        className={`region-port-tab region-port-tab-${side} region-port-tab-${port.direction}${dragging ? ' dragging' : ''}${selected ? ' selected' : ''} nodrag nopan`}
        style={{
          top: top - PORT_TAB_HEIGHT / 2,
          left: tabLeft,
          width: PORT_TAB_WIDTH,
          height: PORT_TAB_HEIGHT,
          borderColor: color,
        }}
        title={`${port.name} (${port.type}) — click to select, drag to reorder, right-click to remove, double-click to rename, Del to delete`}
        onContextMenu={handleContextMenu}
        // Capture-phase pointerdown so React Flow never sees this event
        // and selects the enclosing subpatch node. Without this the
        // pill click would select both the pill and the frame.
        onPointerDownCapture={(e) => e.stopPropagation()}
        onClick={(e) => e.stopPropagation()}
        onMouseDown={handleReorderMouseDown}
        onDoubleClick={(e) => {
          e.stopPropagation();
          setDraft(port.name);
          setEditing(true);
        }}
      >
        {editing ? (
          <input
            ref={inputRef}
            className="region-port-input nodrag"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commit();
              if (e.key === 'Escape') {
                setDraft(port.name);
                setEditing(false);
              }
            }}
          />
        ) : (
          <span className="region-port-name">{port.name}</span>
        )}
      </div>
      <Handle
        id={outerHandleId}
        type={outerHandleType}
        position={side === 'left' ? Position.Left : Position.Right}
        className="region-handle region-handle-outer"
        style={{ background: color, borderColor: color, top, left: outerPinLeft }}
        onContextMenu={handleContextMenu}
      />
      <Handle
        id={innerHandleId}
        type={innerHandleType}
        position={side === 'left' ? Position.Right : Position.Left}
        className="region-handle region-handle-inner"
        style={{ background: color, borderColor: color, top, left: innerPinLeft }}
        onContextMenu={handleContextMenu}
      />
    </>
  );
}
