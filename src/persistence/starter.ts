import type { FloretEdge, FloretNode } from '../editor/editor-state';

/**
 * First-run starter patch.
 *
 * Shown only when there's no autosave at all (the user has never used the
 * app, OR they ran the app on a fresh browser profile). A deliberate
 * "New" via the toolbar still produces an empty canvas — the starter is
 * just the "you never saw this app before" state, replacing the bare
 * EMPTY CANVAS hint so the first thing a user sees is movement.
 *
 * The patch: a centred ellipse whose diameter breathes with `sin(time)`.
 * Five nodes — Time → Sin → Map → Ellipse → Renderer. Picked for being
 * small enough to read at a glance and clearly demonstrating the
 * source→transform→draw→render flow that every patch in Floret follows.
 */

let _seq = 0;
function uid(prefix: string): string {
  _seq += 1;
  return `${prefix}_${_seq}`;
}

function node(
  id: string,
  defType: string,
  pos: { x: number; y: number },
  params: Record<string, unknown> = {},
): FloretNode {
  return {
    id,
    type: 'floret',
    position: pos,
    data: {
      defType,
      params,
      ...(Object.keys(params).length > 0 ? { portValues: { ...params } } : {}),
    },
  };
}

function edge(
  source: string,
  sourcePort: string,
  target: string,
  targetPort: string,
): FloretEdge {
  return {
    id: uid('e'),
    source,
    target,
    sourceHandle: `out-${sourcePort}`,
    targetHandle: `in-${targetPort}`,
  };
}

export function starterPatch(): { nodes: FloretNode[]; edges: FloretEdge[] } {
  _seq = 0;
  const time     = node('starter_time',     'time/seconds',    { x: -360, y:  20 });
  const sinT     = node('starter_sin',      'math/sin',        { x: -200, y:  20 });
  const sizeMap  = node('starter_size',     'math/map',        { x:  -40, y:  20 },
                        { inMin: -1, inMax: 1, outMin: 60, outMax: 220 });
  const ellipse  = node('starter_ellipse',  'draw/ellipse',    { x:  140, y:  20 });
  const bg       = node('starter_bg',       'color/rgb',       { x:  140, y: 140 },
                        { r: 16, g: 14, b: 22, a: 255 });
  const renderer = node('starter_renderer', 'render/renderer', { x:  340, y:  20 },
                        { width: 500, height: 500 });

  return {
    nodes: [time, sinT, sizeMap, ellipse, bg, renderer],
    edges: [
      edge('starter_time',    't',   'starter_sin',     'x'),
      edge('starter_sin',     'out', 'starter_size',    'x'),
      edge('starter_size',    'out', 'starter_ellipse', 'w'),
      edge('starter_size',    'out', 'starter_ellipse', 'h'),
      edge('starter_ellipse', 'out', 'starter_renderer','layers'),
      edge('starter_bg',      'out', 'starter_renderer','background'),
    ],
  };
}
