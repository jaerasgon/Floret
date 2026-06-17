import { registerNode } from '../registry';
import { svgRegistry } from '../../runtime/svg-registry';
import { SvgNode } from '../../editor/nodes/SvgNode';
import { toVec2, ZERO_VEC2 } from '../_shared/coerce';
import { resolveStyle, type Style, type Affine, type ResolvedStyle, type SceneNode } from '../../runtime/scene';
import type { PortValue, Vec2 } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * SVG — load an SVG and emit it as native vector `layer`s.
 *
 * `mode` picks one of three sources (all persisted into params so they
 * survive save/reload): a picked / drag-dropped `.svg` file (stored as a
 * data URL), a remote URL, or pasted raw `<svg>` markup. The node only
 * reads the source for the selected mode, and the component shows only
 * that mode's controls.
 *
 * The SVG is parsed once (cached in svgRegistry) into a `group` of
 * `polygon` shapes, centred on the origin at its native viewBox size.
 * `position` offsets it, `scale` resizes it, and a wired `style`
 * fully REPLACES every shape's paint (Floret convention — a lone Stroke
 * yields outline-only, a lone Fill yields fill-only); unwired → the SVG
 * keeps its own colours. See svg-registry for the parsing strategy and v1 limits
 * (curves flatten to polylines; holes fill solid; no gradients / text /
 * clip-paths / raster).
 *
 * Outputs: `layer`, plus `width` / `height` (viewBox) and `loaded`.
 */
export const svgNode = registerNode({
  type: 'io/svg',
  category: 'IO',
  label: 'SVG',
  description: 'Load an SVG as vector layers. Pick a file, give a URL, or paste markup.',
  inputs: [
    { name: 'position', type: 'vec2', default: ZERO_VEC2 },
    { name: 'scale', type: 'number', default: 1 },
    { name: 'style', type: 'style' },
  ],
  outputs: [
    { name: 'layer', type: 'layer' },
    { name: 'width', type: 'number' },
    { name: 'height', type: 'number' },
    { name: 'loaded', type: 'boolean' },
  ],
  defaultParams: { mode: 'file', markup: '', dataUrl: '', url: '', filename: '' },
  paramOptions: { mode: ['file', 'url', 'markup'] },
  component: SvgNode,
  computeRaw: (inputs: PortValue[], ctx: NodeParamContext): PortValue[] => {
    const mode = ctx.param<string>('mode', 'file');
    let key = '';
    let isUrl = false;
    if (mode === 'markup') {
      key = ctx.param<string>('markup', '').trim();
    } else if (mode === 'url') {
      key = ctx.param<string>('url', '').trim();
      isUrl = true;
    } else {
      key = ctx.param<string>('dataUrl', '').trim(); // file
    }

    if (!key) return [[null], [0], [0], [false]];

    const entry = svgRegistry.ensure(ctx.nodeId, key, isUrl);
    if (!entry.loaded || !entry.layer) return [[null], [0], [0], [false]];

    const pos = toVec2((inputs[0] ?? [])[0]) as Vec2;
    const scale = Number((inputs[1] ?? [])[0] ?? 1) || 1;
    const styleFn = (inputs[2] ?? [])[0] as Style | null | undefined;

    let layer: SceneNode = entry.layer;
    if (styleFn) layer = applyStyle(layer, resolveStyle(styleFn));
    if (scale !== 1 || pos.x !== 0 || pos.y !== 0) {
      // Scale around the (centred) origin, then translate to `position`.
      const transform: Affine = [scale, 0, 0, scale, pos.x, pos.y];
      layer = { kind: 'group', children: [layer], transform };
    }

    return [[layer], [entry.width], [entry.height], [true]];
  },
});

/** Replace every shape's style with the resolved wired style — the SVG's
 *  own fill/stroke are discarded entirely. Returns a fresh tree; the cached
 *  one is untouched. */
function applyStyle(node: SceneNode, resolved: ResolvedStyle): SceneNode {
  if (node.kind === 'group') {
    return { ...node, children: node.children.map((c) => applyStyle(c, resolved)) };
  }
  if ('style' in node && node.style) {
    return { ...node, style: resolved } as SceneNode;
  }
  return node;
}
