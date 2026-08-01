import Switch from '@mui/material/Switch';
import Button from '@mui/material/Button';
import FormControlLabel from '@mui/material/FormControlLabel';
import Typography from '@mui/material/Typography';
import Box from '@mui/material/Box';
import { usePrefs } from '../prefs';
import { ModalShell } from './ModalShell';

/**
 * Settings modal — toggles backed by the prefs store. Flipping a switch
 * persists to localStorage immediately; there's no apply / cancel step.
 * Esc or a backdrop click closes.
 *
 * Resist the urge to add per-section explanatory paragraphs here — each
 * switch's description carries enough context; verbose body text gets
 * stale and bloats the modal.
 */
interface Props {
  onClose: () => void;
}

/** A labelled switch with a secondary description line, label on the left. */
function SettingRow({ label, description, checked, onChange }: {
  label: string; description: string; checked: boolean; onChange: (v: boolean) => void;
}) {
  return (
    <FormControlLabel
      labelPlacement="start"
      sx={{ ml: 0, mr: 0, display: 'flex', justifyContent: 'space-between', width: '100%', alignItems: 'flex-start' }}
      control={<Switch checked={checked} onChange={(e) => onChange(e.target.checked)} />}
      label={
        <Box sx={{ textAlign: 'left' }}>
          <Typography variant="body2" sx={{ fontWeight: 500 }}>{label}</Typography>
          <Typography variant="caption" color="text.secondary">{description}</Typography>
        </Box>
      }
    />
  );
}

export function SettingsModal({ onClose }: Props) {
  const prefs = usePrefs();

  return (
    <ModalShell title="Settings" width={420} onClose={onClose}>
      <div className="settings-body">
        <div className="settings-section">
          <div className="settings-section-title">Editor behavior</div>
          <SettingRow
            label="Wire flow animation"
            description="Marching dashes along spread-kind wires."
            checked={prefs.wireFlowAnimation}
            onChange={(v) => prefs.set('wireFlowAnimation', v)}
          />
          <SettingRow
            label="Auto-grow regions"
            description="Region frames expand when a child is dragged near the edge."
            checked={prefs.autoGrowRegions}
            onChange={(v) => prefs.set('autoGrowRegions', v)}
          />
        </div>

        <div className="settings-section">
          <Button variant="text" size="small" color="inherit" onClick={prefs.reset}>
            Reset to defaults
          </Button>
        </div>
      </div>
    </ModalShell>
  );
}
