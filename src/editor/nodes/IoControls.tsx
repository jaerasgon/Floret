import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { nodeRegistry } from '../../nodes/registry';
import { portColorVar } from '../connection-rules';
import { useNodeParam } from '../useNodeParam';
import { fireBang } from '../../nodes/io/bang-bus';
import type { FloretNodeData } from '../editor-state';

/** Compact node shell — same chrome as the math nodes: header (carries the
 *  name) + pin rows, no bulky body. */
function Shell({
  id, def, selected, children,
}: {
  id: string; def: ReturnType<typeof nodeRegistry.get>; selected: boolean; children: React.ReactNode;
}) {
  void id;
  return (
    <div className="node node-grid" data-selected={selected ? 1 : 0} data-category="io" data-compact={def?.compact ? 1 : 0} title={def?.description ?? def?.type}>
      <div className="node-header">
        <span className="node-label">{def?.label}</span>
        <span className="node-type">{def?.type}</span>
      </div>
      <div className="node-rows">{children}</div>
    </div>
  );
}

/** The right-side output pin, styled by port type (matches GenericNode). */
function OutPin({ name, type }: { name: string; type: 'boolean' | 'number' | 'string' | 'any' }) {
  return (
    <>
      <span className="node-row-spacer" />
      <span className="node-row-label node-row-label-right">{name}</span>
      <Handle
        id={`out-${name}`}
        type="source"
        position={Position.Right}
        className="node-row-handle node-row-handle-right"
        style={{ background: portColorVar(type), borderColor: portColorVar(type), color: portColorVar(type) }}
      />
    </>
  );
}

function useDef(data: unknown) {
  return nodeRegistry.get((data as FloretNodeData).defType);
}

/** Bang — a filled dot you click to fire a one-tick trigger. */
export const BangNode = memo(function BangNode({ id, data, selected }: NodeProps) {
  const def = useDef(data);
  return (
    <Shell id={id} def={def} selected={selected}>
      <div className="node-row">
        <button className="io-shape io-bang nodrag" onClick={() => fireBang(id)} title="Fire a one-tick trigger" />
        <OutPin name={def?.outputs[0]?.name ?? 'bang'} type="boolean" />
      </div>
    </Shell>
  );
});

/** Button — a dot that's filled while held. */
export const ButtonNode = memo(function ButtonNode({ id, data, selected }: NodeProps) {
  const def = useDef(data);
  const [value, setLive, commit] = useNodeParam<boolean>(id, 'value', false);
  const press = () => { setLive(true); commit(); };
  const release = () => { if (value) { setLive(false); commit(); } };
  return (
    <Shell id={id} def={def} selected={selected}>
      <div className="node-row">
        <button
          className="io-shape io-momentary nodrag"
          data-on={value ? 1 : 0}
          onPointerDown={press}
          onPointerUp={release}
          onPointerLeave={release}
          title="True while held"
        />
        <OutPin name={def?.outputs[0]?.name ?? 'bool'} type="boolean" />
      </div>
    </Shell>
  );
});

/** Stepper — − [n] + */
export const StepperNode = memo(function StepperNode({ id, data, selected }: NodeProps) {
  const def = useDef(data);
  const [value, setLive, commit] = useNodeParam<number>(id, 'value', 0);
  const [min] = useNodeParam<number>(id, 'min', -Infinity);
  const [max] = useNodeParam<number>(id, 'max', Infinity);
  const [step] = useNodeParam<number>(id, 'step', 1);
  const nudge = (dir: number) => { setLive(Math.max(min, Math.min(max, value + dir * (step || 1)))); commit(); };
  return (
    <Shell id={id} def={def} selected={selected}>
      <div className="node-row">
        <span className="io-stepper nodrag">
          <button className="io-step-btn" onClick={() => nudge(-1)}>−</button>
          <span className="io-step-val">{value}</span>
          <button className="io-step-btn" onClick={() => nudge(1)}>+</button>
        </span>
        <OutPin name={def?.outputs[0]?.name ?? 'number'} type="number" />
      </div>
    </Shell>
  );
});

/** Select — a compact dropdown; options are a comma-separated list. */
export const SelectNode = memo(function SelectNode({ id, data, selected }: NodeProps) {
  const def = useDef(data);
  const [options, setOptions, commitOptions] = useNodeParam<string>(id, 'options', 'a,b,c');
  const [value, setLive, commit] = useNodeParam<string>(id, 'value', '');
  const list = options.split(',').map((s) => s.trim()).filter(Boolean);
  return (
    <Shell id={id} def={def} selected={selected}>
      <div className="node-row">
        <select className="io-select nodrag" value={value} onChange={(e) => { setLive(e.target.value); commit(); }}>
          {!list.includes(value) && <option value={value}>{value || '—'}</option>}
          {list.map((o) => <option key={o} value={o}>{o}</option>)}
        </select>
        <OutPin name={def?.outputs[0]?.name ?? 'value'} type="string" />
      </div>
      <div className="node-row">
        <input
          className="io-select-opts nodrag"
          value={options}
          placeholder="comma,separated"
          onChange={(e) => setOptions(e.target.value)}
          onBlur={commitOptions}
        />
      </div>
    </Shell>
  );
});
