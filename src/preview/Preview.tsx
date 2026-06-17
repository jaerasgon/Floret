import { useEffect, useRef, useState } from 'react';
import { Plus, Minus, Maximize, Lock, Unlock } from 'lucide-react';
import { createCanvasDriver, getActiveDriver, type ActiveDriver } from '../runtime/canvas-driver';
import { Tooltip } from '../app/shell/Tooltip';
import { useAppStore } from '../app/store';
import { useEditor } from '../editor/editor-state';
import type { RuntimeGraph, NodeId } from '../runtime/graph';

/**
 * Preview pane — mounts a canvas-driver instance into a host div. Each
 * Preview owns ONE driver (one canvas + one worker) bound to ONE
 * Renderer node in the graph. The editor can host multiple Preview
 * panels to show different renderers side-by-side; when no
 * `rendererId` is passed, the preview auto-tracks the first renderer
 * in the graph (back-compat).
 */
function PreviewOverlay({ rendererId }: { rendererId: NodeId | null }) {
  const runtime = useEditor((s) => s.runtime);
  const nodes = useEditor((s) => s.nodes);
  const warnings = useEditor((s) => s.warnings);

  if (!runtime && warnings.length > 0) {
    return (
      <div className="preview-overlay error">
        <div className="overlay-title">compile error</div>
        {warnings.map((w, i) => (
          <div key={i} className="overlay-line">{w.message}</div>
        ))}
      </div>
    );
  }

  const hasAnyRenderer = (runtime?.rendererIds.length ?? 0) > 0;
  const myRendererBound = rendererId && runtime?.rendererIds.includes(rendererId);

  if (!hasAnyRenderer) {
    if (nodes.length === 0) {
      return (
        <div className="preview-overlay">
          <div className="overlay-title">empty canvas</div>
          <div className="overlay-line">
            Drag nodes from the palette, or use <b>Samples ▾</b> to load a demo.
          </div>
        </div>
      );
    }
    return (
      <div className="preview-overlay warn">
        <div className="overlay-title">no Renderer</div>
        <div className="overlay-line">
          Your patch has {nodes.length} node{nodes.length === 1 ? '' : 's'} but no
          <b> Renderer</b>. Add one and wire your drawing chain into its <b>layers</b> port.
        </div>
      </div>
    );
  }

  if (rendererId && !myRendererBound) {
    return (
      <div className="preview-overlay warn">
        <div className="overlay-title">renderer gone</div>
        <div className="overlay-line">
          This preview was bound to a Renderer that no longer exists in the
          patch. Pick another from the dropdown above.
        </div>
      </div>
    );
  }

  return null;
}

export interface PreviewProps {
  /** Which Renderer this preview shows. Null = auto-bind to the first
   *  Renderer in the graph (single-preview back-compat). */
  rendererId?: NodeId | null;
  /** Receives this preview's driver when it mounts (and null on unmount)
   *  so the panel header can export from the exact canvas it owns. */
  onDriver?: (driver: ActiveDriver | null) => void;
}

export function Preview({ rendererId = null, onDriver }: PreviewProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const driverRef = useRef<ActiveDriver | null>(null);
  // Lock freezes the view: trackpad pinch/pan are ignored (no accidental
  // zoom), but the +/−/fit buttons still work. Ref mirrors state so the
  // long-lived wheel handler reads the current value.
  const [locked, setLocked] = useState(false);
  const lockedRef = useRef(false);
  const runningRef = useRef<boolean>(useAppStore.getState().running);
  const graphRef = useRef<RuntimeGraph | null>(useEditor.getState().runtime);
  const refreshKey = useAppStore((s) => s.refreshKey);

  useEffect(() => {
    return useAppStore.subscribe((s) => {
      runningRef.current = s.running;
    });
  }, []);

  useEffect(() => {
    return useEditor.subscribe((s) => {
      graphRef.current = s.runtime;
      useAppStore.getState().setNodeCount(s.runtime?.nodes.size ?? 0);
    });
  }, []);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const setFps = useAppStore.getState().setFps;
    const setFrame = useAppStore.getState().setFrameCount;

    const driver = createCanvasDriver(el, {
      graphRef,
      rendererId,
      isRunning: () => runningRef.current,
      onTick: (fps, frame) => {
        setFps(fps);
        setFrame(frame);
      },
    });
    driverRef.current = driver;
    onDriver?.(driver);

    // Pinch (ctrl+wheel) zooms the preview toward the cursor; two-finger
    // scroll pans. preventDefault stops the page from zooming/scrolling —
    // the zoom stays contained to this panel. Attached to the panel area
    // (host) so the letterbox margins respond too.
    const host = (el.parentElement ?? el) as HTMLElement;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault(); // stop browser zoom/scroll even when locked
      if (lockedRef.current) return;
      if (e.ctrlKey) driver.zoomAt(e.deltaY, e.clientX, e.clientY);
      else driver.panBy(-e.deltaX, -e.deltaY);
    };
    host.addEventListener('wheel', onWheel, { passive: false });

    return () => {
      host.removeEventListener('wheel', onWheel);
      driverRef.current = null;
      onDriver?.(null);
      driver.stop();
    };
  }, [refreshKey, rendererId, onDriver]);

  return (
    <div className="preview-host">
      <div ref={containerRef} className="preview-canvas" />
      <PreviewOverlay rendererId={rendererId} />
      <PreviewControls
        driverRef={driverRef}
        locked={locked}
        onToggleLock={() => { const v = !lockedRef.current; lockedRef.current = v; setLocked(v); }}
      />
    </div>
  );
}

/**
 * Zoom controls — a vertical button stack mirroring the node canvas's
 * React Flow `<Controls>` (same chrome) for consistency: zoom in, zoom
 * out, and fit-in-view (which also resets pan).
 */
function PreviewControls({ driverRef, locked, onToggleLock }: {
  driverRef: React.MutableRefObject<ActiveDriver | null>;
  locked: boolean;
  onToggleLock: () => void;
}) {
  return (
    <div className="preview-controls">
      <Tooltip label="Zoom in">
        <button className="preview-controls-btn" aria-label="Zoom in"
          onClick={() => driverRef.current?.zoomBy(1.2)}>
          <Plus size={14} strokeWidth={1.8} />
        </button>
      </Tooltip>
      <Tooltip label="Zoom out">
        <button className="preview-controls-btn" aria-label="Zoom out"
          onClick={() => driverRef.current?.zoomBy(1 / 1.2)}>
          <Minus size={14} strokeWidth={1.8} />
        </button>
      </Tooltip>
      <Tooltip label="Fit in view">
        <button className="preview-controls-btn" aria-label="Fit in view"
          onClick={() => driverRef.current?.fitInView()}>
          <Maximize size={13} strokeWidth={1.8} />
        </button>
      </Tooltip>
      <Tooltip label={locked ? 'Unlock view (allow pinch / pan)' : 'Lock view (ignore pinch / pan)'}>
        <button
          className={`preview-controls-btn${locked ? ' active' : ''}`}
          aria-label={locked ? 'Unlock view' : 'Lock view'}
          aria-pressed={locked}
          onClick={onToggleLock}
        >
          {locked ? <Lock size={13} strokeWidth={1.8} /> : <Unlock size={13} strokeWidth={1.8} />}
        </button>
      </Tooltip>
    </div>
  );
}

// Re-export for any callers that still grab the active instance.
export { getActiveDriver };
