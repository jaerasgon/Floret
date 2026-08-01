import { useState } from 'react';
import { AlertTriangle, Activity } from 'lucide-react';
import { StatusDot } from '@astryxdesign/core/StatusDot';
import { Badge } from '@astryxdesign/core/Badge';
import { Tooltip } from '@astryxdesign/core/Tooltip';
import { useAppStore } from '../store';
import { useEditor } from '../../editor/editor-state';

// Runtime state → Astryx StatusDot semantic variant. The dot is the strongest
// signal in the whole status bar: green = running clean, yellow = running with
// warnings, red = halted/error, grey = paused.
const DOT_VARIANT = {
  running: 'success',
  warning: 'warning',
  error: 'error',
  paused: 'neutral',
} as const;

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
        <span className="status-state" role="status">
          <StatusDot
            variant={DOT_VARIANT[state]}
            isPulsing={running}
            label={running ? 'Runtime active' : 'Runtime paused'}
            tooltip={running ? 'Runtime active' : 'Runtime paused'}
          />
          <span className="status-state-label">{running ? 'live' : 'paused'}</span>
        </span>
        <span className="status-sep" />
        <Tooltip content="Frames per second">
          <span className="status-pill">
            <Activity size={10} strokeWidth={1.8} />
            <span className="status-pill-label">FPS</span>
            <span className="status-pill-value">{fps.toFixed(1)}</span>
          </span>
        </Tooltip>
        <Tooltip content="Frame count">
          <span className="status-pill">
            <span className="status-pill-label">FRAME</span>
            <span className="status-pill-value">{frameCount}</span>
          </span>
        </Tooltip>
        <Tooltip content="Number of nodes in the patch">
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
            <button
              className="status-warnings"
              type="button"
              onClick={() => setOpen((v) => !v)}
              aria-expanded={open}
              aria-label="Show warnings"
            >
              <Badge
                variant="warning"
                icon={<AlertTriangle size={11} strokeWidth={1.8} />}
                label={`${warnings.length} warning${warnings.length === 1 ? '' : 's'}`}
              />
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
