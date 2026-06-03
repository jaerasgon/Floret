import { useEffect } from 'react';
import { usePrefs } from '../prefs';

/**
 * Settings modal — toggles backed by the prefs store. Clicking a toggle
 * persists to localStorage immediately; there's no apply / cancel
 * step. Esc or clicking the backdrop closes.
 *
 * Resist the urge to add per-section explanatory paragraphs here — each
 * toggle's `title` (tooltip) carries enough context for the hover-the-
 * label crowd; verbose body text gets stale and bloats the modal.
 */
interface Props {
  onClose: () => void;
}

export function SettingsModal({ onClose }: Props) {
  const prefs = usePrefs();

  // Esc to close.
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
        className="settings-modal"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="settings-header">
          <h2 className="settings-title">Settings</h2>
          <button
            className="settings-close"
            onClick={onClose}
            title="Close (Esc)"
          >
            ✕
          </button>
        </div>

        <div className="settings-body">
          <div className="settings-section">
            <div className="settings-section-title">Editor behavior</div>
            <ToggleRow
              label="Wire flow animation"
              value={prefs.wireFlowAnimation}
              onChange={(v) => prefs.set('wireFlowAnimation', v)}
              hint="Marching dashes along spread-kind wires."
            />
            <ToggleRow
              label="Auto-grow regions"
              value={prefs.autoGrowRegions}
              onChange={(v) => prefs.set('autoGrowRegions', v)}
              hint="Region frames expand when a child is dragged near the edge."
            />
          </div>

          <div className="settings-section">
            <button
              className="settings-reset"
              onClick={prefs.reset}
              title="Restore all settings to their defaults"
            >
              Reset to defaults
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function ToggleRow({
  label, value, onChange, hint,
}: {
  label: string;
  value: boolean;
  onChange: (v: boolean) => void;
  hint?: string;
}) {
  return (
    <label className="settings-toggle-row" title={hint}>
      <span className="settings-toggle-label">{label}</span>
      <input
        type="checkbox"
        checked={value}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}
