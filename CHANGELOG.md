# Floret Changelog

A complete record of every change made to this project. Releases come
first, newest on top; below them, the development log in chronological
order. Each section is a coherent phase of work; bullets within are
individual changes (file additions, fixes, behavior shifts).

---

## 0.2.0 — the native app's first release (unreleased)

Floret is now a native desktop app written in Rust (`native/`), replacing
the TypeScript web app at the repo root, which stays in the repository as a
reference only. Patches (`.flora`) written by the web app open in the
native app, and the native app writes the same format.

**Version.** The crates are at **0.2.0**: the first release of a new implementation, not yet
run end to end on a real Mac, and unsigned. A 0.x version says plainly that
things may still change, without the promises a 1.0 (even a 1.0 beta)
makes about stability and file-format freezes. 1.0 can follow once the
Mac QA pass (`docs/qa-checklist-mac.md`) is clean and the app is signed.

### Highlights

- **Native and GPU-rendered:** egui + wgpu (Metal on macOS). Every frame is
  drawn on the GPU by the `floret-render` crate: shapes, text, images,
  particles, gradients, masks, shaders and post-effects.
- **A node editor built for live work:** Tab / double-click search,
  wiring with drop-to-search, inline values you can type or scrub, regions
  (ForEach, If, Cache …) and subpatches, undo for everything, node gravity
  (dragging a node gently pulls its neighbours along) and auto-arrange.
- **175 built-in samples** in the Library, including 48 step-by-step
  tutorials, each opening fitted and centred with a tidy layout.
- **Export:** PNG at 1×/2×/4×, and MP4 (H.264), GIF or PNG sequences of
  seamless loops, rendered offline at a fixed frame rate.
- **Seeds, variations and presets:** a patch seed behind every random and
  noise node, New Variation (Cmd+Shift+R), and saved presets you can recall
  and morph between.
- **Sound, camera, MIDI and body tracking**, all on-device.

### Nodes

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

### Editor

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

### Files

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

### Export

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

### Platform notes

- **macOS 11 or later**, Apple Silicon or Intel, built from source with
  `./floret.sh install` (installs Rust on first run) into
  `/Applications/Floret.app`.
- The app asks for microphone and camera access the first time a patch
  uses them; video and audio never leave the computer.
- Linux builds and runs for development (`./floret.sh`), using OpenH264.
- **Beta downloads.** A manual workflow (`.github/workflows/release.yml`,
  how-to in `docs/releasing.md`) builds macOS (Apple silicon and Intel),
  Windows and Linux packages with the scripts in `tools/package/` and
  attaches them, with checksums, to a GitHub Release. `./floret.sh install`
  and the macOS package share one bundle script
  (`tools/package/build-macos-app.sh`). Windows and Linux are experimental.

### Known limitations

- **Unsigned:** the app is ad-hoc signed, not notarised, so macOS asks you
  to confirm the first launch of a downloaded copy (Privacy & Security >
  Open Anyway); Windows shows SmartScreen. macOS may ask again for camera /
  microphone access after an update.
- **Windows and Linux are experimental** beta downloads with far less
  testing than the Mac app (H.264 through OpenH264 on both).
- Most of the app has been tested under Linux with a software GPU; the Mac
  checklist in `docs/qa-checklist-mac.md` hasn't been run end to end yet.
- One patch per window, one window.

---

## Phase 0 — Planning

- Captured user intent: build a vvvv-gamma-style node editor that wraps
  p5.js for live, parameter-heavy generative work.
- Locked in tech: Tauri 2 shell (deferred — no Rust toolchain on machine),
  React + TypeScript + Vite, React Flow (`@xyflow/react` v12) for the editor
  UI, p5.js in instance mode for the preview.
- Locked in MVP scope: draw + animate, mouse/keyboard input, **vvvv-style
  spreads** (any wire carries a 1-D list; downstream nodes iterate
  automatically).
- Locked in dataflow model: reactive per-frame evaluation driven by p5's
  `draw()` loop; spreads with Cyclic broadcasting; drawing nodes return
  Layer closures (not side effects).

---

## M1 — Scaffold

Goal: working skeleton with the editor, preview, and toolbar wired up.

