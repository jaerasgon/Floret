import { useState } from 'react';
import { AlertTriangle, Activity } from 'lucide-react';
import { useAppStore } from '../store';
import { useEditor } from '../../editor/editor-state';

export function StatusBar() {
  const running = useAppStore((s) => s.running);
  const fps = useAppStore((s) => s.fps);
  const frameCount = useAppStore((s) => s.frameCount);
  const nodeCount = useAppStore((s) => s.nodeCount);
  const warnings = useEditor((s) => s.warnings);
  const [open, setOpen] = useState(false);

  // Click a warning row → select + focus its first referenced node so the
  // user can see exactly what's wrong without hunting through the patch.
  const focusNode = (nodeId: string) => {
    useEditor.setState((s) => ({
      nodes: s.nodes.map((n) => ({ ...n, selected: n.id === nodeId })),
    }));
    window.dispatchEvent(new CustomEvent('floret:focus-node', { detail: nodeId }));
    setOpen(false);
  };

  const state: 'running' | 'warning' | 'paused' | 'error' =
    warnings.length > 0 && running ? 'warning'
      : !running ? 'paused'
      : 'running';

  return (
    <div className="statusbar">
      <div className="statusbar-left">
        <span className={`status-state status-state-${state}`} role="status">
          <span className="status-dot" aria-hidden="true" />
          <span className="status-state-label">{running ? 'live' : 'paused'}</span>
        </span>
        <span className="status-sep" />
        <span className="status-pill" title="Frames per second">
          <Activity size={10} strokeWidth={1.8} />
          <span className="status-pill-label">FPS</span>
          <span className="status-pill-value">{fps.toFixed(1)}</span>
        </span>
        <span className="status-pill" title="Frame count">
          <span className="status-pill-label">FRAME</span>
          <span className="status-pill-value">{frameCount}</span>
        </span>
        <span className="status-pill" title="Number of nodes in the patch">
          <span className="status-pill-label">NODES</span>
          <span className="status-pill-value">{nodeCount}</span>
        </span>
      </div>
      <div className="statusbar-center">
        <span className="statusbar-version">FLORET v0.0.1</span>
      </div>
      <div className="statusbar-right">
        {warnings.length > 0 ? (
          <>
            <button
              className="status-warnings"
              type="button"
              onClick={() => setOpen((v) => !v)}
              aria-expanded={open}
              aria-label="Show warnings"
            >
              <AlertTriangle size={11} strokeWidth={1.8} />
              <span className="status-warnings-badge">{warnings.length}</span>
              <span className="status-warnings-label">warning{warnings.length === 1 ? '' : 's'}</span>
            </button>
            {open && (
              <div className="warnings-popover" role="dialog" onMouseLeave={() => setOpen(false)}>
                {warnings.map((w, i) => {
                  const target = w.nodeIds?.[0];
                  return (
                    <button
                      key={i}
                      className="warnings-row"
                      type="button"
                      onClick={() => target ? focusNode(target) : undefined}
                      disabled={!target}
                      title={target ? `Focus ${target}` : 'No node to focus'}
                    >
                      <AlertTriangle size={10} strokeWidth={1.8} className="warnings-row-bullet" />
                      <span className="warnings-row-msg">{w.message}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </>
        ) : (
          <span className="status-clean"><span className="status-clean-dot" aria-hidden="true" />clear</span>
        )}
      </div>
    </div>
  );
}
