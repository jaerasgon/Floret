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

Use Floret the way you would for real, try to break it, and tell me what
you find. You don't need to read any code. The most useful reports come
from things that feel wrong, confusing or slow, as well as from outright
errors.

## Install

Floret is in beta on every system. The Mac build is the most tested.

Get the build for your computer from the
[Releases page](https://github.com/jaerasgon/Floret/releases). The apps
aren't signed, so each system asks you to confirm the first launch:

- **Mac** (macOS 11 or later, Apple silicon or Intel): open the `.dmg` and
  drag Floret into Applications (or unzip the `.zip`). When you open it,
  macOS says it can't verify the app. Click Done, then open
  **System Settings → Privacy & Security**, scroll down and click
  **Open Anyway** next to Floret. (On macOS 14 and earlier, right-click
  Floret, choose Open, then Open again.) If you'd rather use Terminal, this
  removes the download mark instead:
  `xattr -dr com.apple.quarantine /Applications/Floret.app`
- **Windows** (experimental): unzip the whole folder and run `floret.exe`.
  If a blue "Windows protected your PC" window appears, choose
  **More info** → **Run anyway**.
- **Linux** (experimental): unpack the archive and run `./floret`. You need
  the ALSA sound library (on Ubuntu: `sudo apt install libasound2`) and a
  graphics driver with Vulkan or OpenGL.

The Windows and Linux builds are less tested than the Mac one. If you'd
rather build Floret from its source code, see [building.md](building.md).

### On Windows and Linux

The shortcuts in this guide use the Mac keys. On Windows and Linux, use
**Ctrl** where it says Cmd and **Alt** where it says Option. The Help menu
(with About Floret) is in the menu bar at the top of the window.

## What to try

Work through this list, and add anything else you think of:

- [ ] Open the Library (File → Samples Library, Cmd+Shift+O). Browse the
      categories and open a few samples. Try the tile zoom (Cmd + scroll,
      or pinch).
- [ ] Play with the **Tutorial** samples, in order ("01 · Hello Rect"
      onward). Each introduces one idea, with a note on the canvas. Drag
      the values, change the wiring, see if the notes make sense.
- [ ] Build a small patch of your own: press **Tab** over the canvas to
      add nodes, drag from pin to pin to wire them, and add a Renderer so
      something shows in the Preview. Press `?` for the keyboard shortcuts.
- [ ] Save it (Cmd+S), quit Floret (Cmd+Q), open it again (File → Open
      Recent) and check that it's exactly as you left it.
- [ ] Export a still: **Cmd+E** (PNG). Export a loop: **File → Export
      Video / GIF…** (Cmd+Shift+E) as an MP4, and play the file in
      any video player.
- [ ] If you have a **camera**, open **Webcam Mosaic** or **Body
      Tracking** and click the toggle. Your system may ask for camera
      permission.
- [ ] If you have a **microphone**, open **28 · Audio-Reactive Visual**.
      Your system may ask for microphone permission.
- [ ] Dock and float panels: drag a panel's tab to another place and watch
      where it will land; hold **⌥ (Option)** as you let go to float it as
      a window. View → Reset Layout puts everything back.
- [ ] Try the Light and Dark themes (View → Theme), and Cmd+= / Cmd+- to
      change the interface size.
- [ ] Quit without saving some changes (or force-quit), then open Floret
      again: it should offer to restore your work.

The longer checklist I use is in
[qa-checklist-mac.md](qa-checklist-mac.md) if you want to go deeper.

## Where your files and settings live

Your patches are wherever you saved them. Floret keeps its own files in
these folders:

| System | Folder |
|---|---|
| Mac | `~/Library/Application Support/Floret` (in Finder: Go → Go to Folder…, then paste that path) |
| Windows | `%APPDATA%\Floret` for settings, layout, favourites, recent files and your Library, and `%LOCALAPPDATA%\Floret` for recovery copies and thumbnails (paste either into the File Explorer address bar) |
| Linux | `~/.config/floret` for settings, layout, favourites, recent files and your Library, and `~/.local/share/floret` for recovery copies and thumbnails |

On a Mac everything is in the one folder. What is in them:

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

Use the
[bug report form](https://github.com/jaerasgon/Floret/issues/new?template=bug_report.yml)
(or the
[feedback form](https://github.com/jaerasgon/Floret/issues/new?template=feedback.yml)
for ideas and how it felt to use), or email
[Floret.make@gmail.com](mailto:Floret.make@gmail.com) if you'd rather not
use GitHub. The form asks for what I need, in plain words. Ideas and bug
reports help most when they come to the project rather than to a fork; see
[CONTRIBUTING.md](../CONTRIBUTING.md) for more.

If Floret shows a "Something went wrong" window, press **Copy details**
and paste it into your report; **Save patch as…** keeps a copy of your
work.

## Known limitations

- The app is **not signed or notarised** by Apple. A build you make
  yourself runs on your Mac; a downloaded copy needs the Open Anyway step
  above. macOS may ask for camera or microphone permission again
  after an update.
- The **Windows and Linux builds are experimental**: less tested than the
  Mac build, and MP4 export with sound is Mac-only for now.
- Older Floret patches (from the earlier web version) open as they were,
  except that a Save SVG node is left out (a notice says so). Use Export
  PNG to save pictures.
- A few samples (the image tutorials such as 37 to 40, Photo Mosaic,
  Kaleidoscope, Pixel Sort, Datamosh and Palette From Image) load a photo
  from the internet. Offline they have no picture to work with.
- With several Renderers in one patch, the canvas size that the patch
  itself sees (the mouse coordinates, for example) comes from the first
  one.
- Heavy samples (lots of particles, shaders, large images) run slower on
  older computers. Say which sample and which computer when you report it.
