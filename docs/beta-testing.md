# Floret beta testing guide

Thank you for trying Floret. It is free and open source (Apache License
2.0), and it is early: your feedback shapes it.

## What Floret is

Floret is a node-based app for making generative art. You build a picture
by wiring small boxes (nodes) together on a canvas: shapes, motion, noise,
colour, text, particles, images, sound, MIDI and the camera. The preview
updates live as you edit, and you can save still frames or seamless video
loops. Patches are plain `.flora` files.

## What a beta tester does

Use Floret the way you would for real, try to break it, and tell us what
you find. You don't need to read any code. The most useful reports come
from things that feel wrong, confusing or slow, as well as from outright
errors.

## Install

Pick whichever suits you. All three builds are beta; the Mac build is the
most tested.

### Download (Mac, Windows, Linux)

Get the build for your computer from the
[Releases page](https://github.com/jaerasgon/Floret/releases).

- **macOS:** the app isn't signed by Apple, so the first launch needs
  right-click → **Open** (then Open again), or in Terminal:
  `xattr -dr com.apple.quarantine /Applications/Floret.app`.
- **Windows (experimental):** the app isn't signed, so SmartScreen may
  warn. Choose **More info** → **Run anyway**.
- **Linux (experimental):** needs ALSA (`libasound2`). Make the file
  executable (`chmod +x floret`) and run it.

The Windows and Linux builds are less tested than the Mac one.

### Build on your Mac

You need a Mac with **macOS 11 or later** (Apple Silicon or Intel). Floret
builds on your Mac from its source code. Open Terminal and paste this one
line:

```
F=$(find ~ -maxdepth 4 -name floret.sh -path "*Floret*" 2>/dev/null | head -1); if [ -z "$F" ]; then mkdir -p ~/github && git clone https://github.com/jaerasgon/Floret.git ~/github/Floret && F=~/github/Floret/floret.sh; fi; cd "$(dirname "$F")" && git fetch origin && git checkout main && git pull && ./floret.sh install
```

It finds your copy of Floret (or downloads one into `~/github/Floret`),
updates it and installs.

- If Apple's **command line tools** aren't installed yet, macOS shows a
  dialog offering them. Install them, then paste the line again.
- The **first run takes several minutes**: it installs Rust (for your user
  only) and builds Floret. Later runs only rebuild what changed.
- When it finishes, **Floret.app** is in your Applications folder and
  opens.

To update to the newest version, paste the same line again.

### Build from source (any system)

Install the stable [Rust](https://rustup.rs) toolchain (on Linux also
`libasound2-dev`), then:

```
cd native
cargo build --release -p floret-app
```

The app is `native/target/release/floret`.

## What to try

Work through this list, and add anything else you think of:

- [ ] Open the Library (File → Samples Library, Cmd+Shift+O). Browse the
      categories and open a few samples. Try the tile zoom (Cmd + scroll,
      or pinch).
- [ ] Play with the first ten **Tutorial** samples ("01 · Hello Rect"
      onward). Each introduces one idea, with a note on the canvas. Drag
      the values, change the wiring, see if the notes make sense.
- [ ] Build a small patch of your own: press **Tab** over the canvas to
      add nodes, drag from pin to pin to wire them, and add a Renderer so
      something shows in the Preview. Press `?` for the keyboard shortcuts.
- [ ] Save it (Cmd+S), quit Floret (Cmd+Q), open it again (File → Open
      Recent) and check that it's exactly as you left it.
- [ ] Export a still: **Cmd+E** (PNG). Export a loop: **File → Export
      Video / GIF…** (Cmd+Shift+E) as an MP4, and play the file in
      QuickTime.
- [ ] If you have a **camera**, open **Webcam Mosaic** or **Body
      Tracking** and click the toggle. macOS asks for camera permission.
- [ ] If you have a **microphone**, open **28 · Audio-Reactive Visual**.
      macOS asks for microphone permission.
- [ ] Dock and float panels: drag a panel's tab to another place and watch
      where it will land; hold **⌥ (Option)** as you let go to float it as
      a window. View → Reset Layout puts everything back.
- [ ] Try the Light and Dark themes (View → Theme), and Cmd+= / Cmd+- to
      change the interface size.
- [ ] Quit without saving some changes (or force-quit), then open Floret
      again: it should offer to restore your work.

The longer checklist the developers use is in
[qa-checklist-mac.md](qa-checklist-mac.md) if you want to go deeper.

## Where your files and settings live

Floret keeps its own files in
`~/Library/Application Support/Floret`
(in Finder: Go → Go to Folder…, then paste that path). Your patches are
wherever you saved them. Inside that folder:

| What | Where |
|---|---|
| Preferences | `prefs.json` |
| Panel layout | `dock.json` |
| Favorite nodes | `favorites.json` |
| Recent files | `recent.json` |
| Your saved patches and subpatches ("My patches" in the Library) | the `library` folder |
| Libraries you imported | `library/libraries` |
| Autosaved and crash-recovery copies of unsaved work | the `recovery` folder |
| Sample thumbnails | the `thumbnails` folder |

Deleting `dock.json` resets the panel layout, and deleting `prefs.json`
resets the settings. Both are rebuilt next time you start Floret. Quit
Floret first.

## How to report a problem

Open an issue with the
[bug report form](https://github.com/jaerasgon/Floret/issues/new?template=bug_report.yml)
(or the
[feedback form](https://github.com/jaerasgon/Floret/issues/new?template=feedback.yml)
for ideas and how it felt to use), or email
[Floret.make@gmail.com](mailto:Floret.make@gmail.com) if you'd rather not
use GitHub. Please send ideas, bugs and requests there rather than
forking, so Floret improves for everyone. See
[CONTRIBUTING.md](../CONTRIBUTING.md) for more.

Please include:

1. **Which sample or patch** you were using (its name in the Library, or
   the `.flora` file if it's your own: attach it if you can).
2. **What you did**, step by step, as exactly as you can.
3. **What happened**, and what you expected instead.
4. **A screenshot** (Cmd+Shift+4, or Cmd+Shift+5 to record the screen).
5. **The Floret version**, shown in the About panel (Floret → About
   Floret) and in the bottom bar of the window (for example `v0.2.0`). It
   also helps to mention your Mac model and macOS version.

If Floret shows a "Something went wrong" window, press **Copy details**
and paste it into your report; **Save patch as…** keeps a copy of your
work.

## Known limitations

- The app is **not signed or notarised** by Apple. A build you make
  yourself runs on your Mac; a downloaded copy needs the right-click →
  Open step above. macOS may ask for camera or microphone permission again
  after an update.
- The **Windows and Linux builds are experimental**: less tested than the
  Mac build, and MP4 export with sound is Mac-only for now.
- Patches from the earlier web version open as they were, except that a
  Save SVG node is left out (a notice says so). Use Export PNG to save
  pictures.
- A few samples (the image tutorials such as 37 to 40, Photo Mosaic,
  Kaleidoscope, Pixel Sort, Datamosh and Palette From Image) load a photo
  from the internet. Offline they have no picture to work with.
- With several Renderers in one patch, the canvas size that the patch
  itself sees (the mouse coordinates, for example) comes from the first
  one.
- Heavy samples (lots of particles, shaders, large images) run slower on
  older Macs. Say which sample and which Mac when you report it.
