/**
 * Cook-on-change dirty bit.
 *
 * The frame loop (canvas-driver) skips the entire runtime tick AND layer
 * playback on an idle frame — the Canvas2D surface keeps last frame's
 * pixels, so the user sees a frozen-but-correct frame at ~zero CPU cost.
 *
 * The dirty bit is flipped true by any event that *could* change a node's
 * output: editor mutations (via `_compile`), paramStore writes (via
 * `subscribeAll`), and any caller that explicitly calls `markDirty()`. It's
 * cleared after a tick completes, so a static patch returns to zero-cost
 * idle on the very next frame.
 *
 * The complementary "always tick" signal lives on the compiled graph
 * (`RuntimeGraph.alwaysAnimate`, set when the patch contains a genuinely
 * live node — time / state / live input / audio). The driver gates on
 * `!graph.alwaysAnimate && !isDirty()`; this module owns only the bit.
 *
 * State is module-level rather than per-graph because the driver holds one
 * active graph at a time. On patch swap or HMR, `markDirty()` is called as
 * part of compile so the new graph paints at least once.
 */

let dirty = true;

export function markDirty(): void {
  dirty = true;
}

export function clearDirty(): void {
  dirty = false;
}

export function isDirty(): boolean {
  return dirty;
}
