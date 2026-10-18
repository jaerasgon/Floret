# Plan: feature-complete for generative art (before the Mac .app / 1.0)

Agreed with the owner on 2026-10-02. The native app (`native/`) is the
product; the TypeScript web app is reference-only.

## Where things stand

- **2026-10-03: all nine items below are merged** on `main` (fmt, clippy,
  all tests pass). See `feature-complete-summary.md` for what's new,
  what to test on the Mac, and the owner's decisions.
- **2026-10-03: the owner approved every decision in the summary.** Done
  since:
  - The Presets tab comes right after Inspector in the default dock, and
    opening a patch with presets brings it forward.
  - A **Loop** field in the toolbar's transport group edits the patch's loop
    length ("off" = not set), with undo.
  - floret-engine and floret-render build at `opt-level = 1` in dev.
  - `rust-version` is 1.95 (what the dependencies need), and the clippy lints
    it turns on are fixed.
  - Everything else is kept as implemented (Vary and Cmd+Shift+R, the extra
    Palette / Lerp / Gradient rows, presets, seeds, the Noise / Expression
    conventions, RGB Gradient Fill, `particles/simulation`).
- `main` has the full native app: engine (every web-app node + video, type,
  LFO), wgpu renderer, editor, files / autosave / menus, sound, MIDI,
  camera and body tracking, shaders, profiler. Parity tests pass against
  the web app's recorded traces.
- **Test on the Mac with `./floret.sh`** (pulls, builds incrementally,
  runs). Untested on real hardware so far: sound, mic, camera, body tracking,
  MIDI, video, fonts.
- **CI is manual-only** (Actions tab → native → Run workflow, Linux by
  default). GitHub Actions minutes ran out; macOS counts 10×, Windows 2×.
  Test locally before pushing; never trigger macOS/Windows runs casually.
- **Samples:** slimmed from 1662 to 1266 nodes; the fractal tree is built
  around one ForEach. Parity now reads frozen copies of the original samples
  in `native/fixtures/samples/`, so library samples can change freely
  (`tests/all_samples.rs` checks they compile, run and draw). As the owner
  decided:
  - `noc-fractal-tree` is removed from the Library: it was the same patch
    as `fractal-tree-spread` ("Fractal Tree" now). Its frozen fixture and
    trace stay for parity.
  - Samples whose shapes were placed for top-left anchors (tut-01,
    pixel-sort, datamosh and about twenty more) are centred.
  - Wrong comments and descriptions are fixed (tut-19, tut-20, tut-41,
    kaleidoscope and the others found by checking every sample against its
    graph).
  - The full list is in the summary's "Samples (done)" section.
