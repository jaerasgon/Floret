import { useCallback, useEffect, useState } from 'react';

/**
 * Where a panel lives in the shell. `left` / `right` dock it to the
 * editor pane's vertical rails; `float` makes it a draggable card on
 * top of the editor; `hidden` removes it from the layout (the user
 * can bring it back from the View menu or via the tab handle).
 */
export type PanelDock = 'left' | 'right' | 'float' | 'hidden';

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
}

export interface PanelStateDefaults {
  dock: PanelDock;
  width: number;
  height: number;
  floatX: number;
  floatY: number;
  collapsed?: boolean;
}

const LS_PREFIX = 'floret.panel.';

function load(id: string, defaults: PanelStateDefaults): PanelState {
  try {
    const raw = localStorage.getItem(LS_PREFIX + id);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<PanelState>;
      return {
        dock: parsed.dock ?? defaults.dock,
        width: parsed.width ?? defaults.width,
        height: parsed.height ?? defaults.height,
        floatX: parsed.floatX ?? defaults.floatX,
        floatY: parsed.floatY ?? defaults.floatY,
        collapsed: parsed.collapsed ?? defaults.collapsed ?? false,
      };
    }
  } catch { /* ignore */ }
  return {
    dock: defaults.dock,
    width: defaults.width,
    height: defaults.height,
    floatX: defaults.floatX,
    floatY: defaults.floatY,
    collapsed: defaults.collapsed ?? false,
  };
}

function save(id: string, state: PanelState): void {
  try {
    localStorage.setItem(LS_PREFIX + id, JSON.stringify(state));
  } catch { /* ignore */ }
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

  return {
    state,
    setDock,
    setWidth,
    setHeight,
    setFloatPos,
    setCollapsed,
    toggleCollapsed,
  };
}
