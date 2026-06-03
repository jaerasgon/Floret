import { registerNode } from '../registry';

/**
 * The sink. Takes a Spread<Layer> on its single pinned input (consumed as
 * one bin — see liftCyclic pinned semantics). Passes the spread through
 * as its output, where tick() picks it up and the p5 driver invokes
 * each Layer in array order (= wire order = z-order).
 *
 * The Renderer also owns the **canvas dimensions**: `width` and `height`
 * are inline params (editable on the node itself). The p5 driver reads
 * them from `paramStore` each frame and calls `resizeCanvas` whenever
 * they change — no recompile, no remount.
 *
 * A patch with no Renderer renders nothing.
 */
export const rendererNode = registerNode({
  type: 'render/renderer',
  category: 'Render',
  label: 'Renderer',
  description: 'Render wired layers to the preview canvas.',
  inputs: [
    { name: 'layers', type: 'layer', kind: 'spread' },
    { name: 'background', type: 'color' },
  ],
  outputs: [
    { name: 'layers', type: 'layer', kind: 'spread' },
    // The background colour is surfaced as an output so the p5 driver
    // can read it via the renderer's outputs map each frame.
    { name: 'background', type: 'color' },
  ],
  defaultParams: { width: 500, height: 500 },
  // computeRaw because the Layer spread is pinned — passes through verbatim.
  // The background passes through too so the p5 driver can read it.
  computeRaw: (inputs) => [inputs[0] ?? [], inputs[1] ?? [null]],
});
