import { useState } from 'react';
import { AlertTriangle, Activity } from 'lucide-react';
import Box from '@mui/material/Box';
import Chip from '@mui/material/Chip';
import Tooltip from '@mui/material/Tooltip';
import { useAppStore } from '../store';
import { useEditor } from '../../editor/editor-state';

// Runtime state → status-dot colour. The dot is the strongest signal in the
// whole status bar: green = running clean, amber = running with warnings,
// red = halted/error, grey = paused.
const DOT_COLOR = {
  running: '#7bd88f',
  warning: 'var(--md-sys-color-primary)',
  error: 'var(--md-sys-color-error)',
  paused: 'var(--md-sys-color-outline)',
} as const;

/** Small semantic status dot (replaces Astryx's StatusDot). */
function StatusDot({ state, pulsing, title }: {
  state: keyof typeof DOT_COLOR; pulsing: boolean; title: string;
}) {
  return (
    <Tooltip title={title} arrow>
      <Box
        component="span"
        role="img"
        aria-label={title}
        sx={{
          display: 'inline-block',
          width: 9,
          height: 9,
          borderRadius: '50%',
          backgroundColor: DOT_COLOR[state],
          boxShadow: pulsing ? `0 0 0 0 ${DOT_COLOR[state]}` : 'none',
          animation: pulsing ? 'floret-dot-pulse 1.8s ease-out infinite' : 'none',
          '@keyframes floret-dot-pulse': {
            '0%': { boxShadow: `0 0 0 0 color-mix(in srgb, ${DOT_COLOR[state]} 70%, transparent)` },
            '70%': { boxShadow: `0 0 0 5px color-mix(in srgb, ${DOT_COLOR[state]} 0%, transparent)` },
            '100%': { boxShadow: `0 0 0 0 color-mix(in srgb, ${DOT_COLOR[state]} 0%, transparent)` },
          },
        }}
      />
    </Tooltip>
  );
}

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
            state={state}
            pulsing={running}
            title={running ? 'Runtime active' : 'Runtime paused'}
          />
          <span className="status-state-label">{running ? 'live' : 'paused'}</span>
        </span>
        <span className="status-sep" />
        <Tooltip title="Frames per second" arrow>
          <span className="status-pill">
            <Activity size={10} strokeWidth={1.8} />
            <span className="status-pill-label">FPS</span>
            <span className="status-pill-value">{fps.toFixed(1)}</span>
          </span>
        </Tooltip>
        <Tooltip title="Frame count" arrow>
          <span className="status-pill">
            <span className="status-pill-label">FRAME</span>
            <span className="status-pill-value">{frameCount}</span>
          </span>
        </Tooltip>
        <Tooltip title="Number of nodes in the patch" arrow>
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
              <Chip
                size="small"
                color="warning"
                variant="outlined"
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
