import { type ReactNode } from 'react';
import type { PanelStateDefaults } from './usePanelState';
import { DockClient } from '../dock/DockClient';
import type { DefaultPlacement } from '../dock/dockStore';

/**
 * Compatibility shim. The panel system is now a recursive dock tree
 * (see `src/editor/dock`); this keeps the original `<Panel>` surface so
 * existing panels (NodePalette, HelpPanel, LibraryPanel, PreviewPanel)
 * work unchanged. The old `defaults.dock` maps to a first-mount dock
 * placement; size/float hints feed the floating default rect.
 */

export interface PanelProps {
  id: string;
  title: string;
  defaults: PanelStateDefaults;
  children: ReactNode;
  hideClose?: boolean;
  headerActions?: ReactNode;
}

export function Panel({ id, title, defaults, children, hideClose, headerActions }: PanelProps) {
  const placement: DefaultPlacement =
    defaults.dock === 'float'
      ? { kind: 'float', floatRect: { x: defaults.floatX, y: defaults.floatY, w: defaults.width, h: defaults.height } }
      : defaults.dock === 'left'
        ? { kind: 'root-edge', side: 'left' }
        : defaults.dock === 'bottom'
          ? { kind: 'root-edge', side: 'bottom' }
          : { kind: 'root-edge', side: 'right' };
  return (
    <DockClient id={id} title={title} placement={placement} headerActions={headerActions} hideClose={hideClose}>
      {children}
    </DockClient>
  );
}
