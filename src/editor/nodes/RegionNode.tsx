import { Fragment, memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Handle, NodeResizer, Position, useUpdateNodeInternals, type NodeProps } from '@xyflow/react';
import { useEditor, type FloretNodeData, type RegionPortDescriptor } from '../editor-state';
import { nodeRegistry } from '../../nodes/registry';
import { portColorVar } from '../connection-rules';
import { getRegionStats } from '../../runtime/inspector';
import { LOOP_VAR_NAMES, loopVarHandle, type LoopVarName } from '../../runtime/compile';
import type { PortType } from '../../runtime/types';

/**
 * Region container — a translucent bordered rectangle that scopes a
 * sub-patch. Cross-boundary wires connect to PORTS rendered directly on
 * the frame's edges.
 *
 * Each frame port is one tab straddling the border, with two handle
 * pins at the ends:
 *
 *   Input  port (left edge):
 *     ●━━━━━━[name]━━━━━━●
 *     outer (target)      inner (source) — inside the frame
 *
 *   Output port (right edge):
 *     ●━━━━━━[name]━━━━━━●
 *     inner (target)      outer (source)
 *     (inside the frame)  (outside)
 *
 * The horizontal bar makes the inner→outer data flow visually obvious:
 * whatever wires connect to the inner pin (inside the frame) flows
 * through the bar and out the outer pin (and vice-versa).
 */
// ── Port Pill ──────────────────────────────────────────────────────────
// The draggable pill on a Region's frame edge — bridges an outer pin
// (sitting on the frame, faces the outer scope) and an inner pin
// (sitting on the inside edge of the pill, faces the inner scope).
// Drag the body up/down to reposition the port along the frame.
const PORT_TAB_WIDTH = 56;
const PORT_TAB_HEIGHT = 14;
const PORT_ROW_PITCH = 20;
const PORT_TOP_OFFSET = 14;
/** Where the tab itself begins, inset from the border. */
const PORT_TAB_INSET = 4;
// Top-left pin stack (static inputs like `delay` + built-in loop variables).
// One uniform compact column anchored near the body's top edge.
const TOP_PIN_START = 12;
const TOP_PIN_PITCH = 16;
// All top-pin pills share one fixed width so they line up exactly; the
// handle circle sits INSIDE the pill, inset from its near end (left for
// inputs, right for loop vars) so no dot pokes past the pill bounds. The
// per-side deltas absorb React Flow's horizontal handle-anchor offset so
// the dot lands at the intended inset (tuned against the live canvas).
// Match the frame-port pill's inset (PORT_TAB_INSET) so the top-pins line up
// on the same left edge as the vec2 pill below them. Keep in sync with the
// `left` value in `.region-toppin-label` (styles.css).
const TOP_PIN_LABEL_LEFT = PORT_TAB_INSET;
const TOP_PIN_LABEL_WIDTH = 72;
/** style.left for an input handle (Position.Left) → centred on the pill's
 *  left rounded end (halfPill = height/2 = 7px inset), like the frame pill. */
const TOP_PIN_HANDLE_LEFT = TOP_PIN_LABEL_LEFT + 7;
/** style.left for a loop-var handle (Position.Right) → centred on the pill's
 *  right rounded end (7px inset). */
const TOP_PIN_HANDLE_RIGHT = TOP_PIN_LABEL_LEFT + TOP_PIN_LABEL_WIDTH - 15;
// (Inner pin position is computed inline from PORT_TAB_INSET +
// PORT_TAB_WIDTH − halfPill, see RegionFramePort below.)

