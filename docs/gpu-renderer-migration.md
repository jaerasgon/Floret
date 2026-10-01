# GPU Renderer Migration — Implementation Plan

**Status:** landed in the web app (historical). The web app is now reference-only; the native app's renderer is wgpu — see `docs/native-app-transition.md` and issue #60.
**Date:** 2026-06-27
**Decision:** big-bang replace — convert all rasterizer sites and delete the
Canvas2D renderer in one merge. `main` is untouched until the branch lands and
passes the parity gate.

---

## 1. Goal

Replace the Canvas2D rasterizer (`scene-renderer.ts`) with a single WebGL2 GPU
renderer that consumes the existing retained-mode `SceneNode` IR. The IR is the
source of truth and does **not** change; only the thing that turns it into
pixels changes.

**Why:** lift the render ceiling (Canvas2D `fillStyle`/path-fill is the
bottleneck — measured ~few-thousand shapes vs ~150k at 60fps on GL) and unlock
the shader vocabulary (feedback buffers, post-FX) that Canvas2D cannot express.

**What stays:**
- `SceneNode` IR (`scene.ts`) — the declarative scene description.
- SVG/vector export (`scene-svg.ts`) — a separate IR visitor; keeps plotter /
  print-vector output and reproducible editions.
- Determinism at the IR level (seeded PRNG).

**Single solution, not a mix:** GL is the *only* rasterizer. Where a 2D canvas
remains (thumbnail display surfaces) it is a dumb blit target receiving a
GL-rendered bitmap — it never rasterizes scene content.

---

## 2. Current architecture (what we're replacing)

The IR is rasterized in **7 sites**:

| Site | File | Role | Thread |
|---|---|---|---|
| Preview render | `runtime/render-worker.ts` via `render-bridge.ts` | on-screen Preview(s) | Worker (OffscreenCanvas 2D) |
| Canvas driver | `runtime/canvas-driver.ts` | RAF loop + tick + post to bridge | main |
| Thumbnails | `persistence/thumbnail.ts` | sample/patch thumbnails | main |
| Probe node | `editor/nodes/ProbeNode.tsx` | per-node preview | main |
| Layer→image | `nodes/image/from-layer.ts` | rasterize layers to an image **mid-graph** | main |
| Region cache | `nodes/transform/cache.ts` | cache a rendered region as an image | main |
| Send / e-ink | `nodes/io/net.ts` | rasterize layers → PNG to the bus | main |

Preview path: each Preview owns a `RenderBridge` which `transferControlToOffscreen()`s
its canvas to a dedicated **Worker**; the worker runs `scene-renderer.ts` on a
2D `OffscreenCanvas`. Multiple Previews ⇒ multiple workers. The bridge already
posts the per-frame `layers` (and converts image sources to `ImageBitmap`s).

Mid-graph rasterizers (`from-layer`, `cache`, `net`) run synchronously on the
main thread and produce pixels the graph (or the bus) consumes.

---

## 3. Target architecture

```
SceneNode IR ──┬─► scene-to-batches (walk + bake transforms) ─► FrameBuilder
               │        (commands: sdf | fill | tex | pushLayer/popLayer)
               │
               └─► scene-svg.ts  (unchanged vector export)

FrameBuilder ─► GlRenderer ─► WebGL2
   pipelines:  A. SDF instances     rect / ellipse(exact) / capsule
               B. tessellated fill  triangle/quad/polygon/path (winding rule)
               C. textured quad     text (MSDF atlas) / image
   state:      per-command clip (scissor) + blend (FBO compositing)
```

**Renderer instances / contexts:**
- Each Preview worker owns one GL context (OffscreenCanvas WebGL2). Few previews ⇒ few contexts.
- One **shared main-thread GL context** (singleton, offscreen) serves every
  non-preview surface: thumbnails, probe, slider, and the mid-graph
  rasterizers. Render into an FBO at the target size, then either `drawImage`
  the GL canvas into a 2D display canvas (thumbnails/probe) or `readPixels` /
  `transferToImageBitmap` for graph/PNG consumers.

