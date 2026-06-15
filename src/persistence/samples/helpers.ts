import type { FloretEdge, FloretNode } from '../../editor/editor-state';
import { migrateLegacyEdges, migrateLegacyDrawCoords } from '../serialize';

export interface Sample {
  id: string;
  name: string;
  description: string;
  /**
   * Category bucket — controls which tab the sample appears under in
   * the Samples menu. Missing / empty falls back to "Misc".
   */
  category?: string;
  build: () => { nodes: FloretNode[]; edges: FloretEdge[] };
}

let _seq = 0;
export function uid(prefix: string): string {
  _seq += 1;
  return `${prefix}_${_seq}`;
}

/**
 * Build a node and mirror every entry in `params` into `portValues`.
 *
 * Most node types in the runtime expose their configuration as INPUT
 * ports (e.g. `math/mul.b`, `spread/range.count`, `draw/rect.w`) — not
 * as params. Hand-writing a sample patch with constants for every
 * input would mean dropping a `value/number` node + wire for every
 * single numeric literal. So we sneak the same key/value pairs in via
 * `portValues`, which the runtime uses as the fallback for any
 * unwired input.
 *
 * Entries that don't match an input port (e.g. `{ type: 'sine' }` on
 * sound/oscillator — `type` is a param, not an input) are simply
 * ignored on the portValues side; the params copy still binds.
 */
export function node(
  id: string,
  defType: string,
  pos: { x: number; y: number },
  params: Record<string, unknown> = {},
  parentId?: string,
  extra: Partial<FloretNode> = {},
): FloretNode {
  const n: FloretNode = {
    id,
    type: 'floret',
    position: pos,
    data: {
      defType,
      params,
      ...(Object.keys(params).length > 0 ? { portValues: { ...params } } : {}),
    },
    ...extra,
  };
  if (parentId) {
    n.parentId = parentId;
    n.extent = 'parent';
  }
  return n;
}

/** Backwards-compatible alias — the `node()` helper now mirrors params
 *  into portValues automatically. */
export const nodeWithPorts = node;

/**
 * Post-process pass that scales node positions and region frame
 * dimensions so default samples have breathing room. Hand-written
 * sample positions were laid out tight; this gives each node enough
 * space that headers, pin labels and inline widgets don't overlap.
 *
 * Scales x by SAMPLE_SX, y by SAMPLE_SY. Renderer `width` / `height`
 * params represent CANVAS size, not layout — explicitly skipped. Same
 * for image/from-layer `w` / `h` and io/image-url, which are pixel
 * dimensions of the rendered surface, not editor layout.
 *
 * Y is scaled more aggressively than X because tight y-spacing was
 * the dominant overlap source — many samples stacked nodes at 60–80 px
 * y-steps, but typical node heights are 80–160 px. The x dimension
 * had more breathing room to start with.
 */
const SAMPLE_SX = 1.4;
const SAMPLE_SY = 2.5;

export function spaced(result: { nodes: FloretNode[]; edges: FloretEdge[] }) {
  for (const n of result.nodes) {
    n.position = {
      x: n.position.x * SAMPLE_SX,
      y: n.position.y * SAMPLE_SY,
    };
    // Region frames have width/height in both `style` and `data.params` —
    // they describe the FRAME size in the editor canvas, not anything
    // pixel-rendered. Scale both so the frame keeps fitting its (now
    // spaced) children. render/renderer's width/height represent the
    // p5 CANVAS size and must NOT be touched; same for image/from-layer.
    if (n.data.defType.startsWith('region/') || n.data.defType === 'meta/subpatch') {
      const p = n.data.params as Record<string, unknown>;
      if (typeof p.width === 'number') p.width = p.width * SAMPLE_SX;
      if (typeof p.height === 'number') p.height = p.height * SAMPLE_SY;
      const s = n.style as { width?: number; height?: number } | undefined;
      if (s) {
        if (typeof s.width === 'number') s.width = s.width * SAMPLE_SX;
        if (typeof s.height === 'number') s.height = s.height * SAMPLE_SY;
      }
    }
  }
  // Samples were authored against pre-rename port names and the old draw
  // x/y coordinate inputs. Funnel them through the same migrations the save
  // loader uses: handle renames first (out→semantic, etc.), then the
  // structural x/y → pos (vec2) fix that synthesises a vec2/make. Keeps the
  // sample sources terse instead of rewriting every legacy edge by hand.
  result.edges = migrateLegacyEdges(result.nodes, result.edges);
  const coords = migrateLegacyDrawCoords(result.nodes, result.edges);
  result.nodes = coords.nodes;
  result.edges = coords.edges;
  return result;
}

export function edge(
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

// ─── Sample: Lissajous Threads ─────────────────────────────────────────────
// 240 dots traced along (cos(a·t+φ), sin(b·t+φ)), where t = i/N. Two sliders
// pick the integer ratio (a, b); time drifts the phase. Hue cycles around
// the curve via HSL(i/N·360).


// ─── Tutorial helpers ──────────────────────────────────────────────────────
// Small wrapper for meta/comment so each tutorial reads as a story:
// "here is the idea (comment), here is the patch that does it (nodes)".
export function note(
  id: string,
  pos: { x: number; y: number },
  text: string,
  size: { w?: number; h?: number } = {},
  parentId?: string,
): FloretNode {
  return node(
    id,
    'meta/comment',
    pos,
    { text, width: size.w ?? 280, height: size.h ?? 120 },
    parentId,
  );
}
