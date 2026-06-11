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
    // `multi` so a Renderer can take many layer wires at once — useful
    // for compositing a background polygon, particle batch, trails, and
    // force-viz rings into one frame. A single-wire feed (e.g. from a
    // ForEach region carrying Spread<Layer>) still works: that one
    // wire's items get gathered into the multi-spread verbatim.
    { name: 'layers', type: 'layer', kind: 'multi' },
    // Default to black so an unwired Renderer doesn't paint the canvas
    // the same colour as DEFAULT_STYLE.fill (#ffffff) — which would make
    // every shape invisible against the background. Patches that wire a
    // Color into `background` override this normally.
    { name: 'background', type: 'color', default: { r: 0, g: 0, b: 0, a: 1 } },
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
