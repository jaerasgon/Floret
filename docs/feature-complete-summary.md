# Feature-complete for generative art: summary for the owner

All nine items of `docs/plan-feature-complete.md` are on
`claude/local-testing-setup-80hq09`, merged one branch at a time. On the
merged tree, `cargo fmt --check`, `cargo clippy -D warnings` and
`cargo test --workspace` pass, including parity, `all_samples` and the
app's sample-layout tests. Every new sample was screenshotted under
Xvfb + lavapipe. Nothing has run on a real Mac yet.

## What's new, and the samples that show it

| Item | What you get | Samples |
|---|---|---|
| 1. Recording | **File → Export Video / GIF…** (Cmd+Shift+E), also at the bottom of each preview's Export menu. Exports MP4 (H.264), GIF or a PNG sequence at a chosen size, fps and number of loops. Rendering is offline at a fixed dt, with progress and Cancel. A **loop length** patch setting (`loopLength` in the file), and a `loop` 0..1 output on the Time node. | `loop-bloom` (Motion) |
| 2. Geometry | New **Geometry** category:<br>• **Scatter** (poisson / random / jittered grid / halton)<br>• **Cells** (voronoi / delaunay / hull)<br>• **Path Ops** (resample / smooth / subdivide / simplify / offset)<br>• **Shape Boolean** (union / intersect / subtract / xor)<br>• **L-System** (its own node: none of its pins overlap the others) | `voronoi-mosaic`, `contour-blobs` (Geometry), `lsystem-plant` (Nature) |
| 3. Noise | One **Noise** node: perlin, simplex, value, cellular (F1 / F2 / F2−F1), curl. Octaves, falloff, seed and domain warp. Works per element over spreads. The old Perlin and Curl nodes are hidden but still load, and Noise reproduces their output exactly. | `noise-cellular-warp` (Fields) |
| 4. Expression | **Expression** node: type `sin(x * 3) + y * t`. Each free name becomes a pin (`p.x` gives a vec2 pin, `c.r` a colour pin). It works per element and has `t`, `frame`, `index`, `count`. Errors show on the node with a column number. No loops or IO, and the formula size is capped. | `expression-rose` (Curves) |
| 5. Seeds and presets | **Patch seed**, mixed into every random / noise node (seed 0 = exactly the old output). **Vary** toolbar button, View → New Variation, Cmd+Shift+R. A **Presets** dock tab: save, recall, rename and update presets, and morph A↔B with a slider. Seed and presets are optional fields in the `.flora` file. | `variations` (Fields; ships "Calm" and "Storm" presets) |
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
- **Variations:**
  - Cmd+Shift+R and View → New Variation.
  - Cmd+Z after Vary, Recall and Morph.
  - The Presets tab: dragging the morph slider, renaming, the A/B combo boxes.
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

## Decisions that need your approval

**Visible changes to existing UI or nodes**
1. A **"Vary"** toolbar button, in its own group after the transport buttons, and the **Cmd+Shift+R** shortcut. Cmd+R still restarts.
2. The **Presets** tab is added at the end of the dock. At the default 400 px dock width it is cut off past Profiler. Should it move earlier, or open automatically when a patch with presets loads?
3. Existing **Palette** nodes now show a "mode" row, and **Lerp** and **Gradient** a "space" row, so they are one row taller.
4. The loop length can only be set in the Export window. A toolbar or inspector control is a possible addition.

**New Library categories:** Geometry, Color, Compositing.

**Behaviour choices**
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
10. The Simulation node lives in Particles as `particles/simulation`, not in its own category.
11. Should floret-engine get `opt-level = 1` in the dev profile? Simulations and the engine would run much faster in debug builds.

**Licensing and toolchain**
12. **H.264 patents:**
    - OpenH264 is compiled from source, and the app now *encodes* as well as decodes.
    - Cisco's royalty-free patent coverage only applies to Cisco's own prebuilt binaries.
    - Check this before a commercial release (noted in THIRD_PARTY_NOTICES).
13. **New dependencies:** `delaunator` (ISC), `i_overlay` (MIT / Apache-2.0) and `gif` (MIT / Apache-2.0). All are listed in THIRD_PARTY_NOTICES.
    - delaunator and i_overlay need Rust 1.88, while the workspace still declares `rust-version = "1.85"`.
    - Raising it turns on new clippy lints in existing code, so it was left as is. `./floret.sh` installs current Rust, so the Mac build isn't affected.

## Still open from before

The questions in the plan about samples (noc-fractal-tree vs fractal-tree-spread, off-centre samples, wrong comments) are unchanged, as is the deferred list (app bundle, #77, #78, #81, #82).
