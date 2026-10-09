# Sample review (all 146 Library samples)

A human-eye pass over every sample in `native/samples/` (October 2026,
from `main` at 71c26bc), on top of the automated checks: every sample
already loads, compiles without warnings, ticks, draws a non-flat GPU frame
and survives save → reload.

**Result: 113 OK, 18 fixed, 15 known limitations** (all of these are about
hardware or the network, not bugs). The table below covers every sample.

## How it was checked

- **Rendering:** `review_samples`, an ignored test in
  `native/crates/floret-app/src/sample_sweep.rs`, runs each sample for 240
  frames at 60 fps with a scripted mouse (circling, pressed from frame 90 to
  150) and the R key held from frame 100. It renders every frame at full
  canvas size on the GPU, as the preview does, saves frames 1, 60 and 240,
  and records frame time, the inked box, coverage and frame-to-frame change.
  Every frame was looked at: category contact sheets (frame 60), and strips
  of frames 1, 60 and 240 to check motion.
- **Photos:** the sandbox can't reach picsum.photos, so photo samples were
  also rendered with a stand-in picture (`FLORET_REVIEW_IMAGE`). The video
  decoder is installed as in the app.
- **Comments:** every comment node and catalogue description was read
  against the patch and its picture. Node names were compared with the live
  labels in `nodes.json`, and port and option names with the specs. All
  dropdown values were checked against their options.
- **Controls:** `review_controls`, another ignored test, runs every sample
  that has a Slider, Toggle or Bang twice in step. It moves the control in
  one copy as the editor does, and checks the Renderer's layers change. All
  52 controls work; the exceptions are camera or audio controls, and
  image-only changes that were confirmed by rendering instead.
- **In the app:** checked under Xvfb + lavapipe with xdotool:
  - mouse following, painting, dragging and grabbing in 15 samples;
  - keyboard in tut-30 (the window needs focus explicitly, since Xvfb has no
    window manager);
  - slider drag in tut-05 and Bang click in sim-dla-coral;
  - the Shader panel's buffer tabs;
  - device samples without their device.
- **Layout:** the largest patches were eyeballed in the editor on open
  (color-spaces, type-kinetic-wave, fractal-tree-generative, path-ops-shapes,
  noc-wind-gravity, webcam-mosaic, body-tracking, sim-dla-coral).
- **Timing:** the frame time is the median tick plus render (with readback)
  per frame, in a **debug build on lavapipe** (software Vulkan). The machine
  was shared with two other builds, so absolute numbers vary up to about 2×
  run to run; the ranking is stable. The median sample takes 5.8 ms and the
  90th percentile 41 ms.

Screenshots: `/tmp/agent-samples-review/` (not in the repo).
- `sheets/<Category>-f060.png` are the contact sheets; `sheets/<Category>-strips-N.png` show frames 1 | 60 | 240.
- `frames/` holds every saved frame, with `review-final.tsv` alongside.
- `app/` holds the in-app screenshots, including the `*-mouse.png`, `*-drag.png`, `*-keys.png` and `*-click.png` interaction strips.

To re-run:

```sh
cd native
FLORET_REVIEW_OUT=/tmp/review cargo test -p floret-app -- --ignored review_samples --nocapture
cargo test -p floret-app -- --ignored review_controls --nocapture
# options: FLORET_SAMPLES_ONLY=a,b  FLORET_REVIEW_FRAMES=1,600,1800  FLORET_REVIEW_IMAGE=photo.jpg
```

## Engine / app fixes

- **Unknown family ops now warn.** A Math / Compare / … node whose op isn't
  one of its family's silently ran the default op, so flow-lines' `mul`
  added instead of multiplying. Compile now warns on the node, so
  `all_samples` catches this. Behaviour is unchanged: it still runs the
  default op, so newer files load. Test:
  `an_unknown_op_warns_and_runs_the_default`.
