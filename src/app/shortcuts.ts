import { useEffect } from 'react';
import { history } from '../editor/history';
import { useEditor } from '../editor/editor-state';
import { useOverlay } from '../editor/overlay-store';
import { nodeRegistry } from '../nodes/registry';

/**
 * App-wide keyboard shortcuts. Mounted once at the top of App.
 *
 *   - Ctrl/Cmd + Z       → undo
 *   - Ctrl/Cmd + Shift+Z → redo
 *   - Ctrl/Cmd + Y       → redo (Windows convention)
 *   - Ctrl/Cmd + S       → save (dispatched as a CustomEvent the FileMenu
 *                          listens to — avoids coupling here to file I/O)
 *   - Ctrl/Cmd + C       → copy selected nodes
 *   - Ctrl/Cmd + X       → cut selected nodes
 *   - Ctrl/Cmd + V       → paste at viewport centre
 *   - Ctrl/Cmd + D       → duplicate selected nodes (single quick action)
 *   - Tab                → open node search at the cursor / viewport centre
 *   - F                  → fit-view to the selection (or to all nodes when
 *                          nothing is selected) with a short zoom animation
 *
 * Skipped while focus is in an editable element (inputs, textareas,
 * contenteditable) so typing isn't hijacked.
 */
export function useGlobalShortcuts(): void {
  useEffect(() => {
    function isEditable(el: EventTarget | null): boolean {
      if (!(el instanceof HTMLElement)) return false;
      const tag = el.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
      return el.isContentEditable;
    }

    // Track the last known mouse position so paste / Tab-open can anchor
    // at the cursor rather than the viewport centre when available.
    let lastMouse: { x: number; y: number } | null = null;
    const onMouseMove = (e: MouseEvent) => {
      lastMouse = { x: e.clientX, y: e.clientY };
    };
    window.addEventListener('mousemove', onMouseMove, { passive: true });

    function viewportCentre(): { screen: { x: number; y: number } } {
      return {
        screen: { x: window.innerWidth / 2, y: window.innerHeight / 2 },
      };
    }

    function openNodeSearchAtCursor() {
      const screen = lastMouse ?? viewportCentre().screen;
      // Convert screen → flow coords without a React Flow handle here.
      // We dispatch via a custom event the Canvas listens to so we
      // stay decoupled from the React Flow API.
      window.dispatchEvent(
        new CustomEvent('floret:open-search', { detail: screen }),
      );
    }

    function onKey(e: KeyboardEvent) {
      const editable = isEditable(e.target);

      // Tab → open node search (no modifier).
      if (e.key === 'Tab' && !editable && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        openNodeSearchAtCursor();
        return;
      }

      // I → enter the selected subpatch (or the unique subpatch in the
      // current view if there's exactly one). No modifier.
      if ((e.key === 'i' || e.key === 'I') && !editable && !e.ctrlKey && !e.metaKey && !e.altKey) {
        const s = useEditor.getState();
        const isSubpatch = (id: string) => {
          const n = s.nodes.find((x) => x.id === id);
          if (!n) return false;
          return nodeRegistry.get(n.data.defType)?.regionKind === 'subpatch';
        };
        const selected = s.nodes.filter((n) => n.selected && isSubpatch(n.id));
        if (selected.length === 1) {
          e.preventDefault();
          s.enterSubpatch(selected[0]!.id);
        }
        return;
      }

      // F → fit-view to the current selection (or all nodes if nothing
      // is selected). Canvas listens via `floret:fit-selection` and
      // calls reactFlow.fitView with the chosen ids.
      if ((e.key === 'f' || e.key === 'F') && !editable && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        window.dispatchEvent(new CustomEvent('floret:fit-selection'));
        return;
      }

      // O → exit one subpatch level. No modifier.
      if ((e.key === 'o' || e.key === 'O') && !editable && !e.ctrlKey && !e.metaKey && !e.altKey) {
        const s = useEditor.getState();
        if (s.viewStack.length > 0) {
          e.preventDefault();
          s.exitSubpatch();
        }
        return;
      }

      // [ / ] → reorder the currently-selected edge on its multi pin.
      // No-op if no edge is selected, or if its target isn't a multi pin
      // (the store action checks that and short-circuits).
      if ((e.key === '[' || e.key === ']') && !editable && !e.ctrlKey && !e.metaKey && !e.altKey) {
        const s = useEditor.getState();
        const selected = s.edges.find((x) => x.selected);
        if (!selected) return;
        e.preventDefault();
        s.reorderMultiEdge(selected.id, e.key === '[' ? -1 : +1);
        return;
      }

      // Escape → close any open overlays even when focus is in an input.
      if (e.key === 'Escape') {
        const ov = useOverlay.getState();
        if (ov.searchOpen) ov.closeSearch();
        if (ov.contextOpen) ov.closeContext();
        // Don't preventDefault — let inputs still blur etc.
      }

      const mod = e.ctrlKey || e.metaKey;
      if (!mod) return;

      if (e.key === 's' || e.key === 'S') {
        if (editable) return;
        e.preventDefault();
        window.dispatchEvent(new CustomEvent('floret:save'));
        return;
      }

      if (e.key === 'z' || e.key === 'Z') {
        if (editable) return;
        e.preventDefault();
        if (e.shiftKey) history.redo();
        else history.undo();
        return;
      }

      if (e.key === 'y' || e.key === 'Y') {
        if (editable) return;
        e.preventDefault();
        history.redo();
        return;
      }

      if (e.key === 'c' || e.key === 'C') {
        if (editable) return;
        e.preventDefault();
        useEditor.getState().copySelected();
        return;
      }

      if (e.key === 'x' || e.key === 'X') {
        if (editable) return;
        e.preventDefault();
        useEditor.getState().copySelected();
        useEditor.getState().removeSelected();
        return;
      }

      if (e.key === 'v' || e.key === 'V') {
        if (editable) return;
        e.preventDefault();
        // Anchor paste at the cursor's flow position. Canvas listens to a
        // window event and converts via React Flow's projector.
        const screen = lastMouse ?? viewportCentre().screen;
        window.dispatchEvent(
          new CustomEvent('floret:paste', { detail: screen }),
        );
        return;
      }

      if (e.key === 'd' || e.key === 'D') {
        if (editable) return;
        e.preventDefault();
        const s = useEditor.getState();
        // Duplicate every currently-selected node, offset 24 px.
        const selected = s.nodes.filter((n) => n.selected);
        if (selected.length === 0) return;
        for (const n of selected) s.duplicateNode(n.id);
        return;
      }

      if (e.key === 'g' || e.key === 'G') {
        if (editable) return;
        e.preventDefault();
        useEditor.getState().groupSelectionIntoSubpatch();
        return;
      }
    }

    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('mousemove', onMouseMove);
    };
  }, []);
}
