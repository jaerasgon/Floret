import { memo, useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { useShallow } from 'zustand/react/shallow';
import { nodeRegistry } from '../../nodes/registry';
import { getResolvedInputs, getResolvedOutputs, type PortSpec } from '../../nodes/types';
import { getUpstreamTypes, parseHandle, portColorVar } from '../connection-rules';
import { useEditor, type FloretNodeData } from '../editor-state';
import { useNodeParam } from '../useNodeParam';
import { openPortValueEditor } from '../PortValueEditor';
import { ScrubInput } from '../ScrubInput';
import { ValueTooltip } from '../ValueTooltip';
import { createPortal } from 'react-dom';
import type { PortType } from '../../runtime/types';
import { syntheticSplicerId } from '../../runtime/compile';

/**
 * Default node visual. Rows are PAIRED: left side carries either a pure
 * param (label + editor) or an input port (pin + label + editor); right
 * side carries an output port (label + pin), aligned to the SAME row.
 * If there are more inputs than outputs (or vice-versa), extra rows
 * appear with only one side populated.
 *
 *   ● text  [ Hello ]                out ●
 *   ● x
 *   ● y
 *   ● size  [   16  ]
 *   ● align [ left  ]
 *   ● style
 */
export const GenericNode = memo(function GenericNode(props: NodeProps) {
  const { id, data, selected } = props;
  const nodeData = data as unknown as FloretNodeData;
  const def = nodeRegistry.get(nodeData.defType);

  // Which input ports on this node have an incoming wire?
  const connectedInputPorts = useEditor((s) => {
    const names: string[] = [];
    for (const e of s.edges) {
      if (e.target !== id) continue;
      const parsed = parseHandle(e.targetHandle);
      if (parsed?.kind === 'in') names.push(parsed.name);
    }
    return names.sort().join(',');
  });

  // Compile warnings that reference this node — surface as an inline chip.
  // Join into a single string so the selector returns a primitive and
  // doesn't trigger re-renders on unrelated state changes.
  const warningText = useEditor((s) =>
    s.warnings
      .filter((w) => w.nodeIds?.includes(id))
      .map((w) => w.message)
      .join('\n'),
  );
  const connectedSet = useMemo(
    () => new Set(connectedInputPorts.split(',').filter(Boolean)),
    [connectedInputPorts],
  );

  // Per-port editor-expanded state. Input port labels are clickable
  // toggles — collapsed by default, click to reveal the inline editor.
  // Lives in component state (per-mount); resets if the patch reloads,
  // which is fine — the underlying value is preserved.
  const [expandedPorts, setExpandedPorts] = useState<Set<string>>(() => new Set());
  const togglePort = (portName: string) => {
    setExpandedPorts((prev) => {
      const next = new Set(prev);
      if (next.has(portName)) next.delete(portName);
      else next.add(portName);
      return next;
    });
  };

  if (!def) {
    return (
      <div className="node node-error" data-selected={selected ? 1 : 0}>
        <div className="node-header">Unknown · {nodeData.defType}</div>
      </div>
    );
  }

  const resolvedInputs = getResolvedInputs(def, nodeData.params);
  const portParamKeys = new Set(
    resolvedInputs.map((p) => p.param).filter((x): x is string => !!x),
  );

  // Left column items: pure params first (no pin), then input ports.
  type LeftItem =
    | { kind: 'param'; key: string; defaultVal: unknown }
    | { kind: 'input'; port: PortSpec };
  const pureParamItems: LeftItem[] = Object.entries(def.defaultParams ?? {})
    .filter(([key]) => !portParamKeys.has(key))
    .map(([key, defaultVal]) => ({ kind: 'param' as const, key, defaultVal }));
  const inputItems: LeftItem[] = resolvedInputs.map((port) => ({ kind: 'input' as const, port }));
  const leftItems: LeftItem[] = [...pureParamItems, ...inputItems];
  // Resolve outputs *after* inputs so polymorphic nodes (Repeat, Sort, …)
  // can read upstream types and echo them on their output ports.
  //
  // Two-stage memoization to avoid an infinite re-render loop:
  //   1. The Zustand selector returns a STABLE STRING ("value:color,b:number")
  //      so identity comparison settles between frames.
  //   2. useMemo rebuilds the actual PortSpec[] only when that string flips.
  // The previous shape — selector returning a fresh array on every call —
  // looked fine in dev hot-reload but black-screened on a clean cold-start
  // because Zustand compares by reference and re-rendered forever.
  const upstreamTypesKey = useEditor((s) => {
    if (!def.dynamicOutputs) return '';
    const up = getUpstreamTypes(id, s.edges, s.nodes);
    return Object.keys(up).sort().map((k) => `${k}:${up[k]}`).join(',');
  });
  const resolvedOutputs = useMemo(() => {
    if (!def.dynamicOutputs) return def.outputs;
    const up: Record<string, PortType | undefined> = {};
    for (const pair of upstreamTypesKey.split(',').filter(Boolean)) {
      const [k, v] = pair.split(':');
      if (k && v) up[k] = v as PortType;
    }
    return getResolvedOutputs(def, nodeData.params, up);
  }, [def, nodeData.params, upstreamTypesKey]);
  // Outputs pair with rows starting from row 0 — so single-output
  // source nodes (Number, Time) sit on a single line `value [N] | out`.
  const numRows = Math.max(leftItems.length, resolvedOutputs.length);

  // Math nodes (math/add, math/mul, math/sin, …) get a compact visual
  // since they're the most-used "glue" nodes in any patch. We hide the
  // type subtitle, optionally replace the label with a symbol, and let
  // CSS shrink padding / font size via the `data-math` selector.
  const isMath = def.type.startsWith('math/');
  const mathSymbol = isMath ? MATH_SYMBOLS[def.type] : undefined;
  // Read/Write pair nodes (state + region/accumulator) carry a useful
  // `#slot` badge that already disambiguates them. The long type
  // subtitle (`region/accumulator-write/number`, etc.) just stretches
  // the header — suppress it so the node sizes to its body.
  const isReadWritePair =
    def.type === 'state/read' ||
    def.type === 'state/write' ||
    def.type === 'state/read-spread' ||
    def.type === 'state/write-spread' ||
    def.type.startsWith('region/accumulator-read/') ||
    def.type.startsWith('region/accumulator-write/');
  const hideTypeSubtitle = isMath || isReadWritePair;
  return (
    <div
      className="node node-grid"
      data-selected={selected ? 1 : 0}
      data-category={def.category.toLowerCase()}
      data-warning={warningText ? 1 : 0}
      data-math={isMath ? 1 : 0}
    >
      <div className="node-header" title={def.description ?? def.type}>
        <span className="node-label">{mathSymbol ?? def.label}</span>
        {nodeData.params?.slot !== undefined && (
          <span
            className="node-slot-badge"
            title={`Slot #${nodeData.params.slot} — pairs with the matching Read/Write at this slot.`}
          >
            #{String(nodeData.params.slot)}
          </span>
        )}
        {warningText && (
          <span className="node-warning-chip" title={warningText}>⚠</span>
        )}
        {!hideTypeSubtitle && <span className="node-type">{def.type}</span>}
      </div>
      <div className="node-rows">
        {Array.from({ length: numRows }, (_, i) => {
          const left = leftItems[i];
          const right = resolvedOutputs[i];
          const rightIdx = i;
          return (
            <NodeRow
              key={i}
              nodeId={id}
              left={left}
              right={right}
              rightIdx={rightIdx}
              connectedSet={connectedSet}
              portValues={nodeData.portValues}
              paramOptions={def.paramOptions}
              dynamicOptions={def.dynamicOptions}
              expandedPorts={expandedPorts}
              togglePort={togglePort}
            />
          );
        })}
      </div>
    </div>
  );
});

interface NodeRowProps {
  nodeId: string;
  left: { kind: 'param'; key: string; defaultVal: unknown } | { kind: 'input'; port: PortSpec } | undefined;
  right: PortSpec | undefined;
  rightIdx: number;
  connectedSet: Set<string>;
  portValues: Record<string, unknown> | undefined;
  paramOptions: Record<string, ReadonlyArray<string>> | undefined;
  /** Optional live-resolved dropdown sources — see NodeDef.dynamicOptions. */
  dynamicOptions: import('../../nodes/types').NodeDef['dynamicOptions'];
  expandedPorts: Set<string>;
  togglePort: (portName: string) => void;
}

function NodeRow({
  nodeId, left, right, rightIdx, connectedSet, portValues, paramOptions, dynamicOptions, expandedPorts, togglePort,
}: NodeRowProps) {
  const leftIsInput = left?.kind === 'input';
  const port = leftIsInput ? left.port : undefined;
  const wired = port ? connectedSet.has(port.name) : false;
  const override = port ? portValues?.[port.name] : undefined;
  // What kind of inline editor (if any) exists for this row:
  //   - param row: always available (no pin, value lives in params)
  //   - input row with PortSpec.param: paramStore-backed (live)
  //   - input row of editable type without param: portValues-backed
  let editorMode: 'none' | 'param' | 'portValue' = 'none';
  if (left?.kind === 'param') editorMode = 'param';
  else if (left?.kind === 'input' && !wired && supportsInlineEditor(left.port.type)) {
    editorMode = left.port.param ? 'param' : 'portValue';
  }
  // Pure params show their editor always (they ARE the configuration).
  // Input ports start collapsed — click the label to reveal the editor.
  const isExpanded = port ? expandedPorts.has(port.name) : true;
  const showEditor = editorMode !== 'none' && isExpanded;
  const showReadout = port && override !== undefined && !wired && !showEditor;
  const labelClickable = !!port && editorMode !== 'none';

  const onContextMenu = (e: React.MouseEvent) => {
    if (!port) return;
    e.preventDefault();
    e.stopPropagation();
    openPortValueEditor({
      nodeId,
      portName: port.name,
      portType: port.type,
      initialValue: override !== undefined ? override : port.default,
      screenPosition: { x: e.clientX, y: e.clientY },
    });
  };

  return (
    <div
      className={`node-row${wired ? ' wired' : ''}${showReadout ? ' has-port-value' : ''}`}
      onContextMenu={port ? onContextMenu : undefined}
    >
      {/* Left side: pin (if input) + label + editor */}
      {left && (
        <>
          {leftIsInput && (
            <InputPin
              nodeId={nodeId}
              port={left.port}
              wired={wired}
              override={override}
            />
          )}
          <span
            className={`node-row-label node-row-label-left${labelClickable ? ' clickable' : ''}${isExpanded && labelClickable ? ' expanded' : ''}`}
            onClick={(e) => {
              if (!labelClickable || !port) return;
              e.stopPropagation();
              togglePort(port.name);
            }}
          >
            {left.kind === 'input' ? (left.port.label ?? left.port.name) : left.key}
            {left.kind === 'input' && left.port.constant && (
              <span className="port-flag-constant" title="Set once at compile — won't react per tick">C</span>
            )}
          </span>
          {showEditor && editorMode === 'param' && left.kind === 'input' && (
            <InlineEditor
              nodeId={nodeId}
              paramKey={left.port.param ?? left.port.name}
              type={left.port.type}
              defaultVal={left.port.default}
              options={paramOptions?.[left.port.param ?? left.port.name]}
              dynamic={dynamicOptions?.[left.port.param ?? left.port.name]}
            />
          )}
          {editorMode === 'param' && left.kind === 'param' && (
            <InlineEditor
              nodeId={nodeId}
              paramKey={left.key}
              type={inferTypeFromValue(left.defaultVal)}
              defaultVal={left.defaultVal}
              options={paramOptions?.[left.key]}
              dynamic={dynamicOptions?.[left.key]}
            />
          )}
          {showEditor && editorMode === 'portValue' && left.kind === 'input' && (
            <PortValueInlineEditor
              nodeId={nodeId}
              portName={left.port.name}
              type={left.port.type}
              defaultVal={override !== undefined ? override : left.port.default}
            />
          )}
          {showReadout && (
            <span className="port-value-readout">{formatValue(override)}</span>
          )}
        </>
      )}

      {/* Spacer pushes the output side to the right edge. */}
      <span className="node-row-spacer" />

      {/* Right side: output label + pin (with hover-to-inspect) */}
      {right && <OutputPin nodeId={nodeId} port={right} portIdx={rightIdx} />}
    </div>
  );
}

// ─── Inline editors ──────────────────────────────────────────────────────

function supportsInlineEditor(type: PortType): boolean {
  return type === 'number' || type === 'string' || type === 'boolean';
}

function inferTypeFromValue(v: unknown): PortType {
  if (typeof v === 'number') return 'number';
  if (typeof v === 'boolean') return 'boolean';
  return 'string';
}

interface InlineEditorProps {
  nodeId: string;
  paramKey: string;
  type: PortType;
  defaultVal: unknown;
  options?: ReadonlyArray<string>;
  /** Live-resolved dropdown source. Takes precedence over `options`. */
  dynamic?: import('../../nodes/types').NodeDef['dynamicOptions'] extends Record<string, infer V> | undefined ? V : never;
}

function InlineEditor({ nodeId, paramKey, type, defaultVal, options, dynamic }: InlineEditorProps) {
  if (dynamic) {
    return <DynamicSelectEditor nodeId={nodeId} paramKey={paramKey} defaultVal={String(defaultVal ?? '')} source={dynamic} />;
  }
  if (options && options.length > 0) {
    return <SelectEditor nodeId={nodeId} paramKey={paramKey} defaultVal={String(defaultVal ?? '')} options={options} />;
  }
  if (type === 'number') {
    return <NumberEditor nodeId={nodeId} paramKey={paramKey} defaultVal={Number(defaultVal ?? 0)} />;
  }
  if (type === 'boolean') {
    return <BoolEditor nodeId={nodeId} paramKey={paramKey} defaultVal={Boolean(defaultVal)} />;
  }
  return <StringEditor nodeId={nodeId} paramKey={paramKey} defaultVal={String(defaultVal ?? '')} />;
}

/**
 * Input pin — Handle plus a hover-to-inspect tooltip. Resolves the
 * tooltip value by either:
 *   - Following the connected edge back to its source's output (live, polled).
 *   - Reading the local portValues override (static).
 *   - Falling back to PortSpec.default (static).
 *
 * The hover behaviour mirrors OutputPin so input and output inspection
 * feel like one feature.
 */
export function InputPin({
  nodeId, port, wired, override,
}: { nodeId: string; port: PortSpec; wired: boolean; override: unknown }) {
  const [hoverPos, setHoverPos] = useState<{ x: number; y: number } | null>(null);
  // Resolve upstream source (nodeId + portIdx) when wired — read once on
  // hover-open so the tooltip can poll live values.
  //
  // Region inner-source pins (`out-<portId>` on a region whose port is
  // direction='input') don't appear in the region's runtime outputs;
  // they correspond to a synthesized splicer-in node. We rewrite the
  // lookup to target that splicer's id, port 0.
  //
  // `useShallow` collapses fresh-object selector returns to "equal" so
  // this doesn't trigger a re-render on every editor state change.
  const sourceRef = useEditor(
    useShallow((s) => {
      if (!wired) return null;
      const edge = s.edges.find((e) => {
        if (e.target !== nodeId) return false;
        return parseHandle(e.targetHandle)?.name === port.name;
      });
      if (!edge || !edge.source) return null;
      const srcNode = s.nodes.find((n) => n.id === edge.source);
      if (!srcNode) return null;
      const srcDef = nodeRegistry.get((srcNode.data as FloretNodeData).defType);
      if (!srcDef) return null;
      const srcHandle = parseHandle(edge.sourceHandle);
      if (!srcHandle) return null;
      if (srcDef.regionKind) {
        const ports = (srcNode.data as FloretNodeData).ports ?? [];
        const matchingPort = ports.find((p) => p.id === srcHandle.name);
        if (matchingPort) {
          return {
            srcId: syntheticSplicerId(edge.source, matchingPort.id, matchingPort.direction),
            srcPortIdx: 0,
          };
        }
      }
      const srcPortIdx = srcDef.outputs.findIndex((p) => p.name === srcHandle.name);
      if (srcPortIdx < 0) return null;
      return { srcId: edge.source, srcPortIdx };
    }),
  );

  const onEnter = (e: React.MouseEvent) => {
    const rect = e.currentTarget.getBoundingClientRect();
    setHoverPos({ x: rect.left - 220, y: rect.top - 6 });
  };
  const onLeave = () => setHoverPos(null);

  // Static fallback when we can't resolve a source (unwired, or wired to
  // something we can't introspect — region passthroughs, splicers).
  const staticValue = override !== undefined ? override : port.default;

  return (
    <>
      <Handle
        id={`in-${port.name}`}
        type="target"
        position={Position.Left}
        className={`node-row-handle node-row-handle-left${port.kind === 'spread' ? ' node-row-handle-spread' : ''}${port.constant ? ' node-row-handle-constant' : ''}`}
        title={port.constant ? `${port.name} — constant (set once at compile)` : undefined}
        style={{
          // Spread pins paint yellow so they're immediately identifiable
          // regardless of port type — type info moves to the tooltip.
          background: port.kind === 'spread' ? 'var(--pin-spread)' : portColorVar(port.type),
          borderColor: port.kind === 'spread' ? 'var(--pin-spread)' : portColorVar(port.type),
          // `color` matches the pin colour so the `:hover` glow halo
          // (defined via `currentColor` in styles.css) picks up the type.
          color: port.kind === 'spread' ? 'var(--pin-spread)' : portColorVar(port.type),
          // Constant pins dim slightly — "set once" reads visually distinct.
          opacity: port.constant ? 0.55 : undefined,
        }}
        onMouseEnter={onEnter}
        onMouseLeave={onLeave}
      />
      {hoverPos && sourceRef && createPortal(
        <ValueTooltip
          nodeId={sourceRef.srcId}
          portIdx={sourceRef.srcPortIdx}
          portName={port.label ?? port.name}
          portType={port.type}
          isSpread={port.kind === 'spread'}
          anchor={hoverPos}
          onClose={() => setHoverPos(null)}
        />,
        document.body,
      )}
      {hoverPos && !sourceRef && createPortal(
        <StaticValueTooltip
          portName={port.label ?? port.name}
          portType={port.type}
          isSpread={port.kind === 'spread'}
          value={staticValue}
          anchor={hoverPos}
        />,
        document.body,
      )}
    </>
  );
}

function StaticValueTooltip({
  portName, portType, isSpread, value, anchor,
}: { portName: string; portType: string; isSpread?: boolean; value: unknown; anchor: { x: number; y: number } }) {
  return (
    <div className="value-tooltip" style={{ left: anchor.x, top: anchor.y }}>
      <div className="value-tooltip-header">
        <span className="value-tooltip-port">{portName}</span>
        <span className="value-tooltip-type">
          {portType}{isSpread && <span className="value-tooltip-spread"> · spread</span>}
        </span>
      </div>
      <div className="value-tooltip-body">{formatStaticValue(value)}</div>
    </div>
  );
}

function formatStaticValue(v: unknown): string {
  if (v === undefined || v === null) return '—';
  if (typeof v === 'number') {
    return Number.isInteger(v) ? String(v) : v.toFixed(3).replace(/\.?0+$/, '');
  }
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (typeof v === 'string') return `"${v}"`;
  try { return JSON.stringify(v); } catch { return '[object]'; }
}

/**
 * Output pin row — label + Handle plus a hover-to-inspect tooltip
 * showing the live runtime value at this port. Tooltip is portaled
 * to <body> so it can escape the React Flow node clipping.
 */
export function OutputPin({
  nodeId, port, portIdx,
}: { nodeId: string; port: PortSpec; portIdx: number }) {
  const [hoverPos, setHoverPos] = useState<{ x: number; y: number } | null>(null);
  const onEnter = (e: React.MouseEvent) => {
    const rect = e.currentTarget.getBoundingClientRect();
    setHoverPos({ x: rect.right + 10, y: rect.top - 6 });
  };
  const onLeave = () => setHoverPos(null);
  return (
    <>
      <span
        className="node-row-label node-row-label-right inspectable"
        onMouseEnter={onEnter}
        onMouseLeave={onLeave}
      >
        {port.label ?? port.name}
      </span>
      <Handle
        id={`out-${port.name}`}
        type="source"
        position={Position.Right}
        className={`node-row-handle node-row-handle-right${port.kind === 'spread' ? ' node-row-handle-spread' : ''}`}
        style={{
          background: port.kind === 'spread' ? 'var(--pin-spread)' : portColorVar(port.type),
          borderColor: port.kind === 'spread' ? 'var(--pin-spread)' : portColorVar(port.type),
          color: port.kind === 'spread' ? 'var(--pin-spread)' : portColorVar(port.type),
        }}
        onMouseEnter={onEnter}
        onMouseLeave={onLeave}
      />
      {hoverPos && createPortal(
        <ValueTooltip
          nodeId={nodeId}
          portIdx={portIdx}
          portName={port.label ?? port.name}
          portType={port.type}
          isSpread={port.kind === 'spread'}
          anchor={hoverPos}
          onClose={() => setHoverPos(null)}
        />,
        document.body,
      )}
    </>
  );
}

function NumberEditor({ nodeId, paramKey, defaultVal }: { nodeId: string; paramKey: string; defaultVal: number }) {
  const [val, setLive, commit] = useNodeParam<number>(nodeId, paramKey, defaultVal);
  return (
    <ScrubInput
      value={val}
      step={1}
      className="node-inline-input nodrag"
      onLive={setLive}
      onCommit={commit}
    />
  );
}

function StringEditor({ nodeId, paramKey, defaultVal }: { nodeId: string; paramKey: string; defaultVal: string }) {
  const [val, setLive, commit] = useNodeParam<string>(nodeId, paramKey, defaultVal);
  const isHex = paramKey === 'hex';
  // For `hex` params we keep a TEXT input (so the user can type a hex
  // code directly — Cmd-V a #rrggbb, retype channels by hand, etc.) but
  // tint the input's background with the live value so it still reads
  // as a colour at a glance. We previously used `<input type="color">`
  // which opened the OS picker on click and made typing impossible.
  const isValidHex = isHex && /^#?[0-9a-fA-F]{6}$/.test(val);
  const swatchStyle = isHex && isValidHex
    ? {
        // Small left-side colour chip via a wide left border, so the
        // colour reads but the typing area still spans the input.
        borderLeftWidth: 10,
        borderLeftStyle: 'solid' as const,
        borderLeftColor: val.startsWith('#') ? val : `#${val}`,
      }
    : undefined;
  return (
    <input
      type="text"
      className={`node-inline-input nodrag${isHex ? ' node-inline-hex' : ''}`}
      value={val}
      onChange={(e) => setLive(e.target.value)}
      onBlur={commit}
      onPointerUp={commit}
      style={swatchStyle}
      spellCheck={false}
      autoCorrect="off"
      autoCapitalize="off"
    />
  );
}

function BoolEditor({ nodeId, paramKey, defaultVal }: { nodeId: string; paramKey: string; defaultVal: boolean }) {
  const [val, setLive, commit] = useNodeParam<boolean>(nodeId, paramKey, defaultVal);
  return (
    <input
      type="checkbox"
      className="node-inline-checkbox nodrag"
      checked={!!val}
      onChange={(e) => { setLive(e.target.checked); commit(); }}
    />
  );
}

function SelectEditor({
  nodeId, paramKey, defaultVal, options,
}: { nodeId: string; paramKey: string; defaultVal: string; options: ReadonlyArray<string> }) {
  const [val, setLive, commit] = useNodeParam<string>(nodeId, paramKey, defaultVal);
  return (
    <select
      className="node-inline-input node-inline-select nodrag"
      value={val}
      onChange={(e) => { setLive(e.target.value); commit(); }}
    >
      {options.map((opt) => (
        <option key={opt} value={opt}>{opt}</option>
      ))}
    </select>
  );
}

/**
 * Dropdown whose option list resolves at render time and updates when
 * the underlying source signals a change (e.g. a MIDI device connects
 * or disconnects). Mirrors SelectEditor but pulls options from
 * `source.getOptions()` via `useSyncExternalStore`.
 *
 * If the stored value isn't in the current option list (device gone),
 * we keep showing it as a synthetic "(disconnected)" entry so the
 * patch round-trips intact — the value persists and re-binds the
 * instant the device reconnects.
 */
function DynamicSelectEditor({
  nodeId, paramKey, defaultVal, source,
}: {
  nodeId: string;
  paramKey: string;
  defaultVal: string;
  source: { getOptions: () => ReadonlyArray<{ value: string; label: string }>; subscribe: (fn: () => void) => () => void };
}) {
  const [val, setLive, commit] = useNodeParam<string>(nodeId, paramKey, defaultVal);
  // useSyncExternalStore: subscribe to the source, re-render on changes.
  // The snapshot equality is by reference — `getOptions` should return
  // a fresh array only when the underlying list actually changed (or
  // identity-equal otherwise), but to be safe we let React do its own
  // diffing via the children. Returning the array directly works fine.
  const options = useSyncExternalStore(
    source.subscribe,
    source.getOptions,
    source.getOptions, // server snapshot — same as client; we're SPA-only
  );
  const hasCurrent = val === '' || options.some((o) => o.value === val);
  return (
    <select
      className="node-inline-input node-inline-select nodrag"
      value={val}
      onChange={(e) => { setLive(e.target.value); commit(); }}
    >
      <option value="">(any device)</option>
      {options.map((opt) => (
        <option key={opt.value} value={opt.value}>{opt.label}</option>
      ))}
      {!hasCurrent && (
        <option key="__missing" value={val}>{val} (disconnected)</option>
      )}
    </select>
  );
}

/**
 * Inline editor for input ports that DON'T declare PortSpec.param —
 * the value is stored in `node.data.portValues[portName]`. Uses local
 * React state during typing; commits on blur via setNodePortValue
 * (which recompiles). Edges always override portValues.
 */
function PortValueInlineEditor({
  nodeId, portName, type, defaultVal,
}: { nodeId: string; portName: string; type: PortType; defaultVal: unknown }) {
  const setNodePortValue = useEditor((s) => s.setNodePortValue);
  const initial =
    type === 'number' ? Number(defaultVal ?? 0)
    : type === 'boolean' ? Boolean(defaultVal ?? false)
    : String(defaultVal ?? '');
  const [local, setLocal] = useState(initial);
  // Re-sync local state when the stored value changes from elsewhere
  // (paste, undo, etc.).
  useEffect(() => { setLocal(initial); }, [initial]);

  const commit = (v: unknown) => {
    if (v === initial) return;
    setNodePortValue(nodeId, portName, v);
  };

  if (type === 'boolean') {
    return (
      <input
        type="checkbox"
        className="node-inline-checkbox nodrag"
        checked={!!local}
        onChange={(e) => { setLocal(e.target.checked); commit(e.target.checked); }}
      />
    );
  }
  if (type === 'number') {
    return (
      <ScrubInput
        value={local as number}
        step={1}
        className="node-inline-input nodrag"
        onLive={(v) => setLocal(v)}
        onCommit={() => commit(local)}
      />
    );
  }
  return (
    <input
      type="text"
      className="node-inline-input nodrag"
      value={local as string}
      onChange={(e) => setLocal(e.target.value)}
      onBlur={() => commit(local)}
    />
  );
}

function formatValue(v: unknown): string {
  if (typeof v === 'number') {
    return Number.isInteger(v) ? String(v) : v.toFixed(2).replace(/\.?0+$/, '');
  }
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (v === null || v === undefined) return '—';
  const s = String(v);
  return s.length > 10 ? s.slice(0, 9) + '…' : s;
}

/**
 * Operator-symbol overrides for math nodes — the header label is replaced
 * with the symbol for binary / unary operators where it reads cleaner
 * than the word. Multi-argument ops (Map, Lerp, Clamp, Norm) keep their
 * short label since a symbol would be cryptic.
 */
const MATH_SYMBOLS: Record<string, string> = {
  'math/add': '+',
  'math/sub': '−',
  'math/mul': '×',
  'math/div': '÷',
  'math/mod': 'mod',
  'math/pow': 'xʸ',
  'math/neg': '−x',
  'math/abs': '|x|',
  'math/floor': '⌊x⌋',
  'math/ceil':  '⌈x⌉',
  'math/round': '⌊x⌉',
  'math/fract': '{x}',
  'math/sqrt': '√',
  'math/sq': 'x²',
  'math/exp': 'eˣ',
  'math/log': 'ln',
  'math/sin': 'sin',
  'math/cos': 'cos',
  'math/tan': 'tan',
  'math/asin': 'asin',
  'math/acos': 'acos',
  'math/atan': 'atan',
  'math/atan2': 'atan2',
  'math/min': 'min',
  'math/max': 'max',
  'math/mag': '‖v‖',
  'math/dist': 'dist',
};