export const RegionNode = memo(function RegionNode(props: NodeProps) {
  const { id, data, selected } = props;
  const nodeData = data as unknown as FloretNodeData;
  const def = nodeRegistry.get(nodeData.defType);
  const setNodeParam = useEditor((s) => s.setNodeParam);
  const removeRegionPort = useEditor((s) => s.removeRegionPort);
  const renameRegionPort = useEditor((s) => s.renameRegionPort);
  const setRegionPortY = useEditor((s) => s.setRegionPortY);
  const selectRegionPort = useEditor((s) => s.selectRegionPort);
  // Stringify so the selector returns a primitive (avoids zustand v5
  // re-render storms when other state changes but the relevant
  // selection set is unchanged).
  const selectedSig = useEditor((s) =>
    s.selectedPortKeys.filter((k) => k.startsWith(`${id}#`)).join('|'),
  );
  const selectedKeys = useMemo(
    () => new Set(selectedSig ? selectedSig.split('|') : []),
    [selectedSig],
  );
  const warningText = useEditor((s) =>
    s.warnings
      .filter((w) => w.nodeIds?.includes(id))
      .map((w) => w.message)
      .join('\n'),
  );

  const onResizeEnd = useCallback(
    (_e: unknown, params: { width: number; height: number }) => {
      setNodeParam(id, 'width', params.width);
      setNodeParam(id, 'height', params.height);
    },
    [id, setNodeParam],
  );

  // Prefer React Flow's live width/height (updated every frame by
  // NodeResizer during a drag) over the persisted params. Without this,
  // the visible frame stays at its old size until onResizeEnd commits —
  // only the dashed resize-line moves, which reads as a stuck drag.
  // Params are still the fallback for the first render before RF has
  // measured the node.
  const w = (props.width as number | undefined) ?? (nodeData.params.width as number | undefined) ?? 480;
  const h = (props.height as number | undefined) ?? (nodeData.params.height as number | undefined) ?? 280;
  const label = (nodeData.params.label as string | undefined) ?? def?.label ?? 'Region';
  const kindClass = `region-${def?.regionKind ?? 'foreach'}`;
  const ports: RegionPortDescriptor[] = nodeData.ports ?? [];
  const inputPorts = ports.filter((p) => p.direction === 'input');
  const outputPorts = ports.filter((p) => p.direction === 'output');

  // Top-left pin stack: static NodeDef inputs (If.condition, ForEach.delay)
  // followed by the built-in loop variables (ForEach/Cache). Rendered as one
  // uniform compact column so labels share a size and spacing.
  const hasLoopVars = def?.regionKind === 'foreach' || def?.regionKind === 'cache';
  const topPins: Array<{ kind: 'in' | 'loopvar'; name: string; label: string; type: PortType }> = [
    ...(def?.inputs ?? []).map((p) => ({ kind: 'in' as const, name: p.name, label: p.label ?? p.name, type: p.type })),
    ...(hasLoopVars ? LOOP_VAR_NAMES.map((n) => ({ kind: 'loopvar' as const, name: n, label: n, type: 'number' as PortType })) : []),
  ];
  // Input frame ports must clear the top-pin block so they can't overlap it.
  const minInputTop = topPins.length ? TOP_PIN_START + topPins.length * TOP_PIN_PITCH : PORT_TOP_OFFSET;

  // React Flow caches each handle's position on mount; dynamic ports
  // need an explicit re-measure or wires render at stale coordinates.
  // The signature includes:
  //   - width  — so wires follow the right-edge handles as the user resizes
  //   - height — same, for any output port pinned to the bottom
  //   - each port's id, direction, AND y — so dragging a port tab up/down
  //     refreshes the handle's measured position and the connected wires
  //     follow it. Without `y` in the signature, the effect skips firing
  //     during a drag, so wires stay anchored to the port's old y.
  const updateNodeInternals = useUpdateNodeInternals();
  const portSig = `${w}|${h}|${ports.map((p) => `${p.id}.${p.direction}.${p.y ?? ''}`).join(',')}`;
  useEffect(() => {
    updateNodeInternals(id);
  }, [id, portSig, updateNodeInternals]);

  const onDoubleClick = useCallback((e: React.MouseEvent) => {
    // Open the node search at the cursor. Any node created from this
    // search will be auto-parented into this region (the parent is
    // detected by flow-position in NodeSearch.commit).
    //
    // Bail when the double-click belongs to a different node visually
    // sitting inside the region — those siblings have their own
    // .react-flow__node wrapper, so we just check the nearest one
    // matches us. Port pills swallow their own dblclick for renaming,
    // so we don't need to special-case them here.
    const t = e.target as HTMLElement;
    const nearestNode = t.closest('.react-flow__node') as HTMLElement | null;
    if (nearestNode && nearestNode.dataset.id !== id) return;
    e.stopPropagation();
    window.dispatchEvent(
      new CustomEvent('floret:open-search', { detail: { x: e.clientX, y: e.clientY } }),
    );
  }, [id]);

  return (
    <div
      className={`region-node ${kindClass}`}
      data-selected={selected ? 1 : 0}
      style={{ width: w, height: h }}
      onDoubleClick={onDoubleClick}
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
        <RegionIterationBadge nodeId={id} inputPorts={inputPorts} />
        {warningText && (
          <span className="node-warning-chip" title={warningText}>⚠</span>
        )}
        <span className="region-type">{def?.type ?? ''}</span>
      </div>

      {/* Top-left pin stack — static inputs (If.condition / ForEach.delay)
          then the built-in loop variables (index / count / fraction). One
          uniform compact column; loop vars are number-typed inner sources. */}
      {topPins.map((pin, i) => {
        const top = TOP_PIN_START + i * TOP_PIN_PITCH;
        const isIn = pin.kind === 'in';
        const color = isIn ? portColorVar(pin.type) : 'var(--port-number)';
        return (
          <Fragment key={`toppin-${pin.kind}-${pin.name}`}>
            <span
              className={`region-toppin-label region-toppin-label-${pin.kind}`}
              style={{ top }}
              title={isIn ? `${pin.label} (${pin.type})` : `${pin.label} (number) — per-iteration loop variable`}
            >
              {pin.label}
            </span>
            <Handle
              id={isIn ? `in-${pin.name}` : `out-${loopVarHandle(pin.name as LoopVarName)}`}
              type={isIn ? 'target' : 'source'}
              position={isIn ? Position.Left : Position.Right}
              className="region-handle region-handle-toppin"
              style={{ top, left: isIn ? TOP_PIN_HANDLE_LEFT : TOP_PIN_HANDLE_RIGHT, background: color, borderColor: color, color }}
            />
          </Fragment>
        );
      })}

      {inputPorts.map((p, i) => (
        <RegionFramePort
          key={p.id}
          port={p}
          side="left"
          top={Math.max(p.y ?? minInputTop + i * PORT_ROW_PITCH, minInputTop)}
          minTop={minInputTop}
          regionWidth={w}
          regionHeight={h}
          selected={selectedKeys.has(`${id}#${p.id}`)}
          onRemove={() => removeRegionPort(id, p.id)}
          onRename={(name) => renameRegionPort(id, p.id, name)}
          onDragY={(y) => setRegionPortY(id, p.id, y)}
          onSelect={(additive) => selectRegionPort(id, p.id, additive)}
        />
      ))}
      {outputPorts.map((p, i) => (
        <RegionFramePort
          key={p.id}
          port={p}
          side="right"
          top={p.y ?? PORT_TOP_OFFSET + i * PORT_ROW_PITCH}
          regionWidth={w}
          regionHeight={h}
          selected={selectedKeys.has(`${id}#${p.id}`)}
          onRemove={() => removeRegionPort(id, p.id)}
          onRename={(name) => renameRegionPort(id, p.id, name)}
          onDragY={(y) => setRegionPortY(id, p.id, y)}
          onSelect={(additive) => selectRegionPort(id, p.id, additive)}
        />
      ))}
    </div>
  );
});

