import { useCallback, useEffect, useState } from 'react';
import { loadJSON, saveJSON } from '../../persistence/local-store';

/**
 * Where a panel lives in the shell. `left` / `right` dock it to the
 * editor pane's vertical rails; `float` makes it a draggable card on
 * top of the editor; `hidden` removes it from the layout (the user
 * can bring it back from the View menu or via the tab handle).
 */
export type PanelDock = 'left' | 'right' | 'bottom' | 'float' | 'hidden';

export interface PanelState {
  dock: PanelDock;
  /** Width in px when docked, or the full size of the floating card. */
  width: number;
  /** Height — only used when floating; docked panels fill the rail. */
  height: number;
  /** Floating card position in viewport pixels. */
  floatX: number;
  floatY: number;
  /** True when the body is hidden behind a collapsed header. */
  collapsed: boolean;
  /** In-rail stacking order (CSS `order`). Lower = closer to top of rail.
   *  Fractional values are normalised back to integers after every drop. */
  order: number;
  /** Which column within the dock side. 0 = closest to canvas; higher =
   *  further out. The editor renders one rail container per active
   *  (dock, column) pair; columns are renormalised to 0..N-1 after
   *  every drop. */
  column: number;
}

export interface PanelStateDefaults {
  dock: PanelDock;
  width: number;
  height: number;
  floatX: number;
  floatY: number;
  collapsed?: boolean;
  order?: number;
  column?: number;
}

const LS_PREFIX = 'floret.panel.';

function load(id: string, defaults: PanelStateDefaults): PanelState {
  const parsed = loadJSON<Partial<PanelState>>(LS_PREFIX + id, {});
  return {
    dock: parsed.dock ?? defaults.dock,
    width: parsed.width ?? defaults.width,
    height: parsed.height ?? defaults.height,
    floatX: parsed.floatX ?? defaults.floatX,
    floatY: parsed.floatY ?? defaults.floatY,
    collapsed: parsed.collapsed ?? defaults.collapsed ?? false,
    order: parsed.order ?? defaults.order ?? 0,
    column: parsed.column ?? defaults.column ?? 0,
  };
}

function save(id: string, state: PanelState): void {
  saveJSON(LS_PREFIX + id, state);
}

/**
 * Persist a panel's layout choices (dock, size, float position,
 * collapsed) across reloads, keyed by `id`. Returns the current state
 * and a small set of mutators rather than a single setter so call
 * sites read like English: `update.setDock('float')`.
 */
export function usePanelState(id: string, defaults: PanelStateDefaults) {
  const [state, setState] = useState<PanelState>(() => load(id, defaults));

  useEffect(() => { save(id, state); }, [id, state]);

  const setDock = useCallback((dock: PanelDock) => {
    setState((s) => ({ ...s, dock }));
  }, []);
  const setWidth = useCallback((width: number) => {
    setState((s) => ({ ...s, width: Math.max(160, Math.min(800, width)) }));
  }, []);
  const setHeight = useCallback((height: number) => {
    setState((s) => ({ ...s, height: Math.max(140, Math.min(1200, height)) }));
  }, []);
  const setFloatPos = useCallback((floatX: number, floatY: number) => {
    setState((s) => ({ ...s, floatX, floatY }));
  }, []);
  const setCollapsed = useCallback((collapsed: boolean) => {
    setState((s) => ({ ...s, collapsed }));
  }, []);
  const toggleCollapsed = useCallback(() => {
    setState((s) => ({ ...s, collapsed: !s.collapsed }));
  }, []);
  const setOrder = useCallback((order: number) => {
    setState((s) => ({ ...s, order }));
  }, []);
  const setColumn = useCallback((column: number) => {
    setState((s) => ({ ...s, column }));
  }, []);
  // Incremental resize. A drag fires many small deltas while the handler
  // closure holds a frozen `state` snapshot — applying each delta to the
  // *current* value via the functional updater is the only way it
  // accumulates correctly across the drag.
  const resizeWidthBy = useCallback((dx: number) => {
    setState((s) => ({ ...s, width: Math.max(160, Math.min(800, s.width + dx)) }));
  }, []);
  const resizeHeightBy = useCallback((dy: number) => {
    setState((s) => ({ ...s, height: Math.max(140, Math.min(1200, s.height + dy)) }));
  }, []);

  return {
    state,
    setDock,
    setWidth,
    setHeight,
    resizeWidthBy,
    resizeHeightBy,
    setFloatPos,
    setCollapsed,
    toggleCollapsed,
    setOrder,
    setColumn,
  };
}
