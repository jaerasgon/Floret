import { useEffect } from 'react';

/**
 * Keyboard-shortcut reference. Opened from the toolbar's help affordance or
 * by pressing `?` anywhere outside an editable field (see shortcuts.ts).
 * Reuses the Settings modal shell for a consistent look; Esc / backdrop
 * click closes.
 *
 * Keep this list in sync with the handlers in `shortcuts.ts`.
 */
interface Props {
  onClose: () => void;
}

interface Shortcut {
  keys: string[];
  label: string;
}

interface Group {
  title: string;
  items: Shortcut[];
}

const GROUPS: Group[] = [
  {
    title: 'Editing',
    items: [
      { keys: ['Ctrl', 'Z'], label: 'Undo' },
      { keys: ['Ctrl', 'Shift', 'Z'], label: 'Redo' },
      { keys: ['Ctrl', 'C'], label: 'Copy selection' },
      { keys: ['Ctrl', 'X'], label: 'Cut selection' },
      { keys: ['Ctrl', 'V'], label: 'Paste at cursor' },
      { keys: ['Ctrl', 'D'], label: 'Duplicate selection' },
      { keys: ['G'], label: 'Group selection into a subpatch' },
      { keys: ['[', ']'], label: 'Reorder selected wire on a multi-input pin' },
    ],
  },
  {
    title: 'Add & navigate',
    items: [
      { keys: ['Tab'], label: 'Open node search at cursor' },
      { keys: ['Q'], label: 'Open favorites quick-menu' },
      { keys: ['F'], label: 'Fit view to selection (or all nodes)' },
      { keys: ['I'], label: 'Enter selected subpatch' },
      { keys: ['O'], label: 'Exit current subpatch' },
      { keys: ['Esc'], label: 'Close search / menus / overlays' },
    ],
  },
  {
    title: 'File & view',
    items: [
      { keys: ['Ctrl', 'S'], label: 'Save patch' },
      { keys: ['Shift', 'T'], label: 'Tidy — auto-arrange nodes' },
      { keys: ['?'], label: 'Show this shortcuts reference' },
    ],
  },
];

export function ShortcutsModal({ onClose }: Props) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <div className="settings-backdrop" onMouseDown={onClose}>
      <div
        className="settings-modal shortcuts-modal"
        role="dialog"
        aria-labelledby="shortcuts-title"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="settings-header">
          <h2 id="shortcuts-title" className="settings-title">Keyboard shortcuts</h2>
          <button className="settings-close" onClick={onClose} title="Close (Esc)">
            ✕
          </button>
        </div>

        <div className="settings-body">
          {GROUPS.map((group) => (
            <div className="settings-section" key={group.title}>
              <div className="settings-section-title">{group.title}</div>
              <dl className="shortcuts-list">
                {group.items.map((s) => (
                  <div className="shortcuts-row" key={s.label}>
                    <dt className="shortcuts-keys">
                      {s.keys.map((k, i) => (
                        <kbd key={i}>{k}</kbd>
                      ))}
                    </dt>
                    <dd className="shortcuts-label">{s.label}</dd>
                  </div>
                ))}
              </dl>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
