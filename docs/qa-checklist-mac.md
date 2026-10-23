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
- [ ] Floret → About Floret is the standard macOS About panel: it shows the name, the version and "© 2026 Jaafer Elsadig. Licensed under the Apache License 2.0." (The GPU in use appears only in the About window of the Windows and Linux builds, under Help → About Floret.)
- [ ] Help → Licenses… opens the Licenses window: Floret's own licence (Apache License 2.0) followed by its NOTICE, the fonts, the tracking models, the icons and the Rust libraries it is built from. It can be scrolled and closed.
- [ ] Preferences (Cmd+,): change "On startup, open" to Starter sample, quit and reopen. Grid Ripple opens.
- [ ] Make a change without saving, then force-quit Floret from Activity Monitor and reopen it. Floret offers to restore the unsaved work, and Restore brings it back.
- [ ] **Autosave interval:** in Settings (Cmd+,), Autosave is on, with an "Autosave every" menu (10 seconds, 30 seconds, 1, 2 or 5 minutes) and the path of the recovery files below it. Pick 10 seconds, make a change, wait 15 seconds, force-quit and reopen: the change is offered back. Untick Autosave: nothing is autosaved while you edit, and the "Autosave every" menu is greyed out. Quit normally: no restore offer on the next launch. The choice is still there after a restart.

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
- [ ] Add a Renderer (Tab, `renderer`). Drag from Rect's output pin (`layer`, in the footer under its inputs) to the Renderer's `layer` input: a wire appears and the square shows in the Preview.
- [ ] Inputs and params each have a full-width row, and every value box on a node lines up. Outputs are listed in a slightly darker footer strip under the inputs (one compact row each, pin on the right edge); a node without outputs (Sound Output) has no footer. Check both themes.
- [ ] **Spreads and layers:** open the `layout-bloom` sample. Layout's `positions`, `angles` and `scales` outputs show a small three-bar stack icon between the name and the pin, in the pin's colour; `progress` has none. Hover an icon: the tooltip says "spread". Every layer pin reads `layer` (Group's output, the Renderer's input and output); the Renderer's spread pins are layer-blue, not yellow. A ForEach's port tabs show the icon after their name. Zoomed far out the icons hide with the labels. Check both themes.
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

## Wires, pairs, sections and subpatches

