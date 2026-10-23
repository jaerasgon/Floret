# Floret 0.2.0: the details

The long version of the 0.2.0 entry in [CHANGELOG.md](../CHANGELOG.md).
0.2.0 was published as the beta **0.2.0-beta.1**.

Floret is now a native desktop app written in Rust (`native/`), replacing
the TypeScript web app, which is kept in the repository for reference
(in `web-legacy/`). Patches (`.flora`) written by the web app open in the
native app, and the native app writes the same format.

**Version.** The crates are at **0.2.0**: the first release of a new
implementation, published as a beta, and unsigned. A 0.x version says
plainly that things may still change, without the promises a 1.0 (even a
1.0 beta) makes about stability and file-format freezes. 1.0 can follow
once the Mac QA pass (`docs/qa-checklist-mac.md`) is clean and the app is
signed.

**Licence.** Floret is free and open source under the **Apache License
2.0**. `LICENSE` holds the standard text and a new `NOTICE` file credits Floret and its author: anyone
who builds Floret, or parts of it, into another tool keeps the NOTICE with
it. Help → Licenses… shows the licence and the NOTICE, and the macOS About
panel reads "Licensed under the Apache License 2.0". Feedback goes to
Floret.make@gmail.com or the issue forms on GitHub; `CONTRIBUTING.md`,
`SECURITY.md`, `CODE_OF_CONDUCT.md` and the issue / pull request templates
are new. The README is rewritten for the public repository.

## Highlights

- **Native and GPU-rendered:** egui + wgpu (Metal on macOS). Every frame is
  drawn on the GPU by the `floret-render` crate: shapes, text, images,
  particles, gradients, masks, shaders and post-effects.
- **A node editor built for live work:** Tab / double-click search,
  wiring with drop-to-search, inline values you can type or scrub, regions
  (ForEach, If, Cache …) and subpatches, undo for everything, node gravity
  (dragging a node gently pulls its neighbours along) and auto-arrange.
- **179 built-in samples** in the Library, including 48 step-by-step
  tutorials, each opening fitted and centred with a tidy layout.
- **Export:** PNG at 1×/2×/4×, and MP4 (H.264), GIF or PNG sequences of
  seamless loops, rendered offline at a fixed frame rate.
- **Seeds, variations and presets:** a patch seed behind every random and
  noise node, New Variation (Cmd+Shift+R), and saved presets you can recall
  and morph between.
- **Sound, camera, MIDI and body tracking**, all on-device.

## Nodes

- 373 node types, 252 of them in the Nodes browser (the rest are
  older versions kept hidden so old patches still load). The browser groups
  them by role: Source, Draw, Text, Color & Style, Shape Ops, Patterns,
  Animate, Noise & Random, Math, Lists, Structure, Particles & Physics,
  Image, Effects, Input & Sound. See `NODES.md`.
- **Drawing and style:** shapes, paths, polygons, images and text; fills,
  strokes, Gradient Fill (linear / radial / conic), masks, blend groups.
- **Generative geometry:** Scatter (poisson / random / jittered grid /
  halton, relax), Cells (voronoi / delaunay / hull), Path Ops, Shape
  Boolean, Shape Morph, Trim Path, Connect Points, Contours, L-System,
  Layout (grid / ring / spiral / path with animated transitions), Tiles
  (Truchet and wave-function-collapse) and Symmetry (rotate, kaleidoscope,
  all 17 wallpaper groups). Classic techniques as modes: flow-field lines
  (evenly spaced streamlines) on Contours, plus contour rings around
  shapes; recursive (Mondrian / quadtree) subdivision on Cells; image-
  weighted stippling on Scatter.
- **Noise and maths:** one Noise node (perlin, simplex, value, cellular,
  curl, plus patterns), an Expression node (type a formula, its names
  become pins), Math / Vector Math / Compare / Logic / Statistics /
  Convert families, LFO, easing and springs.