interface RegionFramePortProps {
  port: RegionPortDescriptor;
  side: 'left' | 'right';
  top: number;
  /** Lowest y (px) this port may be dragged to — keeps input ports below
   *  the top-pin block so they can't overlap it. Defaults to 8. */
  minTop?: number;
  regionWidth: number;
  regionHeight: number;
  /** True when this Port Pill is in the editor's port-selection set. */
  selected: boolean;
  onRemove: () => void;
  onRename: (name: string) => void;
  onDragY: (y: number) => void;
  /** Add (additive=true → shift/ctrl-click) or replace (additive=false)
   *  this pill in the editor's port-selection set. */
  onSelect: (additive: boolean) => void;
}

function RegionFramePort({ port, side, top, minTop = 8, regionWidth, regionHeight, selected, onRemove, onRename, onDragY, onSelect }: RegionFramePortProps) {
  const color = portColorVar(port.type);
  const isInput = port.direction === 'input';

  // Pin roles:
  //   input  port: outer = target(in-),  inner = source(out-)
  //   output port: outer = source(out-), inner = target(in-)
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

  // Geometry: both pins sit at the CENTRE of the Port Pill's rounded
  // ends — inset by half the pill height from the pill's bounding-box
  // edge, so each pin lands on the apex of the semicircle rather than
  // hanging off it. Outer pin faces the parent scope; inner pin faces
  // the iteration body.
  const halfPill = PORT_TAB_HEIGHT / 2;
  const outerPinLeft = side === 'left'
    ? PORT_TAB_INSET + halfPill
    : regionWidth - PORT_TAB_INSET - halfPill;
  const innerPinLeft = side === 'left'
    ? PORT_TAB_INSET + PORT_TAB_WIDTH - halfPill
    : regionWidth - PORT_TAB_INSET - PORT_TAB_WIDTH + halfPill;
  const tabLeft = side === 'left' ? PORT_TAB_INSET : regionWidth - PORT_TAB_WIDTH - PORT_TAB_INSET;

  // Right-click on a pill: suppress the browser menu and React Flow's
  // pane menu, but DON'T auto-delete. Deletion is via Del/Backspace
  // after selecting the pill. (The earlier behaviour was a footgun —
  // a stray right-click would silently destroy ports.)
  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };
  // Suppress the "unused prop" complaint while keeping the removal
  // API stable in case future UI surfaces a context menu item.
  void onRemove;

  // Drag the tab vertically to reorder; mouseup without crossing the
  // drag threshold counts as a click → toggle/replace selection.
  // React Flow scales the canvas via CSS transform; we read the scale
  // from the closest .react-flow__viewport element so cursor movement
  // maps 1:1 to canvas pixels regardless of zoom.
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
    // Discover React Flow's zoom from the viewport transform matrix.
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
      const next = Math.max(minTop, Math.min(regionHeight - 8, startTop + dy));
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
      {/* The bridge tab — visually one port spanning the border. The pins
          are positioned at its two ends. */}
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
        // and selects the enclosing region node. Without this the pill
        // click would select both the pill and the frame.
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
        className={`region-handle region-handle-outer`}
        style={{
          background: color,
          borderColor: color,
          color,
          top,
          left: outerPinLeft,
        }}
        onContextMenu={handleContextMenu}
      />
      <Handle
        id={innerHandleId}
        type={innerHandleType}
        position={side === 'left' ? Position.Right : Position.Left}
        className={`region-handle region-handle-inner`}
        style={{
          background: color,
          borderColor: color,
          color,
          top,
          left: innerPinLeft,
        }}
        onContextMenu={handleContextMenu}
      />
    </>
  );
}