- Initialised Vite + React + TypeScript project at `D:\Artifacts\Floret\`.
- Installed `@xyflow/react`, `p5`, `zustand`, `vitest`, plus types.
- Configured `tsconfig.json` (strict, `noUncheckedIndexedAccess`) and
  `tsconfig.node.json` for `vite.config.ts`.
- Added `vite.config.ts` with `@/` path alias to `src/`.
- Built the app shell: toolbar / split pane / status bar
  (`src/app/App.tsx`, `Toolbar.tsx`, `StatusBar.tsx`, `Resizer.tsx`).
- Dark theme with category accent colours (`src/app/styles.css`).
- HMR-safe p5 singleton at `src/p5-driver/instance.ts` — wraps `new p5()`
  and tears down on `import.meta.hot.dispose` so module reloads don't
  leave ghost canvases.
- Stub p5 sketch in `src/p5-driver/Preview.tsx` so the canvas is alive.
- Verified production build (`npm run build`) and dev (`npm run dev`).
- **Tauri shell deferred** — depends on `rustup`; the web build is fully
  functional in the browser.

---

## M2 — Runtime v0 (scalars only)

Goal: working dataflow engine.

- `src/runtime/types.ts` — `Spread<T>`, `PortType`, `Mat3`, `Layer`,
  `FrameCtx`, plus `DrawCtx`, `IDENTITY_MAT3`, `EMPTY_SPREAD`.
- `src/runtime/spread.ts` — **`liftCyclic`**: the heart of the engine.
  Lifts a scalar compute function to spread-aware. Supports `pinned`
  inputs (passed through whole instead of iterated). Empty-spread
  propagation: any empty input → all empty outputs.
- `src/runtime/graph.ts` — `RuntimeNode`, `RuntimeEdge`, `RuntimeGraph`.
  Kahn topological sort with cycle detection.
- `src/runtime/tick.ts` — per-frame evaluation. Walks `graph.order`,
  gathers inputs from upstream node outputs, runs `compute(inputs)`,
  stores result.
- `src/runtime/param-store.ts` — initial scaffold; full implementation
  lands in M5.
- `src/runtime/frame-ctx.ts` — typed frame-state record.
- `src/runtime/__demo__/patch.ts` (later deleted) — hand-wired
  `Time → Sin → Mul(80) → Add(cx) → Translate → Rect → Fill → Renderer`
  patch to exercise the engine before the registry existed.
- `src/runtime/__tests__/spread.test.ts` — 10 tests for liftCyclic.
- `src/runtime/__tests__/graph.test.ts` — 6 tests for topo sort.
- `npm run dev` showed a stub-driven canvas; the runtime ticked but the
  Time-source bug (fixed in M5) silently kept the rect stationary.

---

## M3 — NodeRegistry + compileGraph

Goal: replace the hand-wired patch with a drag-and-wire editor.

- `src/nodes/types.ts` — `NodeDef`, `PortSpec`, `NodeParamContext`.
- `src/nodes/registry.ts` — singleton `nodeRegistry` with
  category grouping, version counter, and subscriber list (ready for
  future hot-reload of node defs).
- 30+ built-in NodeDefs across categories:
  - **Value**: Number
  - **Time**: Time, Frame
  - **Math**: Add, Sub, Mul, Div, Mod, Sin, Cos, Tan, Abs, Neg, Floor,
    Round, Sqrt, Map, Lerp, Clamp
  - **Transform**: Translate, Rotate, Scale
  - **Draw**: Rect, Ellipse, Line
  - **Style**: Fill RGB, Stroke Weight, No Stroke
  - **Render**: Renderer
- `src/runtime/compile.ts` — `compileGraph(editorRefs)` bridges editor
  state to runtime. Resolves handle names → port indices, picks the
  Renderer node automatically, surfaces warnings on bad edges.
- `src/editor/editor-state.ts` — Zustand store mirroring React Flow
  nodes/edges. Recompiles only on **structural** changes (add/remove
  node, add/remove edge), not on drag/select/param edits.
- `src/editor/connection-rules.ts` — strict port-type matching with one
  coercion (`number ↔ boolean`); shared between drag UX
  (`isValidConnection`) and the compile-time edge validity check.
- `src/editor/nodes/GenericNode.tsx` — default node visual with typed
  port handles (port colour by type) and inline param editors for
  numeric params.
- `src/editor/NodePalette.tsx` — left-side palette grouped by category,
  drag source.
- `src/editor/Canvas.tsx` — React Flow canvas with drop-handler,
  keyboard-delete, and connection validity hook.
- Deleted the M2 `__demo__` patch — runtime now driven by the editor.
- `src/runtime/__tests__/compile.test.ts` — 4 tests: handle resolution,
  warnings on bad ports, Renderer auto-pick, live `paramReader` reads.

---

## M4 — Spreads

Goal: the vvvv-defining feature — node-based array generation and ops.

- **Generators** (`src/nodes/spread-ops/generators.ts`):
  Linear Spread, Range, Repeat, Circular Spread.
- **Operators** (`src/nodes/spread-ops/ops.ts`):
  Count, Reverse, Take, Skip, Slice, Stack, Resample.
- All spread ops use `computeRaw` (spread-aware) and declare their
  primary input `pinned: true` to document that the input is consumed
  whole rather than iterated.
- `src/runtime/__tests__/spread-nodes.test.ts` — 11 tests covering
  generation, edge cases (empty / single element), and the headline
  M4 behaviour: a `Linear Spread → Rect.x` wiring produces N Layer
  outputs via cyclic broadcasting.

---

## M5 — Inputs + IO Boxes

Goal: make patches interactive.

- **Bug fix**: `tick.ts` was passing `readFromContext`'s return value as
  inputs to `compute()` instead of using it directly as outputs. Source
  nodes (Time, Mouse, Keyboard, IO boxes) had been silently outputting
  values from a zero-frame. Fixed by short-circuiting `readFromContext`
  to populate `outputs` directly. `RuntimeNode.compute` made optional.
- **Source nodes**:
  - `src/nodes/input/mouse.ts` — outputs x, y (number), pressed (boolean).
  - `src/nodes/input/keyboard.ts` — outputs pressed, key.
- **IO boxes** (each NodeDef + custom React component):
  - `io/slider` + `SliderNode.tsx` — range input with value/min/max.
  - `io/toggle` + `ToggleNode.tsx` — on/off switch.
  - `io/color-picker` + `ColorPickerNode.tsx` — hex picker with r/g/b
    number outputs.
  - `io/xy-pad` + `XYPadNode.tsx` — 2D draggable area, x/y in [0,1].
- **`src/runtime/param-store.ts` rewrite** — now per-node `Record<string,
  unknown>`. Live ParamStore reads via `param.param(key)` in compile.ts.
  Editing a slider mid-drag goes to ParamStore only (no React reconcile);
  on commit (pointerup / blur / change), `setNodeParam` is also called
  to mirror into `node.data.params` for persistence.
- `src/editor/useNodeParam.ts` — `useSyncExternalStore` hook giving an
  IO-box component `[value, setLive, commit]` — exactly the live/commit
  split above.
- `src/editor/nodes/FloretNode.tsx` — dispatcher: looks up the NodeDef's
  optional `component` field and uses it, else falls through to
  `GenericNode`. Registered as the sole React Flow node `type`.
- `src/runtime/__tests__/param-store.test.ts` — 5 tests: live ParamStore
  reads, Mouse reads FrameCtx, per-node subscription isolation,
  `removeNode` cleanup.

---

## M6 — Persistence

Goal: patches survive across sessions; ship-as-a-file.

- `src/persistence/schema.ts` — versioned `PatchFile` JSON shape
  (`{ kind, version, meta, nodes, edges }`); `CURRENT_PATCH_VERSION = 1`.
  `isPatchFile`, `emptyPatch` helpers.
- `src/persistence/migrate.ts` — migration ladder (empty for v1; ready
  for v1→v2). Rejects newer-than-current versions.
- `src/persistence/serialize.ts` — `serializePatch` / `deserializePatch`
  between editor state and JSON.
- `src/persistence/fs.ts` — `savePatch`, `loadPatch` using the File
  System Access API (Chromium) with a `<a download>` / `<input>`
  fallback for other browsers. Also `autosave` / `loadAutosave` /
  `clearAutosave` via `localStorage`.
- `src/app/shell/FileMenu.tsx` — New / Open / Save / Save As buttons.
  Autosaves on every change (400ms debounced) and restores from
  autosave on first mount.
- `src/persistence/__tests__/roundtrip.test.ts` — 5 tests:
  serialize → JSON → parse → deserialize round-trip, `isPatchFile`
  rejects non-patches, migration rejects future versions.

---

## M7 — Polish

Goal: usability cleanup before calling it shippable.

- **Palette fuzzy search** — added a search input to `NodePalette.tsx`
  with substring match on label/type/description/category. Filters and
  re-sorts in real time.
- **Undo / Redo** — `src/editor/history.ts` keeps a snapshot stack of
  `{nodes, edges, params}`. `History.push()` is called on every committed
  mutation:
  - Structural changes (add/remove node, add/remove edge) → 1 entry.
  - `setNodeParam` (IO-box commit) → 1 entry.
  - **Mid-drag scrubs do NOT push** (they write to ParamStore directly).
  - Same-key edits within 250 ms coalesce into a single entry.
  - The architectural promise from the plan: live scrubs don't pollute
    history; only commits do. Validated by tests.
- **`_setHistoryPush`** late-binding pattern between `editor-state.ts`
  and `history.ts` so they can call into each other without a circular
  import.
- **`src/app/shortcuts.ts`** — global keyboard handler:
  - Ctrl/Cmd+Z → undo
  - Ctrl/Cmd+Shift+Z, Ctrl/Cmd+Y → redo
  - Ctrl/Cmd+S → save (dispatched via `floret:save` window event so
    `FileMenu.tsx` stays decoupled from shortcut logic)
  - Skipped when focus is in an INPUT/TEXTAREA/SELECT/contentEditable.
- **Warnings surface in the status bar** — `useEditor.warnings`
  populated by `compileGraph` is now displayed; bad edges no longer
  silently disappear.
- **`src/runtime/batched-layer.ts`** — `batchedLayer(count, draw)`
  helper that collapses N per-slot Layer closures into one Layer that
  loops internally. Documented for node authors who need to draw
  thousands of primitives without allocation pressure.
- `src/editor/__tests__/history.test.ts` — 4 tests: undo/redo
  round-trip, same-key coalescing, redo-stack clears on new mutation,
  remove → undo restores both nodes AND ParamStore entries.

---

## Post-MVP: Sample Patches & UX

After M7 the user requested concrete demos and uncovered some rough
edges in the editor experience.

- **`src/persistence/samples.ts`** — three built-in sample patches:
  - **Hello Rect** — single static rectangle. Smallest possible patch.
  - **Wavy Row** — N circles waving in a sine. Showcases spread
    broadcasting + a Slider for live count.
  - **Mouse Follow** — circle that tracks the mouse over the preview.
- `src/app/shell/SampleMenu.tsx` — dropdown in the toolbar to load a
  sample. Sets `paramStore` and resets history when loaded.
- `src/persistence/__tests__/samples.test.ts` — auto-test that every
  sample compiles cleanly, has a Renderer, and produces ≥1 Layer per
  tick. Adds a new test automatically when a new sample is registered.

### Preview overlay

- **`PreviewOverlay`** inside `Preview.tsx` — shows the user **why**
  the preview is empty when applicable:
  - No nodes → "empty canvas, use Samples ▾"
  - Nodes but no Renderer → "no Renderer — add one and wire your
    drawing chain into its `layers` port"
  - Compile error → red panel listing the warnings

### Bug fixes

- **Node UI spillover**: `GenericNode.tsx`, `ColorPickerNode.tsx`,
  `XYPadNode.tsx` were applying inline `top: i * 18 + 4` to each
  `.port-row` while `.port-row` was `position: relative` — the `top`
  *added* to the natural flex position, so the 5th input on a Map node
  was 76 px past where it should have been (and spilling out the
  bottom). Removed all inline `top` styles; flex with `height: 18 px`
  per row now lays them out correctly. Bumped node `min-width` to
  160 px and added `text-overflow: ellipsis` on port names so long
  labels truncate.
- **Vite watcher EBUSY**: a Python venv at `D:\Artifacts\Floret\ComfyUI`
  was being watched by Vite, hitting a locked `.pyc`. Added
  `server.watch.ignored` patterns for `ComfyUI/**`, `**/.venv/**`,
  `**/__pycache__/**`, etc.

### Renderer canvas dimensions

- The Renderer NodeDef gained `defaultParams: { width: 500, height: 500 }`,
  rendered as inline number editors via `GenericNode`'s param section.
- `src/p5-driver/frame-hook.ts` now reads `width`/`height` from
  `paramStore.getKey(rendererId, ...)` each frame; if the resolved size
  differs from the live canvas, calls `p.resizeCanvas(w, h)` once.
  Values clamped to `[32, 4096]`.
- Default canvas size moved from 440×360 to 500×500.
- Updated all sample patches to set `width: 500, height: 500` on their
  Renderer and to centre their geometry on `(250, 250)`.

### Canvas outline

- Added `outline: 1px solid rgba(255, 255, 255, 0.5)` to the preview
  canvas so the bounds are visible against the black background.
  Used `outline` (not `border`) so it doesn't affect layout or shift
  the pixel grid.

### Circular Rects sample (simplified)

- Added the **Circular Rects** sample — a ring of rotated rectangles
  arranged on a circle, slowly spinning with time. Originally 21 nodes;
  the user asked for fewer.
- Simplified to **14 nodes (16 wires)** by recognising that **drawing
  a rect at `(radius, 0)` and rotating about the canvas origin**
  produces the same final position (`radius·cos θ`, `radius·sin θ`)
  AND the radial orientation that the original computed via separate
  `cos`, `sin`, `Mul × radius`, `Add + centre` chains. The single
  `Rotate` replaces 8 math nodes.
- Updated sample-test (auto-generated): the new patch still passes the
  "compiles, has Renderer, produces ≥1 Layer per tick" suite.

---

## Editor UX Polish (clipboard, context menu, node search)

A user-driven round of editor ergonomics that turns Floret from
"navigable" into "fluent."

### Clipboard

- `src/editor/clipboard.ts` — in-memory clipboard buffer. Selected nodes
  + their internal edges are stored with positions *relative to the
  selection centroid* so paste can land at any anchor and preserve
  layout. Edges crossing the selection boundary are dropped.
- Editor store gained `copySelected`, `pasteAt`, `hasClipboard`,
  `duplicateNode`, `deleteNode`, and a low-level `addEdge` helper. Paste
  seeds `ParamStore` for each new node and selects them.
- Keyboard shortcuts wired in `src/app/shortcuts.ts`:
  - **Ctrl/Cmd + C** copy
  - **Ctrl/Cmd + X** cut (copy + remove)
  - **Ctrl/Cmd + V** paste at the cursor's flow position
  - **Ctrl/Cmd + D** duplicate selected (no clipboard round-trip)
- Window-level `mousemove` listener tracks the cursor so paste lands
  exactly where the mouse is — falls back to viewport centre.

### Floating overlays

- `src/editor/overlay-store.ts` — small Zustand store managing two
  mutually-exclusive popovers: the node-search palette and the
  right-click context menu.

### Node search popover

- `src/editor/NodeSearch.tsx` — command-palette-style picker:
  - Substring match on `label`, `type`, `category`, `description`;
    prefix-matches ranked first.
  - ↑/↓ navigate, Enter commits, Esc dismisses.
  - Auto-clamps to stay on-screen near a viewport edge.
  - When opened from a wire drag, filters to nodes that have at least
    one **type-compatible** port (using `arePortsCompatible`).
- Three ways to open it:
  - **Double-click** an empty area of the canvas (via React Flow's
    `onPaneClick` with `e.detail === 2`).
  - **Drag a wire** from any port handle and release on empty space —
    the search opens at the release point. Selecting a node creates
    the wire automatically to the first compatible port.
  - **Tab key** anywhere in the editor (skipped when typing in inputs).

### Context menu

- `src/editor/ContextMenu.tsx` — single component handles both pane
  and node right-clicks; items list is passed in via the store.
- **Empty-pane right-click**: "Add node…" (opens search) + "Paste"
  (disabled when clipboard is empty).
- **Node right-click**: Duplicate / Copy / Cut / Paste / Delete with
  shortcut hints; auto-selects the right-clicked node if not already
  selected.
- The default browser context menu is suppressed for the entire editor
  via a wrapper-level `onContextMenu={preventDefault}`.

### Wire-drop auto-connect

- Canvas tracks the in-progress connection via
  `onConnectStart`/`onConnectEnd`. If the drag ends on the pane
  (detected by inspecting the DOM target classlist), the node-search
  popover opens with `PendingConnection` carrying the source node id,
  handle name, handle type (source/target), and port type.
- When the user picks a node, the editor's `addEdge` is called with
  the first compatible port automatically — saving the round-trip of
  "add node, drag wire, connect".

### Window-event bridge for shortcuts → flow coords

- Shortcuts dispatch `floret:open-search` / `floret:paste` with screen
  coordinates as `CustomEvent.detail`. `Canvas.tsx` listens for these
  inside the React Flow provider so it can convert to flow coordinates
  via `screenToFlowPosition`. This keeps the shortcuts file decoupled
  from React Flow's hooks (they're not callable outside the provider).

### Changelog file

- `CHANGELOG.md` (this file) added at the project root, recording the
  full history of the project from Phase 0 onward.

---

## Inline Port Constants + Collapsible Palette

The user pointed out that wiring a separate `Number` node into every
input port is fiddly — for one-off constants you want to just type the
value on the node itself.

### Per-port constant overrides

- **Schema**: `FloretNodeData` gained `portValues?: Record<string, unknown>`
  — per-input-port constant overrides. Saved with the patch, so the next
  open round-trips identically.
- **Runtime priority**: `gatherInputs` now picks per port:
  1. Edge value (if a wire is connected)
  2. `defaultOverride(i)` (looks up `portValues[portName]` via the
     editor's `portValueReader`)
  3. NodeDef `PortSpec.default`
- **`compileGraph`** accepts a new optional `portValueReader` and threads
  it through `defToRuntimeNode`. The editor's compile call wires it to
  `node.data.portValues`. Lives live — no recompile needed for the
  runtime to see updated values.
- **Editor store**: added `setNodePortValue` and `clearNodePortValue`;
  both recompile and push history entries (coalesced per `(nodeId,
  portName)` within 250ms via the existing same-key collapse).
- **`src/editor/PortValueEditor.tsx`** — small floating input that mounts
  once at the App root. Opens imperatively at a screen position via
  `openPortValueEditor()`. Type a value, Enter saves, Esc cancels,
  empty + Enter clears the override.
- **`GenericNode.tsx`**: input port rows now intercept right-click and
  open the editor at the click position. Ports with an override get:
  - The handle pin enlarged from 9 → 12 px with a white outline
    (`box-shadow: 0 0 0 2px rgba(255,255,255,0.85)`)
  - The port label brightened to `--fg-0`
  - A small mono-font readout chip next to the label showing the
    formatted value (e.g. `200`, `0.42`, `true`)
- **Tests**: new `compile.test.ts` case verifying the priority order —
  `portValueReader` overrides defaults, but edges always win. 50/50 passing.

### Collapsible palette

- `NodePalette` accepts `collapsed` + `onToggleCollapse` props.
- When collapsed, renders a 28-px-wide sliver with a vertical "NODES"
  label and an expand chevron.
- `App.tsx` holds the collapsed state and animates the grid column
  width (200 px ↔ 28 px) with a 180 ms transition.
- Toggle button on the palette header (◀ / ▶) — accessible via
  `aria-label`.

### R1 — Regions refactor (Stage 1)

Replace implicit cyclic broadcasting with **explicit ForEach regions** —
the vvvv gamma model. Patches no longer iterate via wire-level
broadcasting; iteration happens inside a region whose splicer pins
cross the boundary explicitly.

**Type system**
- `PortSpec.kind: 'single' | 'spread'` added (default `'single'`).
  Compatibility: single→single ✓, spread→spread ✓, single→spread ✓
  (auto-wrap as length-1 spread), spread→single ✗.
- The legacy `pinned: true` flag retires; `Renderer.layers` and all
  spread-op inputs are now `kind: 'spread'`.
- `connection-rules.areKindsCompatible` enforces these rules in the
  drag UX; `compile` enforces the same at the runtime boundary.

**Runtime: scope tree**
- `liftCyclic` is gone. Replaced by `applyKinded(fn, arity, inputKinds,
  outputKinds)` — single call, no iteration. Single inputs unwrap to
  scalars; spread inputs pass through whole.
- `RuntimeNode.parentId` field; `RuntimeGraph.scopes: Map<ScopeKey,
  RuntimeScope>` keyed by parent id (root = `null`).
- `buildRuntimeGraph` builds a global `edgesByTarget` plus a per-scope
  topo order. Cross-scope edges add synthetic topo deps in the outer
  scope (the enclosing region runs after its outer sources).
- `tick.runRegion(...)` reads the region's outer inputs (splicer-in
  spreads), computes `N = min(input lengths)`, runs the inner scope N
  times with an `IterCtx.splicerInValues` binding, and aggregates each
  splicer-out's per-iteration value into outer-side spreads.

**Region NodeDefs**
- `region/foreach` — orchestrator. Dynamic in/out arity derived from
  splicer children at compile time. No `compute`; runtime dispatches to
  `runRegion`.
- `region/splicer-in/{number,layer,boolean}` and
  `region/splicer-out/{number,layer,boolean}` — border-anchored child
  pins. `regionRole` flag on the NodeDef tells compile how to special-
  case them.

**Cross-scope edges**
- Splicer-routed (outer→splicer-in.outer, splicer-out.outer→outer) are
  rewritten by compile to terminate at the enclosing region with port
  index = splicer's position in the region's child list.
- Broadcast (outer-single → inner-non-splicer) kept as-is. The runtime
  pre-populates the inner scope's outputs with the outer outputs map,
  so inner nodes reading a broadcast scalar just find it there. Lets
  `Time` (or any IO box) deliver the same scalar to every iteration.
- Inner→outer non-splicer edges rejected with a warning — inner values
  aren't stable post-iteration; must use a splicer-out.

**Editor**
- `FloretNodeData` declares no new fields, but the React Flow node now
  carries `parentId` + `extent: 'parent'` for region children.
- `sortNodesParentFirst` helper applied on every mutation — React Flow
  v12 requires parents to appear before their children in the array.
- `editor-state.addRegion(kind, position)` action: creates a region
  with default 480×280 dimensions plus one splicer-in (number) and one
  splicer-out (layer) ready to wire.
- `editor-state.setNodeParent(nodeId, parentId)` for manual reparenting.

**Visuals**
- `<RegionNode />` — translucent bordered box, `<NodeResizer />` from
  React Flow on selection. Category-tinted border (green ForEach,
  amber If, blue Cache — last two land in R3).
- `<SplicerInNode />` / `<SplicerOutNode />` — small pill-shaped pins
  with handles on both ends. Rendered as ordinary children of the
  region; the user positions them on the border.
- Toolbar gains a **`+ Region`** button.

**Samples rewritten**
- *Hello Rect* + *Mouse Follow* stay region-free (single Layer paths).
- *Wavy Row* — outside region: `Slider(count)`, `Range`, `LinearSpread`,
  `ColorPicker`, constants. Inside region: 2 splicer-ins (index, xs),
  `Time` broadcast, phase math, `Ellipse`, `Fill`. One splicer-out
  collects per-iter Layers; region → Renderer.
- *Circular Rects* — outside: `CircularSpread` driven by `Time` phase.
  Inside: `Rect` at `(radius, 0)`, `Rotate` by per-iter angle,
  `Translate` to canvas centre.

**Tests**
- Dropped `liftCyclic` tests; dropped cyclic-broadcast spread-node tests.
- `graph.test.ts` rewritten for scope-aware `buildRuntimeGraph`.
- `compile.test.ts` rewritten for scalar/kinded compute paths.
- New `region-foreach.test.ts`: N = splicer-in length, N = min over
  multiple splicers, empty propagation, outer→inner broadcast.
- `samples.test.ts` strengthened: *Wavy Row* must produce exactly 8
  layers (was just "≥1").
- 32/32 passing.

**Not yet (deferred to a polish pass)**
- Drag-into-region auto-reparenting on `onNodeDragStop`.
- "Wrap selection in Region" right-click action.
- Auto-creation of splicers when a wire is dragged across a region
  border (vvvv's splicer-bar UX).

---

### R2 — Accumulator pins

State threaded across iterations of a ForEach region. Enables running
sums, particle trails, scan patterns — anything that depends on the
previous iteration's value.

**Pin model**
- `region/accumulator/{number,boolean}` — one NodeDef per type.
- 4 ports total:
  - Outer **`init`** (input) — seed value for iteration 0
  - Outer **`final`** (output) — value after the last iteration
  - Inner **`read`** (output) — current iteration's value
  - Inner **`write`** (input) — value carried into the next iteration

**Runtime**
- `RuntimeScope.accumulatorIds` holds the accumulator children in
  declaration order, parallel to `splicerInIds` / `splicerOutIds`.
- `runRegion` initialises a `Map<accumId, currentValue>` from the
  outer init slots. After each iteration runs, it gathers each
  accumulator's `write` input value and stores it for the next
  iteration. After all iterations, the final values are appended to
  the region's outputs (`outputs = [...splicerOutSpreads,
  ...accumulatorFinals]`).
- `IterCtx.accumulatorValues` makes the current value available when
  the inner scope evaluates the accumulator node — its `read` output
  is set to `[currentValue]` directly (no `compute` invocation).

**Topo trick**
- An intra-scope edge targeting an accumulator skips the topo
  adjacency. Otherwise we'd have a cycle: accum.read → consumer →
  consumer.out → accum.write → (back to accum.read on next iter).
  The edge still lives in `edgesByTarget`, so `runRegion` reads the
  write value via `gatherInputs` after each iteration.

**Editor**
- `<AccumulatorNode />` — a taller pill with two handles per side and
  a horizontal divider. `init`/`final` on the left (outer-facing);
  `read`/`write` on the right (inner-facing).
- `addRegion` accepts an `accumulatorTypes` option.

**Compile**
- Region orchestrator input layout is now `[...splicer-ins, ...accumulator-inits]`;
  output is `[...splicer-outs, ...accumulator-finals]`.
- New edge-rewrite branches for cross-scope `accum.init` / `accum.final`.

**Tests**
- `region-accumulator.test.ts` — running sum `[0..4]` = 10; accumulator
  with N=0 emits init unchanged; accumulator + splicer-out coexist in
  the same region. 3/3 passing.

---

### R3 — If + Cache regions

Two additional region kinds, additive on top of R1/R2's scope-based
runtime.

**If Region**
- `region/if` NodeDef declares one static input on the region itself:
  `condition: boolean`.
- N = 1 when condition is true (run inner scope once); N = 0 when
  false (skip inner scope, splicer-outs emit empty spreads,
  accumulators forward their init value unchanged as `final`).

**Cache Region**
- `region/cache` — same iteration semantics as ForEach but memoised.
- `RuntimeNode.runtimeState.cache: { hash, outputs }` persists across
  `tick()` calls (rebuilt at each `compile`).
- `hashInputs(outerInputs)` stringifies structurally; functions
  (Layers) hash to `"function"`, so cache regions over Layer inputs
  won't actually hit — by design.
- Hit path: returns the cached `outputs` by reference (the test
  asserts identity via `expect(a).toBe(b)`).

**Static region inputs**
- `RuntimeNode.regionStaticInArity` (= `def.inputs.length`) reserves
  the first N input slots for static region inputs (currently only
  `If.condition`); splicer-in slots shift by `staticIn`, accumulator
  init slots shift by `staticIn + nSplicersIn`. `runRegion` reads
  static inputs from indices `0..staticIn-1`.
- `<RegionNode />` dynamically renders left-side handles for each
  static input declared by the NodeDef.

**Editor**
- Toolbar now has three region buttons: `+ ForEach`, `+ If`, `+ Cache`.
- If / Cache regions start with no splicer children by default —
  ForEach still seeds one number splicer-in + one layer splicer-out
  for convenience.

**Tests**
- `region-if-cache.test.ts` — If with condition=true forwards value;
  condition=false emits empty spread; accumulator init forwarded
  unchanged when condition=false; Cache region returns SAME REFERENCE
  outputs across two ticks with identical inputs. 4/4 passing.

**Final tally**: 39/39 tests passing across 9 test files; production
build clean.

---

### Port indicator refinements + hover hit area

Follow-up tweaks after the initial port-value implementation felt
over-emphasised:

- Dropped the 9 → 12 px size bump and the white box-shadow ring on
  overridden ports. The value chip + brightened label next to the
  port name is now the sole indicator.
- The indicator hides automatically when an edge is connected to that
  port — the wire itself is the signal. `GenericNode` derives a
  per-node Set of `connectedInputPorts` via a Zustand selector
  (stable-string memoised) and only shows the override chip when
  `override !== undefined && !isConnected`.
- All handles now have an **18-px invisible hit area** (`::before`
  pseudo-element with 6 px expansion on every side) so cursor targeting
  is easier without changing the visible size.
- Handles **scale to 170%** on hover with a 100 ms transition — pure
  CSS `transform: scale`, which doesn't affect React Flow's wire-anchor
  position. Tactile feedback once the cursor is near.

---

## Region Frame Ports + Dynamic Inputs + Style Refactor

A large UX pass that replaced the splicer-child model with frame-edge
ports, introduced a dynamic-input mechanism for mode-switching nodes,
and rebuilt the colour/fill pipeline around a `Style` port type.

### Region frame ports

The biggest visual change: splicer-in / splicer-out are no longer
visible child nodes inside a region. Instead, each region has its own
list of dynamic ports rendered as tabs on the frame's edges.

- **Data**: `FloretNodeData.ports?: RegionPortDescriptor[]` — `{ id,
  name, type, direction: 'input' | 'output' }`. Stored on the region
  node itself; persisted via the save format.
- **Visual**: each port is one **bridge tab** straddling the border —
  inner pin inside the frame, outer pin outside, label centred on the
  border so the left→right data flow is obvious. 56×14 tab, 9 px font,
  20 px row pitch. Originally drawn as two separate dots flush to the
  border; redesigned after feedback that it looked like one ambiguous
  blob.
- **Compile**: new `expandRegionPorts(nodes, edges, registry)` preproc
  pass that, for every region with `ports`, synthesises a virtual
  splicer-in or splicer-out RuntimeNode (id `${regionId}__sin_${portId}`
  / `__sout_${portId}`) and rewrites edges so the rest of compile sees
  the same splicer-child shape it always has. Explicit splicer child
  nodes still work — both paths converge on the existing R1 runtime.
- **Editor actions**: `addRegionPort`, `removeRegionPort`, `renameRegionPort`.
  `addRegion(kind, position)` no longer auto-creates splicer children;
  regions start empty and ports are added on demand.
- **Wire-drop creates a port**: in `onConnectEnd`, if the wire was
  dropped within 24 px of a region's left or right edge, a port is
  auto-created of the matching type and direction, and the edge is
  completed in one motion. Hit-testing uses node positions (not
  `elementsFromPoint`, which skips `pointer-events: none` zones).
- **`useUpdateNodeInternals`**: React Flow caches handle positions on
  mount; dynamic ports need explicit re-measure or wires render at
  stale coordinates. RegionNode signals `id|width|portList` changes.
- **Right-click a pin or label to delete the port**. Double-click the
  label to rename inline.
- **Port name de-dup**: auto-generated port names increment with
  numeric suffix instead of all being `out_layer`.

### Space-drag reparenting

Initial implementation used Shift+drag (captured at drag start AND
stop), with a context menu fallback. Later changed to **Spacebar** at
the user's request, with a visual indicator system.

- **Modifier**: hold Space anywhere → reparent mode armed. All regions
  get a dashed accent border so the mode is visible. Drag a node while
  Space is held → region currently under cursor pulses solid green
  with a glow. Drop there to reparent; drop on empty space to detach
  to root.
- **`extent` stripping**: nodes with `parentId` get `extent: 'parent'`,
  which clips drags to the parent's bounds — that's why dragging *out*
  initially didn't work. Now, while reparent mode is on, the projected
  node list passed to React Flow strips `extent` from every parented
  node. The store stays unchanged; extent is re-applied on drop via
  `setNodeParent` (which now takes an optional new position so the
  visual position is preserved across the coord-space change).
- **Spacebar tracking**: window-level keydown/keyup with INPUT /
  TEXTAREA / contentEditable guard so renaming a port still types a
  space. `preventDefault` to suppress page-scroll-on-space. `blur`
  listener clears the mode when the window loses focus.
- **Right-click context menu** on any node: "Move out of region" (when
  inside one), "Move into <Region name>" for every region on the
  canvas, as a discoverable alternative to the modifier-drag.
- **Always-visible resize handles**: dropped `isVisible={!!selected}`
  on `<NodeResizer />`; handles now always render but stay subtle until
  hover/selected so you can drag from any edge or corner without
  needing to select first.

### Persistence: parentId, ports, portValues, style.{width,height}

The serialiser was silently dropping all four — a pre-existing bug
that only became visible once regions persisted across saves.

- `SerializedNode` gained `parentId`, `portValues`, `ports`, `width`,
  `height` (the last two from the React Flow node style). All
  round-trip cleanly now.

### Sample trim

- Removed Hello Rect, Wavy Row, and Mouse Follow. Circular Rects is
  the only remaining sample and was rewritten to use frame-port
  regions + the new Fill node.

### Style port type + shape appearance input

Originally `style/fill-rgb` wrapped a Layer and added a fill. The user
asked for shapes to own their appearance: pass a `style` IN to the
shape, get a fully-styled layer OUT.

- **`PortType` gained `'style'`** (+ purple CSS variable
  `--port-style`). A `Style` value is `(p: p5) => void` — it applies
  drawing state (fill, stroke, strokeWeight, …) to the active context.
- **`DEFAULT_STYLE`** in `runtime/types.ts`: white fill, no stroke.
  Applied by every shape when its `style` input is unconnected.
- **Shape nodes** (Rect / Ellipse / Line) gained a `style: style` input.
  Compute does `push() → applyStyle() → draw() → pop()`. Line's default
  was tweaked to black-stroke / no-fill since a no-stroke line is
  invisible.
- **Style nodes** rewritten to OUTPUT a Style instead of wrapping a
  Layer. Each takes an optional `base: style` input for chaining —
  base runs first, then this node's override applies on top.

### Renderer background

- Renderer NodeDef gained a `background: color` input and a matching
  output (so the p5 driver can read it). The driver reads
  `outputs.get(rendererId)?.[1]?.[0]` each frame and calls
  `p.background(r*255, g*255, b*255)` if set, else falls back to black.
- Color picker now emits a `color` output (a normalised `Color` object)
  in addition to the r/g/b channels.

### `PortSpec.param` — slider + wire fallback without recompile

A new declarative mechanism for IO-style nodes where each input port
can also have a built-in inline editor.

- `PortSpec.param?: string`: when set, the runtime reads the named
  param from `paramStore` each tick when no edge is connected (and no
  `portValues` override). Wire wins when connected; slider/inline
  editor supplies the value otherwise; PortSpec.default as final
  fallback.
- Compile threads this through `defaultOverride`: priority order is
  edge → portValues → param → PortSpec.default.
- Live updates: writes to paramStore go through `useNodeParam.setLive`
  which doesn't recompile. The runtime picks up the new value next
  tick because `defaultOverride` is consulted each frame.

### HSL Color picker

- `io/hsl-picker` node with a custom component. Live colour swatch on
  top, three gradient-tinted range sliders (H 0–360, S/L 0–100). Each
  slider's gradient updates with the current values of the other two,
  so each handle previews what it will produce.
- **Input pins** for h/s/l on the left edge — wired values override the
  slider. When wired, the slider greys out and the label glows accent.
- **Output**: single `color` (no r/g/b/h/s/l channel pins — collapsed
  after user feedback).

### `NodeDef.dynamicInputs` — per-instance input ports

To unify the old fill-rgb / stroke / etc. into one mode-switching Fill
node, the engine needs nodes whose input shape depends on a param.

- `NodeDef.dynamicInputs?: (params) => ReadonlyArray<PortSpec>`. When
  set, overrides the static `inputs` for compile, connection-rules,
  and the rendering component.
- `getResolvedInputs(def, params)` helper exported from `nodes/types`.
  Used by compile, `buildIsValidConnection`, `Canvas.findRegionEdgeAt`'s
  drag-end resolver, and `GenericNode` so they all see the same shape.
- `compile.ts` resolves once per node and caches via an `inputsByNodeId`
  Map so dynamicInputs isn't called repeatedly during edge rewriting.
- `setNodeParam` auto-deletes edges that reference now-defunct input
  ports when the param drives `dynamicInputs`, and triggers a recompile
  in that case (plain params still skip recompile).
- `connection-rules.buildIsValidConnection` takes an additional
  `getParams` lookup so it can resolve dynamic inputs.

### Unified Fill node

- `style/fill` — single node with a **mode dropdown** (rgb / hsl /
  hex / color). Inputs change with mode:
  - `rgb`: r, g, b (0–255), base
  - `hsl`: h (0–360), s/l (0–100), base
  - `hex`: hex string '#rrggbb', base
  - `color`: a Color value, base
- Outputs: `out` (style, for shape's `style` input) **and** `color`
  (Color, reusable — e.g. into the renderer's background or another
  Fill's base).
- Old `style/fill-rgb`, `style/stroke-rgb`, `style/no-fill` collapsed.
  `style/stroke-weight`, `style/no-stroke` kept as separate nodes.
- **Custom `FillNode.tsx` component**: one row per input port — pin +
  label + inline editor (or nothing when wired). Initially had a
  separate `node-params` block above plus a separate input port list
  below; redesigned to collapse them into single rows after user
  feedback. Mode dropdown moved into the node header. ~6 rows total.

### Text node + value/string node + string param editor

- `draw/text` — draws a string at (x, y) with given size and align
  ('left'/'center'/'right'). Same style convention as other shapes.
  `text`, `size`, `align` are inline-editable AND wire-overridable via
  the `param:` fallback mechanism.
- `value/string` — constant string node, mirror of value/number.
- `GenericNode` extended to render an editable text input for string
  params (was readonly text before) using `useNodeParam` for live
  updates. The Text node's `text` field is now editable directly on
  the node.

**Final tally**: 39/39 tests across 10 test files; production build
clean. Tests still exercise the explicit splicer-child path so the
runtime is unchanged; new `region-frame-ports.test.ts` covers the
dynamic-port expansion path.

---

### Unified node row layout

The user asked for every node to look like the Fill node — one row per
input port (pin + label + inline editor) and no repeating field names
between a separate params section and the input list.

- **`GenericNode` rewritten** around a single row primitive:
  - **Pure params** (defaultParams keys that no input port maps to via
    `PortSpec.param`) render as own rows with label + inline editor.
    Example: Number.value, Renderer.width/height.
  - **Input ports** render with pin + label, plus an inline editor when
    `PortSpec.param` is set AND the port isn't wired. When wired, the
    editor hides and the label glows accent. Right-click on the row
    still opens the per-port-value override editor for types without
    inline support (vec2, mat3, etc.).
  - **Output ports** render right-aligned with the pin on the far
    right.
- **Inline editors**: number / string / boolean built in. Hex strings
  (param name === 'hex') get a colour picker. Other types fall through
  to the existing right-click PortValueEditor.
- **No more split sections**: `.node-params` + `.node-ports` collapsed
  into uniform `.node-row-group` blocks separated by hairline borders.
  Eliminates the duplication of e.g. having an `r` param row AND an `r`
  input port row.
- **Compact**: 10 px font, 18 px row height, 60 px editor width. Roughly
  half the vertical space of the old layout for nodes with params.

Custom node components (Slider, Toggle, XYPad, ColorPicker, HslPicker,
Fill, Splicer, Accumulator, Region) opt out of GenericNode entirely
via `def.component`, so their specialised UX is unaffected — but they
already follow the same visual rhythm.

### Row pairing — outputs aligned with inputs

Follow-up: the first pass put outputs in their own group below the
inputs, which still felt boxy. Rewritten so rows are PAIRED — left
side carries an input/param, right side carries an output, on the
same row. The first output sits on the same row as the first input;
the inline editor sits next to its label instead of being pushed to
the right edge.

```
  ● text  [ Hello ]                out ●
  ● x
  ● y
  ● size  [   16  ]
  ● align [ left  ]
  ● style
```

- `numRows = max(leftItems.length, rightItems.length)` — extra inputs
  or extra outputs spill into rows with only one side populated.
- Flex layout: `pin | label | editor | flex-spacer | output-label |
  pin`. Handles are absolutely positioned at the row's left/right edges
  so the spacer cleanly fills.
- Inline editor width fixed at 60 px (28 px for colour swatches, 14 px
  checkbox) so labels stay aligned across rows.

### Convert nodes + 'any' port type

- New `PortType: 'any'` — accepts any other type when wired (gray
  `--port-any` colour). Used by convert nodes whose job is exactly to
  coerce one type to another.
- `arePortsCompatible(a, b)` now returns true if either side is `any`.
- `nodes/convert/convert.ts` adds:
  - `convert/to-string` — any → string. Numbers print compactly,
    vec2 → `"x, y"`, color → `"#rrggbb"`, arrays → joined list.
  - `convert/to-number` — strings parse, booleans → 0/1, else 0.
  - `convert/to-int` — like to-number but `Math.round`.
  - `convert/to-bool` — JS truthiness.
- Text node's `text` input renamed to **`string`** (matching the
  PortType) so a `toString` node wires naturally into it.

### Palette polish + camelCase node labels

- Palette categories are **collapsible** (chevron + click on the title)
  and **collapsed by default**. Each title shows the count of nodes in
  the category on the right. Active search auto-expands all matching
  categories.
- **Compact dark scrollbars** globally — 6 px wide, `--bg-3` thumb on
  transparent track, fades to `--border` on hover. Firefox uses the
  `scrollbar-width: thin` / `scrollbar-color` equivalents.
- **Mono palette item labels** at 11 px, tighter row padding (3 px),
  indented under the category chevron.
- **camelCase node labels** — no spaces in any node name:
  - `Color Picker` → `ColorPicker`
  - `HSL Color`    → `HSLColor`
  - `XY Pad`       → `XYPad`
  - `Linear Spread` / `Circular Spread` → `LinearSpread` / `CircularSpread`
  - `Stroke Weight` / `No Fill` / `No Stroke` → `StrokeWeight` / `NoFill` / `NoStroke`
  - `To String` / `To Number` / `To Int` / `To Bool` → `ToString` / …
  - `Splicer In (number)` → `SplicerIn:number` etc.
  - `IO Boxes` category → `IO`
- Map node's port labels (`in lo` / `out hi`) → `inLo` / `outHi`.

### UX polish pass

Round of paper-cut fixes after the camelCase rename.

- **Every editable input port is inline-editable.** Previously only
  ports that declared `PortSpec.param` got an inline editor; ports
  without it (e.g. Text.x / Text.y, Rect.x/y/w/h) showed nothing.
  Now any unwired input port of type `number` / `string` / `boolean`
  gets an inline editor. Behaviour:
  - With `PortSpec.param`: paramStore-backed, live writes, no
    recompile per keystroke (used by Fill r/g/b, HSL h/s/l, Text size
    / align / string).
  - Without `param`: portValues-backed via local React state +
    `setNodePortValue` on blur — one recompile per commit, not per
    keystroke. Persists across save/load through the existing
    `portValues` serialisation.
- **No more search popover on successful connect.** `onConnect` now
  sets a `connectionMadeRef` flag that `onConnectEnd` checks before
  opening the search fallback. Previously, dropping precisely on a
  handle would sometimes still trigger the search because the
  DOM-target check (`.react-flow__handle.closest`) wasn't reliable
  enough — the flag is unambiguous.
- **Single connection per input.** `onConnect` filters out any
  existing edge to `(target, targetHandle)` before adding the new
  one. Drag a new wire onto a pin that's already wired → the old
  wire is replaced. (Source handles still fan out freely.)
- **Defensive orphan-edge prune** at the start of every `_compile()`:
  drops edges whose source or target node no longer exists. Backs up
  the per-action cleanups in `deleteNode`, `removeRegionPort`, and
  the `dynamicInputs`-driven setNodeParam path.

### Fill migrated to GenericNode + paramOptions

The custom FillNode visual was still using a separate `node-params`
section above the input rows — visually inconsistent with the
unified layout. Migrated to GenericNode by adding two small
primitives:

- **`NodeDef.paramOptions?: Record<string, readonly string[]>`** —
  declare a fixed list of options for an enum-style param.
  GenericNode renders a `<select>` instead of a text input. Used for
  Fill's `mode` (rgb / hsl / hex / color) and Text's `align`
  (left / center / right).
- **Output row offset**: outputs now pair with INPUT rows, skipping
  the pure-param rows at the top. So Fill's `out` and `color` sit on
  the first two input rows instead of starting on the `mode` row —
  semantically cleaner. `numRows = max(leftItems.length,
  pureParams.length + outputs.length)`.
- Deleted `FillNode.tsx`; Fill now uses the default GenericNode
  visual like every other compute node.

Custom components remaining (Slider, Toggle, XYPad, ColorPicker,
HslPicker) are kept because they're genuinely specialised widgets
(2D drag area, gradient sliders, etc.) not just port + editor rows.

### Click-to-toggle input editors

Input editors are now **hidden by default**. Clicking an input port
label toggles its inline editor open/closed. Substantially cuts
visual noise — a Rect with 5 inputs collapses to a tight column of
pin + label rows, and you only expand the one you want to scrub.

- Per-port expanded state held in `GenericNode` component state via
  `Set<string>` (resets on remount, which is fine — the value
  itself is preserved in params/portValues).
- Pure-param rows (e.g. Fill `mode`) **always show their editor** —
  they configure the node's shape so they need to be visible.
- Wired ports don't show the toggle affordance — the wire owns the
  value.
- A small value-readout chip appears next to the label when the
  editor is collapsed AND an override value is set, so you don't
  lose sight of customised inputs.
- Label gets `cursor: pointer` + hover brighten when clickable;
  expanded labels stay in `--fg-0` so you can see what's open.

### UI tweaks: toolbar, palette, regions, ColorPicker

A grab-bag pass of small polish items.

- **Toolbar reorder**: Pause/Play moved to the far right; `+ForEach /
  +If / +Cache` buttons removed (regions are created via palette
  drag now). Title + FileMenu + SampleMenu sit on the left.
- **Status bar gets the version**, centered. Three-column grid:
  `fps/frame/nodes` left, `v0.0.1` centre, `ready` / warnings right.
- **Palette category headings** tinted accent (was just brighter grey)
  so they're visually distinct from the node labels under them.
- **Region frame port arrows removed** — the tab + pin geometry
  already conveys direction. Cleaner.
- **Palette collapses by clicking its right edge** — replaces the
  `◀` / `▶` button in the header. A thin 6 px strip along the
  panel's right edge highlights accent on hover and toggles the
  collapsed state on click. `cursor: ew-resize` so it reads as a
  drag handle even though it's a single click.
- **ColorPicker uses GenericNode** — param renamed from `value` to
  `hex` so the existing `<input type="color">` special case fires.
  The hex picker sits inline on the row paired with the `color`
  output; r/g/b outputs follow on their own rows. Deleted
  `ColorPickerNode.tsx`.
- **Draw-mode region creation**. Drag any region (ForEach / If /
  Cache) from the palette to the canvas:
  - The drop point becomes the first corner of a rubber-band
    rectangle.
  - Mouse-move shows a translucent preview rect colour-coded per
    region kind (green / amber / blue).
  - Left-click commits — the region is created at the drawn bounds.
  - Escape cancels mid-draw.
  - **Auto-reparent**: any root-scope node whose centre lies inside
    the drawn rectangle becomes a child of the new region, with its
    position converted into parent-local space. Regions never nest
    into other regions automatically.
  - `addRegion` now takes `{ width, height, autoReparent }` options
    to support this.

### First-impression polish pass

Four small improvements that together transform the "is this thing
on?" feel.

- **Empty-canvas hint** — three muted lines centred when no nodes
  exist: <kbd>Tab</kbd> to search · drag from the left · or load a
  Sample. First-time users finally have a starting point.
- **Wire-drag affinity feedback** — when a wire is being dragged,
  every handle on the canvas gets tagged via a body
  `data-wire-dragging` flag plus a per-handle `data-wire-affinity`
  attribute (`'match'` / `'miss'`). Compatible handles glow with a
  pulsing accent ring; incompatible ones fade to 18% opacity. Makes
  the typed-port system *teachable* — drag a number out, see every
  number input light up, learn the connection rules by doing.
- **Number scrubbing** — every inline number editor (`ScrubInput`)
  now doubles as a horizontal drag handle. Default = 1 step per 4 px,
  Shift = 10×, Alt = 0.1×. Focus the field to type, drag the field
  to scrub. The border lights up while scrubbing so the mode is
  obvious. Cursor flips from `ew-resize` to `text` on focus.
- **Hover-to-inspect runtime values** — hovering any output pin or
  its label pops a small tooltip showing the current value, updated
  every frame (rAF poll, since only one tooltip is ever open). The
  formatter handles numbers / strings / booleans, vec2 (`vec2(x,
  y)`), Color (`#rrggbb`), arrays (`[a, b, c, …×N]`), and falls
  back to JSON for the rest.
  - Mechanism: `src/runtime/inspector.ts` keeps a module-level
    `latestOutputs` Map, written by `frame-hook.ts` each tick via
    `publishTickOutputs`. Lookup via `getRuntimeValue(nodeId,
    portIdx)`. No Zustand re-render — wouldn't scale to per-frame
    updates across all nodes.

### Always-paired outputs + hidden internal NodeDefs

- **Outputs pair starting from row 0**, regardless of left side. So
  source-only nodes like `Number` collapse to a single line:
  `value [129] | out ●`. Previously the row offset skipped pure-
  param rows, which left `out` orphaned on a second line.
- **`NodeDef.hidden?: boolean`** flag — palette and node-search
  filter these out. Used to hide `region/splicer-in/*` and
  `region/splicer-out/*`, which compile synthesises from region
  frame ports and the user never needs to drop manually. The defs
  stay registered so the runtime and the existing splicer-child
  tests keep working.

---