- **Particles from a text layer spawn inside the letters.**
  `particles/from-layer` sampled `draw/text` as its bounding box, the TS
  fallback kept until the engine could shape text (#61). It now samples the
  glyph outlines, as the geometry nodes already read `draw/text`. Replays of
  TS reference data still get the box, through the new
  `scene::ts_reference()`, so parity and the `ts_cases` stay exact. Test:
  `from_layer_samples_text_inside_its_glyphs`.

## Systemic issues

1. **Comment text is stored twice.** It is in `params.text`, which the
   canvas shows, and in `portValues.text`, which nothing shows. In 7
   tutorials the hidden copy still named removed nodes (Stack, Scale,
   `logic/eq`, `vec2/*`, Sort Spread). They are synced now. Suggestion: have
   save drop or sync the second copy, so it can't go stale again.
2. **Silent fallbacks hid real bugs:**
   - an unknown op ran the default op (now warned);
   - a Stroke with no colour drew black on black (pendulum, tut-15);
   - a renderer background given as a `"#hex"` string drew black
     (pendulum).

   Suggestion: have the renderer read hex strings, as Palette does, or warn.
3. **Shaders may never see `iFrame == 0` in the app.** The preview doesn't
   render the very first tick of a freshly opened patch, so Shadertoy-style
   `if (iFrame < 1)` initialisation never ran: the reaction-diffusion sample
   stayed blank in the app, though fine headless. The sample now also seeds
   where its buffer is empty. Suggestion: start a shader node's `iFrame` at
   its first rendered frame, or make sure frame 0 renders.
4. **Hidden legacy nodes in samples:** `color/hsl` (17 samples),
   `noise/perlin` (9), `color/split-rgb` (2) and `noise/curl` (1). They
   render correctly, but a user can't add them from the browser by the names
   the comments use (tut-17 is called "Perlin Noise"). Suggestion: move them
   onto Color (hsl mode) and Noise (perlin / curl), checked with
   `rewritten_samples_draw_as_before`.
5. **Network photos:** 9 samples load picsum.photos and draw nothing (or
   lack their photo) offline, with no fallback picture.
6. **Software GPU cost:** the post-effect samples are 100–350 ms a frame on
   lavapipe in debug. That comes from large-radius blur, glow and bloom
   passes and Symmetry clipping at full canvas size: inherent to their look,
   not waste. type-raven blurs about 150 letters in their own layers. The
   noise fields (noise-patterns, noise-cellular-warp) are the only
   tick-heavy samples (35–40 ms in debug).
7. **NODES.md drift:** 44 node labels in NODES.md predate the
   "… Spread" / word-spacing renames (e.g. `spread/range` is listed as
   "Range"; the app shows "Range Spread").
8. **Layout drift:** `relayout_samples` on a clean checkout re-places about
   50 samples by a few pixels here (probably font metrics). Only the
   samples changed in this review were relaid out.
9. **Fit-on-open readability:** the largest patches (webcam-mosaic,
   body-tracking, color-spaces, fractal-tree-generative) open fitted, so
   their comments are too small to read until you zoom. Layouts are tidy,
   with no overlaps.

## All samples

Frame ms is the median per frame (tick + render with readback), debug
build, lavapipe. "Known limitation" means the sample needs hardware or the
network it can't get here.

| # | Sample | Category | Status | Frame ms (tick + render) | Notes |
|---|---|---|---|---|---|
| 1 | `tut-01-hello-rect` | Tutorial | OK | 2.0 (0.0 + 2.0) |  |
| 2 | `tut-02-color-fill` | Tutorial | OK | 2.0 (0.0 + 2.0) |  |
| 3 | `tut-03-animate` | Tutorial | fixed | 2.0 (0.0 + 2.0) | Hidden second copy of the comment named removed nodes; synced. |
| 4 | `tut-04-mouse` | Tutorial | OK | 1.8 (0.0 + 1.8) | Checked in the app: follows the cursor. |
| 5 | `tut-05-slider` | Tutorial | OK | 2.1 (0.0 + 2.1) | Checked in the app: dragging the slider widens the rect. |
| 6 | `tut-06-spread-foreach` | Tutorial | OK | 2.5 (0.1 + 2.4) |  |
| 7 | `tut-07-grid` | Tutorial | OK | 2.5 (0.1 + 2.4) |  |
| 8 | `tut-08-palette` | Tutorial | OK | 3.2 (0.1 + 3.1) |  |
| 9 | `tut-09-state` | Tutorial | OK | 2.6 (0.0 + 2.6) |  |
| 10 | `tut-11-polar` | Tutorial | OK | 2.0 (0.0 + 1.9) |  |
| 11 | `tut-12-circle` | Tutorial | OK | 2.4 (0.1 + 2.3) |  |
| 12 | `tut-13-map` | Tutorial | OK | 2.3 (0.0 + 2.3) | App: hue follows mouse X. |
| 13 | `tut-14-colorlerp` | Tutorial | OK | 2.8 (0.0 + 2.8) |  |
| 14 | `tut-15-style-chain` | Tutorial | fixed | 2.7 (0.0 + 2.7) | Stroke was near-black on the black canvas, so the lesson (fill AND stroke) didn't show. Stroke white; comment says so. |
| 15 | `tut-16-group-rotate` | Tutorial | OK | 1.9 (0.0 + 1.9) |  |
| 16 | `tut-17-perlin` | Tutorial | fixed | 2.2 (0.2 + 1.9) | End dots cut by the canvas edges. Row pulled in to +/-180. |
| 17 | `tut-18-random-spread` | Tutorial | OK | 2.6 (0.1 + 2.5) |  |
| 18 | `tut-19-if` | Tutorial | OK | 2.0 (0.0 + 2.0) |  |
| 19 | `tut-20-subpatch` | Tutorial | OK | 1.9 (0.0 + 1.8) |  |
| 20 | `tut-21-trails` | Tutorial | fixed | 3.0 (0.0 + 2.9) | Hidden comment copy said "Stack"; synced (Combine, concat). |
| 21 | `tut-22-cache` | Tutorial | OK | 7.6 (0.0 + 7.5) |  |
| 22 | `tut-23-polygon` | Tutorial | OK | 8.0 (0.0 + 8.0) |  |
| 23 | `tut-24-path-tension` | Tutorial | OK | 3.6 (0.1 + 3.5) |  |
| 24 | `tut-25-spring` | Tutorial | fixed | 2.1 (0.0 + 2.0) | Hidden comment copy said "Scale nodes"; synced (Vector Math, scale). |
| 25 | `tut-26-rasterize` | Tutorial | OK | 3.0 (0.0 + 3.0) |  |
| 26 | `tut-27-imgsample` | Tutorial | OK | 13.1 (4.9 + 7.9) |  |
| 27 | `tut-28-audio-react` | Tutorial | known limitation | 1.8 (0.0 + 1.8) | Needs a microphone: a small still circle without one. Comment says so. |
| 28 | `tut-29-oscillator` | Tutorial | OK | 2.0 (0.0 + 1.9) |  |
| 29 | `tut-30-keyboard` | Tutorial | fixed | 2.2 (0.0 + 2.1) | Hidden comment copy said `logic/eq`; synced (Compare, equal). Draws only while R / B is held (checked in the app: R red rect, B blue ellipse). |
| 30 | `tut-31-vec2math` | Tutorial | fixed | 2.3 (0.0 + 2.3) | Hidden comment copy named `vec2/*` nodes; synced. |
| 31 | `tut-32-normalize` | Tutorial | fixed | 2.4 (0.0 + 2.4) | Hidden comment copy said `vec2/normalize`; synced. |
| 32 | `tut-33-gradient` | Tutorial | OK | 3.2 (0.0 + 3.2) |  |
| 33 | `tut-34-text` | Tutorial | OK | 2.3 (0.0 + 2.3) |  |
| 34 | `tut-35-wanderer` | Tutorial | OK | 2.5 (0.0 + 2.5) |  |
| 35 | `tut-36-gaussian` | Tutorial | OK | 3.2 (0.2 + 3.0) | Some dots fall off the canvas (sd 80 on a 400 canvas) - natural for a bell curve. |
| 36 | `tut-37-image-url` | Tutorial | known limitation | 3.9 (0.0 + 3.9) | Needs the internet (picsum.photos): blank offline. Reviewed with a stand-in photo: OK. |
| 37 | `tut-38-image-adjust` | Tutorial | known limitation | 3.6 (0.0 + 3.6) | Needs the internet (picsum.photos): blank offline. Stand-in: OK. |
| 38 | `tut-39-pixelate` | Tutorial | known limitation | 3.7 (0.0 + 3.7) | Needs the internet (picsum.photos): blank offline. Stand-in: OK; slider verified by rendering. |
| 39 | `tut-40-pixel-sort` | Tutorial | known limitation | 3.5 (0.0 + 3.5) | Needs the internet (picsum.photos): blank offline. Stand-in: OK. |
| 40 | `tut-41-sort-shuffle` | Tutorial | fixed | 3.0 (0.1 + 2.9) | Comment called the node "`Shuffle Spread` (Randomize Spread)"; it is Randomize Spread (Shuffle Spread is another node). Hidden copy said "Sort Spread"; synced. |
| 41 | `tut-42-elementary-ca` | Tutorial | fixed | 2.9 (0.1 + 2.7) | Comment named type ids only; now the Write* / Read* labels too. |
| 42 | `tut-43-gol` | Tutorial | OK | 9.2 (1.6 + 7.4) |  |
| 43 | `tut-44-accumulator` | Tutorial | OK | 2.5 (0.0 + 2.4) |  |
| 44 | `tut-45-midi-cc` | Tutorial | known limitation | 2.9 (0.0 + 2.9) | Needs a MIDI controller: a small still square without one. Comment says so. |
| 45 | `tut-46-filter` | Tutorial | known limitation | 1.9 (0.0 + 1.9) | Sound only: the circle is a static placeholder (comment makes no visual claim). Suggest: Amplitude into its size, as tut-29/48 do. |
| 46 | `tut-47-envelope` | Tutorial | known limitation | 1.9 (0.0 + 1.9) | Sound only: the circle is a static placeholder, so pressing shows nothing on screen. Suggest: Amplitude into its size. |
| 47 | `tut-48-pulse` | Tutorial | OK | 2.6 (0.0 + 2.6) |  |
| 48 | `tut-49-fft` | Tutorial | known limitation | 2.6 (0.0 + 2.6) | Needs a microphone: blank without one. Comment says so. |
| 49 | `type-kinetic-wave` | Type | OK | 23.4 (4.5 + 16.0) |  |
| 50 | `type-halo` | Type | OK | 31.7 (2.1 + 28.6) |  |
| 51 | `type-raven` | Type | OK | 146.3 (2.3 + 143.2) | Slow on the software GPU: each dimmed letter is blurred in its own layer (~150 a frame) - the look. |
| 52 | `type-drift` | Type | OK | 17.8 (3.2 + 12.2) |  |
| 53 | `type-rain` | Type | OK | 13.4 (3.6 + 9.7) |  |
| 54 | `type-weight-wave` | Type | OK | 11.9 (0.1 + 11.8) |  |
| 55 | `type-spiral-path` | Type | OK | 12.4 (0.6 + 11.7) |  |
| 56 | `type-glyph-particles` | Type | OK | 7.8 (1.1 + 6.8) |  |
| 57 | `trim-write-on` | Type | OK | 3.6 (0.1 + 3.5) |  |
| 58 | `kinetic-type` | Type | OK | 2.9 (0.3 + 2.6) |  |
| 59 | `particles-fountain` | Particles | OK | 5.9 (0.0 + 5.8) | Reads as a column once particles fall past the canvas and wrap back in from the top (wrap bounds, as described). |
| 60 | `particles-text-dust` | Particles | fixed | 5.8 (0.2 + 5.6) | Particles spawned in the text's bounding box (engine: from-layer sampled `draw/text` as a box). Engine fixed; now the word reads before it smokes away. Comment / description updated. |
| 61 | `particles-attractor` | Particles | OK | 8.0 (0.1 + 7.8) | The attractor's influence ring is mostly off-canvas, showing only in the corners. |
| 62 | `particles-pinball` | Particles | OK | 8.9 (0.1 + 8.8) |  |
| 63 | `particles-spiral` | Particles | OK | 24.1 (0.2 + 23.9) |  |
| 64 | `lissajous` | Curves | OK | 7.3 (1.2 + 5.5) |  |
| 65 | `kaleidoscope` | Curves | fixed | 112.7 (0.1 + 112.5) | Twelve whole squares overlapped into a pinwheel with a hole: no wedges, no visible mirroring. Now the window feeds Symmetry (kaleidoscope, 6 mirrors), full-bleed. Description updated. Needs the internet (picsum.photos) like the other photo samples; reviewed with a stand-in. |
| 66 | `grid-art` | Curves | OK | 9.2 (0.1 + 9.1) |  |
| 67 | `motion-follow` | Motion | OK | 2.9 (0.1 + 2.8) |  |
| 68 | `eased-stagger` | Motion | OK | 3.1 (0.2 + 2.9) |  |
| 69 | `lfo-motion` | Motion | OK | 2.3 (0.1 + 2.2) |  |
| 70 | `phyllotaxis` | Curves | OK | 6.0 (0.8 + 5.1) |  |
| 71 | `smoothed-path` | Curves | OK | 3.6 (0.2 + 3.4) |  |
| 72 | `grid-ripple` | Fields | OK | 5.8 (0.8 + 5.0) |  |
| 73 | `curl-flow` | Fields | OK | 9.5 (2.5 + 6.9) |  |
| 74 | `ripple-field` | Fields | OK | 13.8 (3.5 + 8.8) |  |
| 75 | `noise-wave` | Fields | OK | 21.2 (11.4 + 9.9) |  |
| 76 | `force-field` | Fields | OK | 6.2 (1.8 + 4.5) |  |
| 77 | `pendulum` | Nature | fixed | 2.5 (0.0 + 2.5) | Rod invisible (Stroke with no colour, black on black); background was a `"#0a0a14"` string the renderer ignores. Rod grey, background a colour. |
| 78 | `noc-wind-gravity` | Nature | OK | 3.3 (0.1 + 3.2) | App: mouse X blows the ball. |
| 79 | `noc-drag` | Nature | OK | 4.9 (0.1 + 4.8) |  |
| 80 | `noc-attraction` | Nature | OK | 3.6 (0.1 + 3.6) | App: the mover follows the cursor. |
| 81 | `noc-spring` | Nature | OK | 2.9 (0.1 + 2.8) |  |
| 82 | `noc-seek` | Nature | OK | 2.9 (0.1 + 2.8) | App: the agent seeks the cursor. |
| 83 | `noc-flow-field` | Nature | OK | 7.1 (2.7 + 4.3) |  |
| 84 | `fractal-tree-spread` | Nature | OK | 3.1 (0.1 + 3.0) |  |
| 85 | `shader-raymarch` | Shaders | OK | 292.1 (0.0 + 292.1) | Slow on the software GPU: a raymarched scene per pixel. |
| 86 | `shader-plasma` | Shaders | OK | 8.3 (0.0 + 8.3) |  |
| 87 | `shader-feedback` | Shaders | OK | 9.7 (0.0 + 9.7) |  |
| 88 | `shader-reaction-diffusion` | Shaders | fixed | 25.5 (0.0 + 25.5) | Unstable 4-neighbour Laplacian grew crosses and diamonds; never seeded in the app (iFrame 0 not rendered). Now Sims kernel, 16-bit packed field, 4 steps a frame (buffers A-D), seeds where empty. Description updated. |
| 89 | `fractal-tree-generative` | Nature | OK | 3.1 (0.2 + 2.9) |  |
| 90 | `datamosh` | Image | known limitation | 23.0 (0.1 + 22.9) | Needs the internet (picsum.photos): blank offline. Stand-in: OK. |
| 91 | `photo-mosaic` | Image | known limitation | 14.5 (5.7 + 7.7) | Needs the internet (picsum.photos): blank offline. Stand-in: OK. |
| 92 | `webcam-mosaic` | Image | known limitation | 15.9 (3.2 + 12.6) | Needs a webcam: without one every cell reads dark (a grid of blue squares). Comment / description say so. |
| 93 | `body-tracking` | Image | known limitation | 3.2 (0.0 + 3.2) | Needs a webcam: blank without one (Camera node shows its error badge). Comment says so. |
| 94 | `video-pixelate` | Image | OK | 4.8 (0.1 + 4.3) | Needs the app's video decoder (installed in the app and the review). |
| 95 | `pixel-sort` | Pixel | known limitation | 25.5 (16.0 + 8.4) | Needs the internet (picsum.photos): blank offline. Stand-in: OK; sliders verified by rendering. |
| 96 | `tone-surface` | Sound | OK | 6.3 (1.8 + 4.5) | App: dots light up around the cursor. |
| 97 | `fft-bloom` | Sound | OK | 2.7 (0.1 + 2.7) | Blank in the headless review (no audio engine), draws in the app even without an output device. |
| 98 | `random-walk` | Feedback | OK | 3.8 (0.5 + 3.4) |  |
| 99 | `paint-trail` | Feedback | OK | 16.9 (9.2 + 7.7) | App: paints under the cursor. |
| 100 | `echo-trails` | Feedback | OK | 36.6 (0.1 + 36.5) |  |
| 101 | `csv-bar-chart` | Data | fixed | 3.8 (0.1 + 3.8) | Bars overflowed both canvas edges by 3 px. Even margins now. |
| 102 | `local-peaks` | Data | OK | 3.8 (0.1 + 3.6) |  |
| 103 | `number-counters` | Data | OK | 8.7 (0.1 + 8.6) |  |
| 104 | `voronoi-mosaic` | Geometry | OK | 16.6 (0.0 + 16.6) |  |
| 105 | `lsystem-plant` | Nature | OK | 6.7 (0.5 + 6.3) |  |
| 106 | `contour-blobs` | Geometry | OK | 19.6 (0.1 + 19.5) |  |
| 107 | `plexus` | Geometry | OK | 11.4 (0.5 + 10.9) |  |
| 108 | `morph-tunnel` | Geometry | OK | 8.3 (0.3 + 8.0) |  |
| 109 | `relaxed-cells` | Geometry | OK | 10.4 (0.0 + 10.4) |  |
| 110 | `loop-bloom` | Motion | OK | 3.2 (0.3 + 3.0) |  |
| 111 | `sim-flocking` | Particles | OK | 8.1 (0.4 + 7.7) |  |
| 112 | `sim-slime-mould` | Particles | OK | 6.5 (1.5 + 4.9) |  |
| 113 | `sim-dla-coral` | Particles | OK | 3.9 (0.3 + 3.5) | App: the Bang restarts the growth. |
| 114 | `sim-cloth` | Particles | OK | 5.8 (0.4 + 5.3) | App: pressing on the cloth grabs it; a fast pull tears a hole, as the comment says. |
| 115 | `gradient-sunset` | Compositing | OK | 32.5 (0.2 + 32.4) |  |
| 116 | `neon-bloom` | Shaders | OK | 179.4 (0.2 + 179.1) | Slow on the software GPU: bloom r40 + three more full-canvas passes. |
| 117 | `variations` | Fields | OK | 20.2 (10.4 + 8.8) |  |
| 118 | `expression-rose` | Curves | OK | 10.4 (2.9 + 7.1) |  |
| 119 | `noise-cellular-warp` | Fields | OK | 72.0 (39.0 + 32.2) | Tick-heavy in debug (warped cellular noise on 64x64). |
| 120 | `topographic` | Fields | OK | 34.1 (12.6 + 21.1) |  |
| 121 | `color-spaces` | Color | OK | 5.0 (0.2 + 4.8) |  |
| 122 | `effect-metaballs` | Shaders | OK | 355.5 (0.3 + 355.2) | Slowest on the software GPU: blur r26 + glow r36 + inner shadow at 600x600. |
| 123 | `effect-colour-grade` | Color | OK | 40.9 (0.3 + 40.5) |  |
| 124 | `effect-rainy-night` | Shaders | OK | 167.5 (0.4 + 167.1) | Slow on the software GPU: lens and motion blur passes. |
| 125 | `effect-warp-speed` | Shaders | OK | 196.9 (0.5 + 196.4) | Slow on the software GPU: spin + zoom blur. |
| 126 | `noise-patterns` | Fields | OK | 139.0 (36.9 + 97.4) | Tick-heavy in debug (domain-warped noise on 3 x 48x48 grids). |
| 127 | `path-ops-shapes` | Geometry | fixed | 5.2 (0.1 + 5.0) | Two unused, unwired Fill nodes sat far below the patch, halving the fit-on-open zoom. Removed. |
| 128 | `flipbook-sequence` | Image | OK | 20.8 (0.1 + 20.5) | Needs the app's video decoder (installed in the app and the review). |
| 129 | `layout-bloom` | Motion | OK | 3.6 (0.1 + 3.5) |  |
| 130 | `point-tracker` | Image | OK | 46.3 (18.0 + 28.1) |  |
| 131 | `plexus-colours` | Geometry | OK | 15.7 (2.7 + 12.0) |  |
| 132 | `topo-type` | Type | OK | 60.9 (0.1 + 60.9) |  |
| 133 | `stippled-sphere` | Geometry | OK | 14.4 (1.7 + 12.7) |  |
| 134 | `shader-time-smear` | Shaders | OK | 15.5 (0.1 + 15.4) |  |
| 135 | `neon-orbits` | Shaders | fixed | 110.7 (0.3 + 110.4) | Largest orbit (420 wide on a 420-tall canvas) clipped top and bottom when vertical. Now 380. |
| 136 | `camera-slit-scan` | Image | OK | 15.2 (0.1 + 15.1) | Without a camera its stand-in (swinging bar) shows the effect, as the comment says. |
| 137 | `flow-lines` | Curves | fixed | 52.3 (11.8 + 37.7) | Math op was `mul` (not an op, so it ran `add`): the field turned ~20x faster than "slowly". Now `multiply`; compile now warns on unknown ops. |
| 138 | `mondrian-splits` | Geometry | OK | 8.2 (0.0 + 8.2) |  |
| 139 | `truchet-tiles` | Geometry | OK | 15.5 (0.0 + 15.5) |  |
| 140 | `wfc-circuits` | Geometry | OK | 9.5 (0.0 + 9.5) |  |
| 141 | `kaleido-slime` | Geometry | OK | 185.5 (0.6 + 184.7) | Slow on the software GPU: Symmetry clips 12 full-canvas copies. |
| 142 | `wallpaper-groups` | Geometry | OK | 30.0 (0.4 + 29.6) |  |
| 143 | `sim-differential-growth` | Particles | OK | 9.8 (0.7 + 8.9) |  |
| 144 | `sim-reaction-diffusion` | Particles | OK | 9.8 (2.8 + 6.6) |  |
| 145 | `ascii-cells` | Image | OK | 38.2 (6.1 + 30.6) |  |
| 146 | `palette-from-image` | Color | known limitation | 4.2 (0.0 + 4.1) | Needs the internet (picsum.photos): offline the photo is missing and the Mondrian uses the fallback palette. Stand-in: OK. |
