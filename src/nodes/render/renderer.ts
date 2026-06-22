import { registerNode } from '../registry';
import { paramStore } from '../../runtime/param-store';

/**
 * The sink. Takes a Spread<Layer> on its single pinned input (consumed as
 * one bin — see liftCyclic pinned semantics). Passes the spread through
 * as its output, where tick() picks it up and the p5 driver invokes
 * each Layer in array order (= wire order = z-order).
 *
 * The Renderer also owns the **canvas dimensions**: `width` and `height`
 * are `param`-linked input pins — editable inline on the node (the value
 * lives in `paramStore`), OR wired from upstream to drive the resolution
 * from the graph (a slider, a constant, computed dims, …). The p5 driver
 * reads the dims from `paramStore` each frame and calls `resizeCanvas`
 * when they change — no recompile, no remount. When a pin is wired,
 * compute writes the resolved value back into `paramStore` so the driver
 * picks it up through that same path (one-frame settle).
 *
 * A patch with no Renderer renders nothing.
 */
export const rendererNode = registerNode({
  type: 'render/renderer',
  category: 'Render',
  label: 'Renderer',
  description: 'Render wired layers to the preview canvas.',
  inputs: [
    // One wire (a single Layer or a Spread<Layer> — e.g. from a ForEach, a
    // Group, or a Merge). To composite several sources into one frame, route
    // them through transform/group or spread/merge first — every pin takes
    // exactly one wire.
    { name: 'layers', type: 'layer', kind: 'spread' },
    // Default to black so an unwired Renderer doesn't paint the canvas
    // the same colour as DEFAULT_STYLE.fill (#ffffff) — which would make
    // every shape invisible against the background. Patches that wire a
    // Color into `background` override this normally.
    { name: 'background', type: 'color', default: { r: 0, g: 0, b: 0, a: 1 } },
    // Canvas resolution. `param`-linked so an unwired pin falls back to the
    // inline-editable paramStore value (no recompile on edit); wiring the
    // pin overrides it and drives the resolution from the graph.
    { name: 'width', type: 'number', param: 'width', default: 500 },
    { name: 'height', type: 'number', param: 'height', default: 500 },
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
  //
  // Sync the resolved width/height back into paramStore: for an unwired
  // pin inputs[2]/[3] already came FROM paramStore (the `param` linkage),
  // so this is a no-op; for a wired pin it pushes the upstream value into
  // paramStore where the p5 driver reads canvas dims. setKey no-ops when
  // unchanged, so this never spins the dirty bit.
  computeRaw: (inputs, ctx) => {
    const w = inputs[2]?.[0];
    const h = inputs[3]?.[0];
    if (typeof w === 'number' && isFinite(w)) paramStore.setKey(ctx.nodeId, 'width', w);
    if (typeof h === 'number' && isFinite(h)) paramStore.setKey(ctx.nodeId, 'height', h);
    return [inputs[0] ?? [], inputs[1] ?? [null]];
  },
});
