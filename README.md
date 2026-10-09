# Floret

Floret is a node-based app for making generative art. You build a picture
by wiring small boxes together on a canvas: shapes, motion, noise, colour,
text, particles, images, sound, MIDI and the camera. The preview updates
live as you edit, so you can play with a patch while it runs, then save
still frames or seamless video loops. Patches are plain `.flora` files.

![Floret with the Slime Mould sample open: the patch on the node canvas, its live preview on the right](docs/images/floret-window.png)

Floret is a native desktop app written in Rust and rendered on the GPU.
For now it's released for macOS.

## Install on a Mac

You need macOS 11 or later (Apple Silicon or Intel). Floret builds on your
Mac from this repository.

1. **Get the code.** Either:
   - in Terminal: `git clone https://github.com/jaerasgon/Floret.git`, or
   - in [GitHub Desktop](https://desktop.github.com): File → Clone
     Repository, and pick `jaerasgon/Floret`.
2. **Install.** Open Terminal in the `Floret` folder (in GitHub Desktop:
   Repository → Open in Terminal; after `git clone`: `cd Floret`) and run
   the installer:

   ```sh
   ./floret.sh install
   ```

   If Apple's command line tools aren't installed yet, a dialog offers
   them: install them, then run `./floret.sh install` again. The first run
   also installs Rust (for your user only) and builds Floret, which takes
   several minutes. Then **Floret.app** is in your Applications folder and
   opens.
3. **Update** whenever you like by running `./floret.sh install` again. It
   pulls the latest version and only rebuilds what changed.

Floret isn't signed by Apple, so the app runs on the Mac that built it but
not on a copy sent to another Mac. The first time a patch uses the
microphone or camera, macOS asks for permission; video and sound stay on
your computer.

**Developer runs:** `./floret.sh` pulls, builds and starts Floret straight
from the repository without installing it (`./floret.sh --no-pull` skips
the pull). This also works on Linux.

## Your first patch in five minutes

1. **New patch.** Choose File → New (Cmd+N). The canvas is empty.
2. **Add a Renderer.** Move the pointer over the canvas, press **Tab**,
   type `renderer` and press Return. Every patch draws through a Renderer:
   it is the canvas (1080 × 1920 to start with), and its picture shows in
   the **Preview** panel on the right. Double-clicking the canvas also
   opens the search.
3. **Add a shape.** Press Tab again, type `ellipse`, press Return.
4. **Wire it.** Drag from the Ellipse's output pin (`layer`, on its right
   edge) to the Renderer's `layers` pin (on its left edge). A small white
   circle appears in the middle of the Preview. Outputs are always on the
   right of a node, inputs on the left.
5. **Change values.** Click the Ellipse's `w` row: an edit box opens on
   it. Click the box, type `400` and press Return; the circle stretches
   into an oval. (Dragging sideways in the box scrubs the number.)
6. **Animate it.** Add an **LFO** (Tab, `lfo`). Open its `min` row the
   same way and set it to `100`, and `max` to `600`. Then wire the LFO's
   `value` pin into the Ellipse's `w`, and again into its `h`. The circle
   now grows and shrinks; the LFO's `rate` sets how many times a second.
7. **Colour it.** Add a **Fill** (Tab, `fill`). Click its `color` row,
   then the colour swatch, pick a colour, and click the canvas to close the
   picker (Esc cancels). Wire Fill's `style` pin into the Ellipse's
   `style`.
8. **Save and export.** File → Save (Cmd+S) writes a `.flora` file.
   Cmd+E exports the current frame as a PNG, and File → Export Video /
   GIF… (Cmd+Shift+E) records a seamless loop as MP4, GIF or PNG frames.

Made a mess? Cmd+Z undoes anything. Press **F** to fit the whole patch in
view. Select a node to read what it does in the **Inspector**.

## Samples and tutorials

Floret ships with 127 samples, including 48 short tutorials ("01 · Hello
Rect", "02 · Colour & Fill", …) that each introduce one idea, with a note
on the canvas explaining it. Open them from:

- the **Welcome** screen that shows when you start with an empty canvas
  (or Help → Welcome),
- **File → Samples Library** (Cmd+Shift+O), which lists every sample by
  category next to your own saved patches, with search.

`.flora` files open from Finder too: double-click one, or use Open With →
Floret.

## Reference

- **Nodes:** [NODES.md](NODES.md) lists every node with its inputs, outputs
  and what it does. In the app, the Inspector and Help panels show the
  same for the selected node.
- **Keyboard shortcuts:** press `?` over the canvas for an overlay, or
  open Help → Keyboard Shortcuts (Cmd+/).
- **Changes:** [CHANGELOG.md](CHANGELOG.md).

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
| `docs/` | Design notes, the Mac QA checklist and release notes drafts. |
| `src/`, `package.json`, … | The earlier TypeScript web version. It's kept for reference only and isn't the app anymore. |

To check your changes, from `native/`: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings` and
`cargo test --workspace`.

## Licence

Floret is proprietary: see [LICENSE](LICENSE). It uses open-source
components under their own licences, listed in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
