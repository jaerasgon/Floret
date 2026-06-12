/**
 * Panel default-state types. The live panel state (dock/size/float) is now
 * owned by the recursive dock store (src/editor/dock); only these defaults
 * survive, consumed by the `<Panel>` compatibility shim to derive a
 * first-mount dock placement.
 */

/** First-mount dock hint. `left` / `right` / `bottom` dock to that edge;
 *  `float` opens a detached card. (`hidden` is legacy and unused.) */
export type PanelDock = 'left' | 'right' | 'bottom' | 'float' | 'hidden';

export interface PanelStateDefaults {
  dock: PanelDock;
  width: number;
  height: number;
  floatX: number;
  floatY: number;
  collapsed?: boolean;
  order?: number;
  column?: number;
}
