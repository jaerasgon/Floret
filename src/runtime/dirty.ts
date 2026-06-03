/**
 * Cook-on-change scheduler.
 *
 * The frame hook checks `shouldTick()` once per RAF. When false, it
 * skips the entire runtime tick AND the layer playback — p5's canvas
 * keeps last frame's pixels naturally (we don't call `background()` or
 * invoke any layers), so the user sees a frozen but correct frame at
 * essentially zero CPU cost.
 *
 * Two signals re-enable ticking:
 *
 *   1. **alwaysAnimate** — set at compile time when the graph contains
 *      any node that genuinely changes value every frame (time, state,
 *      live input, audio updates). Such patches tick forever.
 *
 *   2. **dirty bit** — flipped true by any event that *could* change
 *      a node's output: editor mutations (via `_compile`), paramStore
 *      writes (via `subscribeAll`), and any caller that explicitly
 *      calls `markDirty()`. Cleared after a tick completes, so static
 *      patches return to zero-cost idle on the very next frame.
 *
 * Together: a static patch with no live nodes ticks exactly once per
 * change. Idle frames are O(1). That's the cook-on-change discipline
 * TouchDesigner uses; our advantage over p5.js, which has no graph
 * and so must redraw every frame defensively.
 *
 * State is module-level rather than per-graph because the p5 driver
 * holds only one active graph at a time (the renderer is a singleton).
 * On patch swap or HMR, `markDirty()` is called as part of compile so
 * the new graph paints at least once.
 */

let dirty = true;
let alwaysAnimate = false;

export function markDirty(): void {
  dirty = true;
}

export function clearDirty(): void {
  dirty = false;
}

export function isDirty(): boolean {
  return dirty;
}

/**
 * Called by compile after analysing the graph. `true` when the patch
 * has at least one node whose output legitimately changes every frame
 * (time / state / live input / audio). Pass `true` to also flip the
 * dirty bit, so the first frame after a recompile always paints.
 */
export function setAlwaysAnimate(v: boolean): void {
  alwaysAnimate = v;
  if (v) dirty = true;
}

export function shouldTick(): boolean {
  return alwaysAnimate || dirty;
}
