import { useRef, useState } from 'react';
import { Pause, Play, Download } from 'lucide-react';
import { Preview } from './Preview';
import { Panel } from '../editor/panel/Panel';
import { useAppStore } from '../app/store';
import { Tooltip } from '../app/shell/Tooltip';
import type { NodeId } from '../runtime/graph';
import type { ActiveDriver } from '../runtime/canvas-driver';
import { exportCanvasPng, exportSceneSvg, exportCanvasWebm } from '../persistence/export';

/**
 * Preview panel — `Preview` wrapped in the universal `Panel` chrome
 * so it can dock left/right, float, resize, collapse, or hide along
 * with every other side panel.
 *
 * Each preview is bound 1:1 to a specific `render/renderer` node. The
 * App layer derives previews from the node list — there's no separate
 * user-managed list, no add / remove, no rebinding. Delete the node and
 * the preview goes; add a renderer node and a preview appears. That's
 * also why the × close button is permanently hidden — the only way to
 * "close" a preview is to delete its renderer node.
 */

export interface PreviewPanelProps {
  /** Per-instance panel id. The App passes the renderer node id so
   *  layout state (dock side, size, float position) stays stable
   *  across reloads via the existing panelState localStorage. */
  panelId: string;
  /** The Renderer node this preview is bound to. Always set — null
   *  isn't meaningful in the derived-from-nodes model. */
  rendererId: NodeId;
  /** Optional title override; defaults to "Preview". */
  title?: string;
}

export function PreviewPanel({
  panelId, rendererId, title = 'Preview',
}: PreviewPanelProps) {
  const driverRef = useRef<ActiveDriver | null>(null);
  return (
    <Panel
      id={panelId}
      title={title}
      defaults={{ dock: 'right', width: 460, height: 460, floatX: 360, floatY: 80 }}
      hideClose
      headerActions={<PreviewHeaderControls driverRef={driverRef} name={title} />}
    >
      <Preview rendererId={rendererId} onDriver={(d) => { driverRef.current = d; }} />
    </Panel>
  );
}

/**
 * Pause, hosted on every Preview panel header. The action mutates
 * global app state (`useAppStore`) so click behaviour is shared
 * regardless of how many previews exist. Refresh is global (toolbar) —
 * a scene reset isn't a per-preview thing.
 */
function PreviewHeaderControls({ driverRef, name }: {
  driverRef: React.MutableRefObject<ActiveDriver | null>;
  name: string;
}) {
  const running = useAppStore((s) => s.running);
  const toggleRunning = useAppStore((s) => s.toggleRunning);
  return (
    <>
      <ExportMenu driverRef={driverRef} name={name} />
      <Tooltip label={running ? 'Pause runtime' : 'Resume runtime'} shortcut="Space">
        <button
          className={`toolbar-run-btn${running ? ' running' : ''}`}
          onClick={toggleRunning}
          aria-label={running ? 'Pause' : 'Play'}
        >
          {running ? <Pause size={13} strokeWidth={1.8} /> : <Play size={13} strokeWidth={1.8} />}
          <span>{running ? 'Pause' : 'Play'}</span>
        </button>
      </Tooltip>
    </>
  );
}

/**
 * Export dropdown — PNG (1×/2×/4×), SVG (vector), and WebM video for the
 * canvas this panel owns. Video shows a "recording…" state for its
 * duration. Filenames derive from the panel title.
 */
function ExportMenu({ driverRef, name }: {
  driverRef: React.MutableRefObject<ActiveDriver | null>;
  name: string;
}) {
  const [open, setOpen] = useState(false);
  const [recording, setRecording] = useState(false);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'floret';

  const png = (scale: number) => {
    const d = driverRef.current;
    if (d) exportCanvasPng(d.canvas, slug, scale);
    setOpen(false);
  };
  const svg = () => {
    const d = driverRef.current;
    if (d) {
      const s = d.captureScene();
      exportSceneSvg(s.layers, s.width, s.height, s.background, slug);
    }
    setOpen(false);
  };
  const webm = async () => {
    const d = driverRef.current;
    setOpen(false);
    if (!d) return;
    setRecording(true);
    try { await exportCanvasWebm(d.canvas, slug, 4000, 30); }
    catch { /* surfaced via the disabled state resetting */ }
    setRecording(false);
  };

  return (
    <div ref={wrapRef} style={{ position: 'relative', display: 'inline-flex' }}>
      <Tooltip label={recording ? 'Recording…' : 'Export PNG / SVG / video'}>
        <button
          className={`toolbar-run-btn${recording ? ' running' : ''}`}
          onClick={() => setOpen((v) => !v)}
          aria-label="Export"
          disabled={recording}
          onBlur={() => setTimeout(() => setOpen(false), 150)}
        >
          <Download size={13} strokeWidth={1.8} />
          <span>{recording ? 'Rec…' : 'Export'}</span>
        </button>
      </Tooltip>
      {open && !recording && (
        <div className="export-menu">
          <button className="library-row" onClick={() => png(1)}>PNG · 1×</button>
          <button className="library-row" onClick={() => png(2)}>PNG · 2×</button>
          <button className="library-row" onClick={() => png(4)}>PNG · 4×</button>
          <button className="library-row" onClick={svg}>SVG · vector</button>
          <button className="library-row" onClick={webm}>WebM · 4s video</button>
        </div>
      )}
    </div>
  );
}

