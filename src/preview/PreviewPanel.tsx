import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Pause, Play, Download, Check } from 'lucide-react';
import { Preview } from './Preview';
import { Panel } from '../editor/panel/Panel';
import { useAppStore } from '../app/store';
import { Tooltip } from '../app/shell/Tooltip';
import type { NodeId } from '../runtime/graph';
import type { ActiveDriver } from '../runtime/canvas-driver';
import { exportCanvasPng, exportSceneSvg } from '../persistence/export';

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
  // Stable identity: Preview's driver-creating effect lists `onDriver` as a
  // dependency, so an inline arrow here would re-create the driver — and
  // reset the running scene (particles respawn, trails clear) — on EVERY
  // re-render of this panel, e.g. whenever a node is selected on the canvas.
  const onDriver = useCallback((d: ActiveDriver | null) => { driverRef.current = d; }, []);
  return (
    <Panel
      id={panelId}
      title={title}
      defaults={{ dock: 'right', width: 460, height: 460, floatX: 360, floatY: 80 }}
      hideClose
      headerActions={<PreviewHeaderControls driverRef={driverRef} name={title} />}
    >
      <Preview rendererId={rendererId} onDriver={onDriver} />
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
      <ExportMenu driverRef={driverRef} name={name} />
    </>
  );
}

/**
 * Export dropdown — captures the CURRENT (frozen) frame as PNG (1×/2×/4×)
 * or SVG. Only enabled while the renderer is paused, so what you export is
 * exactly the still you're looking at. Animated / interval / sequence
 * output is intentionally not here — that's expressed in the patch with
 * nodes, not baked into the export button. Filenames derive from the
 * panel title.
 */
function ExportMenu({ driverRef, name }: {
  driverRef: React.MutableRefObject<ActiveDriver | null>;
  name: string;
}) {
  const running = useAppStore((s) => s.running);
  const [menuPos, setMenuPos] = useState<{ left: number; top: number } | null>(null);
  const [saved, setSaved] = useState(false);
  const savedTimer = useRef<number | null>(null);
  const btnRef = useRef<HTMLButtonElement | null>(null);
  const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'floret';
  const open = menuPos !== null;

  const toggle = () => {
    // Export only captures a frozen frame, so it's inert while running.
    // We keep the button enabled (rather than `disabled`) so its tooltip —
    // which explains *why* it's inert — still shows on hover; disabled
    // elements don't emit hover events in most browsers.
    if (running) return;
    if (open) { setMenuPos(null); return; }
    const r = btnRef.current?.getBoundingClientRect();
    if (!r) return;
    // Right-align the menu to the button and hang it below; clamp on-screen.
    const W = 150;
    const left = Math.max(8, Math.min(r.right - W, window.innerWidth - W - 8));
    setMenuPos({ left, top: r.bottom + 4 });
  };

  // A download is silent (no in-app surface), so flash the button to
  // "Saved" briefly — otherwise a successful export reads as "nothing
  // happened" if the browser's download UI is hidden.
  const flashSaved = () => {
    setSaved(true);
    if (savedTimer.current) window.clearTimeout(savedTimer.current);
    savedTimer.current = window.setTimeout(() => setSaved(false), 1400);
  };
  useEffect(() => () => { if (savedTimer.current) window.clearTimeout(savedTimer.current); }, []);

  const png = (scale: number) => {
    const d = driverRef.current;
    if (d) { exportCanvasPng(d.canvas, slug, scale); flashSaved(); }
    setMenuPos(null);
  };
  const svg = () => {
    const d = driverRef.current;
    if (d) {
      const s = d.captureScene();
      exportSceneSvg(s.layers, s.width, s.height, s.background, slug);
      flashSaved();
    }
    setMenuPos(null);
  };

  return (
    <>
      <Tooltip label={running ? 'Pause to export the current frame' : 'Export the paused frame as PNG / SVG'}>
        <button
          ref={btnRef}
          className={`toolbar-run-btn${saved ? ' running' : ''}${running ? ' is-inert' : ''}`}
          onClick={toggle}
          aria-label="Export"
          aria-disabled={running}
          onBlur={() => setTimeout(() => setMenuPos(null), 150)}
        >
          {saved ? <Check size={13} strokeWidth={2} /> : <Download size={13} strokeWidth={1.8} />}
          <span>{saved ? 'Saved' : 'Export'}</span>
        </button>
      </Tooltip>
      {/* Portal to <body> so the dock tab bar's `overflow:hidden` can't
          clip the dropdown to a sliver. Positioned at the button's rect. */}
      {open && !running && menuPos && createPortal(
        <div
          className="export-menu export-menu-fixed"
          style={{ left: menuPos.left, top: menuPos.top }}
          // Keep focus on the button so its onBlur (close) fires after a
          // menu click, not before.
          onMouseDown={(e) => e.preventDefault()}
        >
          <button className="library-row" onClick={() => png(1)}>PNG · 1×</button>
          <button className="library-row" onClick={() => png(2)}>PNG · 2×</button>
          <button className="library-row" onClick={() => png(4)}>PNG · 4×</button>
          <button className="library-row" onClick={svg}>SVG · vector</button>
        </div>,
        document.body,
      )}
    </>
  );
}

