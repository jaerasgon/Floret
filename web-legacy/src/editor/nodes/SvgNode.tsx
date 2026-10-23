import { memo, useCallback, useMemo, useRef, useState } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { useEditor, type FloretNodeData } from '../editor-state';
import { nodeRegistry } from '../../nodes/registry';
import { getResolvedInputs } from '../../nodes/types';
import { portColorVar, parseHandle } from '../connection-rules';
import { InputPin, PortValueInlineEditor, supportsInlineEditor, formatValue } from './GenericNode';

/**
 * SVG loader node. A custom body offers three sources — paste markup,
 * pick / drag-drop a `.svg` file (persisted as a data URL), or a remote
 * URL — written into `data.params` where the NodeDef's compute reads
 * them. Below the body, input pins (style, scale) pair with output pins
 * (layer, width, height, loaded) in the standard row layout.
 *
 * Active-source priority matches compute: markup → file → url.
 */
export const SvgNode = memo(function SvgNode(props: NodeProps) {
  const { id, data, selected } = props;
  const nodeData = data as unknown as FloretNodeData;
  const def = nodeRegistry.get(nodeData.defType);
  const setNodeParam = useEditor((s) => s.setNodeParam);
  const warningText = useEditor((s) =>
    s.warnings.filter((w) => w.nodeIds?.includes(id)).map((w) => w.message).join('\n'),
  );

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

  const inputRef = useRef<HTMLInputElement | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const togglePort = useCallback((name: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name); else next.add(name);
      return next;
    });
  }, []);

  const mode = (nodeData.params.mode as string | undefined) ?? 'file';
  const markup = (nodeData.params.markup as string | undefined) ?? '';
  const url = (nodeData.params.url as string | undefined) ?? '';
  const filename = (nodeData.params.filename as string | undefined) ?? '';

  const ingest = useCallback(
    (file: File) => {
      setError(null);
      if (!/svg/i.test(file.type) && !/\.svg$/i.test(file.name)) {
        setError('Not an SVG file.');
        return;
      }
      const reader = new FileReader();
      reader.onload = () => {
        const result = typeof reader.result === 'string' ? reader.result : '';
        setNodeParam(id, 'dataUrl', result);
        setNodeParam(id, 'filename', file.name);
      };
      reader.onerror = () => setError('Failed to read file.');
      reader.readAsDataURL(file);
    },
    [id, setNodeParam],
  );

  const onFileChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const f = e.target.files?.[0];
      if (f) ingest(f);
      e.target.value = '';
    },
    [ingest],
  );

  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setDragOver(false);
      const f = e.dataTransfer.files?.[0];
      if (f) ingest(f);
    },
    [ingest],
  );

  if (!def) return null;

  const inputs = getResolvedInputs(def, nodeData.params);
  const outputs = def.outputs;
  const rows = Math.max(inputs.length, outputs.length);

  return (
    <div
      className="node node-grid node-svg"
      data-selected={selected ? 1 : 0}
      data-category="io"
      data-warning={warningText ? 1 : 0}
    >
      <div className="node-header" title={def.description ?? def.type}>
        <span className="node-label">{def.label}</span>
        {warningText && <span className="node-warning-chip" title={warningText}>⚠</span>}
        <span className="node-type">{def.type}</span>
      </div>

      <div
        className={`svg-node-body${dragOver && mode === 'file' ? ' file-node-body-drop' : ''}`}
        onDragOver={mode === 'file' ? (e) => { e.preventDefault(); e.stopPropagation(); setDragOver(true); } : undefined}
        onDragLeave={mode === 'file' ? () => setDragOver(false) : undefined}
        onDrop={mode === 'file' ? onDrop : undefined}
      >
        {/* Source mode dropdown — only the chosen mode's controls show below. */}
        <select
          className="node-inline-input node-inline-select nodrag svg-node-mode"
          value={mode}
          onChange={(e) => setNodeParam(id, 'mode', e.target.value)}
          onPointerDown={(e) => e.stopPropagation()}
          title="Where the SVG comes from"
        >
          <option value="file">File</option>
          <option value="url">URL</option>
          <option value="markup">Markup</option>
        </select>

        {mode === 'file' && (
          <>
            <input
              ref={inputRef}
              type="file"
              accept=".svg,image/svg+xml"
              className="nodrag"
              onChange={onFileChange}
              style={{ display: 'none' }}
            />
            <div className="svg-node-row">
              <button
                type="button"
                className="file-node-pick nodrag"
                onClick={(e) => { e.stopPropagation(); inputRef.current?.click(); }}
                title="Pick an .svg file (or drag one onto this node)"
              >
                {filename ? 'Replace…' : 'Pick SVG…'}
              </button>
              {filename && (
                <button
                  type="button"
                  className="file-node-clear nodrag"
                  onClick={(e) => { e.stopPropagation(); setNodeParam(id, 'dataUrl', ''); setNodeParam(id, 'filename', ''); }}
                  title="Clear the picked file"
                >
                  ✕
                </button>
              )}
            </div>
            {filename && (
              <div className="file-node-meta"><span className="file-node-name">{filename}</span></div>
            )}
          </>
        )}

        {mode === 'url' && (
          <input
            type="text"
            className="node-inline-input nodrag svg-node-url"
            placeholder="https://…/icon.svg"
            value={url}
            onChange={(e) => setNodeParam(id, 'url', e.target.value)}
            onPointerDown={(e) => e.stopPropagation()}
            title="Load from a URL"
          />
        )}

        {mode === 'markup' && (
          <textarea
            className="node-inline-input nodrag svg-node-paste"
            placeholder="<svg>…</svg>"
            value={markup}
            spellCheck={false}
            onChange={(e) => setNodeParam(id, 'markup', e.target.value)}
            onPointerDown={(e) => e.stopPropagation()}
          />
        )}

        {error && mode === 'file' && <div className="file-node-error">{error}</div>}
      </div>

      <div className="node-rows">
        {Array.from({ length: rows }, (_, i) => {
          const p = inputs[i];
          const out = outputs[i];
          const editable = !!p && !connectedSet.has(p.name) && supportsInlineEditor(p.type);
          const override = p ? nodeData.portValues?.[p.name] : undefined;
          const showEditor = editable && expanded.has(p!.name);
          const showReadout = editable && override !== undefined && !showEditor;
          return (
            <div key={i} className="node-row">
              {p ? (
                <>
                  <InputPin nodeId={id} port={p} wired={connectedSet.has(p.name)} override={override} />
                  <span
                    className={`node-row-label node-row-label-left${editable ? ' clickable' : ''}${showEditor ? ' expanded' : ''}`}
                    onClick={editable ? (e) => { e.stopPropagation(); togglePort(p.name); } : undefined}
                  >
                    {p.label ?? p.name}
                  </span>
                  {showEditor && (
                    <PortValueInlineEditor nodeId={id} portName={p.name} type={p.type} defaultVal={override ?? p.default} />
                  )}
                  {showReadout && <span className="port-value-readout">{formatValue(override)}</span>}
                </>
              ) : (
                <span className="node-row-spacer" />
              )}
              <span className="node-row-spacer" />
              {out && (
                <>
                  <span className="node-row-label node-row-label-right">{out.label ?? out.name}</span>
                  <Handle
                    id={`out-${out.name}`}
                    type="source"
                    position={Position.Right}
                    className="node-row-handle node-row-handle-right"
                    style={{ background: portColorVar(out.type), borderColor: portColorVar(out.type), color: portColorVar(out.type) }}
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
