import { useEffect, type Dispatch, type MutableRefObject, type SetStateAction } from 'react';
import { useReactFlow } from '@xyflow/react';
import { useEditor } from '../editor-state';

type Screen = { x: number; y: number };
type DrawingRegion = { kind: 'foreach' | 'if' | 'cache'; startFlow: Screen; currentFlow: Screen } | null;

/**
 * Bridge window-level keyboard events (Tab opens search, Ctrl+V pastes,
 * F fits selection, focus-node) into React Flow's coordinate projector.
 * The listeners must live inside the provider because `useReactFlow`
 * only works there.
 */
export function useCanvasEventBus(deps: {
  openSearch: (o: { screenPosition: Screen; flowPosition: Screen }) => void;
  openFavorites: (o: { screenPosition: Screen; flowPosition: Screen }) => void;
  pasteAt: (pos: Screen) => string[];
}): void {
  const reactFlow = useReactFlow();
  const { openSearch, openFavorites, pasteAt } = deps;
  useEffect(() => {
    function onOpenSearch(e: Event) {
      const ev = e as CustomEvent<Screen>;
      const screen = ev.detail;
      const flow = reactFlow.screenToFlowPosition(screen);
      openSearch({ screenPosition: screen, flowPosition: flow });
    }
    function onOpenFavorites(e: Event) {
      const ev = e as CustomEvent<Screen>;
      const screen = ev.detail;
      const flow = reactFlow.screenToFlowPosition(screen);
      openFavorites({ screenPosition: screen, flowPosition: flow });
    }
    function onPaste(e: Event) {
      const ev = e as CustomEvent<Screen>;
      const flow = reactFlow.screenToFlowPosition(ev.detail);
      pasteAt(flow);
    }
    function onFocusNode(e: Event) {
      const ev = e as CustomEvent<string>;
      const id = ev.detail;
      if (!id) return;
      // Pan + zoom to the node. fitView accepts node ids via `nodes` filter.
      reactFlow.fitView({ nodes: [{ id }], duration: 300, padding: 0.4 });
    }
    function onFitSelection() {
      // Fit-view to the current selection (or to every node if nothing
      // is selected). Triggered by the global F shortcut.
      const s = useEditor.getState();
      const ids = s.nodes.filter((n) => n.selected).map((n) => ({ id: n.id }));
      if (ids.length > 0) {
        reactFlow.fitView({ nodes: ids, duration: 300, padding: 0.4 });
      } else {
        reactFlow.fitView({ duration: 300, padding: 0.2 });
      }
    }
    function onFitGraph() {
      // Frame the whole graph once React Flow has measured node sizes —
      // fitView is a no-op against unmeasured (0-size) nodes, and on a cold
      // load measurement can lag the patch by several hundred ms. Poll the
      // node store until every node has a measured width, then fit (bounded
      // so a stuck measure can't loop forever).
      let tries = 0;
      const tick = () => {
        const ns = reactFlow.getNodes();
        const ready = ns.length > 0 && ns.every((n) => (n.measured?.width ?? n.width ?? 0) > 0);
        if (ready || tries++ > 60) reactFlow.fitView({ duration: 300, padding: 0.2 });
        else setTimeout(tick, 40);
      };
      tick();
    }
    window.addEventListener('floret:open-search', onOpenSearch);
    window.addEventListener('floret:open-favorites', onOpenFavorites);
    window.addEventListener('floret:paste', onPaste);
    window.addEventListener('floret:focus-node', onFocusNode);
    window.addEventListener('floret:fit-selection', onFitSelection);
    window.addEventListener('floret:fit-graph', onFitGraph);
    return () => {
      window.removeEventListener('floret:open-search', onOpenSearch);
      window.removeEventListener('floret:open-favorites', onOpenFavorites);
      window.removeEventListener('floret:paste', onPaste);
      window.removeEventListener('floret:focus-node', onFocusNode);
      window.removeEventListener('floret:fit-selection', onFitSelection);
      window.removeEventListener('floret:fit-graph', onFitGraph);
    };
  }, [openSearch, openFavorites, pasteAt, reactFlow]);
}