- **Lists:** Lookup, Sublist, Reorder and Combine families (take, slice,
  sort, concatenate … in one dropdown each), Count, Merge, Neighbor,
  Partition and the other list nodes.
- **Colour:** palettes with harmony modes and colours extracted from an image, OKLab / OKLCH mixing.
- **Particles and simulations:** emitters, forces, colliders and worlds;
  a Simulation node for flocking, slime mould (physarum), DLA growth,
  cloth, differential growth and reaction-diffusion (Gray–Scott presets).
- **Typography:** installed and loaded fonts (variable-font axes), text on
  a path, glyphs, outlines and Text Animate.
- **GPU:** a ShaderToy-style Shader node with buffer passes and a side
  panel editor; an Effect node with bloom, blur, glow, grain, displace,
  dither, halftone, chromatic aberration, vignette, levels, curves, LUTs,
  keying, ASCII art and more; Shader nodes can read past frames, and Echo
  makes trails, delays, freeze frames and slit-scans.
- **Images and video:** image adjust / pixelate / pixel sort / sampling;
  Video File and Video Frame At play MP4 / MOV clips and numbered image
  sequences as images; Point Tracker follows points through video or
  camera frames.
- **Input and sound:** mouse and keyboard, MIDI CC and notes, Camera,
  Landmarks (hand / face / pose tracking), microphone input, FFT and
  amplitude analysis, and a sound engine with oscillators, filters,
  envelopes and effects.
- **Data:** CSV files, number formatting, image URLs.

## Editor

- Dockable panels: one Preview per Renderer, Inspector, Help (live node
  docs), Library, Presets, Shader and Profiler; the layout is remembered.
- Docking: every panel is a dock tab, the node canvas and the Nodes
  palette included, so any of them docks anywhere in the window, splits,
  stacks, floats or closes. A dragged tab lights up the whole region it
  will land in, and dropping it always docks (no stray floating windows).
  Tabs float by holding ⌥ Option / Alt as they're let go (a card shows
  where) or with their right-click Float; floating windows have minimise
  and close buttons and reopen where they were. Each tab has a faint ×
  at its right end that brightens on the hovered or front tab; a closed
  canvas, Nodes palette or preview comes back from View → Panels. Tabs share the tab bar and scroll
  when they don't fit, dividers are easier to grab, and panes can be made
  narrower. View → Panels reopens or closes any panel, and View → Reset
  Layout puts them all back.
- Every Renderer has its own preview pane: a new Renderer's preview opens
  under the others (five Renderers, five panes), each pane follows its
  Renderer, and deleting a Renderer closes its pane. Older saved layouts
  open as the default layout once.
- Nodes panel with search and favourites (Q menu, 1–9 to place).
- Library panel: every sample by category plus your own saved patches, in
  list, icon or column views, with one search; import and download
  libraries.
