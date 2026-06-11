import { registerNode } from '../registry';
import type { ParticleBuffer } from '../../runtime/particles';
import type {
  ParticleBatch,
  ResolvedStyle,
  SceneNode,
} from '../../runtime/scene';
import type { ParticleTemplate } from '../../runtime/particles';
import { DEFAULT_STYLE, resolveStyle } from '../../runtime/scene';
import type { Color, Style } from '../../runtime/types';
import { colorToCss } from '../../runtime/types';

/**
 * particles/render-as — convert a `ParticleBuffer` into a `Layer`
 * (specifically, a `ParticleBatch` SceneNode) so the standard renderer
 * can draw it. One node, one tight render loop downstream.
 *
 * The `template` choice is CONSTANT — switching mid-flight would change
 * what the renderer expects from each particle slot. For
 * `template = 'layer'`, the `templateLayer` input supplies the SceneNode
 * to clone-translate per particle.
 *
 * `useBufferColor` controls whether per-particle colour from the buffer
 * is honoured (true, the default) or a uniform style fill is used (false
 * — cheaper, less expressive).
 */
export const particlesRenderAsNode = registerNode({
  type: 'particles/render-as',
  category: 'Particles',
  label: 'Render',
  description:
    'Render a ParticleBuffer as a Layer. Template controls the per-particle shape.',
  inputs: [
    { name: 'particles', type: 'particle', default: null },
    { name: 'template',  type: 'string',   default: 'circle', param: 'template', constant: true },
    { name: 'templateLayer', type: 'layer', default: null },
    { name: 'style',     type: 'style' },
    { name: 'tint',      type: 'color',    default: null },
    { name: 'useBufferColor', type: 'boolean', default: true, param: 'useBufferColor', constant: true },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  defaultParams: { template: 'circle', useBufferColor: true },
  paramOptions: { template: ['circle', 'rect', 'layer'] },
  computeRaw: (inputs) => {
    const buf = inputs[0]?.[0] as ParticleBuffer | null | undefined;
    if (!buf) return [[{ kind: 'group', children: [] } as SceneNode]];

    const templateKind = String(inputs[1]?.[0] ?? 'circle');
    const templateLayer = inputs[2]?.[0] as SceneNode | null | undefined;
    const userStyle = inputs[3]?.[0] as Style | null | undefined;
    const tint = inputs[4]?.[0] as Color | null | undefined;
    const useBufferColor = Boolean(inputs[5]?.[0] ?? true);

    let template: ParticleTemplate;
    if (templateKind === 'rect') {
      template = { kind: 'rect' };
    } else if (templateKind === 'layer' && templateLayer) {
      template = { kind: 'layer', node: templateLayer };
    } else {
      template = { kind: 'circle' };
    }

    let style: ResolvedStyle = resolveStyle(userStyle ?? null, DEFAULT_STYLE);
    // Tint, when wired, overrides the resolved style fill so a quick
    // "single colour for all particles" wiring doesn't need a Fill node.
    if (tint && !useBufferColor) {
      style = { ...style, fill: colorToCss(tint) };
    }

    const batch: ParticleBatch = {
      kind: 'particle-batch',
      buffer: buf,
      count: buf.count,
      template,
      style,
      useBufferColor,
    };
    return [[batch]];
  },
});
