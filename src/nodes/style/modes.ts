import { registerNode } from '../registry';
import type {
  Style,
  ResolvedStyle,
} from '../../runtime/types';
import type {
  RectMode,
  EllipseMode,
  StrokeCap,
  StrokeJoin,
} from '../../runtime/scene';

/**
 * Mode-style nodes — narrow Style transforms that set a single field.
 * All chain via `base` like every other style: base runs first, then
 * this node's override layers on top.
 */

function applyBase(base: Style | null | undefined, current: ResolvedStyle): ResolvedStyle {
  return base ? base(current) : current;
}

export const rectModeNode = registerNode({
  type: 'style/rect-mode',
  category: 'Style',
  label: 'RectMode',
  description: 'Set how Rect interprets (x, y, w, h).',
  inputs: [
    { name: 'mode', type: 'string', default: 'corner', param: 'mode' },
    { name: 'base', label: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', label: 'style', type: 'style' }],
  defaultParams: { mode: 'corner' as RectMode },
  paramOptions: { mode: ['corner', 'center', 'corners', 'radius'] },
  compute: (mode, base) => {
    const m = ((mode as string) ?? 'corner') as RectMode;
    const baseStyle = base as Style | null;
    const style: Style = (s) => ({ ...applyBase(baseStyle, s), rectMode: m });
    return [style];
  },
});

export const ellipseModeNode = registerNode({
  type: 'style/ellipse-mode',
  category: 'Style',
  label: 'EllipseMode',
  description: 'Set how Ellipse interprets (x, y, w, h).',
  inputs: [
    { name: 'mode', type: 'string', default: 'center', param: 'mode' },
    { name: 'base', label: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', label: 'style', type: 'style' }],
  defaultParams: { mode: 'center' as EllipseMode },
  paramOptions: { mode: ['corner', 'center', 'corners', 'radius'] },
  compute: (mode, base) => {
    const m = ((mode as string) ?? 'center') as EllipseMode;
    const baseStyle = base as Style | null;
    const style: Style = (s) => ({ ...applyBase(baseStyle, s), ellipseMode: m });
    return [style];
  },
});

export const strokeCapNode = registerNode({
  type: 'style/stroke-cap',
  category: 'Style',
  label: 'StrokeCap',
  description: 'Set the line end-cap shape.',
  inputs: [
    { name: 'cap', type: 'string', default: 'round', param: 'cap' },
    { name: 'base', label: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', label: 'style', type: 'style' }],
  defaultParams: { cap: 'round' as StrokeCap },
  paramOptions: { cap: ['butt', 'round', 'square'] },
  compute: (cap, base) => {
    const c = ((cap as string) ?? 'round') as StrokeCap;
    const baseStyle = base as Style | null;
    const style: Style = (s) => ({ ...applyBase(baseStyle, s), strokeCap: c });
    return [style];
  },
});

export const strokeJoinNode = registerNode({
  type: 'style/stroke-join',
  category: 'Style',
  label: 'StrokeJoin',
  description: 'Set how stroke corners are joined.',
  inputs: [
    { name: 'join', type: 'string', default: 'miter', param: 'join' },
    { name: 'base', label: 'style', type: 'style' },
  ],
  outputs: [{ name: 'out', label: 'style', type: 'style' }],
  defaultParams: { join: 'miter' as StrokeJoin },
  paramOptions: { join: ['miter', 'bevel', 'round'] },
  compute: (join, base) => {
    const j = ((join as string) ?? 'miter') as StrokeJoin;
    const baseStyle = base as Style | null;
    const style: Style = (s) => ({ ...applyBase(baseStyle, s), strokeJoin: j });
    return [style];
  },
});
