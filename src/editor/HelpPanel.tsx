import { useMemo } from 'react';
import { useEditor } from './editor-state';
import { nodeRegistry } from '../nodes/registry';
import { getResolvedInputs } from '../nodes/types';
import { portColorVar } from './connection-rules';
import { Panel } from './panel/Panel';

/**
 * Help panel — live documentation for the currently selected node.
 * Hosted in a `Panel` so the user can dock, float, resize, or collapse
 * it with the same chrome as every other panel in the app.
 */
export function HelpPanel() {
  return (
    <Panel
      id="help"
      title="Node Help"
      defaults={{ dock: 'right', width: 280, height: 520, floatX: 320, floatY: 100 }}
    >
      <HelpPanelBody />
    </Panel>
  );
}

function HelpPanelBody() {
  const selectedNode = useEditor((s) => s.nodes.find((n) => n.selected));

  const data = useMemo(() => {
    if (!selectedNode) return null;
    const def = nodeRegistry.get(selectedNode.data.defType);
    if (!def) return null;
    const inputs = getResolvedInputs(def, selectedNode.data.params ?? {});
    return { def, inputs };
  }, [selectedNode]);

  if (!data) {
    return (
      <div className="help-panel-empty">
        <p>Select a node on the canvas to see its documentation.</p>
        <p className="help-panel-hint">
          Tip: hover any pin to peek at its live value.
        </p>
      </div>
    );
  }

  return (
    <div className="help-panel-body">
      <div className="help-panel-node-head">
        <span className="help-panel-label">{data.def.label}</span>
        <span className="help-panel-type">{data.def.type}</span>
      </div>
      <div className="help-panel-category">{data.def.category}</div>
      {data.def.description && (
        <p className="help-panel-desc">{data.def.description}</p>
      )}

      <Section title={`Inputs · ${data.inputs.length}`}>
        {data.inputs.length === 0 && <EmptyRow text="No inputs." />}
        {data.inputs.map((p, i) => (
          <PortRow
            key={`in-${i}-${p.name}`}
            name={p.label ?? p.name}
            type={p.type}
            kind={p.kind ?? 'single'}
            defaultValue={p.default}
          />
        ))}
      </Section>

      <Section title={`Outputs · ${data.def.outputs.length}`}>
        {data.def.outputs.length === 0 && <EmptyRow text="No outputs." />}
        {data.def.outputs.map((p, i) => (
          <PortRow
            key={`out-${i}-${p.name}`}
            name={p.label ?? p.name}
            type={p.type}
            kind={p.kind ?? 'single'}
          />
        ))}
      </Section>

      {data.def.regionKind && (
        <Section title="Region">
          <p className="help-panel-region-note">
            This is a <strong>{data.def.regionKind}</strong> region. Nodes
            placed inside its frame run in its scope. Drag wires onto the
            edge to add input/output ports.
          </p>
        </Section>
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="help-panel-section">
      <h4 className="help-panel-section-title">{title}</h4>
      <div className="help-panel-section-body">{children}</div>
    </section>
  );
}

function EmptyRow({ text }: { text: string }) {
  return <div className="help-panel-port-empty">{text}</div>;
}

function PortRow({
  name, type, kind, defaultValue,
}: {
  name: string;
  type: string;
  kind: 'single' | 'spread' | 'multi';
  defaultValue?: unknown;
}) {
  const color = portColorVar(type as never);
  return (
    <div className="help-panel-port">
      <span className="help-panel-pin" style={{ background: color, borderColor: color }} />
      <span className="help-panel-port-name">{name}</span>
      <span className="help-panel-port-type">
        {type}{kind !== 'single' && <span className="help-panel-port-kind"> · {kind}</span>}
      </span>
      {defaultValue !== undefined && (
        <span className="help-panel-port-default" title="default when unwired">
          {formatDefault(defaultValue)}
        </span>
      )}
    </div>
  );
}

function formatDefault(v: unknown): string {
  if (v === null) return 'null';
  if (typeof v === 'number') {
    return Number.isInteger(v) ? String(v) : v.toFixed(3).replace(/\.?0+$/, '');
  }
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (typeof v === 'string') return `"${v}"`;
  if (typeof v === 'object') {
    try {
      const s = JSON.stringify(v);
      return s.length > 24 ? s.slice(0, 22) + '…' : s;
    } catch { return '[obj]'; }
  }
  return String(v);
}
