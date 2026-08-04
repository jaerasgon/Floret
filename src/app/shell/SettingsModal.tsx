import { useState, type ReactNode } from 'react';
import { Palette, SquarePen, Frame, FolderOpen } from 'lucide-react';
import { Modal, Switch } from './Modal';
import { usePrefs } from '../prefs';

/**
 * Preferences — a Notion-style settings dialog: a left category rail and a
 * scrolling content pane. Every control is backed by the prefs store and
 * persists to localStorage immediately (no apply / cancel step).
 */
interface Props {
  onClose: () => void;
}

type Category = 'appearance' | 'editor' | 'canvas' | 'files';

const CATEGORIES: { id: Category; label: string; icon: ReactNode }[] = [
  { id: 'appearance', label: 'Appearance', icon: <Palette size={15} strokeWidth={1.7} /> },
  { id: 'editor', label: 'Editor', icon: <SquarePen size={15} strokeWidth={1.7} /> },
  { id: 'canvas', label: 'Canvas', icon: <Frame size={15} strokeWidth={1.7} /> },
  { id: 'files', label: 'Files', icon: <FolderOpen size={15} strokeWidth={1.7} /> },
];

const RENDER_PRESETS: { label: string; w: number; h: number }[] = [
  { label: 'Portrait — 1080 × 1920', w: 1080, h: 1920 },
  { label: 'Square — 1080 × 1080', w: 1080, h: 1080 },
  { label: 'Landscape — 1920 × 1080', w: 1920, h: 1080 },
  { label: 'HD — 1280 × 720', w: 1280, h: 720 },
];

