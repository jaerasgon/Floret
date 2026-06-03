import { memo, useCallback, useRef, useState } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { useEditor, type NoduleNodeData } from '../editor-state';
import { nodeRegistry } from '../../nodes/registry';
import { portColorVar } from '../connection-rules';

/**
 * File picker node. Renders the standard 5-output header + a button row
 * with "Pick file…" + drag-and-drop. The chosen file's bytes are read
 * as a base64 data URL and stashed on `data.params.dataUrl`, where the
 * NodeDef's `readFromContext` finds them on every tick.
 *
 * Large files (multi-MB) bloat the patch JSON since the data URL is
 * persisted in-place — fine for typical creative-coding assets, less so
 * for an hour of uncompressed audio. We surface the size next to the
 * filename so the user sees what they're paying.
 */
const SIZE_HINT_LIMIT = 8 * 1024 * 1024; // 8 MB — above this we warn

export const FileNode = memo(function FileNode(props: NodeProps) {
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

  const inputRef = useRef<HTMLInputElement | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);

  const filename = (nodeData.params.filename as string | undefined) ?? '';
  const mimeType = (nodeData.params.mimeType as string | undefined) ?? '';
  const dataUrl = (nodeData.params.dataUrl as string | undefined) ?? '';
  // Estimate file size from the base64 length (4 chars ≈ 3 bytes).
  const sizeBytes = dataUrl
    ? Math.floor((dataUrl.length - dataUrl.indexOf(',') - 1) * 3 / 4)
    : 0;

  const ingest = useCallback(
    (file: File) => {
      setError(null);
      setBusy(true);
      const reader = new FileReader();
      reader.onload = () => {
        const result = typeof reader.result === 'string' ? reader.result : '';
        setNodeParam(id, 'dataUrl', result);
        setNodeParam(id, 'filename', file.name);
        setNodeParam(id, 'mimeType', file.type || 'application/octet-stream');
        setBusy(false);
      };
      reader.onerror = () => {
        setError('Failed to read file.');
        setBusy(false);
      };
      reader.readAsDataURL(file);
    },
    [id, setNodeParam],
  );

  const onFileChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const f = e.target.files?.[0];
      if (f) ingest(f);
      e.target.value = ''; // allow re-picking the same file
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

  const clearFile = useCallback(() => {
    setNodeParam(id, 'dataUrl', '');
    setNodeParam(id, 'filename', '');
    setNodeParam(id, 'mimeType', '');
    setError(null);
  }, [id, setNodeParam]);

  if (!def) return null;

  return (
    <div
      className="node node-grid node-file"
      data-selected={selected ? 1 : 0}
      data-category="io"
      data-warning={warningText ? 1 : 0}
    >
      <div className="node-header" title={def.description ?? def.type}>
        <span className="node-label">{def.label}</span>
        {warningText && (
          <span className="node-warning-chip" title={warningText}>⚠</span>
        )}
        <span className="node-type">{def.type}</span>
      </div>

      <div
        className={`file-node-body${dragOver ? ' file-node-body-drop' : ''}`}
        onDragOver={(e) => { e.preventDefault(); e.stopPropagation(); setDragOver(true); }}
        onDragLeave={() => setDragOver(false)}
        onDrop={onDrop}
      >
        <input
          ref={inputRef}
          type="file"
          className="file-node-input nodrag"
          onChange={onFileChange}
          style={{ display: 'none' }}
        />
        <button
          type="button"
          className="file-node-pick nodrag"
          onClick={(e) => { e.stopPropagation(); inputRef.current?.click(); }}
          title="Open the OS file picker"
        >
          {filename ? 'Replace…' : 'Pick file…'}
        </button>
        {filename && (
          <button
            type="button"
            className="file-node-clear nodrag"
            onClick={(e) => { e.stopPropagation(); clearFile(); }}
            title="Clear the picked file"
          >
            ✕
          </button>
        )}
      </div>

      {filename && (
        <div className="file-node-meta" title={`${mimeType} · ${formatBytes(sizeBytes)}`}>
          <span className="file-node-name">{filename}</span>
          <span
            className={`file-node-size${sizeBytes > SIZE_HINT_LIMIT ? ' file-node-size-warn' : ''}`}
          >
            {formatBytes(sizeBytes)}
          </span>
        </div>
      )}
      {busy && <div className="file-node-busy">Reading…</div>}
      {error && <div className="file-node-error">{error}</div>}
      {dragOver && <div className="file-node-drop-hint">Drop to load</div>}

      <div className="node-rows">
        {def.outputs.map((p, i) => (
          <div key={p.name} className="node-row" data-row-index={i}>
            <span className="node-row-spacer" />
            <span className="node-row-label node-row-label-right">{p.label ?? p.name}</span>
            <Handle
              id={`out-${p.name}`}
              type="source"
              position={Position.Right}
              className="node-row-handle node-row-handle-right"
              style={{
                background: portColorVar(p.type),
                borderColor: portColorVar(p.type),
                color: portColorVar(p.type),
              }}
            />
          </div>
        ))}
      </div>
    </div>
  );
});

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}
