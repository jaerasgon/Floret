import { useState } from 'react';
import { useAppStore } from '../store';
import { useEditor } from '../../editor/editor-state';

export function StatusBar() {
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
    // Bring the node into view via React Flow's fitView event hook.
    window.dispatchEvent(new CustomEvent('nodule:focus-node', { detail: nodeId }));
    setOpen(false);
  };

  return (
    <div className="statusbar">
      <div className="statusbar-left">
        <span>fps {fps.toFixed(1).padStart(5)}</span>
        <span>frame {frameCount.toString().padStart(6)}</span>
        <span>nodes {nodeCount}</span>
      </div>
      <div className="statusbar-center">
        <span className="statusbar-version">v0.0.1</span>
      </div>
      <div className="statusbar-right">
        {warnings.length > 0 ? (
          <>
            <button
              className="status-warnings"
              type="button"
              onClick={() => setOpen((v) => !v)}
              aria-expanded={open}
              style={{ color: 'var(--danger)' }}
            >
              ⚠ {warnings.length} warning{warnings.length === 1 ? '' : 's'}
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
                      <span className="warnings-row-bullet">⚠</span>
                      <span className="warnings-row-msg">{w.message}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </>
        ) : (
          <span>ready</span>
        )}
      </div>
    </div>
  );
}
