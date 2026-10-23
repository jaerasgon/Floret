import { registerNode } from '../registry';
import type { Style, ResolvedStyle } from '../../runtime/types';
import type { TextAlign, TextBaseline } from '../../runtime/scene';

/**
 * TextParams — a Style node that overrides the typographic fields on the
 * resolved style: family, weight (bold), italic, alignment, baseline,
 * letter-spacing (kerning), and line spacing. Chain it before a draw/text
 * node's `style` input — alongside Fill / Stroke / NoFill — to control
 * how the text is laid out without changing the text node's own size or
 * coordinates.
 *
 *   TextParams(font="Inter", bold=true, italic=false, …) → Fill → text
 */

function applyBase(base: Style | null | undefined, current: ResolvedStyle): ResolvedStyle {
  return base ? base(current) : current;
}

export const textParamsNode = registerNode({
  type: 'style/text-params',
  category: 'Style',
  label: 'TextParams',
  description:
    'Override typographic style for downstream text — font family, weight, italic, alignment, kerning, line spacing.',
  inputs: [
    { name: 'font', type: 'string', default: 'sans-serif', param: 'font' },
    { name: 'bold', type: 'boolean', default: false, param: 'bold' },
    { name: 'italic', type: 'boolean', default: false, param: 'italic' },
    { name: 'align', type: 'string', default: 'left', param: 'align' },
    { name: 'baseline', type: 'string', default: 'alphabetic', param: 'baseline' },
    { name: 'kerning', type: 'number', default: 0, param: 'kerning' },
    { name: 'lineSpacing', type: 'number', default: 1.2, param: 'lineSpacing' },
    { name: 'base', type: 'style' },
  ],
  outputs: [{ name: 'style', type: 'style' }],
  defaultParams: {
    font: 'sans-serif',
    bold: false,
    italic: false,
    align: 'left',
    baseline: 'alphabetic',
    kerning: 0,
    lineSpacing: 1.2,
  },
  paramOptions: {
    align: ['left', 'center', 'right'],
    baseline: ['alphabetic', 'top', 'middle', 'bottom'],
  },
  compute: (font, bold, italic, align, baseline, kerning, lineSpacing, base) => {
    const family = ((font as string) || 'sans-serif').trim() || 'sans-serif';
    const weight = bold ? 'bold' : 'normal';
    const fontStyle = italic ? 'italic' : 'normal';
    const al = ((align as string) ?? 'left') as TextAlign;
    const bl = ((baseline as string) ?? 'alphabetic') as TextBaseline;
    const ls = Math.max(0, Number(kerning) || 0);
    const lh = Math.max(0, Number(lineSpacing) || 1);
    const baseStyle = base as Style | null;
    const style: Style = (s) => ({
      ...applyBase(baseStyle, s),
      fontFamily: family,
      fontWeight: weight,
      fontStyle,
      textAlign: al,
      textBaseline: bl,
      letterSpacing: ls,
      lineHeight: lh,
    });
    return [style];
  },
});