/**
 * Live "× N · capped by [port]" pill on the region header. Polls the
 * inspector for the latest tick's stats and updates local state only when
 * the displayed text changes — so it doesn't re-render every frame.
 */
function RegionIterationBadge({
  nodeId, inputPorts,
}: { nodeId: string; inputPorts: ReadonlyArray<RegionPortDescriptor> }) {
  const [label, setLabel] = useState<string>('');
  const [hint, setHint] = useState<string>('');
  useEffect(() => {
    let mounted = true;
    let lastLabel = '';
    let lastHint = '';
    let raf = 0;
    function tick() {
      if (!mounted) return;
      const stats = getRegionStats(nodeId);
      let nextLabel = '';
      let nextHint = '';
      if (stats) {
        nextLabel = `×${stats.iterations}`;
        if (stats.bottleneckIdx >= 0) {
          const bn = inputPorts[stats.bottleneckIdx];
          const lens = stats.inputLengths.map((l, i) => `${inputPorts[i]?.name ?? '?'}=${l}`).join(', ');
          nextHint = `capped by ${bn?.name ?? '?'} (${lens})`;
        } else if (stats.inputLengths.length > 0) {
          nextHint = stats.inputLengths.map((l, i) => `${inputPorts[i]?.name ?? '?'}=${l}`).join(', ');
        }
      }
      if (nextLabel !== lastLabel || nextHint !== lastHint) {
        lastLabel = nextLabel;
        lastHint = nextHint;
        setLabel(nextLabel);
        setHint(nextHint);
      }
      raf = requestAnimationFrame(tick);
    }
    raf = requestAnimationFrame(tick);
    return () => {
      mounted = false;
      if (raf) cancelAnimationFrame(raf);
    };
  }, [nodeId, inputPorts]);
  if (!label) return null;
  const capped = hint.startsWith('capped');
  return (
    <span className={`region-iter-badge${capped ? ' capped' : ''}`} title={hint}>
      {label}
      {capped && <span className="region-iter-cap">⚠</span>}
    </span>
  );
}
