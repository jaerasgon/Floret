# Feature-complete for generative art: summary for the owner

All nine items of `docs/plan-feature-complete.md` are on `main`, merged one
branch at a time. The owner approved the decisions listed below, and the
changes they asked for are done (see
[Decisions](#decisions-approved-by-the-owner-2026-10-03) and
[Samples](#samples-done)). On the merged tree, `cargo fmt --check`,
`cargo clippy -D warnings` and `cargo test --workspace` pass, including
parity, `all_samples` and the app's sample-layout tests. Every new or
changed sample was screenshotted under Xvfb + lavapipe. Nothing has run on
a real Mac yet.

## What's new, and the samples that show it

| Item | What you get | Samples |
|---|---|---|
| 1. Recording | **File → Export Video / GIF…** (Cmd+Shift+E), also at the bottom of each preview's Export menu. Exports MP4 (H.264), GIF or a PNG sequence at a chosen size, fps and number of loops. Rendering is offline at a fixed dt, with progress and Cancel. A **loop length** patch setting (`loopLength` in the file), set in the toolbar's **Loop** field or the Export window, and a `loop` 0..1 output on the Time node. | `loop-bloom` (Motion) |
| 2. Geometry | New **Geometry** category:<br>• **Scatter** (poisson / random / jittered grid / halton)<br>• **Cells** (voronoi / delaunay / hull)<br>• **Path Ops** (resample / smooth / subdivide / simplify / offset)<br>• **Shape Boolean** (union / intersect / subtract / xor)<br>• **L-System** (its own node: none of its pins overlap the others) | `voronoi-mosaic`, `contour-blobs` (Geometry), `lsystem-plant` (Nature) |
| 3. Noise | One **Noise** node: perlin, simplex, value, cellular (F1 / F2 / F2−F1), curl. Octaves, falloff, seed and domain warp. Works per element over spreads. The old Perlin and Curl nodes are hidden but still load, and Noise reproduces their output exactly. | `noise-cellular-warp` (Fields) |
| 4. Expression | **Expression** node: type `sin(x * 3) + y * t`. Each free name becomes a pin (`p.x` gives a vec2 pin, `c.r` a colour pin). It works per element and has `t`, `frame`, `index`, `count`. Errors show on the node with a column number. No loops or IO, and the formula size is capped. | `expression-rose` (Curves) |
| 5. Seeds and presets | **Patch seed**, mixed into every random / noise node (seed 0 = exactly the old output). **Vary** toolbar button, View → New Variation, Cmd+Shift+R. A **Presets** dock tab (right after Inspector, and brought forward when a patch with presets opens): save, recall, rename and update presets, and morph A↔B with a slider. Seed and presets are optional fields in the `.flora` file. | `variations` (Fields; ships "Calm" and "Storm" presets) |
| 6. Gradients and masks | **Gradient Fill** style op: linear / radial / conic, from a colour spread with optional stop positions, extend modes pad / repeat / mirror, applying to fill or stroke. Drawn on the GPU everywhere a colour goes. **Mask** node: alpha or luminance, with invert. | `gradient-sunset` (Compositing) |
| 7. Post-effects | **Effect** node: bloom, grain, displace (by noise or a map layer), dither, halftone, chromatic aberration, vignette. GPU passes; grain animates. | `neon-bloom` (Shaders) |
| 8. Simulations | **Simulation** node (Particles): flocking, physarum (slime mould trail map as an image), DLA growth, Verlet cloth. Stateful, with reset; fixed 1/60 s steps. | `sim-flocking`, `sim-slime-mould`, `sim-dla-coral`, `sim-cloth` (Particles) |
| 9. Colour | A `space` option (rgb / oklab / oklch) on **Lerp** and **Gradient**. The **Palette** node gets modes: stops (as before), cosine (a, b, c, d), complementary, analogous, triadic, split, tetradic. | `color-spaces` (Color) |

## What to test on the Mac

- **Export:** MP4, GIF and PNG sequence, from the File menu and from a preview's Export menu.
  - Cmd+Shift+E through the native menu bar.
  - MP4s play in QuickTime and Safari and loop seamlessly on repeat.
  - GIF colours look right in a browser.
  - Cancel mid-export.
  - 2× and 4K exports.
  - The toolbar **Loop** field: drag it, type a value, type 0 or clear it for "off", and Cmd+Z.
- **Variations:**
  - Cmd+Shift+R and View → New Variation.
  - Cmd+Z after Vary, Recall and Morph.
  - The Presets tab: dragging the morph slider, renaming, the A/B combo boxes.
  - Opening `variations` brings the Presets tab forward.
  - Save and reopen a file with presets.
- **Expression:** edit a formula (it commits on focus loss or Cmd+Enter).
  - Pins update.
  - Wires survive a typo.
  - The error chip is readable.
- **Speed in a `./floret.sh` build:**
  - `neon-bloom` (four full-canvas passes; ~270 ms a frame on the software GPU here).
  - `noise-cellular-warp` (4096 rects).
  - `sim-slime-mould`.
  - Scatter with tens of thousands of points.
- **Rendering:**
  - Banding in large gradients.
  - A mask inside a blend group or a rotated clip.
  - Halftone and dither when the preview is zoomed.
- **Interaction:**
  - Mouse in the four simulation samples (drag and tear in `sim-cloth`, the Bang reset in `sim-dla-coral`).
  - Mode switching on the Noise, Palette, Scatter, Path Ops and Effect nodes; pins should follow the mode.
- Still untested from before: sound, mic, camera, body tracking, MIDI, video, fonts.

## Decisions (approved by the owner, 2026-10-03)

Every decision below was approved. Where one asked for a change, the change
is done.

**Visible changes to existing UI or nodes**
1. **Vary** toolbar button (its own group after the transport) and
   **Cmd+Shift+R**; Cmd+R still restarts. Kept as is.
2. **Presets tab:** it now comes right after Inspector in the default dock,
   so the whole tab is on screen at the default 400 px width (Shader and
   Profiler are the tabs that get cut off now). Opening a patch that has
   presets brings the Presets tab forward. A dock layout saved earlier
   keeps its own tab order until the layout is reset (Reset layout in the
   dock's `+` menu).
3. **Palette** "mode" row and **Lerp** / **Gradient** "space" row: kept.
4. **Loop length in the toolbar:** a compact **Loop** field after the
   transport buttons shows the patch's loop length in seconds, or "off"
   while the patch doesn't set one (the Time node's `loop` output and
   exports then use the 4 s default). Drag it, or click and type; 0 or a
   blank turns it off. It edits the same `loopLength` setting as the Export
   window, and every edit is an undo step.

**New Library categories:** Geometry, Color, Compositing.

**Behaviour choices, all kept as implemented**
5. **What a preset holds:**
   - Unwired pin values and control values (sliders, toggles, pickers…) plus the seed.
   - Not setup params such as files, URLs, sizes, shader source, or slider min/max.
   - Nothing that would change a node's pins, so recalling never rewires the patch.
   - Morph interpolates numbers, vec2 and colours; bools, strings, dropdowns and the seed switch at the halfway point.
6. A variation or a pin edit on a dynamic-pins node rebuilds the engine, so simulations and particles restart, as editing a pin does today.
7. `sound/noise` and `sound/reverb` ignore the patch seed: re-rolling them is inaudible and would click.
8. **Noise and Expression conventions:**
   - Perlin mode isn't clamped, to stay identical to the old node (it overshoots 0..1 by about 0.03); the other noise modes are clamped.
   - In Expression, colour channels are 0..1 and `hsl()` takes 0..1 for all three values (h in turns), unlike the colour nodes' 0..255 / 0..360.
9. **Gradient Fill** blends in RGB (no OKLab option yet). Effects cover the whole canvas, like Shader.
10. The Simulation node stays in Particles as `particles/simulation`.
11. **Debug speed:** floret-engine and floret-render build at
    `opt-level = 1` in the dev profile, next to the other per-crate
    opt-levels in `native/Cargo.toml`.

**Licensing and toolchain**
12. **H.264 patents** (still to check before a commercial release; noted in THIRD_PARTY_NOTICES):
    - OpenH264 is compiled from source, and the app now *encodes* as well as decodes.
    - Cisco's royalty-free patent coverage only applies to Cisco's own prebuilt binaries.
13. **New dependencies:** `delaunator` (ISC), `i_overlay` (MIT / Apache-2.0) and `gif` (MIT / Apache-2.0). All are listed in THIRD_PARTY_NOTICES.
    - The workspace now declares `rust-version = "1.95"`, which is what the dependencies need (egui 0.36 needs 1.95; delaunator and i_overlay need 1.88). Every crate uses the workspace value.
    - The clippy lints this turns on (`chunks_exact_to_as_chunks`, `manual_is_multiple_of`) are fixed.

## Samples (done)

These are changes to the Library samples only. Parity still reads the
frozen copies in `native/fixtures/samples`, which are unchanged.

- **Fractal trees:** `noc-fractal-tree` is removed from the Library because it was the same patch as `fractal-tree-spread`.
  - `fractal-tree-spread` is now named "Fractal Tree" and carries the NOC description.
  - The frozen fixture and trace of `noc-fractal-tree` stay, so parity still covers it.
- **Off-centre pictures:** their positions were written for top-left anchors, or an offset pushed a grid off the origin. They are now centred:
  - single shapes: `tut-01`, `tut-02`, `tut-05`, `tut-14`, `tut-15`, `tut-16`, `tut-19`, `tut-30`, `tut-33`, `tut-45`, `noc-drag` (the water now fills the canvas), `pixel-sort` and `datamosh`;
  - rows and grids: `tut-06`, `tut-17`, `tut-42`, `tut-43` (also twice the size), `curl-flow` (it was in one corner), `force-field`, `ripple-field` and `tone-surface`;
  - bar charts: `tut-41`, `tut-49` and `csv-bar-chart` now stand on a baseline (anchor `bl`) instead of floating, and `local-peaks` moved up to the middle;
  - text: `tut-34` (centre-aligned) and `tut-44`.
- **Pictures that didn't match their comment:**
  - `tut-11`, `tut-32`: the line had the default black stroke on the black canvas. It is white now.
  - `tut-22`, `tut-26`: an open polygon with only a fill draws nothing. They stroke the spiral instead.
  - `tut-27`: it sampled one flat colour, half of it off the image. It now rasterises a drifting diagonal gradient and samples all of it.
  - `tut-44`: the sum was wired into the text's x. It is now the text (it shows 55).
  - `photo-mosaic`: it sampled the photo at canvas coordinates, so only one quarter of the photo showed. It now reads the whole photo.
- **Comments and descriptions fixed:**
  - `tut-19`: the rect blinks on time.
  - `tut-20`: the subpatch is used once, beside an ellipse.
  - `tut-22`, `tut-26`, `tut-27`: match their new pictures.
  - `tut-28`: no mention of a browser.
  - `tut-41`: the shuffle node is in the patch, and sorting undoes it.
  - `tut-43`: it starts from a random soup, not a starter pattern.
  - `tut-47`: the mouse button is the gate.
  - `kaleidoscope`: a photo, not a mouse-controlled triangle.
  - `datamosh`: add / subtract, not screen / difference.
  - `type-kinetic-wave`: 40 rows, and no row envelope.
  - `type-halo`: three rings.
  - `type-rain`: letters dim towards the bottom.
  - `loop-bloom`: mentions the toolbar Loop field.
- The layout tool (`relayout_samples`) was re-run. It also moved nodes in
  some untouched samples whose nodes had grown since its last run (for
  example the taller Palette and Lerp nodes).

## Still open

The deferred list is unchanged: the proper Floret.app bundle, #77, #78, #81 and #82.
