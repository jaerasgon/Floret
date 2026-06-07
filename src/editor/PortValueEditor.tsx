import { useEffect, useRef, useState } from 'react';
import { useEditor } from './editor-state';
import { nodeRegistry } from '../nodes/registry';
import type { PortType } from '../runtime/types';

/**
 * A single instance of this lives at the App level; it shows a small
 * floating input whenever a port is being edited. Position + target are
 * controlled by the imperative `openPortValueEditor` helper below.
 */

interface EditorState {
  open: boolean;
  nodeId: string;
  portName: string;
  portType: PortType;
  initialValue: unknown;
  screenPosition: { x: number; y: number };
}

type Listener = (s: EditorState) => void;

const closedState: EditorState = {
  open: false,
  nodeId: '',
  portName: '',
  portType: 'number',
  initialValue: undefined,
  screenPosition: { x: 0, y: 0 },
};

let _state: EditorState = closedState;
const _listeners = new Set<Listener>();

function setState(next: EditorState) {
  _state = next;
  for (const l of _listeners) l(_state);
}

/** Open the editor at a screen position for a specific (node, port). */
export function openPortValueEditor(args: {
  nodeId: string;
  portName: string;
  portType: PortType;
  initialValue: unknown;
  screenPosition: { x: number; y: number };
}): void {
  setState({ open: true, ...args });
}

function close() {
  setState(closedState);
}

/**
 * Mounted once at the app root. Subscribes to the imperative state above
 * and renders a small floating input when needed.
 */
export function PortValueEditor() {
  const [s, setS] = useState<EditorState>(_state);
  const setNodePortValue = useEditor((st) => st.setNodePortValue);
  const clearNodePortValue = useEditor((st) => st.clearNodePortValue);

  useEffect(() => {
    const listener: Listener = (next) => setS(next);
    _listeners.add(listener);
    return () => { _listeners.delete(listener); };
  }, []);

  const inputRef = useRef<HTMLInputElement | null>(null);
  const [draft, setDraft] = useState<string>('');

  useEffect(() => {
    if (s.open) {
      setDraft(s.initialValue === undefined || s.initialValue === null
        ? ''
        : String(s.initialValue));
      requestAnimationFrame(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      });
    }
  }, [s.open, s.initialValue]);

  useEffect(() => {
    if (!s.open) return;
    const onDown = (e: MouseEvent) => {
      const root = document.getElementById('floret-port-editor');
      if (root && !root.contains(e.target as Node)) close();
    };
    window.addEventListener('mousedown', onDown, true);
    return () => window.removeEventListener('mousedown', onDown, true);
  }, [s.open]);

  if (!s.open) return null;

  const def = nodeRegistry.get(useEditor.getState().nodes.find((n) => n.id === s.nodeId)?.data.defType ?? '');
  const port = def?.inputs.find((p) => p.name === s.portName);
  const label = port?.label ?? s.portName;

  const commit = () => {
    const raw = draft.trim();
    if (raw === '') {
      // Empty input → clear the override (revert to NodeDef default).
      clearNodePortValue(s.nodeId, s.portName);
      close();
      return;
    }
    let value: unknown = raw;
    if (s.portType === 'number') {
      const n = Number(raw);
      if (!Number.isFinite(n)) {
        // Invalid — refuse and keep editor open.
        return;
      }
      value = n;
    } else if (s.portType === 'boolean') {
      value = raw === 'true' || raw === '1';
    }
    setNodePortValue(s.nodeId, s.portName, value);
    close();
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      commit();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      close();
    }
  };

  const POPOVER_W = 200;
  const left = Math.min(s.screenPosition.x, window.innerWidth - POPOVER_W - 8);
  const top = Math.min(s.screenPosition.y, window.innerHeight - 80);

  return (
    <div
      id="floret-port-editor"
      className="port-editor"
      style={{ left, top, width: POPOVER_W }}
    >
      <div className="port-editor-label">
        <span className="port-editor-label-text">{label}</span>
        <span className="port-editor-label-type">{s.portType}</span>
      </div>
      <input
        ref={inputRef}
        className="port-editor-input nodrag"
        type={s.portType === 'number' ? 'number' : 'text'}
        step="any"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={onKey}
        onBlur={commit}
        placeholder={port?.default !== undefined ? `default: ${String(port.default)}` : ''}
      />
      <div className="port-editor-hint">Enter to set · empty to clear · Esc</div>
    </div>
  );
}
