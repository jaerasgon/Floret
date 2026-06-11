import { useEffect, useMemo, useRef, useState } from 'react';
import { Link2, Link2Off } from 'lucide-react';
import { useEditor } from './editor-state';
import { nodeRegistry } from '../nodes/registry';
import { getResolvedInputs } from '../nodes/types';
import { portColorVar } from './connection-rules';
import { Panel } from './panel/Panel';
import { usePrefs } from '../app/prefs';
import { Tooltip } from '../app/shell/Tooltip';

/**
 * Help panel — live documentation for whichever node is currently in
 * focus. "In focus" is one of:
 *
 *   1. The node selected on the canvas (when `autoJumpHelp` pref is ON)
 *   2. A node type the user explicitly picked via the search box
 *      (always wins for as long as auto-jump is OFF; gets overridden by
 *      the next canvas selection when auto-jump is back ON)
 *
 * Hosted in a `Panel` so it docks/floats/collapses with the same chrome
 * as the rest of the workspace.
 */
export function HelpPanel() {
  const autoJump = usePrefs((s) => s.autoJumpHelp);
  const setPref = usePrefs((s) => s.set);
  return (
    <Panel
      id="help"
      title="Help"
      defaults={{ dock: 'right', width: 280, height: 520, floatX: 320, floatY: 100 }}
      headerActions={
        <Tooltip
          label={autoJump
            ? 'Auto-track selected node — click to pin help'
            : 'Pinned — click to resume auto-tracking'}
        >
          <button
            type="button"
            className={`help-jump-toggle${autoJump ? ' active' : ''}`}
            onClick={() => setPref('autoJumpHelp', !autoJump)}
            aria-pressed={autoJump}
            aria-label={autoJump ? 'Auto-track on' : 'Auto-track off'}
          >
            {autoJump
              ? <Link2 size={12} strokeWidth={1.6} />
              : <Link2Off size={12} strokeWidth={1.6} />}
          </button>
        </Tooltip>
      }
    >
      <HelpPanelBody />
    </Panel>
  );
}

function HelpPanelBody() {
  const autoJump = usePrefs((s) => s.autoJumpHelp);
  const selectedNode = useEditor((s) => s.nodes.find((n) => n.selected));

  // `pickedDefType` is the user-pinned type from the search box. When
  // auto-jump is ON, every new canvas selection overwrites it; when
  // auto-jump is OFF, only the search box changes it.
  const [pickedDefType, setPickedDefType] = useState<string | null>(null);

  useEffect(() => {
    if (autoJump) {
      setPickedDefType(selectedNode ? selectedNode.data.defType : null);
    }
    // When auto-jump flips OFF, we deliberately leave pickedDefType
    // alone so the current view stays "pinned" until the user searches.
  }, [autoJump, selectedNode]);

  // Search query — non-empty means "show the results list instead of
  // the active doc". Picking a result both pins the type AND clears
  // the query, returning the panel to doc-view.
  const [query, setQuery] = useState('');

  const trimmed = query.trim().toLowerCase();
  const results = useMemo(() => {
    if (!trimmed) return [];
    const out: { type: string; label: string; category: string }[] = [];
    for (const def of nodeRegistry.all()) {
      if (def.hidden) continue;
      const hay = `${def.type} ${def.label} ${def.category}`.toLowerCase();
      if (hay.includes(trimmed)) {
        out.push({ type: def.type, label: def.label, category: def.category });
      }
    }
    // Most specific match first: prefer matches on the label, then type.
    out.sort((a, b) => {
      const aL = a.label.toLowerCase().startsWith(trimmed) ? 0 : 1;
      const bL = b.label.toLowerCase().startsWith(trimmed) ? 0 : 1;
      if (aL !== bL) return aL - bL;
      return a.label.localeCompare(b.label);
    });
    return out.slice(0, 64);
  }, [trimmed]);

  const inputRef = useRef<HTMLInputElement | null>(null);

  return (
    <div className="help-panel-wrap">
      <div className="help-panel-search-wrap">
        <input
          ref={inputRef}
          className="help-panel-search nodrag"
          type="search"
          placeholder="Search nodes…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') { setQuery(''); inputRef.current?.blur(); }
            else if (e.key === 'Enter' && results[0]) {
              setPickedDefType(results[0].type);
              setQuery('');
              inputRef.current?.blur();
            }
          }}
        />
      </div>
      {trimmed
        ? <HelpResultsList results={results} onPick={(t) => { setPickedDefType(t); setQuery(''); }} />
        : <HelpContent defType={pickedDefType} />}
    </div>
  );
}

function HelpResultsList({
  results, onPick,
}: {
  results: { type: string; label: string; category: string }[];
  onPick: (type: string) => void;
}) {
  if (results.length === 0) {
    return <div className="help-panel-empty"><p>No matches.</p></div>;
  }
  return (
    <ul className="help-panel-results">
      {results.map((r) => (
        <li key={r.type}>
          <button
            type="button"
            className="help-panel-result"
            onClick={() => onPick(r.type)}
          >
            <span className="help-panel-result-label">{r.label}</span>
            <span className="help-panel-result-cat">{r.category}</span>
            <span className="help-panel-result-type">{r.type}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}

function HelpContent({ defType }: { defType: string | null }) {
  const data = useMemo(() => {
    if (!defType) return null;
    const def = nodeRegistry.get(defType);
    if (!def) return null;
    // No node instance is required — we render against the def's
    // default-resolved inputs. Pass an empty params object; dynamic
    // ports show their default arity.
    const inputs = getResolvedInputs(def, {});
    return { def, inputs };
  }, [defType]);

  if (!data) {
    return (
      <div className="help-panel-empty">
        <p>Select a node on the canvas, or search above to look one up.</p>
        <p className="help-panel-hint">
          Tip: toggle the link icon in the header to pin help to a specific node.
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