- Subpatches panel (from the tab bar's `+` menu): the subpatches at the
  level the canvas shows, with their node counts. Click one to frame it,
  double-click to enter, Up to go back. Right-click → Save to library…
  keeps it in the Library with a subpatch mark; clicking it there inserts
  it into the open patch (fresh ids, its own Write / Read slots, one undo
  step), and its right-click menu can also open it on its own.
- Preview: zoom and pan with a pixel grid, pause / step / restart, focus
  mode, and a fullscreen output window on any display.
- Always-editable node values: every unwired value is a rounded value box
  with its name on the left and its value on the right. Drag it to scrub,
  click it to type; dropdowns, colour swatches and checkboxes work
  directly. A vec2 box has its own X and Y cells. (This replaces clicking
  a row's name to open an edit box over a gold read-out.)
- Node rows: every input and param has a full-width row of its own, so the
  value boxes line up. Outputs no longer share rows with inputs: they are
  listed in a compact, slightly darker footer under the inputs (labels
  right-aligned against their pins). Nodes without outputs have no footer.
- One layer type, one name: every layer pin reads `layer` (Group's
  output, the Renderer's `layers` in and out, and the geometry nodes'
  `layers` outputs included). Spread pins now take their type's colour
  instead of yellow, and a small three-bar stack icon after the pin's
  name marks a spread (hover it for "spread") on inputs, outputs, ForEach
  / Cache frame ports and subpatch outputs that carry a spread. Saved
  files keep their port names.
- Bypass any node (header button or B) to pass its input straight through;
  a curve editor for Effect → curves; a file picker for LUTs.
- Minimap, dot grid and snapping, a shortcuts overlay (`?`), tooltips on
  pins and wires, compile warnings on the nodes that cause them, value
  probes (Alt + drop a wire).
- Light and dark themes (or follow the system), interface zoom, reduce
  motion, and preferences for startup, autosave, grid and render size.

## Files

- Real `.flora` files with Open / Save / Save As, recent files, and a
  "Save changes?" prompt before anything would discard edits.
- Autosave to a recovery file, and an offer to restore after a crash or
  force-quit.
- macOS: `.flora` files are Floret documents. Double-clicking one in
  Finder (or Open With → Floret, or dropping it on the Dock icon) opens it,
  whether Floret is running or not. `floret <file.flora>` on the command
  line opens a file on every platform.
- Web-app patches open as they were, except that a Save SVG node is left
  out (with a notice): Floret renders everything on the GPU and saves
  pictures with Export.

## Export

- PNG of the current frame at 1×, 2× or 4× (a true re-render).
- MP4 (H.264), GIF and PNG sequences of whole loops: a patch loop length
  (toolbar Loop field), chosen size, fps and number of loops, with progress
  and Cancel while the app keeps running.
- On macOS, MP4 export and video playback use Apple's AVFoundation /
  VideoToolbox (which macOS licenses); the Mac app contains no OpenH264.
- **Sound in MP4s (macOS):** a patch's sound goes into its exported MP4
  ("Include sound", on by default for patches with a sound Output node). The
  sound is rendered offline with the frames, deterministic and exactly as
  long as the video, and encoded as AAC by Apple's encoder alongside the
  H.264 pictures. Patches with sound nodes also get their own audio engine
  in every export, so audio-reactive visuals (FFT, amplitude) move in GIFs,
  PNG sequences and variations as they do live. Other platforms export
  MP4s without sound for now (no royalty-free AAC encoder to ship).
- **Export Variations:** render many seeds (a range, or N random seeds
  from a base) and/or presets at once, as stills at a chosen time or
  one-loop GIFs, into a new folder of files named by preset and seed
  (`<patch>-seed-1234.png`) plus a labelled contact sheet. Progress and
  Cancel; the app keeps running. Clicking a result puts its preset and
  seed into the patch (one undo step).

## Platform notes

- **macOS 11 or later**, Apple Silicon or Intel, built from source with
  `./floret.sh install` (installs Rust on first run) into
  `/Applications/Floret.app`.
- The app asks for microphone and camera access the first time a patch
  uses them; video and audio never leave the computer.
- Windows and Linux have experimental downloads (and Linux builds with
  `./floret.sh`). Video on both goes through OpenH264.
- **Beta downloads.** A manual workflow (`.github/workflows/release.yml`,
  how-to in `docs/releasing.md`) builds macOS (Apple silicon and Intel),
  Windows and Linux packages with the scripts in `tools/package/` and
  attaches them, with checksums, to a GitHub Release. `./floret.sh install`
  and the macOS package share one bundle script
  (`tools/package/build-macos-app.sh`). Windows and Linux are experimental.

## Known limitations

- **Unsigned:** the app is ad-hoc signed, not notarised, so macOS asks you
  to confirm the first launch of a downloaded copy (Privacy & Security →
  Open Anyway); Windows shows SmartScreen. macOS may ask again for camera /
  microphone access after an update.
- **Windows and Linux are experimental** beta downloads with far less
  testing than the Mac app. MP4 sound is Mac-only for now, and H.264 goes
  through OpenH264 on both.
- One patch per window, one window.
