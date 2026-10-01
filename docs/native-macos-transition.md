# Floret → Native macOS App — Transition Plan

**Status:** proposed · **Date:** 2026-10-01
**Decision so far:** fully native macOS app. No browser layer, no Electron,
no Tauri/web view. macOS is the only target platform.

This replaces issue #30 ("Native kiosk shell (Electron / Tauri)").

---

## 1. What we're porting

| | Today (TypeScript) |
|---|---|
| Code | ~44k lines TS/TSX + 5.6k lines CSS, 313 files |
| Nodes | 298 registered across 24 categories (biggest: spread-ops 56, math 44, vec2 20, sound 19, io 18, draw 14) |
| Samples | 95 (48 tutorials + 47 showcase), built in TS code, not stored as files |
| Tests | 31 files / 629 tests; 24 files are pure engine logic |
| Renderer | WebGL2: SDF shapes, stencil polygon fill, textured quads, blend layers, Gaussian blur, feedback buffers, user GLSL (ShaderToy-style) |
| Audio | Web Audio node graph, 19 sound nodes, no worklets |
| Inputs | mouse, keyboard, camera, MediaPipe hand/face/pose, MIDI, microphone |
| Files | `.flora` JSON (versioned + migrations), localStorage autosave + library |

The engine is the valuable, portable part: compile → topo sort → tick with
regions (ForEach / If / Cache / subpatch), cross-frame state, spreads,
retained-mode scene IR. Most node files (107 of 135) are pure logic with no
browser calls.

---

## 2. Target stack

| Concern | Web today | Native macOS |
|---|---|---|
| Language | TypeScript | **Swift 6** |
| App frame | React app shell | **AppKit + SwiftUI** — SwiftUI for chrome (toolbar, settings, palette, library, help); AppKit for the node canvas |
| Node editor | React Flow | **Custom layer-backed `NSView`** (React Flow has no native equivalent; this is the largest single UI build) |
| Documents | File System Access API + localStorage autosave | **`NSDocument`** — gives ⌘S, autosave, versions, Recent Files, window restore, unlimited size for free |
| Undo | custom snapshot history | **`UndoManager`** |
| Rendering | WebGL2 | **Metal** — same 3 pipelines (SDF instances, stencil fill, textured quads) + blend layers + blur + feedback |
| Text | Canvas2D `measureText`/`fillText` | **Core Text** → glyph atlas |
| User shaders | GLSL ES 3.00 | **Keep GLSL**, translate to Metal at runtime via glslang + SPIRV-Cross (keeps existing shader patches and ShaderToy compatibility) |
| Image/pixel ops | Canvas2D, CSS filters, `getImageData` | **Core Image** (filters, blend modes, resampling) + **vImage/Accelerate** (pixel sort, convolve, threshold) |
| Audio | Web Audio | **AVAudioEngine** (source nodes, EQ/filter, delay, reverb, distortion; FFT via vDSP) |
| Camera | `getUserMedia` | **AVFoundation** capture session |
| Tracking | MediaPipe (JS/WASM) | **Apple Vision** framework (hand pose, body pose, face landmarks) |
| MIDI | Web MIDI | **CoreMIDI** |
| Send/Receive bus | Vite dev-server relay | **In-app HTTP server** (Network.framework) — Raspberry Pi e-ink poller keeps working unchanged |
| Settings | localStorage | `UserDefaults` |
| Library | localStorage JSON | files in `~/Library/Application Support/Floret/` |

---

## 3. Architecture

```
mac/
  FloretEngine/        Swift package — NO AppKit/Metal imports
    Types, Graph, Compile, Tick, Regions, State
    Nodes/<category>/   pure compute for every node
    Scene/              SceneNode IR + SVG export
    Persistence/        .flora load/save + migrations
    Tests/              golden-fixture tests (see §4)
  FloretRender/        Metal renderer (consumes Scene IR)
  FloretMedia/         audio, camera, Vision, MIDI, bus server
  Floret.app/          AppKit/SwiftUI app, editor, document
```

Rules:
- The engine package never imports UI or GPU frameworks, so it is
  unit-testable on its own and keeps the separation the TS code already has.
- Node definitions split into an **engine def** (ports, compute) and an
  **editor def** (custom UI, inline widgets). Today they share one object.

### Fix-on-port list (design changes to make in the Swift version)
- **Styles as data, not closures.** A `Style` on a wire is a JS function today;
  in Swift it becomes a list of style operations applied in order.
- **Colours as RGBA floats**, not CSS strings parsed back with regex.
- **Constant pins honour wires.** Today a wire into a `constant` pin is
  silently ignored (`compile.ts:653` refers to a `resolveWiredConstants`
  pass that was never written), contradicting `CONCEPTS.md`. Implement the
  documented behaviour.
- **Memoisation by value equality** instead of JS reference identity.
- **Cache region hash** defined explicitly (today: `JSON.stringify`).
- **Seeded randomness everywhere.** `Math.random` is used unseeded in
  `particles/from-layer`, `sound/noise`, `sound/reverb`.
