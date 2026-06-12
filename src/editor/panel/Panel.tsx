import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { usePanelState, type PanelStateDefaults } from './usePanelState';
import { beginDrag, updateDrag, type DropTarget } from './dragStore';
import { getPanel, listPanels, registerPanel, updatePanel } from './panelRegistry';

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
  /** Hide the × (close → hidden) button from the header. Used by the
   *  Preview panel when the patch contains a Renderer node so the user
   *  can't strand themselves with no view of their work. The hidden-tab
   *  restore affordance is NOT affected — already-hidden panels can
   *  still be recovered. */
  hideClose?: boolean;
  /** Optional header content rendered between the title and the
   *  action buttons (collapse + close). Lets a panel inject its own
   *  controls (renderer picker, pause/refresh, etc.) into the header. */
  headerActions?: ReactNode;
}

export function Panel({ id, title, defaults, children, hideClose, headerActions }: PanelProps) {
  const { state, setDock, setWidth, setHeight, resizeWidthBy, resizeHeightBy, setFloatPos, toggleCollapsed, setOrder, setColumn } =
    usePanelState(id, defaults);

  // Root element ref — used by the registry so the overlay can read
  // this panel's bounding rect when computing drop zones.
  const rootRef = useRef<HTMLElement | null>(null);

  // Rail container ref — for docked panels, we portal into the
  // matching `panel-rail-<dock>-<column>` element. Resolved
  // synchronously after each commit via useLayoutEffect.
  const [railEl, setRailEl] = useState<HTMLElement | null>(null);
  useLayoutEffect(() => {
    if (state.dock === 'bottom') {
      setRailEl(document.getElementById('panel-rail-bottom-0'));
      return;
    }
    if (state.dock !== 'left' && state.dock !== 'right') {
      setRailEl(null);
      return;
    }
    setRailEl(document.getElementById(`panel-rail-${state.dock}-${state.column}`));
  }, [state.dock, state.column]);

  // Register with the global panel registry. The registry stays in
  // sync with React state via updatePanel() below.
  useEffect(() => {
    return registerPanel({
      id,
      dock: state.dock,
      column: state.column,
      order: state.order,
      getRect: () => rootRef.current?.getBoundingClientRect() ?? null,
      setDock,
      setColumn,
      setOrder,
      setFloatPos,
    });
    // Intentionally only on mount/unmount; mutable fields are pushed
    // through updatePanel below to avoid re-registering on every
    // state change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  useEffect(() => {
    updatePanel(id, { dock: state.dock, column: state.column, order: state.order });
  }, [id, state.dock, state.column, state.order]);

  const onHeaderMouseDown = useCallback(
    (e: React.MouseEvent) => {
      // Header drag → live-float the panel (so it follows the cursor)
      // AND open a drop session so the <DropZoneOverlay> can resolve
      // the cursor into a drop target (per-panel quadrant, rail-edge,
      // or free space). On mouseup the overlay calls `onDrop` with
      // the resolved target.
      if (e.button !== 0) return;
      if ((e.target as HTMLElement).closest('button')) return;
      e.preventDefault();
      const startX = e.clientX;
      const startY = e.clientY;
      let started = false;
      const DRAG_THRESHOLD = 6;
      // Capture where on the panel the cursor was pressed so the
      // floating preview tracks the cursor without snapping the
      // titlebar to the cursor's exact position.
      const rect = rootRef.current?.getBoundingClientRect();
      const grabOffsetX = rect ? startX - rect.left : 60;
      const grabOffsetY = rect ? startY - rect.top : 14;

      // Re-issue column values across one dock side so they're a tight
      // 0..N-1 range after a drop. Returns the final column self should
      // get; assumes self has just been assigned `selfColumnHint`
      // (typically a fractional value used to slot between siblings).
      const normaliseColumns = (side: 'left' | 'right', selfColumnHint: number): number => {
        const onSide = listPanels()
          .filter(p => p.dock === side && p.id !== id)
          .map(p => ({ id: p.id, col: p.column, setColumn: p.setColumn }));
        const all = [...onSide, { id, col: selfColumnHint, setColumn }];
        const sorted = [...new Set(all.map(a => a.col))].sort((a, b) => a - b);
        const remap = new Map<number, number>();
        sorted.forEach((c, i) => remap.set(c, i));
        all.forEach(a => {
          const finalCol = remap.get(a.col)!;
          if (a.id === id) return;
          if (a.col !== finalCol) a.setColumn(finalCol);
        });
        return remap.get(selfColumnHint)!;
      };

      // Compact a side that self is *leaving* — drops any empty
      // columns that opened up. Panels are reseated to 0..N-1 by
      // sorted column.
      const compactSide = (side: 'left' | 'right') => {
        const remaining = listPanels()
          .filter(p => p.dock === side && p.id !== id);
        const sorted = [...new Set(remaining.map(p => p.column))].sort((a, b) => a - b);
        const remap = new Map<number, number>();
        sorted.forEach((c, i) => remap.set(c, i));
        remaining.forEach(p => {
          const finalCol = remap.get(p.column)!;
          if (p.column !== finalCol) p.setColumn(finalCol);
        });
      };

      // Re-issue in-column orders so each panel in (side, column) gets
      // a distinct integer; insert self at `intent`.
      const placeInColumn = (
        side: 'left' | 'right',
        column: number,
        intent: { kind: 'append' | 'prepend' } | { kind: 'before' | 'after'; panelId: string },
      ): number => {
        const siblings = listPanels()
          .filter(p => p.dock === side && p.column === column && p.id !== id)
          .sort((a, b) => {
            if (a.order !== b.order) return a.order - b.order;
            const ar = a.getRect();
            const br = b.getRect();
            if (!ar || !br) return 0;
            return ar.top - br.top;
          });
        let insertAt: number;
        // Check the panelId-bearing member first so TS narrows `intent`
        // to it inside the block. (The other member's `kind` is itself a
        // union — `'append' | 'prepend'` — so an `else` tail doesn't fully
        // eliminate it, and `intent.panelId` wouldn't typecheck there.)
        if (intent.kind === 'before' || intent.kind === 'after') {
          const idx = siblings.findIndex(s => s.id === intent.panelId);
          if (idx < 0) insertAt = siblings.length;
          else insertAt = intent.kind === 'before' ? idx : idx + 1;
        } else if (intent.kind === 'prepend') {
          insertAt = 0;
        } else {
          insertAt = siblings.length; // append
        }
        siblings.forEach((p, i) => p.setOrder(i < insertAt ? i : i + 1));
        return insertAt;
      };

      // Move self to (side, column) with an in-column intent. Handles
      // dock, column normalisation, order placement, and compacting
      // the old side when self moves rails (so empty columns collapse).
      const moveTo = (
        side: 'left' | 'right',
        columnHint: number,
        intent: { kind: 'append' | 'prepend' } | { kind: 'before' | 'after'; panelId: string },
      ) => {
        const oldSide: 'left' | 'right' | null =
          state.dock === 'left' || state.dock === 'right' ? state.dock : null;
        setDock(side);
        const finalCol = normaliseColumns(side, columnHint);
        setColumn(finalCol);
        const finalOrder = placeInColumn(side, finalCol, intent);
        setOrder(finalOrder);
        // If self changed sides, the old side might now have an empty
        // column slot — squeeze it out.
        if (oldSide && oldSide !== side) compactSide(oldSide);
      };

      const onDrop = (target: DropTarget, dropX: number, dropY: number) => {
        if (target?.kind === 'edge') {
          if (target.side === 'bottom') {
            // Bottom rail is a single horizontal strip (no columns) —
            // append after the current rightmost bottom panel.
            const oldSide: 'left' | 'right' | null =
              state.dock === 'left' || state.dock === 'right' ? state.dock : null;
            const maxOrder = listPanels()
              .filter(p => p.dock === 'bottom' && p.id !== id)
              .reduce((m, p) => Math.max(m, p.order), -1);
            setDock('bottom');
            setColumn(0);
            setOrder(maxOrder + 1);
            // A side panel's height (often 500–600) makes a clumsy strip —
            // start bottom docks at a slimmer default the user can grow.
            if (state.height > 320) setHeight(240);
            if (oldSide) compactSide(oldSide);
            return;
          }
          // Edge of viewport → land at the *outermost* column on that side.
          const outerCol = listPanels()
            .filter(p => p.dock === target.side && p.id !== id)
            .reduce((m, p) => Math.max(m, p.column), -1) + 1;
          moveTo(target.side, outerCol, { kind: 'append' });
          return;
        }
        if (target?.kind === 'panel') {
          const sibling = getPanel(target.panelId);
          if (!sibling || sibling.dock !== 'left' && sibling.dock !== 'right') {
            setDock('float');
            setFloatPos(dropX - grabOffsetX, dropY - grabOffsetY);
            return;
          }
          const side = sibling.dock as 'left' | 'right';
          // 'above' / 'below' → SAME column as sibling, before / after.
          if (target.position === 'above') {
            moveTo(side, sibling.column, { kind: 'before', panelId: sibling.id });
            return;
          }
          if (target.position === 'below') {
            moveTo(side, sibling.column, { kind: 'after', panelId: sibling.id });
            return;
          }
          // 'left' / 'right' of a panel → create a NEW column adjacent
          // to the sibling's column on the visual side the user dropped.
          //
          // For RIGHT-side panels: columns grow outward to the right
          // (column 0 = nearest canvas). LEFT of target = closer to
          // canvas = smaller column. RIGHT of target = further out =
          // larger column.
          //
          // For LEFT-side panels: columns grow outward to the LEFT.
          // LEFT of target = further from canvas = larger column.
          // RIGHT of target = closer to canvas = smaller column.
          let hint: number;
          if (side === 'right') {
            hint = target.position === 'left' ? sibling.column - 0.5 : sibling.column + 0.5;
          } else {
            hint = target.position === 'left' ? sibling.column + 0.5 : sibling.column - 0.5;
          }
          // Self is the lone occupant of a freshly-created column —
          // skip placeInColumn (which would consult the still-stale
          // registry) and just setOrder(0). normaliseColumns has
          // already remapped sibling columns and returned self's
          // final integer column.
          const oldSide: 'left' | 'right' | null =
            state.dock === 'left' || state.dock === 'right' ? state.dock : null;
          setDock(side);
          const finalCol = normaliseColumns(side, hint);
          setColumn(finalCol);
          setOrder(0);
          if (oldSide && oldSide !== side) compactSide(oldSide);
          return;
        }
        // Free space — float at the cursor.
        setDock('float');
        setFloatPos(dropX - grabOffsetX, dropY - grabOffsetY);
      };

      const onMove = (ev: MouseEvent) => {
        if (!started) {
          if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < DRAG_THRESHOLD) return;
          started = true;
          // Live-float immediately: the panel detaches from its rail
          // and follows the cursor, so dragging feels like a real
          // window grab rather than "phantom chip moves while panel
          // stays put".
          setDock('float');
          setFloatPos(ev.clientX - grabOffsetX, ev.clientY - grabOffsetY);
          beginDrag({
            id,
            title,
            x: ev.clientX,
            y: ev.clientY,
            width: state.width,
            hover: null,
            onDrop,
          });
        } else {
          setFloatPos(ev.clientX - grabOffsetX, ev.clientY - grabOffsetY);
          updateDrag({ x: ev.clientX, y: ev.clientY });
        }
      };
      const onUp = () => {
        document.removeEventListener('mousemove', onMove);
        document.removeEventListener('mouseup', onUp);
        // Overlay owns the drop commit.
      };
      document.addEventListener('mousemove', onMove);
      document.addEventListener('mouseup', onUp);
    },
    [id, title, state.width, state.height, setDock, setFloatPos, setOrder, setColumn, setHeight],
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
      <span className="panel-title" title="Drag to move — drop on an edge to dock, drop in the canvas to float.">
        <span className="panel-grip" aria-hidden>⋮⋮</span>
        {title}
      </span>
      <div className="panel-header-actions">
        {headerActions}
        <button
          type="button"
          className="panel-action"
          title={state.collapsed ? 'Expand' : 'Collapse'}
          onClick={toggleCollapsed}
        >
          {state.collapsed ? '▸' : '▾'}
        </button>
        {!hideClose && (
          <button
            type="button"
            className="panel-action panel-action-close"
            title="Hide"
            onClick={() => setDock('hidden')}
          >
            ×
          </button>
        )}
      </div>
    </header>
  );

  if (state.dock === 'float') {
    return createPortal(
      <FloatingShell
        rootRef={rootRef as React.MutableRefObject<HTMLDivElement | null>}
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

  // Docked: portal into the matching rail container (resolved above by
  // useLayoutEffect). The rail is a flex column, so panels stack
  // vertically within it; `style.order` decides vertical position.
  const isBottom = state.dock === 'bottom';
  const dockedNode = (
    <div
      ref={rootRef as React.MutableRefObject<HTMLDivElement | null>}
      className={`panel panel-docked panel-dock-${state.dock}`}
      data-panel-id={id}
      data-collapsed={state.collapsed ? 1 : 0}
      style={{
        // Left/right rails share a width (each panel expresses a min-
        // width preference); bottom panels get a fixed strip height so
        // the top-edge resize maps directly to it and tall content
        // scrolls inside rather than stretching the strip.
        ...(isBottom
          ? { height: state.collapsed ? undefined : state.height }
          : { minWidth: state.collapsed ? undefined : state.width }),
        // Multiply by 1000 so fractional `state.order` values (used
        // briefly during a drop before normalisation completes)
        // survive integer truncation in CSS `order`.
        order: Math.round(state.order * 1000),
      }}
    >
      {header}
      {!state.collapsed && (
        <>
          <div className="panel-body panel-body-docked">{children}</div>
          {isBottom ? (
            // Drag the top edge up to grow the strip (dragging up = -dy).
            <ResizeEdge axis="vertical" onResize={(dy) => resizeHeightBy(-dy)} />
          ) : (
            <ResizeEdge
              axis="horizontal"
              side={state.dock === 'left' ? 'left' : 'right'}
              onResize={(dx) => resizeWidthBy(state.dock === 'left' ? dx : -dx)}
            />
          )}
        </>
      )}
    </div>
  );
  // Wait one tick for the rail to mount before portaling — avoids a
  // first-paint flash of panels in the wrong place.
  if (!railEl) return null;
  return createPortal(dockedNode, railEl);
}

/**
 * Resize bar on a docked panel's inward edge. Horizontal panels (left/
 * right rails) get a vertical bar dragged left/right to change width;
 * bottom-rail panels get a horizontal bar on their top edge dragged
 * up/down to change height. `onResize` receives the cursor delta along
 * the relevant axis.
 */
function ResizeEdge(
  props:
    | { axis: 'horizontal'; side: 'left' | 'right'; onResize: (dx: number) => void }
    | { axis: 'vertical'; onResize: (dy: number) => void },
) {
  const onMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    let last = props.axis === 'horizontal' ? e.clientX : e.clientY;
    const onMove = (ev: MouseEvent) => {
      const cur = props.axis === 'horizontal' ? ev.clientX : ev.clientY;
      const delta = cur - last;
      last = cur;
      props.onResize(delta);
    };
    const onUp = () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  };
  const cls = props.axis === 'vertical'
    ? 'panel-resize-edge panel-resize-edge-top'
    : `panel-resize-edge panel-resize-edge-${props.side === 'left' ? 'right' : 'left'}`;
  return <div className={cls} onMouseDown={onMouseDown} />;
}

/**
 * Floating shell — wraps the panel in an absolutely-positioned card on
 * top of everything else, with a bottom-right resize grip. Drag the
 * grip to resize when floating; the header drag is handled by Panel.
 */
function FloatingShell({
  rootRef, x, y, width, height, collapsed, onResize, children,
}: {
  rootRef?: React.MutableRefObject<HTMLDivElement | null>;
  x: number; y: number; width: number; height: number;
  collapsed: boolean;
  onResize: (w: number, h: number) => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  // Forward the inner ref to the registry-tracked rootRef so the
  // panel registry can read the floating shell's bounding rect.
  const setRef = (el: HTMLDivElement | null) => {
    ref.current = el;
    if (rootRef) rootRef.current = el;
  };

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
      ref={setRef}
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
