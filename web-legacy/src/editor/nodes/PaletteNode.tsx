import { memo, useCallback, useMemo } from 'react';
import { type NodeProps } from '@xyflow/react';
import { useEditor, type FloretNodeData } from '../editor-state';
import { nodeRegistry } from '../../nodes/registry';
import { getResolvedInputs } from '../../nodes/types';
import { useNodeParam } from '../useNodeParam';
import { InputPin, OutputPin } from './GenericNode';
import { parseHandle } from '../connection-rules';
import type { PortSpec } from '../../nodes/types';

/**
 * Palette node — N (input pin + inline hex swatch) rows plus a spread
 * output. `+` / `−` grow / shrink the stop count, mirroring GroupNode.
 *
 * Each pin reuses the editor's standard `InputPin` so it gets the
 * normal hover-tooltip + upstream value resolution. Output uses
 * `OutputPin`, which also handles the spread-pin yellow colour + the
 * "value · spread" tooltip suffix automatically.
 */

const MIN_STOPS = 1;
const MAX_STOPS = 64;
const DEFAULT_STOPS = 4;

export const PaletteNode = memo(function PaletteNode(props: NodeProps) {
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
  // Which stop pins on this node have an incoming wire? Same approach
  // as GenericNode — read as a stable string then memo into a Set so
  // InputPin's `wired` prop is stable per port.
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

  const stopCount = Math.max(
    MIN_STOPS,
    Math.min(MAX_STOPS, Math.floor(Number(nodeData.params.stopCount) || DEFAULT_STOPS)),
  );

  const addStop = useCallback(() => {
    if (stopCount >= MAX_STOPS) return;
    setNodeParam(id, 'stopCount', stopCount + 1);
  }, [id, stopCount, setNodeParam]);

  const removeStop = useCallback(() => {
    if (stopCount <= MIN_STOPS) return;
    setNodeParam(id, 'stopCount', stopCount - 1);
  }, [id, stopCount, setNodeParam]);

  if (!def) return null;

  const inputs = getResolvedInputs(def, nodeData.params);
  const output = def.outputs[0]!;

  return (
    <div
      className="node node-grid node-palette"
      data-selected={selected ? 1 : 0}
      data-category="color"
      data-warning={warningText ? 1 : 0}
    >
      <div className="node-header palette-header" title={def.description ?? def.type}>
        <span className="node-label">{def.label}</span>
        <span className="group-slot-controls nodrag">
          <button
            type="button"
            className="group-slot-btn"
            onClick={(e) => { e.stopPropagation(); removeStop(); }}
            disabled={stopCount <= MIN_STOPS}
            title="Remove last stop"
          >
            −
          </button>
          <span className="group-slot-count" title={`${stopCount} stops`}>
            {stopCount}
          </span>
          <button
            type="button"
            className="group-slot-btn"
            onClick={(e) => { e.stopPropagation(); addStop(); }}
            disabled={stopCount >= MAX_STOPS}
            title="Add a stop"
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
        {inputs.map((p) => (
          <PaletteStopRow
            key={p.name}
            nodeId={id}
            port={p}
            wired={connectedSet.has(p.name)}
          />
        ))}
        <div className="node-row palette-output-row">
          <span className="node-row-spacer" />
          <OutputPin nodeId={id} port={output} portIdx={0} />
        </div>
      </div>
    </div>
  );
});

/** One palette stop: input pin (with the standard hover tooltip) +
 *  inline hex picker. The picker stores `c<i>` as a hex string in the
 *  node's params; compute uses it as the fallback when the pin is
 *  unwired. */
function PaletteStopRow({
  nodeId, port, wired,
}: { nodeId: string; port: PortSpec; wired: boolean }) {
  const [hex, setLive, commit] = useNodeParam<string>(nodeId, port.name, '#ffffff');
  return (
    <div className="node-row palette-stop-row">
      <InputPin
        nodeId={nodeId}
        port={port}
        wired={wired}
        override={typeof hex === 'string' ? hex : undefined}
      />
      <input
        type="color"
        className="node-inline-input node-inline-color palette-stop-swatch nodrag"
        value={typeof hex === 'string' ? hex : '#ffffff'}
        onChange={(e) => setLive(e.target.value)}
        onBlur={commit}
        onPointerUp={commit}
        title={port.name}
      />
    </div>
  );
}