/**
 * Region draw tool (armed from the palette). Clicking a region in the
 * palette arms `armedRegionKind`; the canvas then shows a crosshair and a
 * left press-drag on the empty pane draws the region rectangle (press =
 * first corner, release = opposite corner). Escape, a right-click, or
 * completing the draw disarms.
 */
export function useRegionDrawTool(
  armedRegionKind: 'foreach' | 'if' | 'cache' | null,
  setDrawingRegion: Dispatch<SetStateAction<DrawingRegion>>,
  deps: {
    addRegion: (
      kind: 'foreach' | 'if' | 'cache',
      pos: Screen,
      opts: { width: number; height: number; autoReparent: boolean },
    ) => string;
    disarmRegionDraw: () => void;
  },
): void {
  const reactFlow = useReactFlow();
  const { addRegion, disarmRegionDraw } = deps;
  useEffect(() => {
    if (!armedRegionKind) return;
    let startFlow: Screen | null = null;

    function onDown(e: MouseEvent) {
      if (e.button !== 0) return;
      // Only begin on the empty canvas pane — not on a node, edge, panel,
      // or control — so the tool never hijacks other interactions.
      if (!(e.target as HTMLElement).closest?.('.react-flow__pane')) return;
      e.preventDefault();
      e.stopPropagation();
      startFlow = reactFlow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
      setDrawingRegion({ kind: armedRegionKind!, startFlow, currentFlow: startFlow });
    }
    function onMove(e: MouseEvent) {
      if (!startFlow) return;
      const flow = reactFlow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
      setDrawingRegion((prev) => (prev ? { ...prev, currentFlow: flow } : prev));
    }
    function onUp(e: MouseEvent) {
      if (!startFlow) return;
      const begin = startFlow;
      startFlow = null;
      const flow = reactFlow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
      const x = Math.min(begin.x, flow.x);
      const y = Math.min(begin.y, flow.y);
      const w = Math.max(60, Math.abs(flow.x - begin.x));
      const h = Math.max(60, Math.abs(flow.y - begin.y));
      addRegion(armedRegionKind!, { x, y }, { width: w, height: h, autoReparent: true });
      setDrawingRegion(null);
      disarmRegionDraw();
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') { startFlow = null; setDrawingRegion(null); disarmRegionDraw(); }
    }
    // Capture-phase down/up so we beat React Flow's pane handlers (box-
    // select / pan) before they start.
    window.addEventListener('mousedown', onDown, true);
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp, true);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', onDown, true);
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp, true);
      window.removeEventListener('keydown', onKey);
    };
  }, [armedRegionKind, reactFlow, addRegion, disarmRegionDraw, setDrawingRegion]);
}

/**
 * Spacebar tracking — flips the reparent-on-drop modifier. Listens at
 * window level so it works regardless of focus, but skips when typing in
 * an input/textarea/contentEditable. Mirrors the held state into
 * `spaceDownRef` (read mid-drag) and `setReparentMode` (re-renders to
 * strip child `extent`).
 */
export function useSpacebarReparent(
  spaceDownRef: MutableRefObject<boolean>,
  setReparentMode: Dispatch<SetStateAction<boolean>>,
): void {
  useEffect(() => {
    function isEditing(target: EventTarget | null): boolean {
      const el = target as HTMLElement | null;
      if (!el) return false;
      const tag = el.tagName;
      return tag === 'INPUT' || tag === 'TEXTAREA' || el.isContentEditable;
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.code !== 'Space' || isEditing(e.target)) return;
      // Prevent page scroll on space.
      e.preventDefault();
      if (spaceDownRef.current) return;
      spaceDownRef.current = true;
      setReparentMode(true);
      document.body.setAttribute('data-reparent-mode', '1');
    }
    function onKeyUp(e: KeyboardEvent) {
      if (e.code !== 'Space') return;
      if (!spaceDownRef.current) return;
      spaceDownRef.current = false;
      setReparentMode(false);
      document.body.removeAttribute('data-reparent-mode');
    }
    function onBlur() {
      spaceDownRef.current = false;
      setReparentMode(false);
      document.body.removeAttribute('data-reparent-mode');
    }
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', onBlur);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
    };
  }, [spaceDownRef, setReparentMode]);
}
