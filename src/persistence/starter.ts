import type { FloretEdge, FloretNode } from '../editor/editor-state';
import { buildShowcase } from './samples/builds/Showcase';

/**
 * First-run starter patch.
 *
 * Shown only when there's no autosave at all (the user has never used the
 * app, OR they ran the app on a fresh browser profile). A deliberate
 * "New" via the toolbar still produces an empty canvas — the starter is
 * just the "you never saw this app before" state, replacing the bare
 * EMPTY CANVAS hint so the first thing a user sees is movement.
 *
 * It loads the Showcase build: a grid of stroked squares rippling under a
 * time-driven radial sine, coloured from a cycled palette, with two sliders
 * to invite a first interaction. Picked to show off the load-bearing ideas
 * in Floret at a glance — a spread source, a ForEach region, vec2 math, and
 * time animation — rather than the bare source→transform→draw→render flow.
 */
export function starterPatch(): { nodes: FloretNode[]; edges: FloretEdge[] } {
  return buildShowcase();
}
