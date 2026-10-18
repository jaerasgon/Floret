# Floret

[![ko-fi](https://ko-fi.com)]([https://ko-fi.com](https://ko-fi.com/jaerasgon))


**A node-based tool for generative art.** Wire shapes, motion, noise,
particles, text, images, sound and live input together, and watch the
result change as you edit. Floret is a native app, drawn on your GPU
(Rust and wgpu), and it is free and open source.

![Floret with the Kaleido Slime sample open: the patch on the node canvas, its live preview on the right, the Inspector below](docs/images/floret-window.png)

You build a picture by wiring small boxes, called nodes, together on a
canvas. A noise node feeds a shape, the shape feeds a colour, an LFO moves
it all. Every value stays editable while the patch runs, so you can play
with it instead of waiting for it. When you like what you see, save a still,
a seamless loop, or a sheet of variations. Your patches are plain `.flora`
files.

Floret is **early (beta)**, and your feedback shapes it. See
[Feedback and help](#feedback-and-help).

## What Floret offers

**Make**

- **252 nodes** for shapes, paths and text; noise and maths; colour and
  palettes; patterns and layouts; particles and simulations; images,
  video and shader effects; and sound, MIDI, camera and body tracking.
  [NODES.md](NODES.md) lists every one.
- **175 sample patches** to open, pull apart and remix, including **48 short
  tutorials** that each teach one idea, with a note on the canvas.
- **Classic generative techniques built in:** flow fields, Voronoi and
  Delaunay cells, Truchet and wave-function-collapse tiles, L-systems,
  wallpaper symmetry and kaleidoscopes, stippling, flocking, slime mould,
  reaction-diffusion, cloth and more.
- **Shaders when you want them:** a ShaderToy-style Shader node with buffer
  passes and its own editor panel, and an Effect node with bloom, blur,
  grain, halftone, dither, ASCII art, LUTs and more.

**Work live**

- **A live GPU preview.** Every Renderer in a patch gets its own preview
  panel, with a fullscreen output for a second display.
- **Always-editable values.** Every unwired value is a box you can scrub or
  type into while the patch runs. Everything can be undone.
- **Patches read left to right.** Wires only run forward, so a patch reads
  like a sentence. Memory across frames uses **Read / Write pairs**: place a
  Write and its Read comes with it. The editor refuses a wire that points
  backwards.
- **Stay organised.** Wrap a selection in a ForEach, If, Cache, Section or
  **subpatch** from the right-click menu. The Subpatches panel lists them,
  and you can save a subpatch to the Library and reuse it in any patch.
- **Bypass** any node to compare with and without it.
- **Presets and variations.** Roll a new variation (Cmd+Shift+R), save the
  ones you like as presets, and morph between them.
- **A layout that is yours.** Every panel docks, splits, stacks or floats.
  Light and dark themes.

**Share**

- **Export** a still as PNG (up to 4x), or a seamless loop as **MP4**
  (with the patch's sound, on a Mac), **GIF** or a **PNG sequence**.
- **Export Variations** renders a dozen (or a few hundred) seeds or presets
  in one go, with a contact sheet to pick favourites from.
- **Sound, MIDI, camera and body tracking** (hands, face, pose) work as
  inputs. Camera and microphone data stay on your computer.

### The 252 nodes, by area

| Area | Nodes | What is in it |
|---|---|---|
| Source | 16 | Numbers, strings, sliders, toggles, XY pads, files, images, SVG, video, CSV |
| Draw | 14 | Rect, Ellipse, Line, Path, Polygon, Image, plots, and the Renderer |
| Text | 13 | Text, glyphs, text on a path, outlines, variable fonts |
| Color & Style | 17 | Colours, gradients, palettes, fills and strokes |
| Shape Ops | 16 | Transforms, groups, masks, blends, symmetry, path and shape booleans |
| Patterns | 25 | Grids, spirals, scatter and stippling, Voronoi, contours and flow lines, tiles, L-systems, curves |
| Noise & Random | 7 | Perlin, simplex and more; uniform, Gaussian, wanderers, shuffles |
| Animate | 18 | Time, LFOs, easing, springs, envelopes, triggers, counters, delays |
| Image | 14 | Rasterize, crop, resize, sample, blend, pixel get and set |
| Effects | 9 | Effect, Shader, filters, convolve, pixelate, pixel sort |
| Particles & Physics | 18 | Emitters, simulations, forces, colliders, trails |
| Input & Sound | 28 | Mouse, keyboard, MIDI, camera, landmark tracking, audio in, analysis, synthesis |
| Math | 23 | Maths, vectors, compare, logic, statistics, mapping, conversion |
| Lists | 17 | Lookup, sublist, reorder, combine, repeat, merge, partition |
| Structure | 17 | ForEach, If and Cache regions, Read / Write, subpatches, sections, comments |

## A look inside

| | |
|---|---|
| ![The Weather Map sample: a node graph of subpatches, a ForEach region and Vector Math nodes on the left, and on the right the live preview of a pressure map with coloured bands, isobars, wind flow lines and arrows](docs/images/floret-system.png) | ![The Flock Herder sample: the Simulation, Palette, Connect Points and subpatch nodes on the left, and on the right the live preview of colourful flocks of boids with trails](docs/images/floret-particles.png) |
| **A complex system: Weather Map.** 149 nodes with subpatches and ForEach regions, wired into a living pressure map: bands, isobars, wind flow lines and arrows. | **A particles sample: Flock Herder.** A Simulation node steers 320 boids, Connect Points draws the lines between neighbours, and everything moves live in the preview. |
| ![The Library: every sample by category, with search](docs/images/floret-library.png) | ![Floret with several panels docked: the Generative Poster sample with the canvas, Library, live preview and Presets](docs/images/floret-panels.png) |
| **The Library.** 175 samples by category, next to your own saved patches. | **Panels dock, split, stack and float** wherever you like. View → Reset Layout puts them back. |

## Download

Get Floret from the **[Releases page](https://github.com/jaerasgon/Floret/releases)**.
All three builds are beta. The Mac build is the most tested; the Windows and
Linux builds are **experimental** and less tested.

| Platform | Notes |
|---|---|
| **macOS** 11 or later (Apple Silicon and Intel) | The app isn't signed by Apple, so the first launch needs **right-click, then Open**. Or run `xattr -dr com.apple.quarantine /Applications/Floret.app` in Terminal. MP4 export with sound works here. |
| **Windows** (experimental) | Not signed, so SmartScreen may warn. Choose **More info**, then **Run anyway**. |
| **Linux** (experimental) | Needs ALSA (`libasound2`). Make the download executable (`chmod +x floret`) and run it. |

The first time a patch uses the microphone or camera, your system asks for
permission. Video and sound stay on your computer.

### Build it yourself

**On a Mac**, paste this one line into Terminal. It finds your copy of
Floret (or clones it into `~/github/Floret`), updates it and installs
**Floret.app** into Applications. The first run installs Rust and builds
Floret, which takes several minutes; run the same line again whenever you
want to update.

```sh
F=$(find ~ -maxdepth 4 -name floret.sh -path "*Floret*" 2>/dev/null | head -1); if [ -z "$F" ]; then mkdir -p ~/github && git clone https://github.com/jaerasgon/Floret.git ~/github/Floret && F=~/github/Floret/floret.sh; fi; cd "$(dirname "$F")" && git fetch origin && git checkout main && git pull && ./floret.sh install
```

If Apple's command line tools aren't installed yet, a dialog offers them:
install them, then run the line again.

**On any system**, with the stable [Rust](https://rustup.rs) toolchain
(and `libasound2-dev` on Linux):

```sh
git clone https://github.com/jaerasgon/Floret.git
cd Floret/native
cargo build --release -p floret-app
```

The app is `native/target/release/floret`. `./floret.sh` in the repository
root builds and runs it in one step, on a Mac or on Linux.

## Your first patch in five minutes

1. **New patch.** File → New (Cmd+N). The canvas is empty.
2. **Add a Renderer.** Over the canvas, press **Tab**, type `renderer` and
   press Return. Every patch draws through a Renderer; its picture shows in
   the **Preview** panel.
3. **Add a shape.** Tab, `ellipse`, Return.
4. **Wire it.** Drag from the Ellipse's output pin (`layer`, right edge) to
   the Renderer's `layer` pin (left edge). A white circle appears.
5. **Change values.** Click the Ellipse's `w` box, type `400`, press
   Return. Dragging sideways on a box scrubs the number.
6. **Animate it.** Add an **LFO** (Tab, `lfo`), set `min` to `100` and `max`
   to `600`, and wire its `value` into the Ellipse's `w` and `h`.
7. **Colour it.** Add a **Fill**, pick a colour, and wire its `style` into the
   Ellipse's `style`.
8. **Save and export.** Cmd+S saves a `.flora` file. Cmd+E exports the
   current frame as a PNG; Cmd+Shift+E records a loop.

Made a mess? Cmd+Z undoes anything. Press **F** to fit the patch in view,
and `?` over the canvas for the keyboard shortcuts. Select a node to read
what it does in the **Inspector**. The tutorials ("01 · Hello Rect" and on)
are in the Library (File → Samples Library, Cmd+Shift+O).

## Feedback and help

**Floret is early and your feedback shapes it.** The most useful thing you
can do is tell us what happens when you use it.

- **Open an issue:** a [bug report](https://github.com/jaerasgon/Floret/issues/new?template=bug_report.yml)
  or an [idea or feedback](https://github.com/jaerasgon/Floret/issues/new?template=feedback.yml).
- **Email:** [Floret.make@gmail.com](mailto:Floret.make@gmail.com), if you
  would rather not use GitHub.
- **Send patches and samples that don't work**, or that surprised you. A
  `.flora` file and a sentence about what you expected is perfect.
- **Tell us what you were trying to make.** That shows us what is missing.

Please send ideas, bugs and requests here rather than forking, so Floret
improves for everyone. [CONTRIBUTING.md](CONTRIBUTING.md) has what makes a
useful report and how to send a pull request. Security problems go by email,
privately: see [SECURITY.md](SECURITY.md).

## Support Floret

Floret is free and open source, and it will stay that way. If it's useful to
you and you'd like to help me keep building it, you can leave a tip on
[Ko-fi](https://ko-fi.com/jaerasgon). Feedback matters just as much as
money, so telling me what works and what doesn't is a great way to help too.

## Licence

Floret is free and open source under the **[Apache License 2.0](LICENSE)**:
use it, change it and build on it. If you rebuild Floret, or parts of it,
into another tool, keep the [NOTICE](NOTICE) file with it and credit Floret
and its author, Jaafer Elsadig.

Floret includes open-source software, fonts and tracking models under their
own licences, listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and
in the app under Help → Licenses….

## Project layout

| Path | What it is |
|---|---|
| `native/` | **The app.** A Rust workspace (`native/Cargo.toml`). |
| `native/crates/floret-app` | The desktop app: window, node editor, panels, menus, files, export (egui + wgpu). |
| `native/crates/floret-engine` | The dataflow engine: loads and compiles patches, runs the nodes each frame. No GPU, UI or OS code. |
| `native/crates/floret-render` | The GPU renderer: turns the engine's scenes into pixels (wgpu). |
| `native/crates/floret-media` | The real-time audio engine behind the sound nodes. |
| `native/crates/floret-input` | MIDI, cameras, and hand / face / pose tracking. |
| `native/crates/floret-video` | Video playback and MP4 / GIF export. |
| `native/crates/floret-open-files` | Opening `.flora` files from Finder (macOS). |
| `native/samples/` | The built-in samples and their catalogue. |
| `floret.sh` | Build, run and install script. |
| `docs/` | The beta tester guide, the Mac QA checklist, the release notes draft and the log of new nodes. `docs/archive/` keeps old planning notes for history. |
| `src/`, `package.json`, … | The earlier TypeScript web version, kept for reference only. It isn't the app anymore. |

To check your changes, from `native/`: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings` and
`cargo test --workspace`.

## Docs

- [NODES.md](NODES.md): every node, with its inputs, outputs and what it
  does. The Inspector and Help panels in the app show the same.
- [docs/beta-testing.md](docs/beta-testing.md): installing, what to try, and
  how to report a problem.
- [docs/qa-checklist-mac.md](docs/qa-checklist-mac.md): the longer checklist
  used to test the Mac build.
- [CHANGELOG.md](CHANGELOG.md): what changed, release by release.
- [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md) and
  [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
- Keyboard shortcuts: `?` over the canvas, or Help → Keyboard Shortcuts
  (Cmd+/).