export function SettingsModal({ onClose }: Props) {
  const prefs = usePrefs();
  const [cat, setCat] = useState<Category>('appearance');

  return (
    <Modal onClose={onClose} title="Preferences" subtitle="Choose how Floret looks and behaves" width={780}>
      <div className="prefs-layout">
        <nav className="prefs-nav" aria-label="Settings categories">
          {CATEGORIES.map((c) => (
            <button
              key={c.id}
              type="button"
              className={`prefs-nav-item${cat === c.id ? ' active' : ''}`}
              onClick={() => setCat(c.id)}
            >
              {c.icon}
              {c.label}
            </button>
          ))}
          <div className="prefs-nav-spacer" />
          <button
            type="button"
            className="prefs-nav-item prefs-nav-reset"
            title="Restore all settings to their defaults"
            onClick={prefs.reset}
          >
            Reset to defaults
          </button>
        </nav>

        <div className="prefs-content">
          {cat === 'appearance' && (
            <Section>
              <SelectRow
                label="Theme"
                desc="Choose a colour theme, or follow your system."
                value={prefs.theme}
                onChange={(v) => prefs.set('theme', v as typeof prefs.theme)}
                options={[
                  { value: 'light', label: 'Light' },
                  { value: 'dark', label: 'Dark' },
                  { value: 'system', label: 'Use system setting' },
                ]}
              />
              <ToggleRow
                label="Ambient canvas motion"
                desc="A slow generative drift behind the graph on a quiet patch."
                checked={prefs.ambientMotion}
                onChange={(v) => prefs.set('ambientMotion', v)}
              />
              <ToggleRow
                label="Show minimap"
                desc="The overview map in the canvas corner."
                checked={prefs.showMinimap}
                onChange={(v) => prefs.set('showMinimap', v)}
              />
              <ToggleRow
                label="Show dot grid"
                desc="The dotted background grid on the canvas."
                checked={prefs.showGrid}
                onChange={(v) => prefs.set('showGrid', v)}
              />
            </Section>
          )}

          {cat === 'editor' && (
            <Section>
              <ToggleRow
                label="Wire flow animation"
                desc="Marching dashes along spread-kind wires."
                checked={prefs.wireFlowAnimation}
                onChange={(v) => prefs.set('wireFlowAnimation', v)}
              />
              <ToggleRow
                label="Auto-grow regions"
                desc="Region frames expand when a child is dragged near the edge."
                checked={prefs.autoGrowRegions}
                onChange={(v) => prefs.set('autoGrowRegions', v)}
              />
              <ToggleRow
                label="Snap to grid"
                desc="Snap nodes to a grid while dragging."
                checked={prefs.snapToGrid}
                onChange={(v) => prefs.set('snapToGrid', v)}
              />
              <ToggleRow
                label="Help panel follows selection"
                desc="The Help panel auto-tracks the selected node instead of staying pinned."
                checked={prefs.autoJumpHelp}
                onChange={(v) => prefs.set('autoJumpHelp', v)}
              />
            </Section>
          )}

          {cat === 'canvas' && (
            <Section>
              <div className="prefs-row">
                <div className="prefs-row-text">
                  <div className="prefs-row-label">Default render size</div>
                  <div className="prefs-row-desc">Resolution seeded into a newly-created Renderer node. Pick a preset or type an exact size.</div>
                </div>
                <div className="prefs-row-control prefs-render">
                  <select
                    className="modal-select prefs-select"
                    value={
                      RENDER_PRESETS.find((p) => p.w === prefs.defaultRenderWidth && p.h === prefs.defaultRenderHeight)
                        ?.label ?? '__custom__'
                    }
                    onChange={(e) => {
                      const p = RENDER_PRESETS.find((pr) => pr.label === e.target.value);
                      if (p) { prefs.set('defaultRenderWidth', p.w); prefs.set('defaultRenderHeight', p.h); }
                    }}
                  >
                    {RENDER_PRESETS.map((p) => (
                      <option key={p.label} value={p.label}>{p.label}</option>
                    ))}
                    <option value="__custom__">Custom…</option>
                  </select>
                  <div className="prefs-dims">
                    <input
                      type="number" min={32} max={4096} className="modal-input prefs-num" aria-label="Width"
                      value={prefs.defaultRenderWidth}
                      onChange={(e) => prefs.set('defaultRenderWidth', clampDim(e.target.value))}
                    />
                    <span className="prefs-dims-x">×</span>
                    <input
                      type="number" min={32} max={4096} className="modal-input prefs-num" aria-label="Height"
                      value={prefs.defaultRenderHeight}
                      onChange={(e) => prefs.set('defaultRenderHeight', clampDim(e.target.value))}
                    />
                  </div>
                </div>
              </div>
              <SelectRow
                label="Grid step"
                desc="Spacing used when Snap to grid is on."
                value={String(prefs.gridSize)}
                onChange={(v) => prefs.set('gridSize', Number(v))}
                options={[8, 16, 24, 32].map((n) => ({ value: String(n), label: `${n} px` }))}
              />
            </Section>
          )}

          {cat === 'files' && (
            <Section>
              <SelectRow
                label="On startup, open"
                desc="What loads when you launch Floret."
                value={prefs.startupPatch}
                onChange={(v) => prefs.set('startupPatch', v as typeof prefs.startupPatch)}
                options={[
                  { value: 'last', label: 'Last session' },
                  { value: 'starter', label: 'Starter showcase' },
                  { value: 'blank', label: 'Blank canvas' },
                ]}
              />
              <ToggleRow
                label="Autosave"
                desc="Continuously save the working patch to this browser."
                checked={prefs.autosaveEnabled}
                onChange={(v) => prefs.set('autosaveEnabled', v)}
              />
            </Section>
          )}
        </div>
      </div>
    </Modal>
  );
}

function clampDim(v: string): number {
  const n = Math.round(Number(v) || 0);
  return Math.max(32, Math.min(4096, n));
}

function Section({ children }: { children: ReactNode }) {
  return <div className="prefs-section">{children}</div>;
}

function ToggleRow({
  label, desc, checked, onChange,
}: { label: string; desc?: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="prefs-row">
      <div className="prefs-row-text">
        <div className="prefs-row-label">{label}</div>
        {desc && <div className="prefs-row-desc">{desc}</div>}
      </div>
      <div className="prefs-row-control">
        <Switch label={label} checked={checked} onChange={onChange} />
      </div>
    </div>
  );
}

function SelectRow({
  label, desc, value, onChange, options,
}: {
  label: string;
  desc?: string;
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
}) {
  return (
    <div className="prefs-row">
      <div className="prefs-row-text">
        <div className="prefs-row-label">{label}</div>
        {desc && <div className="prefs-row-desc">{desc}</div>}
      </div>
      <div className="prefs-row-control">
        <select className="modal-select prefs-select" value={value} onChange={(e) => onChange(e.target.value)}>
          {options.map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>
      </div>
    </div>
  );
}
