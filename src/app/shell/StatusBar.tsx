import { useState } from 'react';
import { AlertTriangle, Activity } from 'lucide-react';
import { useAppStore } from '../store';
import { useEditor } from '../../editor/editor-state';
import { Tooltip } from './Tooltip';

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
    window.dispatchEvent(new CustomEvent('nodule:focus-node', { detail: nodeId }));
    setOpen(false);
  };

  // State dot color tells the story at a glance:
  // green = running clean, yellow = running with warnings, red = halted/error,
  // grey = paused. The dot is the strongest signal in the whole status bar.
  const state: 'running' | 'warning' | 'paused' | 'error' =
    warnings.length > 0 && running ? 'warning'
      : !running ? 'paused'
      : 'running';

  return (
    <div className="statusbar">
      <div className="statusbar-left">
        <Tooltip label={running ? 'Runtime active' : 'Runtime paused'} side="top">
          <span className={`status-state status-state-${state}`} role="status">
            <span className="status-dot" />
            <span className="status-state-label">
              {running ? 'live' : 'paused'}
            </span>
          </span>
        </Tooltip>
        <span className="status-sep" />
        <Tooltip label="Frames per second" side="top">
          <span className="status-pill">
            <Activity size={10} strokeWidth={1.8} />
            <span className="status-pill-label">FPS</span>
            <span className="status-pill-value">{fps.toFixed(1)}</span>
          </span>
        </Tooltip>
        <Tooltip label="Frame count" side="top">
          <span className="status-pill">
            <span className="status-pill-label">FRAME</span>
            <span className="status-pill-value">{frameCount}</span>
          </span>
        </Tooltip>
        <Tooltip label="Number of nodes in the patch" side="top">
          <span className="status-pill">
            <span className="status-pill-label">NODES</span>
            <span className="status-pill-value">{nodeCount}</span>
          </span>
        </Tooltip>
      </div>
      <div className="statusbar-center">
        <span className="statusbar-version">FLORET v0.0.1</span>
      </div>
      <div className="statusbar-right">
        {warnings.length > 0 ? (
          <>
            <Tooltip label="Show warnings" side="top">
              <button
                className="status-warnings"
                type="button"
                onClick={() => setOpen((v) => !v)}
                aria-expanded={open}
              >
                <AlertTriangle size={11} strokeWidth={1.8} />
                <span className="status-warnings-badge">{warnings.length}</span>
                <span className="status-warnings-label">
                  warning{warnings.length === 1 ? '' : 's'}
                </span>
              </button>
            </Tooltip>
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
          <span className="status-clean">all clear</span>
        )}
      </div>
    </div>
  );
}
