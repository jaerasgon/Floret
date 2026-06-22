import { useReactFlow } from '@xyflow/react';

/**
 * Top-left breadcrumb. Visible at root view too — there it just shows
 * the label "Project" as a non-clickable header. Each subpatch segment
 * is clickable: clicking depth N navigates back to that level.
 */
export function SubpatchBreadcrumb({
  viewStack, nodes, onNavigate, onExit,
}: {
  viewStack: string[];
  nodes: ReadonlyArray<{ id: string; data: { params?: Record<string, unknown>; defType?: string } }>;
  onNavigate: (depth: number) => void;
  onExit: () => void;
}) {
  const segments: { label: string; depth: number }[] = [{ label: 'Project', depth: 0 }];
  for (let i = 0; i < viewStack.length; i++) {
    const id = viewStack[i]!;
    const n = nodes.find((x) => x.id === id);
    const label = (n?.data.params?.label as string | undefined) ?? 'Subpatch';
    segments.push({ label, depth: i + 1 });
  }
  return (
    <div className="subpatch-breadcrumb" data-empty={viewStack.length === 0 ? 1 : 0}>
      {segments.map((s, i) => (
        <span key={i} className="subpatch-crumb-row">
          {i > 0 && <span className="subpatch-crumb-sep">/</span>}
          <button
            className="subpatch-crumb"
            data-current={i === segments.length - 1 ? 1 : 0}
            onClick={() => onNavigate(s.depth)}
            title={i === segments.length - 1 ? 'Current view' : `Go to ${s.label}`}
          >
            {s.label}
          </button>
        </span>
      ))}
      {viewStack.length > 0 && (
        <button
          className="subpatch-crumb-exit"
          onClick={onExit}
          title="Exit one level (O)"
        >
          ↑ O
        </button>
      )}
    </div>
  );
}

/**
 * Muted hint shown when the canvas is empty — the first thing a new
 * user sees, so it has to point at the three main ways to populate
 * the canvas without being noisy.
 */
export function EmptyCanvasHint() {
  return (
    <div className="empty-canvas-hint">
      <div className="empty-canvas-line">
        <kbd>Tab</kbd> to search nodes
      </div>
      <div className="empty-canvas-line">
        Drag from the left panel
      </div>
      <div className="empty-canvas-line">
        Or load a <em>Sample</em>
      </div>
    </div>
  );
}

/**
 * Translucent preview rectangle drawn while the user is rubber-banding
 * a new region. Positioned in screen coords by projecting the two
 * flow-coord corners back through `flowToScreenPosition`.
 */
export function RegionDrawPreview({
  startFlow, currentFlow, kind,
}: { startFlow: { x: number; y: number }; currentFlow: { x: number; y: number }; kind: 'foreach' | 'if' | 'cache' }) {
  const reactFlow = useReactFlow();
  const a = reactFlow.flowToScreenPosition(startFlow);
  const b = reactFlow.flowToScreenPosition(currentFlow);
  const left = Math.min(a.x, b.x);
  const top = Math.min(a.y, b.y);
  const width = Math.abs(b.x - a.x);
  const height = Math.abs(b.y - a.y);
  return (
    <div
      className={`region-draw-preview region-draw-preview-${kind}`}
      style={{ left, top, width, height }}
    >
      <span className="region-draw-preview-label">{kind}</span>
    </div>
  );
}
