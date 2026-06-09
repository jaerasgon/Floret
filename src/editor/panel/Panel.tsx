import { useCallback, useEffect, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { usePanelState, type PanelDock, type PanelStateDefaults } from './usePanelState';

/**
 * Universal panel chrome — title bar, collapse, dock-side toggles,
 * float button, resize handles. Wraps any content as a uniform
 * resizable / collapsible / detachable / dockable panel.
 *
 * Three modes:
 *   - docked-left / docked-right: lives in a vertical rail of the
 *     editor pane. Resize handle on its inward edge.
 *   - float: portaled to <body>, draggable by the title bar, resizable
 *     from the bottom-right corner.
 *   - hidden: removed from the tree; reveal via the small tab handle
 *     rendered in its dock rail.
 */

export interface PanelProps {
  id: string;
  title: string;
  defaults: PanelStateDefaults;
  children: ReactNode;
}

export function Panel({ id, title, defaults, children }: PanelProps) {
  const { state, setDock, setWidth, setHeight, setFloatPos, toggleCollapsed } =
    usePanelState(id, defaults);

  const onHeaderMouseDown = useCallback(
    (e: React.MouseEvent) => {
      // Drag the header to move the panel. When docked, dragging > 6px
      // pops it into float mode at the cursor; when already floating,
      // dragging just moves the card. Drop near the left or right edge
      // of the viewport to re-dock to that side.
      if (e.button !== 0) return;
      // Buttons inside the header handle their own clicks.
      if ((e.target as HTMLElement).closest('button')) return;
      e.preventDefault();
      const startX = e.clientX;
      const startY = e.clientY;
      const startFx = state.floatX;
      const startFy = state.floatY;
      let undocked = state.dock === 'float';
      let crossed = false;
      const DOCK_EDGE = 80;
      const DRAG_THRESHOLD = 6;

      const onMove = (ev: MouseEvent) => {
        const dx = ev.clientX - startX;
        const dy = ev.clientY - startY;
        if (!crossed && Math.hypot(dx, dy) >= DRAG_THRESHOLD) {
          crossed = true;
        }
        if (!crossed) return;
        if (!undocked) {
          // Pop out — start floating at the cursor's offset within the
          // panel. We approximate by placing the card so the cursor is
          // ~14px into its title bar.
          setDock('float');
          setFloatPos(ev.clientX - 60, ev.clientY - 14);
          undocked = true;
          return;
        }
        setFloatPos(startFx + dx, startFy + dy);
      };
      const onUp = (ev: MouseEvent) => {
        document.removeEventListener('mousemove', onMove);
        document.removeEventListener('mouseup', onUp);
        if (!crossed) return;
        const w = window.innerWidth;
        if (ev.clientX < DOCK_EDGE) setDock('left');
        else if (ev.clientX > w - DOCK_EDGE) setDock('right');
      };
      document.addEventListener('mousemove', onMove);
      document.addEventListener('mouseup', onUp);
    },
    [state.dock, state.floatX, state.floatY, setDock, setFloatPos],
  );

  // Hidden mode: render a small tab so the user can bring it back.
  if (state.dock === 'hidden') {
    return (
      <div className="panel-hidden-tab-wrap">
        <button
          type="button"
          className="panel-hidden-tab"
          title={`Show ${title}`}
          onClick={() => setDock('right')}
        >
          {title}
        </button>
      </div>
    );
  }

  const header = (
    <header
      className="panel-header"
      data-dragging-hint={state.dock === 'float' ? 1 : 0}
      onMouseDown={onHeaderMouseDown}
    >
      <span className="panel-title">{title}</span>
      <div className="panel-header-actions">
        <DockButton current={state.dock} target="left" onClick={() => setDock('left')} label="⫷" tip="Dock left" />
        <DockButton current={state.dock} target="right" onClick={() => setDock('right')} label="⫸" tip="Dock right" />
        <DockButton current={state.dock} target="float" onClick={() => setDock('float')} label="⧉" tip="Float" />
        <button
          type="button"
          className="panel-action"
          title={state.collapsed ? 'Expand' : 'Collapse'}
          onClick={toggleCollapsed}
        >
          {state.collapsed ? '▸' : '▾'}
        </button>
        <button
          type="button"
          className="panel-action panel-action-close"
          title="Hide"
          onClick={() => setDock('hidden')}
        >
          ×
        </button>
      </div>
    </header>
  );

  if (state.dock === 'float') {
    return createPortal(
      <FloatingShell
        x={state.floatX}
        y={state.floatY}
        width={state.width}
        height={state.height}
        collapsed={state.collapsed}
        onResize={(w, h) => { setWidth(w); setHeight(h); }}
      >
        {header}
        {!state.collapsed && <div className="panel-body panel-body-float">{children}</div>}
      </FloatingShell>,
      document.body,
    );
  }

  // Docked: width is fixed to state.width; height fills the rail.
  return (
    <div
      className={`panel panel-docked panel-dock-${state.dock}`}
      data-collapsed={state.collapsed ? 1 : 0}
      style={{ width: state.collapsed ? undefined : state.width }}
    >
      {header}
      {!state.collapsed && (
        <>
          <div className="panel-body panel-body-docked">{children}</div>
          <ResizeEdge
            side={state.dock}
            onResize={(dx) => setWidth(state.width + (state.dock === 'left' ? dx : -dx))}
          />
        </>
      )}
    </div>
  );
}

function DockButton({
  current, target, label, tip, onClick,
}: { current: PanelDock; target: PanelDock; label: string; tip: string; onClick: () => void }) {
  return (
    <button
      type="button"
      className="panel-action"
      data-active={current === target ? 1 : 0}
      title={tip}
      onClick={onClick}
    >
      {label}
    </button>
  );
}

/**
 * The vertical resize bar on the inward edge of a docked panel. Drag
 * left/right to widen or narrow.
 */
function ResizeEdge({ side, onResize }: { side: 'left' | 'right'; onResize: (dx: number) => void }) {
  const onMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    let lastX = e.clientX;
    const onMove = (ev: MouseEvent) => {
      const dx = ev.clientX - lastX;
      lastX = ev.clientX;
      onResize(dx);
    };
    const onUp = () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  };
  return (
    <div
      className={`panel-resize-edge panel-resize-edge-${side === 'left' ? 'right' : 'left'}`}
      onMouseDown={onMouseDown}
    />
  );
}

