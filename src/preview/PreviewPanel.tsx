import { Pause, Play } from 'lucide-react';
import { Preview } from './Preview';
import { Panel } from '../editor/panel/Panel';
import { useAppStore } from '../app/store';
import { Tooltip } from '../app/shell/Tooltip';
import type { NodeId } from '../runtime/graph';

/**
 * Preview panel — `Preview` wrapped in the universal `Panel` chrome
 * so it can dock left/right, float, resize, collapse, or hide along
 * with every other side panel.
 *
 * Each preview is bound 1:1 to a specific `render/renderer` node. The
 * App layer derives previews from the node list — there's no separate
 * user-managed list, no add / remove, no rebinding. Delete the node and
 * the preview goes; add a renderer node and a preview appears. That's
 * also why the × close button is permanently hidden — the only way to
 * "close" a preview is to delete its renderer node.
 */

export interface PreviewPanelProps {
  /** Per-instance panel id. The App passes the renderer node id so
   *  layout state (dock side, size, float position) stays stable
   *  across reloads via the existing panelState localStorage. */
  panelId: string;
  /** The Renderer node this preview is bound to. Always set — null
   *  isn't meaningful in the derived-from-nodes model. */
  rendererId: NodeId;
  /** Optional title override; defaults to "Preview". */
  title?: string;
}

export function PreviewPanel({
  panelId, rendererId, title = 'Preview',
}: PreviewPanelProps) {
  return (
    <Panel
      id={panelId}
      title={title}
      defaults={{ dock: 'right', width: 460, height: 460, floatX: 360, floatY: 80 }}
      hideClose
      headerActions={<PreviewHeaderControls />}
    >
      <Preview rendererId={rendererId} />
    </Panel>
  );
}

/**
 * Pause, hosted on every Preview panel header. The action mutates
 * global app state (`useAppStore`) so click behaviour is shared
 * regardless of how many previews exist. Refresh is global (toolbar) —
 * a scene reset isn't a per-preview thing.
 */
function PreviewHeaderControls() {
  const running = useAppStore((s) => s.running);
  const toggleRunning = useAppStore((s) => s.toggleRunning);
  return (
    <Tooltip label={running ? 'Pause runtime' : 'Resume runtime'} shortcut="Space">
      <button
        className={`toolbar-run-btn${running ? ' running' : ''}`}
        onClick={toggleRunning}
        aria-label={running ? 'Pause' : 'Play'}
      >
        {running ? <Pause size={13} strokeWidth={1.8} /> : <Play size={13} strokeWidth={1.8} />}
        <span>{running ? 'Pause' : 'Play'}</span>
      </button>
    </Tooltip>
  );
}

