import { registerNode } from '../registry';
import type { Layer } from '../../runtime/types';
import type { BlendMode as SceneBlendMode } from '../../runtime/scene';

/**
 * Blend — wrap an inner Layer in a `group` that sets a blend mode for
 * its children, then restores it.
 *
 * Modes are the CSS/SVG blend-mode set (source-over is the default;
 * the option names are the patch-facing aliases kept for continuity).
 */

type BlendOption =
  | 'blend' | 'add' | 'darkest' | 'lightest' | 'difference' | 'exclusion'
  | 'multiply' | 'screen' | 'replace' | 'overlay' | 'hard-light'
  | 'soft-light' | 'dodge' | 'burn' | 'subtract';

const BLEND_MODES: ReadonlyArray<BlendOption> = [
  'blend', 'add', 'darkest', 'lightest', 'difference', 'exclusion',
  'multiply', 'screen', 'replace', 'overlay', 'hard-light',
  'soft-light', 'dodge', 'burn', 'subtract',
];

function toCanvasBlend(m: BlendOption): SceneBlendMode {
  switch (m) {
    case 'add': return 'lighter';
    case 'darkest': return 'darken';
    case 'lightest': return 'lighten';
    case 'difference': return 'difference';
    case 'exclusion': return 'exclusion';
    case 'multiply': return 'multiply';
    case 'screen': return 'screen';
    case 'replace': return 'copy';
    case 'overlay': return 'overlay';
    case 'hard-light': return 'hard-light';
    case 'soft-light': return 'soft-light';
    case 'dodge': return 'color-dodge';
    case 'burn': return 'color-burn';
    case 'subtract': return 'difference'; // closest blend-mode-set approximation
    case 'blend':
    default: return 'source-over';
  }
}

export const blendNode = registerNode({
  type: 'transform/blend',
  category: 'Transform',
  label: 'Blend',
  description: 'Composite a layer using the chosen blend mode.',
  inputs: [
    { name: 'mode', type: 'string', default: 'blend', param: 'mode' },
    { name: 'layer', type: 'layer' },
  ],
  outputs: [{ name: 'blended', type: 'layer' }],
  defaultParams: { mode: 'blend' as BlendOption },
  paramOptions: { mode: BLEND_MODES as unknown as ReadonlyArray<string> },
  compute: (mode, layer) => {
    const m = ((mode as string) ?? 'blend') as BlendOption;
    const inner = layer as Layer;
    const node: Layer = {
      kind: 'group',
      children: [inner],
      blendMode: toCanvasBlend(m),
    };
    return [node];
  },
});
