import { useEffect, useRef } from 'react';
import { mountCanvas, unmountCanvas, getActiveDriver } from '../runtime/canvas-driver';
import { useAppStore } from '../app/store';
import { useEditor } from '../editor/editor-state';
import type { RuntimeGraph } from '../runtime/graph';

/**
 * Preview pane — mounts the canvas driver into a host div. The driver
 * owns the canvas, the RAF loop, and input event listeners. We just
 * hand it a `graphRef` that points at the latest compiled runtime
 * graph + an `isRunning` getter for the global pause toggle.
 */
function PreviewOverlay() {
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

  if (!runtime?.rendererId) {
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

  return null;
}

export function Preview() {
  const containerRef = useRef<HTMLDivElement | null>(null);
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

    mountCanvas(el, {
      graphRef,
      isRunning: () => runningRef.current,
      onTick: (fps, frame) => {
        setFps(fps);
        setFrame(frame);
      },
    });

    return () => {
      unmountCanvas();
    };
  }, [refreshKey]);

  return (
    <div className="preview-host">
      <div ref={containerRef} className="preview-canvas" />
      <PreviewOverlay />
    </div>
  );
}

// Re-export for any callers that still grab the active instance.
export { getActiveDriver };
