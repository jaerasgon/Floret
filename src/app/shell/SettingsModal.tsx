import { Dialog, DialogHeader } from '@astryxdesign/core/Dialog';
import { Switch } from '@astryxdesign/core/Switch';
import { Button } from '@astryxdesign/core/Button';
import { usePrefs } from '../prefs';

/**
 * Settings modal — toggles backed by the prefs store. Flipping a switch
 * persists to localStorage immediately; there's no apply / cancel step.
 * Esc or a backdrop click closes (handled by Astryx Dialog).
 *
 * Resist the urge to add per-section explanatory paragraphs here — each
 * switch's `description` carries enough context; verbose body text gets
 * stale and bloats the modal.
 */
interface Props {
  onClose: () => void;
}

export function SettingsModal({ onClose }: Props) {
  const prefs = usePrefs();

  return (
    <Dialog
      isOpen
      onOpenChange={(open) => { if (!open) onClose(); }}
      purpose="info"
      width={420}
    >
      <DialogHeader title="Settings" onOpenChange={(open) => { if (!open) onClose(); }} />
      <div className="settings-body">
        <div className="settings-section">
          <div className="settings-section-title">Editor behavior</div>
          <Switch
            label="Wire flow animation"
            description="Marching dashes along spread-kind wires."
            value={prefs.wireFlowAnimation}
            onChange={(v) => prefs.set('wireFlowAnimation', v)}
            labelPosition="start"
            labelSpacing="spread"
          />
          <Switch
            label="Auto-grow regions"
            description="Region frames expand when a child is dragged near the edge."
            value={prefs.autoGrowRegions}
            onChange={(v) => prefs.set('autoGrowRegions', v)}
            labelPosition="start"
            labelSpacing="spread"
          />
        </div>

        <div className="settings-section">
          <Button
            variant="ghost"
            size="sm"
            label="Reset to defaults"
            tooltip="Restore all settings to their defaults"
            onClick={prefs.reset}
          />
        </div>
      </div>
    </Dialog>
  );
}
