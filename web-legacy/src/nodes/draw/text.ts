import { registerNode } from '../registry';
import {
  resolveStyle,
  type Style,
  type ResolvedStyle,
  DEFAULT_STYLE,
  type Layer,
} from '../../runtime/types';
import { toVec2 } from '../_shared/coerce';

/**
 * Text — draws a string at (x, y). `size` (param) is the text height
 * in pixels and `align` controls the horizontal anchor (left/center/
 * right). Default style is white fill, no stroke.
 */
export const textNode = registerNode({
  type: 'draw/text',
  category: 'Draw',
  label: 'Text',
  description: 'Draw a text string at `pos`.',
  inputs: [
    { name: 'string', type: 'string', default: 'Hello', param: 'string' },
    { name: 'pos', type: 'vec2', default: { x: 0, y: 0 } },
    { name: 'size', type: 'number', default: 16, param: 'size' },
    { name: 'align', type: 'string', default: 'left', param: 'align' },
    { name: 'style', type: 'style' },
  ],
  outputs: [{ name: 'shape', type: 'layer' }],
  defaultParams: { string: 'Hello', size: 16, align: 'left' },
  paramOptions: { align: ['left', 'center', 'right'] },
  compute: (text, pos, size, align, style) => {
    const p = toVec2(pos);
    const sz = Math.max(1, size as number);
    const al = ((align as string) ?? 'left') as 'left' | 'center' | 'right';
    // Inherit family/weight/italic/letterSpacing/lineHeight from the wired
    // style chain (e.g. a TextParams node) — override fontSize from this
    // node's own size param, and textAlign from the align param.
    const base: ResolvedStyle = resolveStyle(style as Style | null, DEFAULT_STYLE);
    const styled: ResolvedStyle = {
      ...base,
      fontSize: sz,
      textAlign: al,
    };
    const node: Layer = {
      kind: 'text',
      text: String(text ?? ''),
      x: p.x,
      y: p.y,
      style: styled,
    };
    return [node];
  },
});