/**
 * Floating shell — wraps the panel in an absolutely-positioned card on
 * top of everything else, with a bottom-right resize grip. Drag the
 * grip to resize when floating; the header drag is handled by Panel.
 */
function FloatingShell({
  x, y, width, height, collapsed, onResize, children,
}: {
  x: number; y: number; width: number; height: number;
  collapsed: boolean;
  onResize: (w: number, h: number) => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement | null>(null);

  // Keep the card clamped on-screen when the window resizes underneath us.
  useEffect(() => {
    const clamp = () => {
      const el = ref.current;
      if (!el) return;
      const maxX = window.innerWidth - 80;
      const maxY = window.innerHeight - 40;
      if (x > maxX || y > maxY) {
        el.style.left = `${Math.min(x, maxX)}px`;
        el.style.top = `${Math.min(y, maxY)}px`;
      }
    };
    window.addEventListener('resize', clamp);
    return () => window.removeEventListener('resize', clamp);
  }, [x, y]);

  const onResizeStart = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const startW = width;
    const startH = height;
    const startX = e.clientX;
    const startY = e.clientY;
    const onMove = (ev: MouseEvent) => {
      onResize(startW + (ev.clientX - startX), startH + (ev.clientY - startY));
    };
    const onUp = () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  };

  return (
    <div
      ref={ref}
      className="panel panel-float"
      style={{ left: x, top: y, width, height: collapsed ? undefined : height }}
    >
      {children}
      {!collapsed && (
        <div className="panel-resize-grip" onMouseDown={onResizeStart} title="Resize" />
      )}
    </div>
  );
}
