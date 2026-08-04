import { Modal, Toggle } from './Modal';
import { usePrefs } from '../prefs';

/**
 * Settings modal — toggles backed by the prefs store. Flipping a switch
 * persists to localStorage immediately; there's no apply / cancel step.
 * Esc or a backdrop click closes.
 */
interface Props {
  onClose: () => void;
}

export function SettingsModal({ onClose }: Props) {
  const prefs = usePrefs();

  return (
    <Modal onClose={onClose} title="Settings" width={440}>
      <div className="modal-section">
        <div className="modal-section-title">Editor behavior</div>
        <Toggle
          label="Wire flow animation"
          description="Marching dashes along spread-kind wires."
          checked={prefs.wireFlowAnimation}
          onChange={(v) => prefs.set('wireFlowAnimation', v)}
        />
        <Toggle
          label="Auto-grow regions"
          description="Region frames expand when a child is dragged near the edge."
          checked={prefs.autoGrowRegions}
          onChange={(v) => prefs.set('autoGrowRegions', v)}
        />
      </div>

      <div className="modal-actions">
        <button
          type="button"
          className="modal-btn modal-btn-ghost"
          title="Restore all settings to their defaults"
          onClick={prefs.reset}
        >
          Reset to defaults
        </button>
      </div>
    </Modal>
  );
}
