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

## No-code samples (agent A2)

The owner wants every Library sample built from visual nodes, with no
`math/expression` node in any patch (the node stays available). The 525
formulas in 43 samples were read one by one; most are plumbing that a
visual node already covers (Math, Map, Color, Vec2, Compare), but several
recurring patterns had no visual node at all, or only a five-node chain
with a State loop. The building blocks below cover them. Each is a
general tool for generative work (none is shaped around one sample), works
per element over spreads, carries its state across patch edits where it
has any (`carry` / `adopt`), and is flagged effectful when it reads time.
Counts below are from that formula list (approximate: one formula often
mixes several patterns).

### Switch (`logic/switch`, Math)

- **Missing:** a choice among more than two values. 155 formulas use `?:`,
  31 of them nested (`a ? x : b ? y : z`), and 51 compare a key or index
  against constants (`k == 0 ? … : k == 1 ? …`, `s == 0 ? rgb(…) : s == 1 ?
  …`: the epidemic's three state colours, the poster's five shape kinds, the
  breakout ball's three velocity cases). Select takes two values and a
  chain of Selects with Compares is unreadable.
- **Design:** one node, two modes, `− n +` cases (slotCount, 1..32).
  `index`: `index` → value `v<index>` (`range` clamp / wrap; NaN → 0).
  `first-true`: pins `if0, v0, if1, v1, … else`; the first true condition
  wins. Value pins are `any` spreads and keep their names across modes. A
  single selector passes the chosen spread through whole (a spread of layers
  stays a spread); a selector spread picks per element with inputs cycling.
  Extra output `which` (case index, −1 = else) doubles as "which button".
- **Tests:** `nodes/control.rs` (pins per mode / slot count incl. clamping,
  pass-through, clamp / wrap / floor / NaN / ±∞ indices, per-element
  choice with cycling, empty cases → Null, empty index → empty, ternary
  chain order, layers through).

### Latch (`io/latch`, Animate)

- **Missing:** a remembered on/off. 11 formulas build rising edges by hand
  (`p && !was`) and 7 toggle (`hit && k == 3 ? 1 - s : s`), always with a
  State loop: brush / sequencer / flock / music-box mode toggles, sequencer
  cells, "game started".
- **Design:** `set`, `reset`, `toggle` (bool spreads, rising-edge) and
  `initial` → `state`, `changed` (the tick it flips), per element. Reset wins
  over set in the same tick. A separate node rather than a mode: Gate holds
  any value while a condition is open (sample-and-hold, a different job),
  Counter is numeric and is being extended in parallel (dec / min / max /
  wrap); a flip-flop with three edge inputs per element would have bent
  either out of shape.
- **Tests:** in `nodes/control.rs` (edges vs held inputs, reset priority,
  per-element initial / changed, empty spreads, state carried over an edit).

### Integrate (`motion/integrate`, Animate)

- **Missing:** a value that runs at a speed. 22 formulas accumulate by hand
  — `ph + min(dt, 0.1) * rate` (arpeggiator / sequencer playheads),
  `a + min(d, 0.1) * s` (kaleidoscope / music-box angle), `c + min(dt, 0.05) *
  v` (scrolling), `p + v * h` (positions from velocities), `r ? 0 : min(1, g +
  dt / 9)` (growth), `y != p ? 0 : s + dt` (time since a change), `ate ? 0 :
  a + h` (ages) — each needing a State node and a `min(dt, …)` guard; another
  13 wrap with `mod`/`fract`. `t × rate` jumps when the rate changes.
- **Design:** `rate`, `start` (adaptive: number, vec2 or colour), `run`,
  `reset` (held), `limit` none / clamp / wrap / bounce with `min` / `max`,
  `maxDt` (default 0.1 s) → `value`, `hit` (wrapped / bounced / arrived).
  Per element; growing spreads start new elements at `start`.
- **Tests:** `nodes/timing/tests.rs` (accumulation, dt cap, pause, reset,
  vec2 per element, growth / shrink / empty, clamp / wrap / bounce with hit
  flags, zero-width range, NaN rate, carried state, pins per limit and
  value type).

### Envelope (`motion/envelope`, Animate)

- **Missing:** shapes of time after an event. 10 formulas take `t - b`
  (b = when it happened) and shape it: `1 - exp(-6 * a) * cos(15 * a)`
  (typing toy's spring-in letters), `0.9 * exp(-4 * a) * sin(18 * a)` and
  `2.5 * exp(-a * 5) * sin(a * 70)` (boing / pluck), `clamp(1 - (t - l) * 1.6,
  0, 1)` (mic bloom flash), `(1 - exp(-3a)) * clamp(1 - (a - 24) / 8, 0, 1)`
  (MIDI garden bloom). Ease maps 0..1 → 0..1 and the sound Envelope shapes
  audio, so neither fits.
- **Design:** `shape` decay / attack-decay / fade / boing / spring with
  `time` (seconds to settle: decays reach 1 % at `time`), `attack` / `hold`
  and `frequency` (Hz) as the shape needs. `from` `age` (seconds, per
  element) or `trigger` (bool per element; the node stamps each rising edge
  with the tick time). Outputs `value`, `age`, `active`. Rests at 0 before
  the event (a spring at 1). Label "Envelope" like the sound node; the group
  (Animate vs Input & Sound) and description tell them apart.
- **Tests:** `nodes/timing/tests.rs` (each shape's key values, overshoot /
  wobble, rest before / never, zero times finite, ages per element, self
  timing incl. held triggers, re-triggers and carried trigger times, pins).

### Beat (`time/beat`, Animate)

- **Missing:** a step clock. 16 formulas build one from time —
  `floor(t * 6) % 16` and `fract(t * 6) < 0.45` (gates), `mod(ph + dt *
  bpm / 15, 16)`, `floor(t / s)` / `fract(t / s)` with `s = 30 / b`, `fract(t /
  (s * 2)) < 0.8` — in music viz, step sequencer, mic bloom. LFO has a bpm
  but no steps, bars, gate or trigger; Metro has a trigger only.
- **Design:** `bpm`, `steps` per bar, `beats` per bar, `swing` (0..1: every
  second step late, 1 = 3:1), `gate` width, `run`, `reset`, `sync` free /
  loop → `step`, `phase`, `bar`, `gate`, `trigger` (step starts), `count`.
  Free mode accumulates dt (tempo changes never jump); loop mode fits a
  whole number of bars in the patch loop, so recordings loop.
- **Tests:** `nodes/timing/tests.rs` (steps / phase / bars / triggers at
  120 bpm, stop and reset, swing mapping, loop lock and wrap trigger,
  carried state without a spurious trigger, bpm 0 / NaN / 0 steps).

### Hit Test (`geometry/hit-test`, Shape Ops)

- **Missing:** "is this point in that shape". Breakout bricks and paddle
  (`abs(c.x - q.x) <= 37 && abs(c.y - q.y) <= 17`, `q.y >= 222 && … abs(q.x -
  px) <= 59`), buttons and cells (`distance(q, m) < 10`, the sequencer's
  `floor((m.y + 174) / 36) * 16 + floor((m.x + 288) / 36)`), food / infection
  radii. Collider nodes only work inside the particle system.
- **Design:** modes `rects` (centres, sizes), `circles` (centres, radii),
  `layer` (every drawn shape of a layer: nonzero winding over its closed
  outlines so glyph holes count, open lines by half their stroke weight);
  `margin` grows / shrinks every shape (a ball's radius). Per point
  `inside` and `index` (first shape by list order), per shape `hits`.
  Bounding-box rejects keep layer tests cheap.
- **Tests:** `geometry/hit_test.rs` (rects with edges, margin and overlap,
  circles, negative margin, layer shapes with a group transform, holes,
  lines by stroke, margins on outlines, empty points / shapes, NaN, zero
  sizes).
- **Check frame:** `/tmp/agent-nodes-a2/hit-test.png` (Scatter points
  against a star and a rose Curve; Switch in `index` mode with `wrap` turns
  `index` 0 / 1 / −1 into three colours).

### Curve (`geometry/curve`, Patterns)

- **Missing:** classic parametric curves. 18 formulas trace one by hand:
  the expression rose (`vec2(cos(u * tau), sin(u * tau)) * (50 + 170 *
  abs(cos(petals / 2 * u * tau + t / 3)))`), Lissajous orbits (`vec2(190 *
  sin(tau * l * 3), 110 * sin(tau * l * 4))`, echo trails, slit scan, time
  smear), the preset-morph hypotrochoid (a 200-character formula), clock
  and music-viz rings.
- **Design:** modes lissajous (`a`, `b`), rose (`n`, `d`), hypotrochoid /
  epitrochoid (`R`, `r`, `d`), spiral / log-spiral (`turns`, `inner`), star
  (`points`, `inner`), superformula (`m`, `n1..n3`); shared `count`,
  `center`, `radius` (fit), `phase`, `rotation`, `style`. Closed curves run
  exactly one period (continued fractions find it from the ratios, capped at
  64 turns) and don't repeat their first point; `phase` 0..1 is one seamless
  cycle in every mode. Outputs the path layer, the points and tangent angles.
- **Tests:** `geometry/curve.rs` (every mode finite and fitted, closed
  curves join, phase 0 = phase 1 ≠ phase 0.37, rose through the centre, star
  corners and notches, spiral radii, zero / negative counts, zero radius,
  irrational ratios, NaN params, determinism, periods from ratios).
- **Check frame:** `/tmp/agent-nodes-a2/curve.png`.

### Plot (`geometry/plot`, Draw)

- **Missing:** values → a chart. Ecosystem / epidemic population graphs
  (`vec2(-330 + 660 * index / 239, 292 - 108 * v)`, 6 formulas), data story
  bars, and the landscape's five mountain ridges, each closed into a filled
  area by `index == 100 ? vec2(400, 260) : (index == 101 ? vec2(-400, 260) :
  …)` corner points.
- **Design:** modes line / area (closed to `baseline`) / bars (`gap`) in a
  box (`center`, `size`); `range` auto / zero / fixed (`min`, `max`); y grows
  downward so larger values are higher; NaN on the baseline. Outputs the
  layer, the value points and the `base` points under them.
- **Tests:** `geometry/plot.rs` (line spans the box, zero / fixed ranges,
  values past a fixed range, flat series, area closing and clamped
  baseline, bar slots and gap, empty / NaN / one value / zero size).
- **Check frame:** `/tmp/agent-nodes-a2/plot.png`.

### Move (`particles/move`, Particles & Physics)

- **Missing:** movers on plain lists. Positions from velocities with
  wrap / bounce (`vec2(mod(p.x + 320, 640) - 320, …)`, breakout's `q.x <
  -353 ? abs(v.x) : …`, orbit / weather integration `p + v * h`, `v + a * h`)
  — 13 formulas plus their State loops. The particle system (Emitter, World,
  Bounds) is buffer-based: it can't take a spread of positions, hand back
  spreads, or report which wall was hit.
- **Design:** `position`, `velocity`, `acceleration`, `drag`, `maxSpeed`,
  `radius` (spreads, cycling), `edge` none / wrap / bounce / clamp in a box
  (`size` 0 × 0 = the canvas), `restitution` (bounce), `timeScale`; `run`
  mode keeps state seeded from the inputs with per-element `reset`
  (respawn), `step` mode moves the inputs once (State loops). Outputs
  positions, velocities, `hit`, wall `normal`. Semi-implicit Euler, dt
  capped at 0.1 s. It complements the particle system rather than
  duplicating it: small counts, everything visible as lists.
- **Tests:** `nodes/mover.rs` (motion and acceleration, state and dt cap,
  bounce / wrap / clamp / none with radius, box smaller than a mover,
  max speed, drag, restitution, per-element reset, step mode, empty lists,
  carried state, NaN inputs, pins per mode / edge).

### Noise: `gradient` output and field modes (`noise/noise`)

- **Missing:** fields you can steer by. The weather sample differences the
  same 300-character pressure formula four times by hand to get a wind
  vector; city / weather add Gaussian blobs (`exp(-dot(c, c) / 50000)`,
  `s * exp(-((q.x - c.x) / 170) ^ 2)`); 22 formulas call `noise()`.
- **Design:** every Noise mode outputs `gradient` (vec2): the slope per
  unit of `pos` (forward differences in noise space × scale), pointing
  uphill. Two field modes join the pattern fields: `radial` (Σ weight · (1 −
  (d/r)²)², 0 past the radius) and `gaussian` (Σ weight · e^(−(d/r)²)) with
  `center*`, `radius*`, `weight*` (one bump per centre; negative weights
  dig): pressure highs and lows, metaballs, attractor maps, and they add up
  with Math (so do their gradients). Modes on Noise rather than a new node:
  Noise already hosts the non-noise pattern fields (checker, stripes,
  rings) with the same per-point pins and warp. Cost: two extra samples per
  point (memoised when the inputs don't change).
- **Tests:** `nodes/noise.rs` (gradient vs a central difference of the value
  in every noise mode at scale 0.5, per element, checker flat; radial /
  gaussian values, uphill gradient, summing and negative weights, no bumps /
  zero / NaN radii; field pins only in field modes).

### Lookup → `bin` op (`spread/lookup`)

- **Missing:** band index from thresholds. `ev < s - 0.08 ? water : (ev < s
  ? shallows : (ev < s + 0.04 ? sand : …))` (tile world biomes), music-viz
  `clamp(floor((noise(…) - 0.22) / 0.56 * 8), 0, 7)`, `i < floor(s) ? … : (i ==
  floor(s) ? … : …)`.
- **Design:** op `bin` on Lookup (hidden backing node `spread/bin`):
  `spread` = thresholds (any order; non-numbers ignored), `values` → per
  value the number of thresholds at or below it (0 … len), NaN → −1. Pair
  with `pick` for a colour per band.
- **Tests:** `nodes/spread/bin.rs` (bands and edges, unsorted / junk
  thresholds, no thresholds / values, NaN / ±∞, repeated thresholds, pins).

### Suggestions (not done)

- Hit Test's `index` picks the first shape by list order; a `last` option
  (topmost drawn) would suit overlapping buttons.
- Noise's gradient costs two extra samples per point even when unwired; the
  compiler knows which outputs are wired and could tell the node (a small
  `NodeInit` addition, left alone while the engine core is being reworked).
- Curve could take `count` per mode defaults (stars need few points) and a
  `close` override for open Lissajous segments.
## No-code samples (agent A1)

The owner wants every Library sample built from visual nodes, with no
`math/expression` formulas (525 Expression nodes in 43 samples, to be
rebuilt later). This section is the groundwork: what the visual nodes
lacked for those rebuilds, and what was added. No new node types.

### Per-element evaluation over spreads (format v4)

- **Missing:** the scalar nodes read only slot 0 of a spread. A spread
  arriving on a single-value pin of Math, Compare, Vec2, Colour … used
  its first item, and a spread *output* wired into one was dropped with
  the warning "only slot 0 reaches it. Wrap the consumer in a ForEach".
  Only Expression, Noise, Scale, Format Number, the Statistics / List
  families and LFO (`count`) worked over whole spreads, and 132 of the
  formulas map over spreads — so a visual rebuild needed a ForEach around
  every bit of arithmetic.
- **Design:** the value nodes now run **once per element** as soon as a
  spread of two or more values reaches a single-value pin, with
  Expression's rule (`nodes/shared.rs` `cyclic_len` / `cyclic`; here
  `node::lift_len`): the longest spread sets the count, shorter ones —
  single values included — cycle (`s[i % len]`), and an empty one empties
  every output; spread-kind pins (Gradient's `colors`) stay whole. Every
  output carries one value per element.
  - **One deliberate difference from Expression:** while no pin carries
    two or more values, nothing lifts — the node takes its old one-value
    path, where an *empty* pin reads as "nothing" (its default / `0` /
    false), not as an empty result. Empty spreads mostly come from an
    upstream error (a node that fails emits empty outputs) or a device
    that isn't there: emptying the result there broke three tutorials
    (a missing microphone made Amplitude → Math → Ellipse draw NaN) and
    `camera-slit-scan` (a Select on the camera's `ready` stopped falling
    back to its animation). The cost: a spread that is empty while every
    other pin holds one value gives one value from the defaults instead of
    none (as before v4). Making it exactly Expression's is a one-line
    change in `lift_len` if the owner prefers.
  - Which nodes: `nodes::lifts(type)` — registered with `r.lifted(&[…])`
    next to each node: Math, Vector Math, Compare, Logic and Convert (every
    op), Select, Map, Norm, Lerp, Clamp, Wrap, Step, Smoothstep, Vec2 /
    Split / FromAngle / ToPolar, Color / Split / Set Alpha / Lerp Color /
    Gradient (per `t`), Ease, Uniform / Gaussian / Wanderer, MIDI ↔ Freq,
    String Length, Image Sample (per `x`, `y`), and the hidden single-op
    nodes behind the families (≈100 types; a test builds every one, every
    family op, and checks it lifts).
  - How: `NodeImpl::lift_spreads()` switches a node on. `node::Scalar`
    (the `compute` wrapper) lifts natively — while every single pin holds
    exactly one value it takes the old stack-argument path untouched, so
    the common case allocates nothing new. Raw nodes that read params
    (Color, Lerp Color, Gradient, Ease, Color Split) are wrapped in
    `node::per_element`, which runs the inner node per element with
    one-value inputs.
  - **Stateful nodes:** Smooth (the owner's plan) and Spring keep state
    **per element** (`node::per_element_state`: element *i* always runs
    its own instance; a new element starts at its target, a dropped one
    forgets its state; element 0 is the instance an unlifted patch runs).
    Delay Line, Trigger, Edge, Counter, Metro, Gate and LFO still read
    slot 0 — per-element history for Delay Line would cost up to 4096
    samples per element, and the edge detectors drive events, which a
    ForEach or Expression handles better. Draw / style / transform / text
    / force nodes don't lift (one layer per node); Format Number and Noise
    already took whole spreads.
  - **Old files are untouched:** `CURRENT_PATCH_VERSION` is 4. The
    migration ladder's v3 → v4 step sets a patch-level flag,
    `PatchFile::legacy_slot0` (saved as `"legacySlot0": true`, absent in
    newer files), which the compiler reads: in a legacy patch no node
    lifts, and a spread output into a single pin is still dropped with
    the old warning. The flag is kept on save, so a user's file computes
    as it always did however often it's re-saved; the frozen parity
    fixtures (`native/fixtures/samples/`, v2) stay legacy too. New
    patches (File → New, the Library's v4 samples) lift.
  - **Editor:** a spread output may now be wired into a single-value pin
    of a lifting node (refused into any other, and in a legacy patch);
    the compiler keeps that wire instead of dropping it.
  - **Not done:** an "upgrade this file" command that drops
    `legacySlot0` from an old patch (its spreads would then lift); pasting
    nodes copied from a legacy patch into a new one makes them lift
    (the flag is per patch).
- **Tests:** `nodes/lift_tests.rs` — per-element results equal
  Expression's for numbers (Math ops, Clamp, Map, Smoothstep, Lerp,
  Compare, Select), vec2s (Math, Make, Vector Math, Clamp, Map) and
  colours (Math, Color, Color Split); the cycling rule with mixed single /
  spread inputs; empty spreads (beside a spread: empty; alone: the
  one-value path, Select still falls back); several outputs (Split); Gradient keeping
  its stops; Smooth's per-element state; every registered type lifts; and
  end to end, the same file as v4 (wire kept, per element, no warning) and
  as v3 (warning, wire dropped, a dynamic spread reads slot 0).
  `patch.rs` `older_files_keep_slot0_scalars`; `editor/connect.rs`
  `connection_rules`.
- **Library samples:** none relied on slot-0 truncation (the QA sweep
  already forbade the "only slot 0" warning, and no sample feeds a
  multi-value spread dynamically into a lifting pin), so all 176 were
  re-saved as v4 — only their `version` changed (plus `sys-data-story`'s
  Counter, moved up 90 px: its new `dec` pin made it taller). Verified
  with the
  `rewritten_samples_draw_as_before` harness (`sample_sweep.rs`,
  `FLORET_SAMPLES_BEFORE` = the v2 / v3 originals): every node's outputs
  over the compared frames and the rendered pixels are identical for all
  176 (pixels compared where a sample renders deterministically run to
  run; a first full run flagged `effect-colour-grade` and `sys-clock`
  through load / wall-clock timing — identical on re-run — and the four
  empty-spread cases above, which led to the rule above).

### Small extensions (modes and pins on existing nodes)

Each keeps old files computing exactly as before (the new pin's default
or the new mode's absence is the old behaviour); pins recorded in
`tests/frozen_ports.txt`.

- **LFO `sync`** (`motion/lfo`): `free` (default, as before) or `loop`.
  *Missing:* an LFO free-runs at `rate` Hz, so an exported loop only
  joins if the rate happens to divide the loop length, and the random
  shapes never repeat. *Design:* in `loop`, `rate` counts whole cycles per
  patch loop (rounded, at least one) and the phase is computed from the
  clock (`time::loop_phase` × cycles, snapped onto cycle boundaries), so
  frame N of a recording equals frame 0; the random shapes key on the
  cycle within the loop, so they repeat too. `bpm` / `reset` don't apply.
  *Test:* `lfo.rs` `loop_sync_repeats_every_loop` (sine, saw, random,
  smooth-random over two loops at a fixed dt).
- **Counter `dec`, `limit`, `min` / `max`** (`io/counter`): `dec` subtracts
  `step` on its rising edge; `limit` none (default) / clamp (`[min, max]`)
  / wrap (`[min, max)`, like the Wrap node: min 0, max 4 counts
  0 1 2 3 0 …); `min` / `max` pins appear only in clamp / wrap (dynamic
  inputs). *Missing:* menus, carousels and game scores counted both ways
  and wrapped, which took a Counter plus Expression. *Test:* `io.rs`
  `counter_counts_down_and_keeps_within_limits`.
- **Keyboard `hits*`** (`input/keyboard`, keys mode): one boolean per
  listed key, true on the frame that key went down (several when pressed
  together). *Missing:* `hit` only said *some* listed key went down and
  `index` names one, so a drum pad / piano that retriggers each key needed
  an edge detector per key. Built on the existing `FrameCtx::keys_down`
  diff the node already keeps (no new frame plumbing; a key pressed and
  released between two frames is missed, as `hit` already is). *Test:*
  `input.rs` `keyboard_hits_say_which_listed_key_went_down`.
- **Delta `max`** (`time/delta`): above 0 it caps `dt` (e.g. 0.05), so
  hand-integrated motion doesn't jump after a stall; 0 (default) = as
  before. Delta became a raw node (it now has an input) wrapped in
  `always_eval` and flagged effectful, so it is never memoised, as before.
  *Test:* `time.rs` `delta_caps_at_max`.
- **Clamp, Wrap, Map adapt to vec2 / colour** (`math/clamp`, `math/wrap`,
  `math/map`): `adaptiveType` with a `valueType` latch, like Math and
  Lerp — once a vec2 wire latches them, every pin is vec2 and they compute
  per component (numbers broadcast). *Missing:* keeping a point in a box or
  wrapping it around the canvas took a Split, two nodes and a Make. Unset
  `valueType` (old files) keeps the old number code path exactly — the TS
  reference cases (a vec2 into Clamp gives NaN) still pass. *Test:*
  `math/tests.rs` `clamp_wrap_and_map_adapt_to_vec2`, plus the lifted
  vec2 cases above.
- **Smoothstep `window`** (`math/smoothstep`): a `mode` dropdown — `step`
  (default, the classic ramp) or `window`, with pins `edge0, edge1,
  edge2, edge3, x`: rises across `[edge0, edge1]`, falls back to 0 across
  `[edge2, edge3]` (`smoothstep(e0, e1, x) · (1 − smoothstep(e2, e3, x))`).
  Four edges rather than centre / width: they read like GLSL's
  smoothstep, allow asymmetric attack / release, and the formulas being
  replaced are written that way. *Missing:* pulses, bands and fades in
  and out were two Smoothsteps and a multiply. *Test:* `math/tests.rs`
  `smoothstep_window_rises_and_falls`.
- **Spiral / Fibonacci `inner`** (`spread/spiral`, `spread/fibonacci`):
  Spiral's radius runs `inner → radius` instead of `0 → radius`;
  Fibonacci leaves a hole of radius `inner` with `r = √(inner² + scale² ·
  i)`, keeping the sunflower's even spacing (adding a constant to the
  radius would crowd the inner rings). 0 (default) takes the old formula
  exactly. *Missing:* rings of seeds around a centre disc, spirals that
  start off-centre. *Test:* `spread/tests.rs`
  `spiral_and_fibonacci_leave_an_inner_hole`.