- **Audio fan-out.** Today each audio source can drive only one consumer
  (`connect` disconnects the previous one). AVAudioEngine fixes this.
- **Exact PRNG port.** `hash32` / `mulberry32` must use wrapping 32-bit
  arithmetic (`&*`, `&+`) so seeded patches look identical.

---

## 4. How we prove parity: golden fixtures

Before writing Swift, add an exporter to the **current** TS repo that writes:

1. **Every sample as a `.flora` file** (95). These become the native app's
   built-in samples and its test corpus. Samples stop being TS code.
2. **Scene trees per sample per frame** — fixed time steps, fixed seed, JSON
   (`samples.test.ts` already compiles and ticks every sample; extend it to
   dump output).
3. **Reference PNGs** per sample at a fixed frame, for pixel-diff tests of
   the Metal renderer (with tolerance).
4. **Unit fixtures** from the pure tests: compile, graph, regions, adaptive
   math, spread ops, easing, curl noise, SVG output, migrations.

The Swift engine then has a mechanical definition of "done": load the same
patch, produce the same scene tree (float tolerance), render the same pixels
(within tolerance). This is what makes a rewrite of this size safe.

Known unavoidable differences (accept and document, don't chase):
text metrics (Core Text vs Canvas2D), image filters (Core Image vs CSS
filters), and resampling.

---

## 5. Phases

| # | Phase | Delivers | Size |
|---|---|---|---|
| 0 | **Prep (in TS repo)** | Golden-fixture exporter, samples as `.flora`, constant-pin bug fixed, web app frozen to bug fixes only | S |
| 1 | **Engine core** | Swift package: types, compile, graph, tick, regions, state; math / spread-ops / vec2 / logic / convert / value nodes. Passes compile + region fixtures | L |
| 2 | **Scene + Metal renderer** | Scene IR, draw / style / color / transform nodes, Metal renderer, SVG export, headless render. Passes scene + pixel fixtures | L |
| 3 | **App MVP** | `NSDocument` app, node canvas editor, palette, node search, inline port editors, preview window, undo, open/save. First usable build for visual patches | XL |
| 4 | **Motion & simulation** | time, motion, random, particles, forces, colliders, physics | M |
| 5 | **Image, text, shaders** | Core Image / vImage nodes, Core Text, GLSL→Metal shader node + shader panel | L |
| 6 | **Media in/out** | AVAudioEngine sound nodes, microphone, MIDI, camera, Vision tracking | L |
| 7 | **IO** | Send/Receive bus server, e-ink, file / image / SVG / CSV nodes, Save SVG | M |
| 8 | **Parity polish** | Docking panels, library, help, welcome, preferences, light/dark, focus mode / fullscreen output, PNG/SVG export, all 95 samples green | L |
| 9 | **Release** | App icon, signing + notarization, auto-update, 1.0 | S |

Rough total: **6–9 months** of focused work to reach today's feature set.
Phases 1–2 and the engine half of 4–7 are mechanical against fixtures and
move fastest; phase 3 (the editor) carries the most risk.

The web version stays as the reference implementation and fixture generator
until phase 8 is green, then gets archived.

---

## 6. What changes for users

- **Mac only.** No Windows, Linux, or browser editor. (The Pi e-ink display
  still works — it's a client of the bus, not an editor.)
- **Share links go away.** `#patch=` URLs have no meaning without a browser;
  share `.flora` files instead (optionally a `floret://` URL scheme later).
- **Tracking landmarks change.** Apple Vision gives different point sets than
  MediaPipe — notably ~76 face points vs MediaPipe's 478 face-mesh points.
  Patches that index specific landmarks (Body Tracking, Webcam Mosaic) need a
  mapping table or rework.
- **Image filters and text** will look very slightly different.
- **Gains:** real files and autosave with no size limit, native menus and
  shortcuts, fullscreen output on any display, kiosk / launch-at-login, and a
  path to native DMX / serial / NDI / Syphon (issues #32, #33, #34).

---

## 7. Decisions needed

1. **Minimum macOS version.** Recommend **macOS 14 (Sonoma)** — needed for
   Swift's Observation framework and current SwiftUI; covers ~all active Macs.
2. **Distribution.** Recommend **direct download** (Developer ID signing +
   notarization, Apple Developer Program, $99/yr) with Sparkle for updates.
   The Mac App Store's sandbox would block the hardware IO roadmap
   (serial, DMX, NDI).
3. **Tracking.** Recommend **Apple Vision** (built in, fast, no bundling) and
   accept fewer face points. Alternative: bundle MediaPipe's native C++
   library — heavy build, much more maintenance.
4. **Web version during the rewrite.** Recommend **freeze to bug fixes** so
   the fixtures don't move under the port.
5. **Where the work happens.** The Mac app can only be built and run on a Mac
   with **Xcode** installed, so phases 2–9 need local sessions on your Mac.
   Phase 0 (TS fixtures) can run anywhere, including cloud sessions.

---

## 8. Next step

Phase 0: build the golden-fixture exporter and convert the 95 samples to
`.flora` files in this repo. It's useful even before any Swift exists — it
fixes the samples-as-code problem and gives the port its definition of done.