- **Deferred until feature complete:** the proper Floret.app bundle (icon,
  Finder double-click, signing / notarisation), port-name freeze (#78),
  README (#77), release (#81), QA (#82). Windows / Linux come later.

## Ground rules

- **As few nodes as possible.** One node per concept, with a mode dropdown
  whose inputs change with the mode (dynamic inputs, see
  `spec.rs` `DynamicInputsFn`). Extend existing nodes rather than adding
  near-duplicates. Old node types stay loadable (hide superseded ones from
  the palette instead of deleting them).
- **No look/behaviour changes to the existing UI without asking the owner
  first.** New panels or menu items are fine when part of a feature below;
  list anything else as a suggestion.
- Every feature ships with:
  - a sample in the Library (category fitting the feature)
  - unit tests
  - a visual check under Xvfb + lavapipe
- New nodes are native-only (no TS trace). `tests/all_samples.rs` must still
  pass (every sample compiles, ticks and draws).
- Every sample's Library cover must show something
  (`sample_sweep.rs` `every_sample_cover_shows_something`). A sample that
  grows or starts empty sets `coverFrame` in `samples/catalogue.json`; one
  that is empty until played sets `coverPlay` (its input lives in
  `sample_scripts.rs`); one that needs a camera, microphone or tracking gets
  a bundled `samples/covers/<id>.png` (`BUNDLED_COVERS`, then
  `FLORET_UPDATE_COVERS=1 cargo test -p floret-app -- --ignored update_covers`).
- New nodes that read time, devices, files or a clock must not be frozen by
  the memo: flag them stateful / effectful in `nodes.json`, or wrap them in
  `always_eval`.
- From `native/`, before every push: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (use `CARGO_INCREMENTAL=0`; watch disk).
- Commit messages: plain and descriptive.

## The nine items

### 1. Recording loops and video (app feature, ~1 node)
- Export menu: **MP4 (H.264)**, **GIF** and **PNG sequence**, at a chosen
  size and fps.
  - Render offline, frame by frame, at a fixed dt, so output is smooth and
    deterministic regardless of machine speed.
  - The OpenH264 encoder is already a dependency (floret-video).
- **Loop length:** a patch setting (seconds). Exports of exactly one loop
  join seamlessly.
- A `loop` phase (0..1) output on the existing time node, so animations can
  be built to loop perfectly.
- Progress and cancel in the UI. Issue #27.

### 2. Geometry (4 nodes)
- **Scatter:**
  - Modes: Poisson disc, random, jittered grid, Halton.
  - Inputs: bounds or shape, density / min distance, seed.
  - Output: a points spread.
- **Cells:** Voronoi cells, Delaunay triangles, or the convex hull of a
  points spread. Outputs polygons as layers and as point lists.
- **Path ops:**
  - Modes: resample (even spacing), smooth (Chaikin), subdivide, simplify,
    offset / inset.
  - Works on point lists and path layers.
- **Shape boolean:** union, intersect, subtract, xor of two shapes/layers →
  polygons.
- The L-system generator lives here too, as a 5th node or a mode, whichever
  is cleaner (see item 8).

### 3. Noise (1 node)
- **Noise:**
  - Modes: Perlin, simplex, value, cellular / Worley (F1, F2, F2−F1), curl
    (vec2 out).
  - Inputs: octaves, falloff, seed, and domain warp (amount and scale).
  - Works on single values and spreads.
- The existing `noise/perlin` / `noise/curl` stay loadable (hidden from the
  palette).

### 4. Expression (1 node)
- **Expression:** type a formula, e.g. `sin(x * 3) + y * t`.
- Each variable it uses becomes an input pin.
- It works per element over spreads, and supports math functions, vec2 / colour
  components, `t` and `frame`.
- Clear errors on the node; safe evaluation (no loops / IO). Issue #45.

### 5. Seeds, variations, presets (app + engine)
- **Patch seed:** every random / noise node mixes it into its own seed.
- A **"New variation"** button (and shortcut) re-rolls the patch seed.
- **Presets / snapshots:**
  - Save, recall and morph between parameter sets, stored in the `.flora`
    file (new optional top-level field, backward compatible).
  - A small Presets panel in the dock.
- Issue #26.

### 6. Gradient fills and masks (extend style + 1 node)
- **Gradient fill:** linear, radial, conic, from a colour spread / stops.
  - Usable wherever a fill colour goes (a style op, so Fill / Stroke accept
    it).
  - Drawn by the renderer on the GPU.
- **Mask:** a layer masked by another layer, by alpha or luminance, with
  invert.

### 7. Post-effects (1 node)
- **Effect:**
  - Modes: bloom / glow, film grain, displacement (by noise or image),
    dither, halftone, chromatic aberration, vignette.
  - Applied to a layer on the GPU (renderer passes).
  - Inputs follow the mode.
- Issue #37.

### 8. Simulations (1–2 nodes)
- **Simulation:**
  - Modes: flocking (boids), slime mould (physarum trail map), DLA growth,
    springs / cloth (Verlet).
  - Stateful, with reset.
  - Outputs points / lines / an image as fits the mode.
- **L-system:** axiom, rules, angle, iterations → turtle path (points /
  layer). Can live under Geometry (item 2).

### 9. Colour (extend existing nodes, no new node unless needed)
- A `space` option (RGB / OKLab / OKLCH) on colour lerp and gradient, for
  even, non-muddy blends.
- Palette generator modes on the palette node:
  - cosine palettes (a, b, c, d)
  - harmonies: complementary, analogous, triadic, split, tetradic, from a
    base colour

## Suggested execution

Run in parallel as worktree agents (keep shared-file edits additive), then
merge one by one with tests between merges:

| Agent | Items |
|---|---|
| A | 1. Recording and loops (app, floret-video encoder) |
| B | 2 + L-system: Geometry |
| C | 3 + 4 + 9: Noise, Expression, Colour (engine maths) |
| D | 5: Seeds, variations, presets (engine + app) |
| E | 6 + 7: Gradients, masks, post-effects (renderer) |
| F | 8: Simulations |

Afterwards, write a short summary for the owner covering:
- what's new, and which samples show it
- what to test on the Mac
- decisions that need approval
