import { useMemo } from 'react';
import { Preview } from './Preview';
import { Panel } from '../editor/panel/Panel';
import { useEditor } from '../editor/editor-state';
import type { NodeId } from '../runtime/graph';

/**
 * Preview panel — `Preview` wrapped in the universal `Panel` chrome
 * so it can dock left/right, float, resize, collapse, or hide along
 * with every other side panel.
 *
 * Each instance is bound to a specific Renderer node by id. The header
 * dropdown lets the user re-bind to another Renderer if the patch has
 * more than one.
 *
 * The panel id is parameterised so the App can render multiple
 * PreviewPanels with independent layout state (each with its own
 * dock-side, size, float position, …).
 */

export interface PreviewPanelProps {
  /** Per-instance panel id used for localStorage layout persistence. */
  panelId: string;
  /** Which Renderer this preview is bound to. Null means "track the
   *  first Renderer in the graph" — useful for the single-preview
   *  default. */
  rendererId: NodeId | null;
  /** User-rebinds this preview to another Renderer. The parent owns the
   *  current selection so it can also surface the same dropdown
   *  elsewhere (e.g. a status bar pill) if desired. */
  onSelectRenderer: (id: NodeId | null) => void;
  /** Optional title override; defaults to "Preview". */
  title?: string;
}

export function PreviewPanel({
  panelId, rendererId, onSelectRenderer, title = 'Preview',
}: PreviewPanelProps) {
  return (
    <Panel
      id={panelId}
      title={title}
      defaults={{ dock: 'right', width: 460, height: 460, floatX: 360, floatY: 80 }}
    >
      <RendererPicker
        rendererId={rendererId}
        onSelectRenderer={onSelectRenderer}
      />
      <Preview rendererId={rendererId} />
    </Panel>
  );
}

function RendererPicker({
  rendererId, onSelectRenderer,
}: {
  rendererId: NodeId | null;
  onSelectRenderer: (id: NodeId | null) => void;
}) {
  const runtime = useEditor((s) => s.runtime);
  const editorNodes = useEditor((s) => s.nodes);

  const options = useMemo(() => {
    const ids = runtime?.rendererIds ?? [];
    return ids.map((id, idx) => {
      // Surface a friendly name. Default = "Renderer #1", "Renderer #2", …
      const node = editorNodes.find((n) => n.id === id);
      const label = (node?.data.params as Record<string, unknown> | undefined)?.label as string | undefined;
      return { id, label: label && label.trim() ? label : `Renderer #${idx + 1}`, idx };
    });
  }, [runtime, editorNodes]);

  // When no rendererIds exist there's nothing to choose from — keep the
  // picker around but disabled so users still see WHERE rebinding lives.
  const showAuto = !rendererId;

  return (
    <div className="preview-picker">
      <label className="preview-picker-label">Renderer</label>
      <select
        className="preview-picker-select nodrag"
        value={rendererId ?? ''}
        onChange={(e) => {
          const v = e.target.value;
          onSelectRenderer(v === '' ? null : v);
        }}
      >
        <option value="">Auto (first)</option>
        {options.map((o) => (
          <option key={o.id} value={o.id}>{o.label}</option>
        ))}
      </select>
      {showAuto && options.length > 1 && (
        <span className="preview-picker-hint" title="This preview tracks the first Renderer in the patch">
          tracking #1
        </span>
      )}
    </div>
  );
}
