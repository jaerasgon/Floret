import { registerNode } from '../registry';
import { sceneToSvg } from '../../runtime/scene-svg';
import { colorToCss } from '../../runtime/scene';
import type { Color, PortValue, SceneNode } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Save SVG — exports the scene wired to its `layers` input as a vector
 * SVG file when `save` rises from false to true.
 *
 * Lives as a NODE rather than a toolbar action because the export is
 * part of the patch's logic: you wire whatever subgraph you want to
 * export (often the same Layer spread that feeds the Renderer), pick
 * canvas dimensions and a filename via inline params, and trigger via
 * any boolean source — a key press, a button, an LFO crossing zero,
 * a MIDI note-on, whatever. The data-flow framing matches the rest of
 * the tool.
 *
 * Rising-edge semantics follow the `sound/envelope` precedent: per-node
 * state map tracks `lastSave`, fire on false → true transition. Holding
 * the trigger high does not re-fire; the user must release and press
 * again. This avoids the obvious 60fps-of-downloads footgun if the
 * trigger source is e.g. `mouseIsPressed`.
 *
 * Output: passes the input layer spread through unchanged, so the same
 * scene can be wired to a Renderer in parallel without splitting.
 */

interface SaveState {
  lastSave: boolean;
}

const states = new Map<string, SaveState>();

export const saveSvgNode = registerNode({
  type: 'io/save-svg',
  category: 'IO',
  label: 'SaveSVG',
  description: 'Export wired layers as an SVG file when `save` rises.',
  inputs: [
    { name: 'layers', type: 'layer', kind: 'spread' },
    { name: 'save', type: 'boolean', default: false },
    { name: 'background', type: 'color' },
  ],
  outputs: [
    // Pass-through so the same scene can feed a Renderer in parallel
    // without the user having to split the wire.
    { name: 'layers', type: 'layer', kind: 'spread' },
  ],
  defaultParams: {
    width: 500,
    height: 500,
    filename: 'drawing',
  },
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const layers = (inputs[0] ?? []) as ReadonlyArray<SceneNode | SceneNode[]>;
    const save = !!((inputs[1] ?? [])[0]);
    const bg = (inputs[2] ?? [])[0] as Color | null | undefined;

    let state = states.get(params.nodeId);
    if (!state) {
      state = { lastSave: false };
      states.set(params.nodeId, state);
    }

    if (save && !state.lastSave) {
      const width = Math.max(1, Math.floor(params.param<number>('width', 500)));
      const height = Math.max(1, Math.floor(params.param<number>('height', 500)));
      const filename = sanitiseFilename(params.param<string>('filename', 'drawing'));

      const flat = flattenLayers(layers);
      const background = bg && typeof bg === 'object' && 'r' in bg ? colorToCss(bg) : null;
      const svg = sceneToSvg(flat, { width, height, background });
      triggerDownload(svg, `${filename}.svg`);
    }
    state.lastSave = save;

    return [layers as PortValue];
  },
});

/**
 * The runtime's Layer spread can contain nested arrays when a ForEach
 * region produces a spread-of-layers per iteration. The canvas driver
 * flattens these before rendering; we do the same so the SVG output
 * matches what the user sees on the canvas.
 */
function flattenLayers(
  layers: ReadonlyArray<SceneNode | SceneNode[]>,
): SceneNode[] {
  const out: SceneNode[] = [];
  for (const layer of layers) {
    if (Array.isArray(layer)) {
      for (const n of layer) if (n) out.push(n);
    } else if (layer) {
      out.push(layer);
    }
  }
  return out;
}

/**
 * Same blob+anchor download trick as `persistence/library.ts`. We don't
 * import that helper directly because the payload type and filename
 * are different, but the mechanism is identical.
 */
function triggerDownload(svg: string, filename: string): void {
  if (typeof window === 'undefined' || typeof document === 'undefined') return;
  const blob = new Blob([svg], { type: 'image/svg+xml' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/**
 * Strip characters that the OS won't allow in filenames. Empty / all-
 * stripped input falls back to "drawing" so we never end up with a
 * `.svg` file (no stem).
 */
function sanitiseFilename(name: string): string {
  const cleaned = name.replace(/[\\/:*?"<>|]+/g, '').trim();
  return cleaned || 'drawing';
}
