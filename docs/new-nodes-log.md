# New nodes log

Capabilities that Library samples needed and Floret lacked: what was
missing, why, the design and the sample that needs it. Each agent keeps its
own section.

## Systems samples (agent)

The 15 "Systems" samples (`native/samples/sys-*.flora`) combine several
subsystems each. Three capabilities were missing; all three were built as
modes / pins on existing nodes or one small node, following the ground rules
in `plan-feature-complete.md`.

### Lookup → `nearest` op (`spread/lookup`)

- **Missing:** a way to ask "which item of this point list is nearest each of
  those points". Without it, agent systems can't find their nearest food,
  neighbour or infected person; a ForEach × ForEach brute force isn't
  expressible (no nested ForEach) and Connect Points' `nearest` mode only
  works within one set and gives lines, not indices.
- **Design:** a new op on the existing Lookup family, `nearest`, with pins
  `spread: vec2*` (the candidates) and `points: vec2*` (the queries) →
  `result: number*`, the index into `spread` per query (ties → lower index,
  −1 when `spread` holds no points). Pair it with `pick` for the position.
  A uniform grid with ring search keeps it sub-quadratic. Like every family
  op it is backed by a hidden single-op spec (`spread/nearest`) that holds
  its pins and compute.
- **Tests:** `nodes/spread/nearest.rs` (grid vs brute force inside and far
  outside the grid, ties, empty lists, non-point items).
- **Samples:** `sys-ecosystem` (the boid nearest each food item eats it),
  `sys-epidemic` (each person's nearest infected person).

### Simulation flocking → `food*` pin (`particles/simulation`)

- **Missing:** boids that each head for *their own* nearest target. The
  flock had one `target`, so a grazing flock all converged on one point.
- **Design:** an optional `food*` spread pin in `flocking` mode (the name the
  physarum mode already uses, so no new saved pin name): while it holds
  points, `attract` steers every boid toward the food point nearest it, the
  short way round the wrapping field; otherwise `target` as before.
  Unchanged behaviour when unwired.
- **Tests:** `simulation/tests.rs` (`boids_head_for_their_nearest_food`,
  `food_is_reached_the_short_way_round_the_field`).
- **Sample:** `sys-ecosystem` (with `count` driven by a population State:
  a changing count already added / dropped boids without restarting).

### Clock (`time/clock`)

- **Missing:** the wall clock. `time/seconds` is time since the patch
  started; there was no time of day, date, weekday or year phase, so a clock
  or calendar piece couldn't be built.
- **Design:** one effectful node in the Animate group, `Clock`: `offset`
  (hours, for time travel / other cities) and a `zone` dropdown (`local`,
  `utc`) → `hours, minutes, seconds` (fractional, running smoothly), `day,
  month, year, weekday, yearPhase, days` (days since 1970, for long cycles
  such as the Moon's). A separate node rather than a Time mode so the Time
  node (on dozens of existing samples) keeps its look. The engine stays free
  of OS dependencies: the app installs the local zone lookup
  (`floret_engine::nodes::time::install_local_offset`, `localtime_r` on
  macOS / Linux); without it — headless tests, Windows for now — `local`
  reads as UTC.
- **Tests:** `nodes/time.rs` (`clock_parts` against known dates incl. leap
  days and pre-1970, the node shifted by `offset`).
- **Sample:** `sys-clock`.

### Noted, not built

- **Spreads into single-value draw pins.** Drawing one dot per point still
  needs a ForEach (Rect / Ellipse / Line / Text take one value at the top
  level). Most of the systems samples use small ForEach regions for this;
  a broadcast at the top level would make such patches smaller. Worth
  considering, but it changes the editor's model, so it's left as a note.
- **SVG export** for the plotter sample (`sys-plotter` exports PNG; its
  "clean output" toggle prepares the frame for that).

## Interactive samples (agent)

Fifteen interaction-first samples (Library categories **Interactive** and
**Sound**: `play-*`, `sound-*`). Most of what they do is built from existing
nodes — Expression over spreads with State / State* for game logic,
nearest-point picking (Expression `distance` → Statistics `min` → Lookup
`index-of`), click edges (`pressed && !was` with a State), list editing with
Combine / Sublist. Three things could not be built from nodes at all:

### Keyboard: held keys and typed text (`input/keyboard` modes)

- **Missing:** the Keyboard node only knew the *last* key pressed and whether
  *any* key was down. A piano needs to know which keys are held (chords, the
  latest press for a mono synth, a re-trigger on each new press); a typing
  toy needs the characters typed — case, punctuation, Backspace — which the
  last key can't rebuild (pressing the same key twice, keys overlapping).
- **Design:** no new node — a `mode` dropdown on Keyboard (pins via
  `DynamicInputsFn`): `last` (unchanged, no pins), `keys` (a `keys` string:
  one character per key, or names split by commas; outputs `down*` per key
  and `index`, the latest press still held) and `text` (`clear`, `max`;
  output `text`, kept across patch edits). A `hit` output fires on the frame
  a key goes down in every mode. The frame carries two new fields,
  `FrameCtx::keys_down` and `FrameCtx::typed` (egui `Text` events, `\n` for
  Enter, `\u{8}` for Backspace), filled by the preview. Keyboard turned from a
  context node into an effectful raw node (it keeps state); `last` mode
  outputs exactly what it did (parity compares its first two outputs).
- **Used by:** sound-keyboard-synth, play-typing-toy, and the key controls of
  nearly every sample (brush, sequencer, sandbox, voronoi, flock,
  kaleidoscope, reaction, MIDI garden fallback, music box).

### Scale (`sound/scale`)

- **Missing:** turning a mouse position, a noise value, a grid row or a hand
  into notes *in key* took a Lookup per scale and octave arithmetic in every
  patch, with no way to switch scales.
- **Design:** one node, `mode` degree / quantize / range, `scale` a dropdown
  pin that can be wired (an `io/select` on the canvas switches it live) —
  12 scales plus `custom` (`steps`). Per element over spreads; outputs MIDI
  `note`, `freq` and the `index` in the scale (for colours). Pure, memoised.
- **Used by:** sound-step-sequencer (rows → degrees), sound-hand-theremin
  (x → range, snapped or gliding), sound-music-box (rings → degrees),
  sound-mic-bloom (its demo loop), and the theremin's lane display.

### Mix (`sound/mix`)

- **Missing:** every audio processor and the Output take one input, so
  several voices could only reach the speakers through separate Outputs —
  they couldn't share a filter, delay, reverb or an FFT.
- **Design:** `sound/mix` with `slotCount` audio inputs (`− n +` in the
  header) and `level`. The engine declares `AudioNode::Mix { inputs, level }`;
  floret-media orders it after all its inputs and sums them into a scratch
  bus on the audio thread (no allocation), then applies the level ramp like
  Gain.
- **Used by:** sound-step-sequencer (4 voices), sound-keyboard-synth (lead +
  sub), sound-hand-theremin, sound-mic-bloom (mic + demo → FFT), sound-music-box
  (3 bells, each two partials).

### Suggestion (not done: UI behaviour)

While the pointer is over a preview, the node canvas still acts on its
single-key shortcuts — F fits, Tab opens search, Q the favourites menu,
Backspace / Delete delete the selection — and the preview takes Space for
pause. Keyboard-played samples work (the patch reads keys before the
canvas does) but the editor reacts too; their comments suggest Focus mode.
Letting the preview own plain keys while it is hovered would fix it, but
changes existing UI behaviour, so it is left for the owner to decide.