- [ ] **Refused backward wires:** put two nodes side by side (a Number and a Math, say). Drag a wire from the left node's output to the right node's input: it connects. Drag one from the right node's output to the left node's input: while you drag, the pins that would point backwards dim, and dropping on one makes no wire and shows the hint "Wires flow left to right — use a Write → Read pair for feedback". The same inside a ForEach or subpatch frame (the frame's input pins count as its left edge).
- [ ] **Read/Write pairs, created together:** press Tab and add a `write`. A Read appears to its left at the same height, and both are selected. The headers show the same `#` number.
- [ ] **Naming:** double-click either header (or right-click → Name pair…), type `ball position` and press Return. Both halves now read "ball position".
- [ ] **Seeing the pair:** select one half: a dashed link joins it to its partner. Hover one half: the other lights up.
- [ ] Build a counter that flows left to right: Read → Math (add 1) → Write. The number keeps growing frame by frame (watch it in the Inspector).
- [ ] Delete the Write: its Read goes too. Undo brings both back. Copy and paste a Write: the copy has a fresh number and brings its own Read.
- [ ] **Warnings:** change a Write's `slot` so it no longer matches its Read. The status bar shows a warnings count (a Read with no Write, a Write with no Read); click it for the list, and click a warning to select its node. Set the slot back, and the warnings go.
- [ ] **Bypass:** select an Effect (or any node with an input and an output) and press B. It dims and passes its input through unchanged; the picture shows the patch without it. Press B again to turn it back on. Select several nodes: B toggles them together, and one Cmd+Z undoes the lot. A Renderer and a Write can't be bypassed.
- [ ] **Sections:** select a few nodes, right-click one of them → Wrap in Section. A titled, coloured backdrop appears around them. Double-click its title to rename it, right-click it for a colour, drag its corner to resize it, drag it to move the nodes with it. The picture doesn't change. Right-click it → Unwrap section removes it and keeps the nodes.
- [ ] **Wrap / Unwrap:** the same selection → Wrap in ForEach, Wrap in If and Wrap in Cache each put the nodes in that frame; right-click the frame → Unwrap puts them back. The Edit menu has the same Wrap and Unwrap commands. Cmd+Z undoes each.
- [ ] **Subpatches panel:** select some nodes and press Cmd+G. Open View → Panels → Subpatches (or the tab bar's +). It lists the subpatch with its node count.
  - [ ] Click the row: the canvas frames that subpatch. Double-click it (or press I over the subpatch): you enter it, and the panel lists what's inside. Click Up (or press O): back a level.
  - [ ] Right-click the row → Save to library…, name it and save. Open the Library: it is under My patches with a small subpatch mark.
  - [ ] Open another (or a new) patch, click the saved subpatch in the Library: a copy is inserted into the open patch, at the level you're viewing. Right-click it for Insert into patch or Open on its own.
  - [ ] Inside a ForEach, the panel lists the subpatches at that level too.

## Nodes browser and search

- [ ] Cmd+\ shows and hides the Nodes panel (a dock tab, left of the canvas). Its groups (Source, Draw, …) open and close.
- [ ] Typing in the Nodes panel's search filters the list. Dragging a node from it places it where you drop it, and a click adds it mid-canvas.
- [ ] Tab search finds nodes by name (`ellipse`), by type id (`draw/ellipse`) and by old category name (`Style`).
- [ ] Hovering over a node in the Nodes panel shows its description. Selecting a node on the canvas shows its docs in the Help tab.
- [ ] Press Q over the canvas: the Favorites menu opens, and 1–9 place a favourite.

## Inspector, Help, Shader and Profiler panels

- [ ] **Inspector:** with nothing selected it says "No node selected". Select a node: its name, type, description and live output values show, and they change as the patch runs. Select a Camera or MIDI node: its device status shows too.
- [ ] **Help panel:** select a node on the canvas: its docs show (description, pins with their types and defaults, and the notes from NODES.md).
  - [ ] Type `pixelate` in its search box and pick the result: Pixelate's docs show. Try `camera`, `metro` and `ring`: each has notes as well as pins.
  - [ ] Turn the link toggle off: selecting other nodes on the canvas no longer replaces the docs you searched for. Turn it on: it follows the selection again.
- [ ] **Shader panel:** add a Shader node (Tab, `shader`) and select it. The panel shows the code with tabs for the Image pass and Buffer A–D, line numbers, the detected uniforms and the iChannel routing.
  - [ ] Break the code (delete a semicolon): an error appears with the right line number,. Fix it: the error goes away within a moment of your last keystroke.
  - [ ] Add a line `uniform float amount;`: a matching `amount` pin appears on the node.
- [ ] **Profiler:** open `grid-ripple` and the Profiler tab. It shows the tick time, memo hits and the slowest nodes, slowest first. (The engine times nodes only while this tab is showing.)

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
- [ ] Open `hand-sparks` and click its toggle. Your hands show as faint dots; waving them throws sparks, faster waving throws more, and a hand held still throws almost none.
- [ ] Open `camera-embers` and click its toggle. Without a camera a swaying glow sheds embers; with one, the bright parts of the picture (you, a lit wall) glow with embers that rise and burn out. Turn the toggle off and the glow returns.
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

## Docking and panels

- [ ] A fresh install (or one updated from an earlier build): the canvas fills the left of the window as a tab titled CANVAS; on the right the preview sits on top and Inspector, Presets, Help, Library, Shader and Profiler are tabbed below. If the Nodes panel was showing before the update, it is a narrow NODES column at the far left.
- [ ] Drag the Help tab over the Library pane. While you drag, the region it will land in lights up gold (the top or bottom half to split, the middle to join as a tab, the tab bar to slot in between tabs), and the pane is outlined. Drop it: it lands exactly there.
- [ ] Every panel docks anywhere, the canvas too: drag the CANVAS tab to the right half of the side column, then drag Inspector to the left of the canvas. Both land where the highlight showed, and the canvas keeps working (pan, zoom, add a node).
- [ ] Drag a tab around and let go anywhere in a pane: it always docks, never pops out as a floating window.
- [ ] Drag the Library tab and hold ⌥ Option while you let go: a gold card shows where it will float (the gold drop highlight goes away while ⌥ is down), and it floats there as a window. Let go of ⌥ before the mouse: it docks instead.
- [ ] ⌥-drop the canvas to float it. In the floating canvas, drag a value on a node and wire two nodes: both work as in the docked canvas.
- [ ] Right-click a tab → Float: it floats as a window. Right-click → Close closes it; a middle click closes it too.
- [ ] Every tab has a faint × at its right end. Hover the tab, or bring it to the front: the × gets brighter. Hover the × itself: it's full strength on a shaded square. Click it: the panel closes. View → Panels (or the tab bar's +) brings it back.
- [ ] Close the canvas with its ×, then open View → Panels → Canvas: it comes back at the left of the window. Close the Nodes panel and press Cmd+\: it comes back left of the canvas.
- [ ] Stack the canvas behind another tab (drop Inspector onto the canvas's tab bar, so Inspector shows). From the Library, insert a saved subpatch: switch back to the canvas, and the subpatch is there, selected. The Subpatches panel's Focus and the toolbar's Arrange also work while the canvas is hidden. (Edit-menu commands such as Undo act on the canvas, so they need it showing.)
- [ ] Close a preview with its ×: it stays closed while you edit. View → Panels → Preview (unticked now) brings every preview back; opening another patch does too.
- [ ] A floating window has a − and a × at the right of its tab bar. − shrinks it to its tab bar; the button (or the arrow on its left) brings it back at its old size. × closes the window. A floating preview's × puts the preview back under the others. The tab right under the − × has no × of its own; it closes from its right-click menu or a middle click.
- [ ] Move and resize a floating window, quit Floret and reopen it: the window is where you left it, at the same size. Close the canvas, quit and reopen: it is still closed (View → Panels brings it back).
- [ ] Drag a tab back from a floating window into the dock: it docks where the highlight showed.
- [ ] Drag the dividers between panes: they're easy to grab, and a pane can be made much smaller than before. Split a pane left/right in the side column: both halves can be resized.
- [ ] With a pane narrow, the tabs that don't fit are not cut off: scroll the tab bar (two-finger swipe over it, or its thin scroll bar).
- [ ] View → Panels lists Canvas, Nodes, Preview, Inspector, Presets, Help, Library, Shader, Profiler and Subpatches, each ticked while open. Untick one: it closes. Tick it again: it reopens and comes to the front. View → Nodes Panel, the toolbar's sidebar button and Settings → Nodes panel open and close the same Nodes tab.
- [ ] Close a few panels (the canvas too) and float one, then View → Reset Layout: every panel is back where it started (the Nodes column stays if it was open). The tab bar's + menu still lists the closed panels and Reset layout.
- [ ] Open `grid-ripple` and add two more Renderer nodes (Tab, `renderer`). Three preview panes show, stacked one under another with even heights, each titled after its Renderer (or Preview, Preview 2, Preview 3).
- [ ] Wire something into the second Renderer: only its pane shows it. Delete the middle Renderer: its pane closes and the other two keep their pictures, zoom and pan.
- [ ] With three Renderers: Focus mode, Output → Fullscreen and Export (PNG, video, variations) from each pane's header act on that pane's Renderer. Each pane can be dragged elsewhere or floated.
- Known limit (not a bug to report): the canvas size the patch itself sees (mouse coordinates, for example) comes from the first Renderer.

## UI themes

- [ ] View → Theme → Light, Dark, Use System Setting: the whole window changes (canvas, panels, dialogs).
- [ ] With "Use System Setting", switching macOS between Light and Dark (System Settings → Appearance) switches Floret too while it's open.
- [ ] Cmd+= / Cmd+- / Cmd+0 change and reset the interface size, and text stays sharp on a Retina display.
- [ ] Text is readable in both themes: node labels, the Inspector, tooltips and the status bar.
- [ ] Preferences → Reduce motion stops camera glides and the ambient drift.

## Library and files

- [ ] File → Library (Cmd+Shift+O) shows every sample by category in the list, icon and column views. Search filters across them.
- [ ] Save to library stores the open patch in "My patches", and it reopens from there.
- [ ] **Library zoom:** in the Library, switch to the Tiles (icon) view. The − and + buttons and the slider beside them make the tiles smaller and larger. Hold Cmd and scroll over the tiles, or pinch on the trackpad: the tiles resize smoothly between a small and a large size, and the titles scale with them. Quit and reopen Floret: the tile size is the same.
- [ ] **Covers:** every sample tile shows its cover picture within a few seconds (scroll through all categories). None is blank, black or stretched, and they stay sharp at the largest size. A tile with no picture yet shows a muted colour swatch.
- [ ] The Library's list and columns views still work: search filters across all three views, and clicking a sample opens it.
- [ ] A saved subpatch carries a small mark in the Library (see Wires, pairs, sections and subpatches).
- [ ] File → Save (Cmd+S) on a new patch asks where, and adds `.flora`. File → Save As… (Cmd+Shift+S) saves a copy.
- [ ] A file saved by Floret opens again with the same nodes, wires, values and presets.
- [ ] **Pictures beside the patch:** make a folder with a picture in it (a PNG), add an ImageURL node, click the … button on its `url` pin and choose that picture, wire it into an Image (draw) node and a Renderer, then File → Save the patch in the same folder. Move or rename the whole folder, or copy it to another Mac or account, and open the patch from there: the picture still shows. Do the same with a font file on a Font node, a LUT file on an Effect set to lut, and a CSV on a CSVFile node. A patch saved earlier with a full path to a picture still opens.
- [ ] Save As into a different folder: the picture still shows, and the copy works when that folder is moved.
- [ ] A picture in a faraway folder (not beside or below the patch) still shows after Save and reopen.

## The crash screen

If an internal error happens, Floret shows a "Something went wrong" window instead of closing. There is no way to trigger it on purpose in the installed app, so this is a check for if you ever see it (note which sample or patch you were on, and take a screenshot):

- [ ] The window says what was affected (evaluating the patch, drawing the preview, or the editor), that nothing was lost, the error message and where it happened, and where a recovery copy of the patch was saved.
- [ ] Show backtrace expands the technical detail. Copy details puts it on the clipboard (paste it into the bug report). Save patch as… writes a `.flora` copy of your patch.
- [ ] Continue closes the window and lets you keep editing; after an evaluation or drawing error the preview stays paused, and a "Evaluation paused after an error" bar offers Resume, Reload patch and Details.
- [ ] Reload patch restarts the patch from a fresh state and evaluation runs again.
- [ ] After an error in the editor, the edit in progress is undone and the rest of the patch is still there.

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