Total contexts ≈ (#preview workers) + 1. Well under the ~16 limit; the old
"one 2D canvas per surface" model does not translate to GL and must not be
copied.

**Proposed production file layout** (promote the spike out of `src/spike/`):
```
src/runtime/gl/
  gl-renderer.ts        # programs, FBO stack, draw loop (from gl-vector-renderer.ts)
  scene-to-batches.ts   # SceneNode walk → FrameBuilder (from shader-spike.ts emit)
  tessellate.ts         # winding-rule polygon tessellation
  text-atlas.ts         # MSDF / glyph atlas + layout
  shared-context.ts     # singleton main-thread GL context + FBO render helpers
  __tests__/…
```

---

## 4. Parity matrix (the contract)

Every IR feature maps to a GPU technique. Status: ✅ done in spike · ◻ todo.

**Shapes**
| kind | technique | status |
|---|---|---|
| rect (+ rounded) | SDF instance | ✅ |
| ellipse / circle | exact SDF + circle fast-path | ✅ |
| line | capsule SDF | ✅ |
| point | small rect SDF | ✅ |
| triangle / quad | tessellated fill + capsule edges | ✅ |
| polygon (open/closed) | tessellated fill + capsule edges | ✅ (simple polys) |
| polygon **self-intersecting / holes** | winding-rule tessellation | ◻ |
| path (Catmull–Rom) | flatten béziers → polygon | ✅ |
| arc | sampled polyline | ✅ (approx) |
| text | textured quad (canvas atlas) | ✅ → ◻ MSDF for scale |
| image | textured quad + uv + tint | ✅ |
| particle-batch | one SDF ellipse instance per particle | ✅ |
| particle-trail-batch | capsule per segment (+ taper/fade) | ◻ verify/finish |

**Style / composition**
| feature | technique | status |
|---|---|---|
| fill / stroke | SDF fill + distance-band stroke | ✅ |
| strokeAlign center | band offset 0 | ✅ |
| strokeAlign inside/outside (closed) | band offset ±w/2 (SDF); polygon strokes ◻ | ◻ |
| stroke caps (butt/round/square) | capsule = round; butt/square ◻ | ◻ |
| stroke joins (miter/bevel/round) | capsule overlap = round; miter/bevel ◻ | ◻ |
| transform (translate/rotate/scale/**shear**) | per-instance 2×2 matrix | ✅ |
| group **clip** | per-command scissor (world AABB, nested) | ✅ (no sample exercises yet) |
| group **blend mode** | render-to-FBO + composite | ◻ |
| **blur** (`style.blur`) | separable Gaussian post pass | ◻ |
| image tint (multiply) | composite shader | ✅ (approx) → verify |
| anti-aliasing | analytic `fwidth` (SDF) + MSAA (fills) | ✅ |

---

## 5. Stage 0 — engine parity (current stage)

Each increment ends green (tsc + tests) and is checked against the **A/B diff
harness** (see §8). Order chosen so the highest-risk/most-used items land first.

1. **Clip rects + command-state machinery** — ✅ done.
   Per-command world-AABB scissor, nested intersection, run-breaking on
   clip/blend change. Foundation for blend.
2. **Blend modes (FBO compositing)** — ◻ next.
   - Add `pushLayer` / `popLayer(blend)` commands. A blend group renders its
     children into a fresh FBO; on pop, composite that texture onto the parent
     target with the blend mode.
   - Render the *whole* scene into an FBO chain (not the default framebuffer)
     so non-separable modes (multiply/screen/overlay/…) can sample both source
     and destination textures; final blit FBO→canvas.
   - Separable modes via blend func where possible (perf); advanced modes via
     the composite shader.
   - Verify against `datamosh` (the one sample using blend) + the feedback/
     trail samples.
3. **Winding-rule polygon fills** — ◻.
   Replace ear-clip with a winding-rule tessellator (libtess-style monotone
   decomposition) so self-intersecting / holed polygons fill correctly. Verify
   against `tut-23-polygon` (the star) and `grid-art`.
4. **Stroke parity + blur** — ◻.
   - Polygon stroke inside/outside align (offset the outline), miter/bevel
     joins, butt/square caps (currently round-only via capsules).
   - `style.blur`: render the shape/group to an FBO, separable Gaussian pass,
     composite. Reuses the blend FBO machinery.
5. **Text MSDF + trail batches + verify** — ◻.
   - MSDF glyph atlas (per font) for resolution-independent text; replaces the
     per-string canvas atlas which softens when scaled and balloons with unique
     strings. Honour align/baseline/letterSpacing/lineHeight.
   - Finish `particle-trail-batch` (taper + fade) and verify against
     `particles-text-dust` / a trail sample.
   - Exact arc (true arc SDF or denser sampling), verify image tint.

**Gate to leave Stage 0:** A/B harness shows every visual sample within
tolerance (or a documented, accepted diff), tsc clean, suite green.

---

## 6. Stage 1 — preview render path on GL

- Swap `render-worker.ts` from `OffscreenCanvas.getContext('2d')` +
  `scene-renderer` to `getContext('webgl2')` + `GlRenderer`.
- The worker holds a `GlRenderer` + a `scene-to-batches` walk. `RenderBridge`'s
  per-frame protocol is mostly unchanged (it already posts `layers`, `bg`,
  `centreOrigin`, and transfers image bitmaps); image sources arrive as
  `ImageBitmap` and become GL textures in the worker.
- `centreOrigin` maps to the view uniform (origin centre vs top-left).
- Verify: full sample set live in the actual Preview panel, multiple Previews
  side-by-side (multiple workers/contexts), resize, fps.

---

## 7. Stage 2 — main-thread surfaces & mid-graph rasterizers

- Build `shared-context.ts`: a lazily-created singleton offscreen WebGL2 canvas
  + `GlRenderer`, with `renderToCanvas(layers, w, h, bg) → HTMLCanvasElement`
  and `renderToBitmap(...) → ImageBitmap` and `renderToPng(...) → Blob`.
- **Thumbnails / Probe / Slider:** render via shared context → `drawImage` into
  the existing 2D display canvas (2D canvas = blit target only).
- **`from-layer` / `cache`:** render to FBO → `transferToImageBitmap` /
  readback into the canvas the graph expects downstream.
- **`net` (Send/e-ink):** render to FBO → `readPixels` → encode PNG (replaces
  `canvas.toBlob`). Keep the bus protocol identical.
- **Export:** high-res render via shared context at arbitrary size.

---

## 8. Verification — A/B diff harness (built in Stage 0)

A dev harness that, for every catalogue sample: ticks N frames deterministically,
renders the same `layers` through **both** Canvas2D (`scene-renderer`) and GL,
and computes a per-sample pixel diff (mean abs error + max region). Output a
table; flag any sample over tolerance. This is the gate for retiring Canvas2D —
parity is *measured*, not eyeballed. Keep it as a dev page until Stage 3, then
retire with the old renderer.

Plus: existing `vitest` suite (IR-level, unaffected), perf benchmark
(shapes@fps across GPUs), manual checklist (resize, multi-preview, context
loss).

---

## 9. Stage 3 — retirement

- Delete `scene-renderer.ts` and the 2D paths in `canvas-driver` / worker.
- Promote `src/spike/*` → `src/runtime/gl/*`; remove `shader-spike.html` and the
  spike harness (fold useful bits into the A/B page or delete).
- Confirm zero remaining `getContext('2d')` rasterization of scene content
  (`rg "getContext\('2d'\)"` should only hit blit/measure/text-atlas helpers).
- Update `CONCEPTS.md` (render architecture section).

---

## 10. Risks & mitigations

| risk | mitigation |
|---|---|
| WebGL context loss (tab backgrounded, GPU reset) | `webglcontextlost`/`restored` handlers; rebuild programs/textures; previews re-tick |
| Context limit with many previews | few previews in practice; shared context for all small surfaces |
| Headless CI can't run GL | IR-level tests stay; rasterizing checks gated/skipped in Node (as the 2 camera tests already are) |
| Cross-GPU pixel variance | parity tolerance in the A/B harness; determinism stays at IR level; reference raster for editions if needed |
| Tiny-scene overhead (GL setup > 2D for 5 shapes) | negligible at 60fps; acceptable trade for the ceiling |
| Text atlas memory (many unique strings) | MSDF per-glyph atlas (bounded) instead of per-string canvases |
| Worker image-source transfer cost | already `ImageBitmap`-transferred; upload once, cache by identity |
| Blend FBO cost per group | only groups with a blend mode allocate an FBO; pool/reuse FBOs by size |

## 11. Rollback

Entirely on `spike/shader-renderer`; `main` is unaffected until merge. Any
increment can be reverted in isolation. The A/B gate prevents merging a
regressing renderer.

## 12. Sequencing summary

Stage 0 (this branch, now) → Stage 1 → Stage 2 → Stage 3, each green and
A/B-checked. Estimated several focused sessions; blend/FBO and the
worker+surfaces integration are the two largest pieces.
