# Floret on the Mac: manual QA checklist

A pass over the native app before a release. Each line is something to do
and what should happen. Tick it, or note what went wrong next to it (with
the sample name and a screenshot if you can).

Automated checks already cover the basics on every build (see
`native/crates/floret-engine/tests/all_samples.rs` and
`native/crates/floret-app/src/sample_sweep.rs`): every Library sample loads,
compiles without warnings, runs, draws, renders on the GPU and saves and
reloads unchanged. This list is for what only a person on a real Mac can
check: hardware, the system, look and feel, and speed.

Before you start: quit Floret, then in Terminal run `./floret.sh install`
from the repo folder.

## Install and launch

- [ ] `./floret.sh install` finishes with "✓ Installed /Applications/Floret.app" and opens Floret.
- [ ] Floret shows in Launchpad and Spotlight with the Floret icon, and Finder's Get Info shows the version and build.
- [ ] Quit (Cmd+Q) and open Floret from the Dock or Launchpad: it opens with no warning about the app being damaged.
- [ ] The first launch with no patch open shows the Welcome screen (unless it's turned off in Preferences). Its tutorial tiles get thumbnails within a few seconds.
- [ ] Help → About Floret (in the Floret menu) shows the version and the GPU in use (for example "Apple M1 · Metal").
- [ ] Preferences (Cmd+,): change "On startup, open" to Starter sample, quit and reopen. Grid Ripple opens.
- [ ] Make a change without saving, then force-quit Floret from Activity Monitor and reopen it. Floret offers to restore the unsaved work, and Restore brings it back.

## Double-click a .flora file

- [ ] In Finder, a `.flora` file shows as a "Floret Patch" document (Get Info → Kind) with Floret as its "Open with" app.
- [ ] Floret not running: double-click a `.flora` file. Floret launches and opens that patch, not the Welcome screen or the startup patch. The window title shows the file name.
- [ ] Floret running with no unsaved changes: double-click another `.flora`. Floret comes to the front and opens it.
- [ ] Floret running with unsaved changes (title starts with `*`): double-click a `.flora`. You're asked "Save changes to …?". Cancel keeps the current patch. Don't Save opens the new file. Save… saves first, then opens it.
- [ ] Floret minimised to the Dock: double-click a `.flora`. The window comes back and shows the file.
- [ ] Right-click a `.flora` → Open With → Floret works the same way.
- [ ] Drag a `.flora` onto Floret's Dock icon: it opens.
- [ ] Double-click a `.flora` file that isn't valid JSON. Floret stays open and the status bar says "Couldn't open …". It doesn't crash.
- [ ] Terminal: `/Applications/Floret.app/Contents/MacOS/floret ~/path/to/patch.flora` opens that patch.
- [ ] File → Open Recent lists the files opened by double-clicking.

## Canvas editing

- [ ] File → New (Cmd+N) gives an empty canvas with the hint "Double-click or press Tab to add a node".
- [ ] Press Tab over the canvas, type `rect` and press Enter. A Rect node appears at the pointer.
- [ ] Add a Renderer (Tab, `renderer`). Drag from Rect's output pin (`layer`) to the Renderer's `layers` input: a wire appears and the square shows in the Preview.
- [ ] Drag a wire from a pin and drop it on empty canvas: the node search opens, and the node you pick comes with the wire already connected.
- [ ] Double-click a wire and pick a node: it is inserted into the wire.
- [ ] Every unwired value on a node is a value box (name left, value right; for example Rect's `w`): drag a value to scrub, click to type and press Return. The preview follows. A wired input shows just its name.
- [ ] A dropdown box opens its menu on click; a checkbox toggles when you click it or its name.
- [ ] A colour box opens a colour picker: pick a colour and click the canvas to keep it; Esc puts the old colour back.
- [ ] Select several nodes with a drag box and with Shift+click. Then Cut, Copy, Paste, Duplicate (Cmd+X / C / V / D) and Delete (Backspace).
- [ ] Undo and redo every step above (Cmd+Z / Cmd+Shift+Z).
- [ ] Alt+drag a node: a copy is dragged out.
- [ ] **Node gravity:** drag a node that has wires. The nodes wired to it trail along a little, and nearby ones less, and everything settles where it was pulled when you let go. One Cmd+Z puts them all back. Turn off Preferences → Node gravity: only the dragged node moves.
- [ ] Snap to grid on: nodes snap while dragging.
- [ ] Select some nodes and press Cmd+G: they become a subpatch. I enters it, O exits it.
- [ ] Pan with two fingers and zoom with a pinch on the trackpad. Pan with a Space-drag. Press F to fit the view.
- [ ] The minimap in the corner shows where you are. Dragging in it pans and scrolling in it zooms.
- [ ] Right-click a node, a wire and the empty canvas: each shows its context menu.
- [ ] Press `?`: the shortcuts overlay shows. Esc closes it.

## Nodes browser and search

- [ ] Cmd+\ shows and hides the Nodes panel. Its groups (Source, Draw, …) open and close.
- [ ] Typing in the Nodes panel's search filters the list. Dragging a node from it places it where you drop it, and a click adds it mid-canvas.
- [ ] Tab search finds nodes by name (`ellipse`), by type id (`draw/ellipse`) and by old category name (`Style`).
- [ ] Hovering over a node in the Nodes panel shows its description. Selecting a node on the canvas shows its docs in the Help tab.
- [ ] Press Q over the canvas: the Favorites menu opens, and 1–9 place a favourite.

## Preview and export

- [ ] Open `grid-ripple`. The Preview animates smoothly. Space over the preview pauses and resumes. Cmd+R restarts.
- [ ] Zoom and pan inside the preview, use "Actual pixels" and "Fit in view", and lock the view.
- [ ] Focus mode shows the preview alone, and Esc leaves it.
- [ ] With a second display connected, Output → Fullscreen on display sends the preview there, and Esc closes it.
- [ ] Export PNG (Cmd+E) at 1×, 2× and 4×: the files open in Preview.app at the right sizes and match what's on screen.
- [ ] File → Export Video / GIF… (Cmd+Shift+E) with `loop-bloom`:
  - [ ] MP4: plays in QuickTime and Safari, colours match the preview, and it loops without a jump.
  - [ ] GIF: plays in a browser with sensible colours.
  - [ ] PNG sequence: a folder of numbered frames.
  - [ ] Cancel during an export stops it and leaves no broken file.
  - [ ] Export over an existing file replaces it.
- [ ] Export Video with `fft-bloom` (a patch with sound) as MP4: the Export window shows **Include sound**, ticked.
  - [ ] The MP4 plays with the patch's sound in QuickTime and Safari, in sync with the pictures (the bloom pulses with the sound), and the sound track is exactly as long as the video (QuickTime → Window → Show Movie Inspector).
  - [ ] Export it twice: both files sound the same.
  - [ ] Untick Include sound: the MP4 has no sound track.
  - [ ] With `tone-surface` (1 loop), loop the MP4 in QuickTime (View → Loop): the sound repeats without a click if the patch's sound repeats over the loop.
  - [ ] Export while the app's volume is muted: the file still has sound (exports don't follow the volume control).
- [ ] File → Export Variations… with `variations` (also in each preview's Export menu as "Variations…"):
  - [ ] Seed → Random, 12, Still at 2 s, Contact sheet on: choose a folder. A new `variations-variations` folder appears with 12 PNGs named `variations-seed-<n>.png` and `variations-contact-sheet.png`; Export again makes `…-variations-2`.
  - [ ] The contact sheet shows every variation in a grid with readable "seed <n>" labels, and each matches its PNG.
  - [ ] Turn on Presets as well (Calm + Storm): 24 files named `variations-calm-seed-<n>.png` / `variations-storm-seed-<n>.png`; labels read "Calm · seed <n>".
  - [ ] Loop (GIF): one GIF per variation, each looping.
  - [ ] Progress and Cancel work, the app stays responsive, and a cancelled export leaves no folder behind.
  - [ ] Click a thumbnail under Results: the patch takes that preset and seed (Presets panel shows the seed), the preview matches the thumbnail, and Cmd+Z goes back. Typing a seed from the contact sheet into the Presets panel's Seed field gives the same picture.
- [ ] The toolbar's Loop field: drag it, type a value, type 0 for "off", then Cmd+Z.

## Presets and variations

- [ ] Open `variations`. The Presets tab comes forward with "Calm" and "Storm".
- [ ] Recall each preset: the picture changes and the patch's wires don't.
- [ ] Drag the Morph slider between two presets: the change is smooth.
- [ ] New Variation (Cmd+Shift+R, or the Vary button) changes the seed and the picture. Cmd+Z undoes it.
- [ ] Save a preset, rename it (double-click), save the file, reopen it: the preset is still there.

## Sound and microphone

- [ ] Open `tut-29-oscillator`: a tone plays and its pitch follows the mouse.
- [ ] Open `tut-28-audio-react`: macOS asks for microphone access the first time ("Floret uses the microphone…"). Allow it. Making a noise grows the circle.
- [ ] `tut-49-fft` and `fft-bloom` react to sound.
- [ ] `tut-46-filter`, `tut-47-envelope`, `tut-48-pulse`, `tone-surface`: sound plays with no clicks or crackle, and stops when you open another patch.
- [ ] Deny microphone access in System Settings → Privacy & Security → Microphone, then reopen `tut-28-audio-react`: Floret shows an error on the node and doesn't crash.

## Camera and body tracking

- [ ] Open `webcam-mosaic` and click its toggle. macOS asks for camera access the first time. Allow it, and the mosaic follows the camera.
- [ ] Open `body-tracking` and click its toggle. Dots and lines follow your body or hands, each person or hand in its own colour.
- [ ] The camera light turns off when you turn the toggle off or open another patch.
- [ ] With an external webcam plugged in, the Camera node's device dropdown lists it, and picking it works.

## MIDI

- [ ] Plug in a MIDI controller and open `tut-45-midi-cc`. The device shows in the MidiCC node's device list. Turning the chosen knob scales the rectangle.
- [ ] A MidiNote node responds to keys pressed on the controller.
- [ ] Unplug the controller while the patch runs: no crash. Plug it back in: it shows up again in the list.

## Video files

- [ ] Open `video-pixelate`: its embedded clip plays and loops, and the slider changes the pixel size.
- [ ] Click Replace… on its VideoFile node and pick an H.264 MP4 from your Mac: it plays. Try a HEVC MOV (iPhone video) and a ProRes file too.
- [ ] Save the patch next to the video, move the folder somewhere else, reopen it: the video still plays (paths are relative to the patch).

## Fonts and text

- [ ] Open `type-raven`, `type-kinetic-wave` and `type-glyph-particles`: the text draws with the right font, and nothing shows as boxes.
- [ ] On a Font node, the family dropdown lists your installed fonts. Picking one (for example Helvetica Neue or Georgia) changes the text.
- [ ] A variable font with a weight axis "breathes" in `type-glyph-particles`.
- [ ] Set a Font node's `file` to a `.ttf` / `.otf` file: the text uses it, and the node lists the font's variation axes.

## UI themes

- [ ] View → Theme → Light, Dark, Use System Setting: the whole window changes (canvas, panels, dialogs).
- [ ] With "Use System Setting", switching macOS between Light and Dark (System Settings → Appearance) switches Floret too while it's open.
- [ ] Cmd+= / Cmd+- / Cmd+0 change and reset the interface size, and text stays sharp on a Retina display.
- [ ] Text is readable in both themes: node labels, the Inspector, tooltips and the status bar.
- [ ] Preferences → Reduce motion stops camera glides and the ambient drift.

## Library and files

- [ ] File → Samples Library (Cmd+Shift+O) shows every sample by category in the list, icon and column views. Search filters across them.
- [ ] Save to library stores the open patch in "My patches", and it reopens from there.
- [ ] File → Save (Cmd+S) on a new patch asks where, and adds `.flora`. File → Save As… (Cmd+Shift+S) saves a copy.
- [ ] A file saved by Floret opens again with the same nodes, wires, values and presets.

## Performance spot-checks

Watch the fps in the status bar (or the Profiler tab). Note the Mac model.

- [ ] `grid-ripple` and `lissajous`: 60 fps.
- [ ] `particles-text-dust`, `sim-flocking`, `sim-slime-mould`: smooth (note the fps).
- [ ] `neon-bloom`, `effect-rainy-night`, `shader-raymarch`: smooth at the default size; note the fps.
- [ ] `noise-cellular-warp` (4096 rects) and `voronoi-mosaic`: note the fps.
- [ ] `type-glyph-particles`: note the fps.
- [ ] Panning and zooming the canvas stays smooth in a big patch (`body-tracking` or `webcam-mosaic`).
- [ ] Leave `grid-ripple` running for 10 minutes: memory in Activity Monitor stays flat.
- [ ] Export a 10-second 1080×1080 MP4 of `loop-bloom` and note how long it takes.
