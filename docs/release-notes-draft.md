# Floret 0.2.0

*The first release of the native app. The full list is the 0.2.0 entry in
`CHANGELOG.md`.*

Floret is a node-based tool for generative art: wire shapes, motion,
noise, particles, text, images, sound and live input together, and watch
the result change as you edit. This is the first release of Floret as a
native Mac app.

## What's in it

- **A fast node editor.** Press Tab or double-click to add a node and drag
  between pins to wire. Every value on a node is a live box: drag it to
  scrub, click it to type. Inputs fill the node; outputs sit in a slim
  footer underneath. Everything can be undone. Dragging a node gently
  pulls its neighbours along.
- **Patches read left to right.** Wires only run forward; memory across
  frames goes through named Write → Read pairs (placing a Write places its
  Read, naming one names both). The editor won't connect a wire backwards.
- **Organise as you go:** right-click a selection to wrap it in a ForEach,
  If, Cache, a labelled Section, or a subpatch. The Subpatches panel lists
  the subpatches where you are; save one to the Library and drop it into
  any other patch.
- **Live GPU preview:** every Renderer in your patch gets its own preview
  panel, with focus mode and a fullscreen output window for a second
  display. Panels dock, split and float; View → Panels brings back any you
  closed.
- **252 nodes**: shapes and paths, typography with variable fonts,
  noise and formulas, palettes, particles and simulations (flocking, slime
  mould, coral growth, cloth, differential growth, reaction-diffusion),
  shaders and post-effects (including ASCII art), images, video and image
  sequences, point tracking, sound, MIDI, camera and body tracking.
- **Classic generative techniques built in:** flow-field lines, Mondrian
  subdivision, Truchet and wave-function-collapse tiles, wallpaper
  symmetry, stippling, plexus lines, contour maps, layouts that animate.
- **Bypass** any node to compare with and without it (B).
- **175 samples**, including 48 short tutorials, in the Library
  (File → Samples Library, Cmd+Shift+O), all built from visual nodes. The
  Library's tiles can be zoomed.
- **Variations and presets:** roll a new variation with Cmd+Shift+R, save
  the ones you like as presets, and morph between them.
- **Export** a still as PNG (up to 4×) or a seamless loop as MP4, GIF or a
  PNG sequence. MP4s include the patch's sound.
- **Export Variations** renders a dozen (or a few hundred) seeds or presets
  in one go, with a contact sheet to pick favourites from; click one to
  bring it back into the patch.
- **Real files:** patches are `.flora` files. Double-click one in Finder to
  open it in Floret. Unsaved work is autosaved and offered back after a
  crash. Patches from the earlier web version open as they are.
- Light and dark themes, or follow macOS.

## Install

You need a Mac with macOS 11 or later. Floret is built on your Mac from its
source code:

1. Get the repository: `git clone https://github.com/jaerasgon/Floret.git`
   in Terminal, or File → Clone Repository in GitHub Desktop.
2. In Terminal, go into the `Floret` folder and install:

   ```
   cd Floret
   ./floret.sh install
   ```

If Apple's command line tools aren't installed, a dialog offers them:
install them and run the command again. The first run then installs Rust
and builds Floret, which takes several minutes. Floret.app then appears in
Applications and opens.
Run `./floret.sh install` again at any time to update.

## Known limitations

- The app isn't signed or notarised, so it can only run on the Mac that
  built it. macOS may ask again for camera or microphone access after an
  update.
- Mac only for now. Windows and Linux versions come later.
- Patches from the web version open as they were, except that a Save SVG
  node is left out (a notice says so); save pictures with Export PNG.

Something not working? Note the sample or patch, what you did and what
happened, and send it along with a screenshot.
