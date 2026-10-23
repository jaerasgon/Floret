# Floret Nodes

Every built-in node, grouped by category (type-id prefix); the Nodes
browser groups them by role instead (below). The Help panel reads this
file, and a test fails when a node in the Nodes panel has no row here or a row
names a node that doesn't exist: keep it in sync when adding or renaming
nodes.

Format per row:

| Label · `type` | Inputs → Outputs | Description |

The label is the node's name in the Nodes panel. Pins are written as you
see them on the node. Where the name saved in patch files is different, it
follows in brackets: `layer (shape)` is a pin that reads "layer" and is
saved as `shape`. Pins of one type are grouped (`w, h: number`), a `*`
after a type marks a spread pin, and `—` means no pins. A few nodes change
their pins with a setting (Math, Lookup, Effect …): their cells are
written by hand and describe each setting.

Port types: `number`, `boolean`, `string`, `vec2`, `color`, `mat3`,
`layer`, `style`, `audio`, `image`, `any`.
There is one layer type: on the canvas every layer pin reads `layer` (the
Renderer's `layers` and the geometry nodes' `layers` outputs included,
their spread icon saying they carry several). Pins that tell several
layers apart (`guides`, `inner`, `glyphs`, `cells` …) keep their names.

To check these tables against the node catalogue, run
`python3 -I tools/nodes-doc-check.py` (add `--fix` to rewrite them).

**Pin kinds** — three flavours, distinguished visually:
- *single* (default): round pin, accepts one wire.
- *spread*: round pin in its type's colour, accepts one wire carrying a
  list. On the canvas a small three-bar stack icon (≡) right after the
  pin's name marks it (hover it for "spread"); ForEach / Cache port tabs
  and subpatch outputs that carry a spread show it too.
- *multi* (input only): **taller pill-shaped pin**, accepts N wires; the
  runtime gathers them into a spread. Used by `transform/group` so you
  can drop many layer wires onto one input without a Stack chain.

**Spreads into single pins — per element** (patches saved from
format v4 on). The value nodes — Math, Vector Math, Compare, Logic,
Select, Convert, Map, Norm, Lerp, Clamp, Wrap, Step, Smoothstep, Vec2 /
Split / FromAngle / ToPolar, Color / Split / Set Alpha / Lerp Color /
Gradient (per `t`), Ease, Uniform / Gaussian / Wanderer, MIDI ↔ Freq,
String Length, Image Sample (per `x`, `y`) and the hidden single-op nodes
behind them — run **once per element** when a spread reaches a
single-value pin, the same rule as Expression: the longest spread sets the
count, shorter spreads (and single values) cycle, and an empty spread
empties the result. Every output then carries one value per element.
Lifting starts only when a pin carries two or more values: until then the
node runs exactly as before, an empty pin reading as "nothing" (so a
missing device or an upstream error still falls back to defaults). A
spread *output* (Linear, Grid, Range …) may be wired straight into such a
pin. **Smooth** and **Spring** keep their state per element (a new element
starts at its target). Delay Line, Trigger, Edge, Counter, LFO and the
draw / style / transform / text nodes still read slot 0 of a spread (wrap
them in a ForEach). Files saved before v4 keep their old behaviour: they
load with `legacySlot0` set, where those nodes read slot 0 of a spread and
a spread output wired into a single pin is dropped with a warning — saved
again, they stay that way.

`audio` is a handle to a live sound source that the app's audio engine
keeps alive across frames. Wires of this type carry signal flow: Source
nodes (Oscillator, Noise, …) emit it; Processors (Filter, Gain, …)
consume and re-emit a processed version; the **Output** sink terminates
the chain. Per-frame compute updates parameters on the existing audio
object — the audio nodes themselves only get re-created when the node
type changes or the node is deleted.

## Flow rule

Every patch, subpatch and region reads **left to right**: a wire only runs
from a node to a node to its right (the target's left edge sits right of
the source's). A region or subpatch frame's input pins count as its left
edge and its output pins as its right edge, so wires between a frame and
what it holds always read forward. The editor refuses a wire that would
point back, and the Library samples are checked by a guard test
(`native/crates/floret-engine/tests/flow_rule.rs`).

**Feedback goes through a named Write → Read pair**, never a wire back:
put the **Write** at the end of the flow and its paired **Read** at the
start. The Read gives the value the Write stored last frame (`init` on the
first frame). Placing one half places the other; give the pair a short
name ("ball position", "velocity") and both halves show it. The old
single-box State / State\* loops are hidden from the node browser (old files
still load). See [State](#state).

**Subpatches, sections and loops from a selection:** select nodes and
right-click one of them:

- **Wrap in ForEach / If / Cache** puts the selection inside that region;
- **Wrap in Section** puts it on a titled backdrop (visual only);
- **Wrap in subpatch** (**Group N into subpatch** for several nodes,
  Ctrl+G / ⌘G) folds it into a subpatch;
- right-click a frame for **Unwrap** (a region or section) or **Ungroup
  subpatch**.

The same commands are in the Edit menu.

**The Subpatches panel** (open it from the `+` at the end of the panel
tabs) lists the subpatches at the level the canvas shows, including those
inside a ForEach / If / Cache region there, with how many nodes each
holds. Click one to frame it on the canvas, double-click to enter it, and
press **Up** to go back a level. Right-click → **Save to library…** saves
it to the Library, where it carries a small subpatch mark: clicking it
there inserts a copy into the open patch, in the level you are viewing
(right-click it for **Insert into patch** or **Open on its own**).

## Node browser groups

The Nodes browser and the Tab / double-click search sort nodes by what
they are *for*, in roughly the order a patch is built. The groups don't
change a node's type id, ports or category (the tables below stay
grouped by category, i.e. by type-id prefix); searching still matches the
old category names and type ids.

| Group | What's in it |
|---|---|
| Source | Values, controls and files: Number, String, Slider, Toggle, XY Pad, Select, Stepper, Bang, Button, File, Image URL, SVG, Video File / Frame, CSV. |
| Draw | Shapes and images as layers (Rect, Ellipse, Line, Path, Polygon, Image …), Plot (charts) and the Renderer. |
| Text | Text, Glyphs, Text on Path, Text Outline, Font, Text Params, Font Variations; string Concat / Length / Split / Word Indices. |
| Color & Style | Color, Split Color, Gradient, Lerp (colour), Palette, Set Alpha, Color Picker; Fill, Stroke, Gradient Fill and the other styles. |
| Shape Ops | Translate, Rotate, Scale, Shear, Apply Matrix; Group, Blend, Mask, Cache, Symmetry; Path Ops, Shape Boolean; Hit Test. |
| Patterns | Grid, Ring, Spiral, Fibonacci, Layout, Scatter (incl. stippling), Cells (Voronoi, subdivision), Contours (incl. flow lines), Tiles (Truchet, wave function collapse), L-System, Curve (Lissajous, rose, Spirograph, spirals, stars, superformula), points from a layer; Linear / Range / Sine / Geometric spreads; random walks and cellular automata. |
| Noise & Random | Noise, Uniform, Gaussian, Wanderer2D, Noise Spread, Shuffle, Randomize. |
| Animate | Time, Frame, Delta, Clock, Beat, LFO, Ease, Envelope, Smooth, Spring, Integrate, Delay Line, Trigger, Metro, Counter, Edge, Gate, Latch. |
| Image | Rasterize, Crop, Resize, Dims, Sample, Slicer, Blend (images), pixel get / set. |
| Effects | Effect (bloom, blur, shadow, glow, levels, curves, LUT, key …), Shader, Filter, Adjust, Convolve, Pixelate, Pixel Sort, Blur. |
| Particles & Physics | Emitter, Simulation, World, Bounds, Trail, Particle Shapes, forces, colliders, Walker Cluster; Move (movers on plain lists). |
| Input & Sound | Mouse, Keyboard, MIDI, Camera, Landmarks, Point Tracker; audio in, analysis (FFT, Amplitude …) and synthesis. |
| Math | Math, Vector Math, Compare, Logic, Statistics, Convert, Map, Norm, Lerp, Clamp, Wrap, Step, Smoothstep, Expression, Cumulative, Delta, Histogram, Vec2 / Split / FromAngle / ToPolar, Select, Switch. |
| Lists | Lookup, Sublist, Reorder, Combine (op dropdowns); Count, Count Where, Repeat, Cycle, Resample, Merge, Neighbor; Cross Product, Pairwise, Partition, Windows, Uninterleave, Ungroup. |
| Structure | ForEach, If, Cache regions and their accumulators; State, Read / Write; Subpatch, Section, Comment, Probe. |

**Op families.** Math, Vector Math, Compare, Logic, Statistics and
Convert, and in Lists Lookup, Sublist, Reorder and Combine, are one node
each with an `op` (Convert: `to`) dropdown; the pins follow the op. They
replace 82 single-op nodes (`math/mul`, `vec2/dot`, `logic/lt`,
`convert/to-int`, `spread/take` …), which stay loadable and
compute identically but are hidden from the browser — see each
section's *Legacy single-op nodes*. A search for an op's name ("mul",
"normalize", "xor") offers it directly and places the family node set
to that op.

---

## Value

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Number** · `value/number` | — → `value (number): number` | A constant number. Edit inline. |
| **String** · `value/string` | — → `text: string` | A constant string. Edit inline. |

## Time

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Time** · `time/seconds` | — → `t, loop: number` | Seconds since runtime start, and `loop`: the phase 0..1 through the patch's loop length (File → Export Video / GIF… sets it). Build on `loop` and a recording of one loop repeats seamlessly. |
| **Frame** · `time/frame` | — → `frame: number` | Integer frame counter. |
| **Delta** · `time/delta` | `max: number` → `dt: number` | Seconds since the previous frame. `max` above 0 caps it, e.g. 0.05, so motion doesn't jump after a stall; 0 (the default) leaves it alone. |
| **LFO** · `motion/lfo` | `shape: string, rate, offset, min, max, width: number, reset: boolean, count, spread, bpm, seed: number` → `value, phase: number` | Oscillates between `min` and `max`, `rate` times a second (sine, triangle, saw, ramp-down, square, random, smooth-random); `count` > 1 gives that many staggered copies. `sync` loop: `rate` counts whole cycles per patch loop (at least one) and the phase follows the clock, so a recording of whole loops joins seamlessly — random shapes repeat every loop; `bpm` and `reset` don't apply. |
| **Smooth** · `motion/smooth` | `value: any, rate: number` → `value (result): any` | Eases toward `value` (number, vec2 or colour). Over a spread (v4 files) every element eases on its own. |
| **Clock** · `time/clock` | `offset: number, zone: string` → `hours, minutes, seconds, day, month, year, weekday, yearPhase, days: number` | The wall clock, read every frame (effectful): the time of day in the computer's time `zone` (`local`, from the OS on macOS and Linux; elsewhere UTC) or `utc`, moved by `offset` hours (time travel, other cities). `hours` 0..24, `minutes` / `seconds` 0..60, all running on smoothly (fractions included); `day` 1..31, `month` 1..12, `year`, `weekday` 0 = Monday … 6 = Sunday, `yearPhase` 0..1 through the year, `days` since 1 January 1970 (for cycles longer than a day, e.g. the Moon's 29.53). Sample: `sys-clock`. |
| **Beat** · `time/beat` | `bpm, steps, beats, swing, gate (width): number, run, reset: boolean, sync` → `step, phase, bar: number, gate, trigger: boolean, count: number` | A musical step clock: `bpm`, `beats` per bar, the bar cut into `steps`. `step` 0..steps−1, `phase` 0..1 through the step, `bar`, `gate` (on for the first `gate` fraction of a step), `trigger` (the frame a step starts), `count` (steps since the start). `swing` 0..1 delays every second step (1 = 3:1). `run` off pauses, `reset` restarts. A step due on a frame starts on it (summed frame times are snapped onto whole steps, so 6 steps a second at 60 fps is exactly one every 10 frames). `sync` `loop`: a whole number of bars fills the patch loop (tempo bends a little), so a recorded loop repeats seamlessly. Replaces `floor(t * 6) % 16`, `fract(t * 6) < 0.45`, `mod(ph + dt * bpm / 15, 16)` step counters. Effectful; state survives edits. |

## Motion

Smoothing, easing and time-based shaping. (Ease, Smooth, Spring, LFO, Delay Line and Trigger are in the node browser's Animate group.)

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Integrate** · `motion/integrate` | `rate, start: any*` (number / vec2 / colour), `run, reset: boolean*`, `limit` (+ `min, max: any*` unless `none`), `max dt (maxDt): number` → `value: any*, hit: boolean*` | `value += rate · dt` every frame, per element. Starts at `start`; `reset` (held) returns there; `run` off pauses. `limit`: `clamp` stops at `min` / `max`, `wrap` jumps round (phases, playheads, scrolling), `bounce` runs back and forth; `hit` = the frame it wraps, bounces or arrives. `dt` is capped at `maxDt` (0 = no cap). Changing `rate` never jumps, unlike `t × rate`. Replaces `ph + min(dt, 0.1) * rate`, `p + v * h`, `r ? 0 : min(1, g + dt / 9)`, `fract(s + t / 40)`. Effectful; state survives edits. |
| **Envelope** · `motion/envelope` | `from` (`age: number*` or `trigger: boolean*`), shape pins (`time` / `attack, hold, time` / `frequency, time`: number*) → `value, age: number*, active: boolean*` | A shape in time after an event, per element. `shape`: `decay` (1 → 1 % at `time`), `attack-decay` (ease up over `attack`, `hold`, exponential decay over `time`), `fade` (linear ramps), `boing` (damped sine about 0 at `frequency` Hz, dying over `time`), `spring` (0 → 1 with overshoot, settled by `time`). `from` `age`: seconds since the event; `trigger`: times itself from each rising edge. Rests at 0 before the event (a spring at 1 once settled); `active` while moving. Replaces `exp(-4 * (t - b)) * sin(18 * (t - b))`, `1 - exp(-6 * a) * cos(15 * a)`, `clamp(1 - (t - l) * 1.6, 0, 1)`. Effectful; trigger times survive edits. |
| **Ease** · `motion/ease` | `t: number` → `value (result): number` | Bends a 0..1 progress value through an easing curve; `t` is clamped to 0..1 first. `curve`: `linear`, `inQuad` / `outQuad` / `inOutQuad`, `inCubic` / `outCubic` / `inOutCubic` or `inElastic` / `outElastic` / `inOutElastic` (default `inOutQuad`). "in" starts slowly, "out" ends slowly, "inOut" does both; the elastic curves overshoot and wobble. Wire Time's `loop` or a Slider into `t`. Runs once per element over spreads. |
| **Spring** · `motion/spring` | `value: any` (number, vec2 or colour), `stiffness, damping: number` → `value (result): any` | Follows `value` like a weight on a spring: it overshoots and settles instead of easing straight there. `stiffness` (default 120) is how hard it pulls, `damping` (default 14) how fast the swing dies away; lower damping bounces more. It starts at the first value it sees, so nothing jumps at the start. Steps in small slices of frame time (at most 0.1 s a frame). The pins take the type of what's wired in, and over a spread (v4 files) each element has its own spring. The motion restarts when the patch is recompiled. |
| **Trigger** · `motion/trigger` | `value, threshold: number` → `pulse: boolean` | A one-frame `true` pulse when `value` does something; it never fires on the first frame. `mode` `rising` (default): `value` goes up through 0.5, so a boolean turning on. `change`: `value` differs from last frame. `threshold`: `value` goes up through the `threshold` pin (default 0.5). Turns a continuous signal into an event; Edge also offers a falling mode. Reads slot 0 of a spread (wrap it in a ForEach). |
| **DelayLine** · `motion/delay-line` | `value: any` (number, vec2 or colour), `delay: number` → `value (result): any` | Plays `value` back as it was `delay` seconds ago (default 0.5), interpolating between recorded frames. Until that much history exists it shows the oldest value it has. Keeps at most 4096 samples, and the history clears when the patch is recompiled. For a delayed picture rather than a delayed number, use Echo set to `delay`. Reads slot 0 of a spread (wrap it in a ForEach). |

## Math

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Math** · `math/math` | `op` picks the pins: `a, b, …` (add, subtract, multiply, divide, min, max — number, vec2 or colour, component-wise; the header's `− n +` sets how many), `a, b` (mod, pow), `x` (negate, abs, sign, floor, ceil, round, fract, sqrt, square, exp, log, sin, cos, tan, asin, acos, atan), `y, x` (atan2), `x, y` (length), `x1, y1, x2, y2` (distance) → `value (result): number` (vec2 / colour when the arithmetic ops are wired so) | Arithmetic and maths functions in one node. Each op computes exactly as the single-op node it replaced (the legacy rows below, same pins and edge cases). The header shows the op's symbol (`+`, `×`, `sin`, `x^y` …). |
| **Statistics** · `math/statistics` | `op`; `spread: number*` → `value (result): number` | mean, median, sum, min, max or stddev (population) of a number spread — exactly the old Mean, Median, Sum, Min Spread, Max Spread and StdDev. |
| **Expression** · `math/expression` | one pin per name in the formula (`x, y` to start): `number*`, or `vec2*` / `color*` when read as `p.x` / `c.r` → `value: any` | Type a formula (`expr` param), e.g. `sin(x * 3) + y * t`. Runs once per element over spreads (shorter ones cycle). Built in: `t`, `frame`, `index`, `count`, `pi`, `tau`, `e`. Numbers, vec2s and colours (0..1 channels) with per-component arithmetic; `+ - * / % ^`, comparisons, `&& \|\| !`, `c ? a : b`; maths functions plus `noise`, `vec2`, `length`, `distance`, `dot`, `normalize`, `angle`, `rgb`, `hsl`. No loops or IO; errors show on the node. |
| **Map** · `math/map` | `x, inMin, inMax, outMin, outMax: number` (vec2 / colour when wired so) → `value (result): number` | Remap x from [inMin..inMax] to [outMin..outMax]. Adapts like Math (`valueType`): with vec2s, per component (numbers apply to both). |
| **Norm** · `math/norm` | `x, start, stop: number` → `value (result): number` | Map x from [start, stop] into [0, 1]. Inverse of Lerp. |
| **Lerp** · `math/lerp` | `a, b, t: number` → `value (result): number` | `a + (b − a) × t` |
| **Clamp** · `math/clamp` | `x, min, max: number` (vec2 / colour when wired so) → `value (result): number` | `max(min, min(max, x))`. Adapts like Math: with vec2s, per component — keep a point inside a box. |
| **Wrap** · `math/wrap` | `x, min, max: number` (vec2 / colour when wired so) → `value (result): number` | Wrap x into `[min, max)`. Adapts like Math: with vec2s, per component — wrap a point around the canvas. |
| **Smoothstep** · `math/smoothstep` | `mode` step: `edge0, edge1, x`; window: `edge0, edge1, edge2, edge3, x` → `value (result): number` | step: the smooth 0 → 1 ramp across `[edge0, edge1]`. window: a smooth bump — up across `[edge0, edge1]`, back down to 0 across `[edge2, edge3]`. |
| **Step** · `math/step` | `edge, x: number` → `value (result): number` | A hard threshold: 0 while `x` is below `edge`, 1 once it reaches it. Smoothstep is the soft version of the same cut. Runs once per element over spreads. |
| **Cumulative** · `math/cumulative` | `spread: number*` → `sums: number*` | Running sum. Output length matches input. |
| **Delta** · `math/delta` | `spread: number*` → `differences: number*` | Pairwise differences. Length = N − 1. |
| **Histogram** · `math/histogram` | `spread: number*, bins, min, max: number` → `counts: number*` | Bin counts across `[min, max]`. |

### Legacy single-op nodes

Each is now an op of **Math** (or **Statistics**): `math/mul` is Math set to multiply, `math/sq` square, `math/mag` length, `math/mean` Statistics set to mean … Hidden from the node browser; old patches load and run unchanged. Searching an old name ("mul", "math/sin") offers the op.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Add** · `math/add` | `a, b: number` → `value (result): number` | a + b |
| **Sub** · `math/sub` | `a, b: number` → `value (result): number` | a − b |
| **Mul** · `math/mul` | `a, b: number` → `value (result): number` | a × b (b defaults to 1) |
| **Div** · `math/div` | `a, b: number` → `value (result): number` | a ÷ b (b defaults to 1) |
| **Mod** · `math/mod` | `a, b: number` → `value (result): number` | a mod b (Euclidean) |
| **Pow** · `math/pow` | `a, b: number` → `value (result): number` | aᵇ |
| **Min** · `math/min` | `a, b: number` → `value (result): number` | `min(a, b)` |
| **Max** · `math/max` | `a, b: number` → `value (result): number` | `max(a, b)` |
| **Sin** · `math/sin` | `x: number` → `value (result): number` | `sin(x)` — radians in, −1..1 out |
| **Cos** · `math/cos` | `x: number` → `value (result): number` | `cos(x)` |
| **Tan** · `math/tan` | `x: number` → `value (result): number` | `tan(x)` |
| **Asin** · `math/asin` | `x: number` → `value (result): number` | Arcsine in radians. Input clamped to [-1, 1]. |
| **Acos** · `math/acos` | `x: number` → `value (result): number` | Arccosine in radians. Input clamped to [-1, 1]. |
| **Atan** · `math/atan` | `x: number` → `value (result): number` | Arctangent in radians. |
| **Atan2** · `math/atan2` | `y, x: number` → `value (result): number` | Angle of (x, y) in radians — full [-π, π] range. |
| **Abs** · `math/abs` | `x: number` → `value (result): number` | `abs(x)` |
| **Neg** · `math/neg` | `x: number` → `value (result): number` | −x |
| **Floor** · `math/floor` | `x: number` → `value (result): number` | `floor(x)` |
| **Ceil** · `math/ceil` | `x: number` → `value (result): number` | `ceil(x)` |
| **Round** · `math/round` | `x: number` → `value (result): number` | `round(x)` |
| **Fract** · `math/fract` | `x: number` → `value (result): number` | Fractional part — `x − floor(x)`. Always positive. |
| **Sqrt** · `math/sqrt` | `x: number` → `value (result): number` | `sqrt(x)` |
| **Sq** · `math/sq` | `x: number` → `value (result): number` | x² |
| **Exp** · `math/exp` | `x: number` → `value (result): number` | eˣ |
| **Log** · `math/log` | `x: number` → `value (result): number` | Natural log. `log(x ≤ 0) = 0`. |
| **Mag** · `math/mag` | `x, y: number` → `value (result): number` | Distance from origin: `√(x² + y²)`. |
| **Dist** · `math/dist` | `x1, y1, x2, y2: number` → `value (result): number` | Distance between (x1, y1) and (x2, y2). |
| **Sum** · `math/sum` | `spread: number*` → `value (result): number` | Σ of a number spread. |
| **Mean** · `math/mean` | `spread: number*` → `value (result): number` | Arithmetic mean. Empty spread → 0. |
| **Min Spread** · `math/min-spread` | `spread: number*` → `value (result): number` | Minimum value across a spread. |
| **Max Spread** · `math/max-spread` | `spread: number*` → `value (result): number` | Maximum value across a spread. |
| **Median** · `math/median` | `spread: number*` → `value (result): number` | Median (average of middles when even). |
| **StdDev** · `math/stddev` | `spread: number*` → `value (result): number` | Population standard deviation. |

## Logic

Booleans for `region/if` and conditional flow. Comparisons take numbers; logical ops take booleans; `Select` works on any type.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Compare** · `logic/compare` | `op`; `a, b: number` → `result: boolean` | less, less-equal, greater, greater-equal, equal (`=`) or not-equal — exactly the old Less … NotEqual. The header shows `<`, `≤`, `>`, `≥`, `=`, `≠`. |
| **Logic** · `logic/logic` | `op`; `a, b: boolean` (and, or, xor) or `x: boolean` (not) → `result: boolean` | and, or, not — exactly the old And / Or / Not — and xor (exactly one of a, b true; new). |
| **Select** · `logic/select` | `cond: boolean, a, b: any` → `selected: any` | Ternary — `cond ? a : b`. Pick between any two values. |
| **Switch** · `logic/switch` | `index` mode: `index: number*, 0 (v0), 1 (v1) … N (vN): any*, range`; `first-true` mode: `if0: boolean*, 0 (v0) … N (vN): any*, else: any*` → `out: any*, which: number*` | One of N values of any type (header `− n +` sets N). `index`: value number `index` (`range` clamp / wrap). `first-true`: the value after the first true `if`, else `else` — a ternary chain without a formula. A single selector passes the chosen spread through whole (layers stay layers); a selector spread chooses per element, inputs cycling. `which` = case chosen (−1 = else). Replaces `a ? x : b ? y : z`, `k == 0 ? … : k == 1 ? …`. |

### Legacy single-op nodes

Each is now an op of **Compare** or **Logic**. Hidden from the node browser; old patches load and run unchanged.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Greater** · `logic/gt` | `a, b: number` → `result: boolean` | a > b |
| **Less** · `logic/lt` | `a, b: number` → `result: boolean` | a < b |
| **GreaterEq** · `logic/gte` | `a, b: number` → `result: boolean` | a ≥ b |
| **LessEq** · `logic/lte` | `a, b: number` → `result: boolean` | a ≤ b |
| **Equal** · `logic/eq` | `a, b: number` → `result: boolean` | a == b (numeric) |
| **NotEqual** · `logic/neq` | `a, b: number` → `result: boolean` | a ≠ b (numeric) |
| **And** · `logic/and` | `a, b: boolean` → `result: boolean` | a && b |
| **Or** · `logic/or` | `a, b: boolean` → `result: boolean` | a \|\| b |
| **Not** · `logic/not` | `x: boolean` → `result: boolean` | !x |

## Convert

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Convert** · `convert/convert` | `to`; `in: any` (`deg: number` for radians, `in: number` for degrees) → `value (result)` (number / boolean / string by `to`) | `to` number, integer (rounded), boolean, string, radians (from degrees) or degrees (from a 0..1 turn) — exactly the old ToNumber … ToDegrees. |

### Legacy single-op nodes

Each is now a `to` choice of **Convert**. Hidden from the node browser; old patches load and run unchanged.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **ToString** · `convert/to-string` | `in: any` → `text: string` | Convert any value to a string. Numbers print compactly; vec2 → "x, y"; color → "#rrggbb". |
| **ToNumber** · `convert/to-number` | `in: any` → `value (number): number` | Convert any value to a number. Strings parse; booleans → 0/1; anything else → 0. |
| **ToInt** · `convert/to-int` | `in: any` → `integer: number` | Convert any value to an integer (rounded). |
| **ToBool** · `convert/to-bool` | `in: any` → `bool: boolean` | Convert any value to a boolean (JS truthiness). |
| **ToDegrees** · `convert/to-degrees` | `in: number` → `degrees: number` | Scale a unit value (0..1) into a hue/angle in degrees (0..360). |
| **ToRadians** · `convert/to-radians` | `deg: number` → `radians: number` | Convert an angle in degrees to radians. |
| **Format Number** · `string/format-number` | `value: number*`, `decimals` + mode pins (`separator, pad, sign` / `sign`), `prefix, suffix: string` → `text: string*` | Numbers as text for labels, counters and charts, element by element. Modes: `number` (decimals, thousands `separator`, zero `pad` to that many integer digits, `sign` always shows + / −), `percent` (0.25 → 25%), `compact` (1.2k, 3.4M, 5.6B, 7.8T), `time` (seconds as m:ss, h:mm:ss from an hour, `decimals` of a second). Halves round away from zero; NaN reads "NaN", infinities "∞". |

## Random

Seeded, deterministic noise and PRNGs. Wire `time/frame` into `seed` for per-frame variation, or a `region/foreach` iteration index for one value per slot.

**Patch seed**: every seeded node — these, the spread shufflers / noise / Game of Life seed / random walk, Wind and Turbulence, Emitter and FromLayer, WalkerCluster and the LFO's random shapes — mixes the patch's seed into its own (`EvalCtx::seed`, `floret_engine::seed`). Seed 0, the default, leaves every node's seed as it is; **Vary** (Ctrl/⌘+Shift+R) re-rolls it for a new variation of the same patch. The seed and saved **presets** live in the `.flora` file (optional top-level `seed` / `presets`) and are edited in the Presets panel.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Noise** · `noise/noise` | `pos: vec2*`, `x, y, scale, seed, octaves, falloff, warp, warp scale (warpScale): number*`, `gradient step (step): number` (+ `feature: string` in cellular mode, `angle: number*` in stripes mode) → `noise: number, flow, gradient: vec2` | 2D noise at (pos + (x, y)) · scale, per element over spreads. `mode`: perlin, simplex, value, cellular (Worley; `feature` F1 / F2 / F2-F1) or curl (`flow` = curl of Perlin fBm). Pattern fields in the same pins: checker (1 / 0 squares), stripes (+ `angle: number*`, radians; a smooth wave, one stripe per unit) and rings (round the origin); warp them for marble / wood / op art. `octaves` > 1 = fBm, `falloff` = amplitude ratio; `warp` / `warpScale` domain-warp the point. Perlin mode at defaults equals the old Perlin node, curl mode the old CurlNoise. Field modes (+ `center: vec2*, radius, weight: number*`): `radial` (Σ weight · (1 − (d/r)²)², 0 past the radius) and `gaussian` (Σ weight · e^(−(d/r)²)), measured in `pos` units (`scale` unused) — pressure highs / lows, metaballs, attractor maps. `gradient: vec2` (every mode): the slope per unit of `pos`, uphill (forward differences); steer Move with it. `step` above 0 (the last pin, every mode) takes it by central differences `step` units of `pos` either side instead: the slope at that scale, smoothing finer detail (a weather map's pressure slope over a few kilometres); 0, the default, is the local slope. Replaces `exp(-dot(c, c) / 50000)` blobs and hand-differenced `noise(...)` pressure gradients. |
| **Perlin** · `noise/perlin` | `x, y, seed, octaves, falloff: number` → `noise: number` | 2D Perlin noise in [0, 1]. `octaves` > 1 yields fractal Brownian motion; `falloff` is the amplitude ratio between octaves (default 0.5). Hidden from the palette: use Noise. |
| **Uniform** · `random/uniform` | `min, max, seed: number` → `sample: number` | Uniform random in [min, max). |
| **Gaussian** · `random/gaussian` | `mean, sd, seed: number` → `sample: number` | Gaussian (normal) random with the given mean and standard deviation. |
| **Wanderer2D** · `random/wanderer2d` | `t, speed, area, seed: number` → `position: vec2` | A 2D point that drifts smoothly with Perlin noise, centred on (0, 0). Pure function of `t` — feed `time/seconds` for live drift. |

## Spread

The Lists group. **Lookup**, **Sublist**, **Reorder** and **Combine** are one node each with an `op` dropdown whose pins follow the op; they replace 24 single-op nodes (see *Legacy single-op nodes* below). Kept as nodes of their own: Count and Count Where (one number from a list), Repeat, Cycle and Resample (lists of a given length), Merge (any number of wires into one pin, with its own slot count), Neighbor (its own `edges` dropdown) and the nodes with two or more outputs (Cross Product, Pairwise, Partition, Windows, Uninterleave, Ungroup).

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Lookup** · `spread/lookup` | `op` picks the pins: `spread, index` (at), `source, indices` (pick), `flat, groupSize, index` (group-at), `spread, value` (index-of, indices-of), `spread, points` (nearest), `spread, values` (bin) → `value (result)` | Read from a list: the item at an index (negative from the end, out of range → `null`), the items at an index spread (wrapping), the Nth group of a flat + groupSize pair, or the first index (−1 if missing) / every index where a value occurs. `nearest`: for each of `points`, the index of the nearest point in `spread` (ties: the lower index; −1 when `spread` holds no points) — pick it with `pick`; a uniform grid keeps thousands against thousands cheap (agents finding food, the nearest infected neighbour). `bin`: for each of `values`, the band it falls in between the `spread` thresholds (unsorted is fine): below the lowest 0, at or above the highest `len`, NaN −1 — noise → biome bands, a value → its colour class (then `pick` a colour); replaces `ev < s ? a : ev < s + 0.04 ? b : …` chains. The other ops compute exactly as the nodes they replaced (the legacy rows below). |
| **Sublist** · `spread/sublist` | `op` picks the pins: `spread, count` (take, skip), `spread, start, end` (slice), `spread, index, count` (remove), `spread, keep: boolean*` (keep-where), `spread` (unique) → `value (result): T*` | Keep part of a list, in order: the first N, all but the first N, a `start..end` slice (`end` ≤ 0 counts from the end), everything but a removed range, the items whose parallel boolean is true, or each value once (first occurrence). |
| **Reorder** · `spread/reorder` | `op` picks the pins: `spread` (reverse), `spread, shift` (rotate), `spread, descending` (sort), `spread, keys, descending` (sort-by-key), `spread, indices` (permute), `spread, i, j` (swap) → `value (result): T*` | The same items in a new order: reversed, rotated by N (negative shifts left), sorted numerically, sorted by a parallel key spread, permuted by an index spread (out of range → `null`), or with two items swapped. |
| **Combine** · `spread/combine` | `op` picks the pins: `spread, tail` (append), `head, spread` (prepend), `a, b` (concat, interleave), `spread, index, insertion` (insert), `spread, index, replacement` (replace), `spread, marks, count` (inject) → `value (result): T*` | Put lists together: append or prepend one value, concatenate two lists, interleave them (`a0, b0, a1, b1 …`, to the shorter length), insert a list at an index, replace the item at an index with a list, or inject `marks` at N evenly spaced positions. |
| **Linear Spread** · `spread/linear` | `from, to, count: number` → `values: number*` | A spread from `from` to `to`, of length `count` (inclusive endpoints). |
| **Range Spread** · `spread/range` | `count: number` → `indices: number*` | Integers `0 .. count-1`. |
| **Repeat Spread** · `spread/repeat` | `source: any*, count: number` → `values: T*` | Repeat a value N times. |
| **Randomize Spread** · `spread/randomize` | `source: any*, seed: number` → `values: T*` | Shuffles a spread into a random order; the same `seed` always gives the same order. |
| **Grid Spread** · `spread/grid` | `cols, rows, w, h: number, center: vec2` → `positions: vec2*` | A `cols × rows` grid of points centred on the origin, spanning `(-w/2, -h/2)` to `(w/2, h/2)`. Row-major. |
| **Spiral Spread** · `spread/spiral` | `count, turns, radius, phase, inner: number` → `positions: vec2*` | Archimedean spiral — `count` points across `turns` rotations, radius growing `inner → radius` (`inner` 0, the default: from the centre). |
| **Fibonacci Spread** · `spread/fibonacci` | `count, scale, phase, inner: number` → `positions: vec2*` | Phyllotaxis / sunflower spiral — points at golden-angle increments, radius = `scale · √i`; `inner` > 0 leaves a hole of that radius (`r = √(inner² + scale² · i)`, still evenly spaced). |
| **Ring Spread** · `spread/ring` | `count, radius: number, center: vec2, phase: number` → `positions: vec2*, angles: number*` | `count` points (default 24) evenly spaced round a circle of `radius` (default 100) about `center`; point i sits at the angle `phase + i·TAU/count` (radians, 0 pointing right, turning clockwise on screen). `positions` are ready to place shapes on; `angles` drive orientation, spirals or hue. |
| **Count Spread** · `spread/count` | `spread: any*` → `count: number` | Length of a spread. |
| **Shuffle Spread** · `spread/shuffle` | `spread: number*, seed: number` → `values: number*` | Fisher–Yates shuffle, deterministic per seed. |
| **Resample Spread** · `spread/resample` | `spread: number*, count: number` → `values: number*` | Resample a spread to length `count` (linear interpolation). |
| **Sine Spread** · `spread/sine` | `count, amplitude, frequency, phase, offset: number` → `samples: number*` | `count` samples of `offset + amplitude · sin(2π · frequency · i/count + phase)`. |
| **Geometric Spread** · `spread/geometric` | `start, ratio, count: number` → `values: number*` | Geometric progression `start, start·ratio, start·ratio², …`. |
| **Noise Spread** · `spread/noise` | `count, frequency, phase, seed: number` → `samples: number*` | 1-D Perlin noise sampled at regular intervals, in [0, 1]. |
| **Uninterleave Spread** · `spread/uninterleave` | `spread: any*` → `even, odd: T*` | Split into even- and odd-indexed elements. |
| **Cross Product Spread** · `spread/cross` | `a, b: any*` → `a: T*, b: U*` | Cartesian product as two parallel spreads of length `\|a\|·\|b\|`. |
| **Count Where Spread** · `spread/count-where` | `keep: boolean*` → `count: number` | Number of `true` entries. |
| **Partition Spread** · `spread/partition` | `spread: any*, bin: number` → `flat: T*, groupSize: number*` | Chunk into fixed-size groups; pair with Lookup set to group-at to read a single group. |
| **Pairwise Spread** · `spread/pairwise` | `spread: any*` → `a: T*, b: T*` | Adjacent pairs: `(s[0..n-1], s[1..n])` for line-segment patterns. |
| **Windows Spread** · `spread/windows` | `spread: any*, size: number` → `flat: T*, windowSize: number*` | Sliding windows of length `size`, stride 1. |
| **Cycle Spread** · `spread/cycle` | `spread: any*, count: number` → `values: any*` | Repeat the spread cyclically until it has `count` elements. |
| **Merge** · `spread/merge` | `1 (s1), 2 (s2) … N (sN): T*` → `merged: any*` | Concatenate every numbered slot in order (the header's `− n +` sets how many). Feeds several wires into one input pin. |
| **Neighbor** · `spread/neighbor` | `spread: any*, index, offset: number` → `value: T` | `spread[index + offset]`; the `edges` dropdown says what happens past the ends: clamp, wrap or zero (`null`). Inside a ForEach, wire the region's `index`. |
| **Ungroup** · `spread/ungroup` | `groups: any*` → `1 (out1), 2 (out2), 3 (out3) … N (outN): T*` | One output pin per sub-spread (`count` sets how many); missing groups are empty. |
| **CA Seed Spread** · `spread/ca-seed` | `count, index: number` → `row: boolean*` | The first generation of a one-dimensional cellular automaton: `count` cells (default 64), all dead except one live cell at `index` (negative, the default, means the middle). Feed it to Elementary CA Spread. |
| **Elementary CA Spread** · `spread/elementary-ca` | `row: boolean*, rule: number` → `next: boolean*` | Advances a one-dimensional cellular automaton by one generation using Wolfram's numbered `rule` (0..255, default 90, which draws Sierpinski triangles; 30 is chaotic, 110 is intricate). Each cell's next state comes from itself and its two neighbours, and the row wraps round at the ends. Run it inside a Read\* → Elementary CA → Write\* pair, stacking the rows with Scroll Up Spread to draw the history. |
| **CA Grid Seed Spread** · `spread/ca-grid-seed` | `cols, rows, index: number` → `grid: boolean*` | A blank `cols × rows` grid (defaults 64 × 32) as one flat row-major list, with a single live cell in the bottom row at column `index` (negative, the default, means the middle). The start of a CA picture that scrolls upward, with Scroll Up Spread. |
| **Scroll Up Spread** · `spread/scroll-up` | `grid, newRow: boolean*, cols: number` → `grid: boolean*` | Moves a flat grid up one row of `cols` cells (default 64): the top row falls off and `newRow` is added at the bottom. Empty if the grid's length isn't a whole number of rows. With Last Row Spread and Elementary CA Spread it shows a CA's history as a scrolling picture. |
| **Last Row Spread** · `spread/last-row` | `grid: boolean*, cols: number` → `row: boolean*` | The last `cols` cells (default 64) of a flat grid: its newest row. Empty when the grid is shorter than one row. |
| **Life Seed Spread** · `spread/gol-seed` | `cols, rows, density, seed: number` → `grid: boolean*` | A random `cols × rows` Game of Life grid (defaults 16 × 16) as a flat row-major list. Each cell is alive with chance `density` (0..1, default 0.35); the same `seed` (mixed with the patch seed) gives the same grid. |
| **Life Step Spread** · `spread/gol-step` | `grid: boolean*, cols, rows: number` → `next: boolean*` | Conway's Game of Life, one generation: a live cell with 2 or 3 live neighbours survives, a dead cell with exactly 3 comes alive. The `cols × rows` grid wraps round at the edges, and the two must multiply to the grid's length or the result is empty. Run it inside a Read\* → Life Step → Write\* pair, with Life Seed Spread into the Read\*'s `init`. |
| **Walk Step Spread** · `spread/walk-step` | `trail: vec2*, time, step, seed, bounds: number` → `trail: vec2*` | Advances a random walk by one step: drops the oldest point of `trail` and adds a new one `step` units (default 4) from the last, in a direction that depends on `time` (60 steps a second; wire Time's `t`) and `seed`. A step that would leave the square `bounds` (default 250) either side of (250, 250) is turned back. The trail keeps its length, so feed it back through a Read\* / Write\* pair, starting from a trail of points. |

### Legacy single-op nodes

Each is now an op of **Lookup**, **Sublist**, **Reorder** or **Combine**: `spread/take` is Sublist set to take, `spread/stack` Combine set to concat, `spread/sift` Sublist set to keep-where, `spread/at` Lookup set to at … Hidden from the node browser; old patches load and run unchanged. Searching an old name ("take", "spread/sift") offers the op.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **At Spread** · `spread/at` | `spread: any*, index: number` → `value: T` | One element by index; negative counts from the end; out of range or empty → `null`. |
| **Pick Spread** · `spread/pick` | `source: any*, indices: number*` → `values: T*` | `source[indices[i] mod n]` for each index; out-of-range indices wrap. |
| **At Group Spread** · `spread/at-group` | `flat: any*, groupSize: number*, index: number` → `values: T*` | Pick the Nth group as a sub-spread; negative index counts from the end. |
| **Index Of Spread** · `spread/index-of` | `spread: any*, value: any` → `index: number` | First index of `value`, or `-1` if missing. |
| **Indices By Value Spread** · `spread/indices-by-value` | `spread: any*, value: any` → `indices: number*` | All indices matching `value`. |
| **Take Spread** · `spread/take` | `spread: any*, count: number` → `values: T*` | First N elements. `Take(0)` yields an empty spread. |
| **Skip Spread** · `spread/skip` | `spread: any*, count: number` → `values: T*` | Drop the first N elements. |
| **Slice Spread** · `spread/slice` | `spread: any*, start, end: number` → `values: T*` | Elements `[start..end)`. Negative `end` means count-from-end. |
| **Remove Slice Spread** · `spread/remove-slice` | `spread: any*, index, count: number` → `values: T*` | Delete `count` elements starting at `index`. |
| **Sift Spread** · `spread/sift` | `spread: any*, keep: boolean*` → `values: T*` | Keep elements where the parallel boolean is true. |
| **Unique Spread** · `spread/unique` | `spread: any*` → `values: T*` | Deduplicate, preserving first occurrence. |
| **Reverse Spread** · `spread/reverse` | `spread: any*` → `values: any*` | Reverse element order. |
| **Rotate Spread** · `spread/rotate` | `spread: any*, shift: number` → `values: T*` | Cyclic shift by `shift` positions; negative shifts left. |
| **Sort Spread** · `spread/sort` | `spread: number*, descending: boolean` → `values: number*` | Numeric sort. `descending` flips order. |
| **Sort By Key Spread** · `spread/sort-by-key` | `spread: any*, keys: number*, descending: boolean` → `values: T*` | Sort the value spread by a parallel number spread. |
| **Permute Spread** · `spread/permute` | `spread: any*, indices: number*` → `values: T*` | Reorder by an index spread; out-of-range yields `null`. |
| **Swap Spread** · `spread/swap` | `spread: any*, i, j: number` → `values: T*` | Swap two elements by index. |
| **Append Spread** · `spread/append` | `spread: any*, tail: any` → `values: T*` | Append a single value. |
| **Cons Spread** · `spread/cons` | `head: any, spread: any*` → `values: T*` | Prepend a single value. |
| **Stack Spread** · `spread/stack` | `a, b: any*` → `values: T*` | Concatenate two spreads: `a ++ b`. |
| **Interleave Spread** · `spread/interleave` | `a, b: any*` → `values: T*` | `[a0,b0,a1,b1,…]`, truncated to the shorter input. |
| **Insert Slice Spread** · `spread/insert-slice` | `spread: any*, index: number, insertion: any*` → `values: T*` | Insert a spread at `index` without overwriting. |
| **Set Slice Spread** · `spread/set-slice` | `spread: any*, index: number, replacement: any*` → `values: T*` | Replace the element at `index` with the replacement spread (length grows when replacement > 1). |
| **Inject Spread** · `spread/inject` | `spread, marks: any*, count: number` → `values: T*` | Insert `marks` at `count` evenly-spaced positions in `spread`. |

## Vec2

Operations on 2D vectors. Default for any unconnected `vec2` input is the origin `(0, 0)`.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Vector Math** · `vec2/math` | `op`; `v: vec2` (length, heading, normalize), `a, b: vec2` (distance, dot, angle-between, project), `v: vec2` + `mag` / `max` / `s` / `angle: number` (set-length, limit, scale, rotate), `v, normal: vec2` (reflect) → `value (result): number` (length, distance, dot, angle-between, heading) or `vec2` | Vector operations in one node, each exactly the old single-op node. Adding / subtracting / multiplying vec2s is Math; blending them is Lerp. |
| **Vec2** · `vec2/make` | `x, y: number` → `vector: vec2` | Build a vec2 from (x, y). |
| **Split** · `vec2/split` | `v: vec2` → `x, y: number` | Decompose a vec2 into x and y. |
| **FromAngle** · `vec2/from-angle` | `angle, mag: number` → `vector: vec2` | Unit vector at `angle` radians, scaled by `mag`. |
| **ToPolar** · `vec2/to-polar` | `v: vec2` → `angle, radius: number` | Cartesian to polar: `angle` is the vector's direction in radians (−π..π, 0 pointing right, positive turning clockwise on screen) and `radius` its length. FromAngle turns them back into a vector. Runs once per element over spreads. |

### Legacy single-op nodes

Add / Sub / Mul / Lerp were already folded into the adaptive Math / Lerp; the rest are ops of **Vector Math**. Hidden from the node browser; old patches load and run unchanged.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Add** · `vec2/add` | `a, b: vec2` → `vector: vec2` | a + b |
| **Sub** · `vec2/sub` | `a, b: vec2` → `vector: vec2` | a − b |
| **Mul** · `vec2/mul` | `a, b: vec2` → `vector: vec2` | Component-wise multiply. |
| **Scale** · `vec2/scale` | `v: vec2, s: number` → `scaled: vec2` | Multiply a vec2 by a scalar. |
| **Lerp** · `vec2/lerp` | `a, b: vec2, t: number` → `vector: vec2` | Component-wise lerp. |
| **Mag** · `vec2/mag` | `v: vec2` → `length: number` | `‖v‖` |
| **Dist** · `vec2/dist` | `a, b: vec2` → `distance: number` | `‖b − a‖` |
| **Dot** · `vec2/dot` | `a, b: vec2` → `dot: number` | a · b |
| **Heading** · `vec2/heading` | `v: vec2` → `angle: number` | `atan2(y, x)` — vector angle in radians. |
| **AngleBetween** · `vec2/angle-between` | `a, b: vec2` → `angle: number` | Signed angle from a to b, in radians. |
| **Normalize** · `vec2/normalize` | `v: vec2` → `normalized: vec2` | Unit vector in the same direction. Zero stays zero. |
| **SetMag** · `vec2/set-mag` | `v: vec2, mag: number` → `vector: vec2` | Rescale the vector to the given magnitude. |
| **Limit** · `vec2/limit` | `v: vec2, max: number` → `vector: vec2` | Clamp the magnitude to at most `max`. |
| **Rotate** · `vec2/rotate` | `v: vec2, angle: number` → `rotated: vec2` | Rotate the vector by `angle` radians around the origin. |

## Color

Channel conventions match the Fill node: r/g/b/a are 0..255 on node boundaries; h is 0..360; s/l are 0..100.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Color** · `color/color` | `mode` picks the pins: `r, g, b, a: number` (rgb), `h, s, l, a: number` (hsl), `hex: string` (hex) → `color: color` | Builds a colour in one node. `rgb`: `r`, `g`, `b` and `a` are 0..255 (all default 255, opaque white). `hsl`: `h` 0..360, `s` and `l` 0..100 (default 0, 100, 50), `a` 0..255. `hex`: `"#rrggbb"`, with or without the `#`; anything else reads as white. In `rgb`, out-of-range channels are clamped; in `hsl`, `h` wraps round the colour wheel. Runs once per element when a spread reaches a pin. Replaces RGB, HSL and Hex, which stay loadable but are hidden. |
| **SplitColor** · `color/split` | `color: color` → `r (c1), g (c2), b (c3), a: number` | Takes a colour apart. `mode` `rgb` (default): the first three outputs are r, g and b, 0..255. `mode` `hsl`: they are h (0..360), s and l (0..100). `a` is always the alpha, 0..255. Runs once per element over a spread of colours. Replaces SplitRGB and SplitHSL, which stay loadable but are hidden. |
| **RGB** · `color/rgb` | `r, g, b, a: number` → `color: color` | Build a color from r/g/b (0..255) and alpha (0..255). |
| **HSL** · `color/hsl` | `h, s, l, a: number` → `color: color` | Build a color from hue (0..360), saturation/lightness (0..100), alpha (0..255). |
| **Hex** · `color/hex` | `hex: string` → `color: color` | Parse `"#rrggbb"` into a color. |
| **SplitRGB** · `color/split-rgb` | `color: color` → `r, g, b, a: number` | Decompose into r/g/b/a (0..255). |
| **SplitHSL** · `color/split-hsl` | `color: color` → `h, s, l, a: number` | Decompose into h (0..360), s/l (0..100), a (0..255). |
| **SetAlpha** · `color/set-alpha` | `color: color, a: number` → `color: color` | Replace the alpha channel (0..255). |
| **Lerp** · `color/lerp` | `a, b: color, t: number` → `color: color` | Blend a → b. `space` param: rgb (per channel, the default), oklab (perceptually even) or oklch (round the hue wheel). |
| **Gradient** · `color/gradient` | `colors: color*, t: number` → `color: color` | Multi-stop gradient lookup; stops are equally spaced and `t` is clamped to [0, 1]. `space` param as on Lerp. |
| **Palette** · `color/palette` | `stops`: `1 (c0), 2 (c1) … N (cN): color`, one pin per swatch; the other modes have their own pins → `colors: color*` | A list of colours. `mode`: stops (authored swatches, the default), cosine (`a + b·cos(2π(c·t + d))`, `stopCount` colours), image (`image, sort`: the `stopCount` dominant colours of a picture — k-means in OKLab over its opaque pixels, the same picture always giving the same palette — sorted by `lightness`, darkest first, `frequency`, most of the picture first, or `hue`, greys first), or a harmony of a `base` colour — complementary, analogous, triadic, split, tetradic (HSL hue turns; analogous / split take `angle`). |

## Transform

All transform nodes use `push`/`pop` so they don't leak state into sibling layers.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Group** · `transform/group` | `1 (s1), 2 (s2) … N (sN): layer*` → `layer (grouped): layer` | Combine N layers into one layer. The header's `− n +` sets how many pins (2 by default); each takes a layer or a spread of layers, appended in pin order. Lets a single Transform reposition a whole ForEach result as one unit. |
| **Translate** · `transform/translate` | `by: vec2, layer: layer` → `layer (translated): layer` | Translate an inner Layer by (x, y). |
| **Rotate** · `transform/rotate` | `angle: number, layer: layer` → `layer (rotated): layer` | Rotate an inner Layer by `angle` radians. |
| **Scale** · `transform/scale` | `sx, sy: number, layer: layer` → `layer (scaled): layer` | Scale an inner Layer by (sx, sy). |
| **ShearX** · `transform/shear-x` | `angle: number, layer: layer` → `layer (sheared): layer` | Shear an inner Layer along the X axis by `angle` radians. |
| **ShearY** · `transform/shear-y` | `angle: number, layer: layer` → `layer (sheared): layer` | Shear an inner Layer along the Y axis by `angle` radians. |
| **ApplyMatrix** · `transform/apply-matrix` | `a, b, c, d, e, f: number, layer: layer` → `layer (transformed): layer` | Multiply the current transform by `[[a, c, e], [b, d, f], [0, 0, 1]]`. Defaults are identity. |
| **Blend** · `transform/blend` | `mode: string, layer: layer` → `layer (blended): layer` | Draw the inner Layer with a blend mode (add / multiply / screen / difference / overlay / …), then restore. |
| **Cache** · `transform/cache` | `layer: layer*, w, h: number` → `layer (rasterized): layer` | Rasterise a Layer (or spread) into a persistent offscreen canvas, then blit it each frame. Re-renders only when upstream inputs change — the runtime already memoises compute, so a stable layer chain stays bitmap-cached at ~0.07ms / frame regardless of how many primitives it contains. Massive speedup for static or rarely-updated scenes (ForEach grids, dense generative art, hand-drawn backgrounds). `w` / `h` default to 0 → match the active renderer's canvas size. Skip Cache for layers that change every frame — it would re-rasterise every frame and add overhead. |
| **Mask** · `transform/mask` | `layer, mask: layer*, invert: boolean` → `layer (masked): layer` | Show `layer` through `mask`. The `mode` dropdown picks the coverage: the mask's `alpha`, or its `luminance` (white shows, black hides, transparent hides). `invert` shows the layer where the mask is empty instead. |
| **Symmetry** · `transform/symmetry` | `layer: layer*` + mode pins (`count, center, angle, clip` / `group, cell, center, angle, size, clip`) → `layer (out): layer` | Repeat a layer by a symmetry. `rotate`: `count` copies turned evenly round `center` (n-fold), the first at `angle` (radians). `kaleidoscope` (default): the same with `count` mirrors through `center`, every other copy reflected. `wallpaper`: one of the 17 plane `group`s (p1, p2, pm, pg, cm, pmm, pmg, pgg, cmm, p4, p4m, p4g, p3, p3m1, p31m, p6, p6m) over `cell`-wide lattice cells (hexagonal for p3 / p6 groups), turned by `angle`, filling `size` round `center` — its mirrors and rotation centres pass through `center`, so draw the motif near it, about a cell across. `clip`: round modes cut each copy to its wedge so copies meet edge to edge (on by default for kaleidoscope); wallpaper cuts the pattern to `size`. At most 20 000 copies. |

## Draw

Shapes accept an optional `style` input. Unconnected = white fill, no stroke (or for Line: black stroke, no fill).

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Rect** · `draw/rect` | `pos: vec2, w, h: number, anchor: string, style: style` → `layer (shape): layer` | Rectangle. |
| **Ellipse** · `draw/ellipse` | `pos: vec2, w, h: number, anchor: string, style: style` → `layer (shape): layer` | Ellipse. |
| **Line** · `draw/line` | `a, b: vec2, style: style` → `layer (shape): layer` | Line segment. Defaults to black stroke / no fill. |
| **Arc** · `draw/arc` | `pos: vec2, w, h, start, stop: number, anchor: string, style: style` → `layer (shape): layer` | Arc on an ellipse from `start` to `stop` (radians). |
| **Triangle** · `draw/triangle` | `a, b, c: vec2, style: style` → `layer (shape): layer` | Triangle from three corners. |
| **Quad** · `draw/quad` | `a, b, c, d: vec2, style: style` → `layer (shape): layer` | Quadrilateral from four corners. |
| **Point** · `draw/point` | `pos: vec2, style: style` → `layer (shape): layer` | Single point. Uses stroke colour (not fill). |
| **Polygon** · `draw/polygon` | `points: vec2*, closed: boolean, style: style, widths: number*` → `layer (shape): layer` | Polygon from a spread of vec2 vertices. When `closed`, fills with `style` (default white fill); when open, strokes only. |
| **Path** · `draw/path` | `points: vec2*, color: color, strokeWidth, tension: number, widths: number*` → `layer (shape): layer` | A stroked open line through the points; it is never filled (use Polygon for that). `tension` 0 (default) joins the points with straight segments, up to 1 rounds them into a Catmull–Rom curve through every point. `strokeWidth` (default 2) is the line width and `color` defaults to black. Wire `widths` (one per point) to taper the line, which draws it as a filled ribbon in `color`. |
| **Text** · `draw/text` | `string: string, pos: vec2, size: number, align: string, style: style` → `layer (shape): layer` | Draw a text string at (x, y) with the given size. `align`: left/center/right. |
| **Image** · `draw/image` | `image: image, pos: vec2, w, h: number, anchor: string, tint: color` → `layer (shape): layer` | Draw an image at (x, y) sized (w, h). Zero in `w` or `h` uses the image's natural size. `tint` multiplies a color into every pixel. |
| **ImageRegion** · `draw/image-region` | `image: image, sx, sy, sw, sh, dx, dy, dw, dh: number, tint: color` → `layer (shape): layer` | Draw a sub-rectangle of an image — source `(sx, sy, sw, sh)` onto destination `(dx, dy, dw, dh)`. Zero in `sw/sh` means "to the image edge"; zero in `dw/dh` means "match source size". |
| **ImageTile** · `draw/image-tile` | `image: image, pos: vec2, w, h, cols, rows: number, anchor: string, tint: color` → `layer (shape): layer` | Tile an image `cols × rows` times across a `(w × h)` rect anchored at `(x, y)`. Cell size = `(w/cols, h/rows)`. |
| **WalkerCluster** · `draw/walker-cluster` | `colors: color*, pos: vec2, size, density, steps, seed: number` → `layer (shape): layer` | A generative picture of random walkers: they spawn on a grid and each draws a trail of lines or of paired circles that fade as it goes, in colours drawn (with slight variation) from `colors`, or from a built-in 14-colour palette when none is wired. It fills a `size × size` square (default 140) centred on `pos`. `density` (0..2, default 1) is how many walkers spawn, `steps` (default 200) how long each walks, and `seed` fixes the result. |

## Text

Generative typography. Shaping (kerning, ligatures, Arabic and other scripts, right-to-left runs, combining marks) and font fallback happen in the engine; `text/glyphs` and `text/outline` lay text out exactly like `draw/text` given the same inputs, so their outputs sit on the drawn text.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Font** · `text/font` | `family, file: string` → `font: string, loaded: boolean, axes: string*, axisMin, axisDefault, axisMax: number*` | An installed family (dropdown) or a font file (.ttf/.otf/.ttc/.woff/.woff2, picked or typed as a path; it wins). `font` goes into `style/text-params`. Lists the variation axes. |
| **Glyphs** · `text/glyphs` | `string: string, pos: vec2, size: number, align: string, spacing: number, style: style` → `layer (shape): layer, glyphs: layer*, positions: vec2*, advances: number*, chars: string*, charIndex: number*` | Split text into shaped glyphs for kinetic type: per glyph a layer at the origin (translate it to its position), its baseline position, advance, characters and index. |
| **Text on Path** · `text/on-path` | `string: string, points: vec2*, path: layer, closed: boolean, size, offset, spacing: number, align: string, shift: number, style: style` → `layer (shape): layer, glyphs: layer*, positions: vec2*, angles: number*` | Lay text along a polyline or a path layer; each glyph turns to the tangent. `offset` slides it along, `shift` lifts it off the path. |
| **Text Outline** · `text/outline` | `string: string, pos: vec2, size: number, align: string, spacing: number, mode: string, style: style` → `points: vec2*, contours, glyphs: number*` | Points every `spacing` units along the glyph outlines (`outline`) or on a grid inside them (`fill`), for particles from type and morphing. |
| **Text Animate** · `text/animate` | `text: layer*, progress, stagger: number, order, ease: string, offset: vec2, rotate, scale, opacity, seed: number` → `layer (shape): layer, glyphs: layer*, amounts: number*` | Glyph-by-glyph animation without a ForEach, on `draw/text`, Glyphs or Text on Path. `in`: each glyph eases from its start state (moved by `offset`, turned by `rotate` radians, at `scale` and `opacity`) to its place as `progress` runs 0 → 1; `stagger` 0 moves all together, 1 one after another, in `order` (forward, backward, center, edges, random). Run `progress` 1 → 0 to animate out. `wave`: `progress` is a phase; glyphs swing by `offset` / `rotate` and towards `scale` / `opacity` on a sine, `stagger` cycles apart. Glyphs turn and scale about their centres; `amounts` is each glyph's eased progress (or wave), for colours or anything else per glyph. |

## String

Text as data: building, measuring and cutting strings (Text above is for typography).

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Concat** · `string/concat` | `parts: string*, separator: string` → `text: string` | Joins strings into one. Wire several strings into `parts` (it takes many wires, or a list) and they're joined in order with `separator` between them (default none). Numbers and booleans are written out as text. |
| **Length** · `string/length` | `text: string` → `length: number` | How many characters `text` has: letters, not bytes, so an emoji counts as one. Runs once per element over a list of strings. |
| **Split** · `string/split` | `text, by: string` → `parts: string*` | Cuts `text` into a list. `by` empty (the default) gives one item per character; `" "` gives words, `","` comma-separated items. Neighbouring separators leave empty items. Pair with Lookup to pick one, or ForEach to build something per piece. |
| **WordIndices** · `string/word-indices` | `text, by: string` → `indices: number*` | For every character of `text`, the number of the word it belongs to (0, 1, 2 …); any character in `by` (default a space) separates words. A separator takes the number of the word it ends, so per-character colouring or animation stays continuous. |

## Image

Image manipulation. The `image` wire carries a picture — loaded by `io/image-url` or `io/file`, derived by one of the nodes below, or rendered from a Layer via `image/from-layer`. Derived pictures are remade only when an input changes, so the result is reused across frames.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Dims** · `image/dims` | `image: image` → `width, height, aspect: number` | Pixel dimensions of an image. `aspect` is `width / height`. All zero while loading. |
| **Sample** · `image/sample` | `image: image, x, y: number` → `color: color` | Read one pixel as a Color. Coordinates clamp to the image bounds. |
| **Sample Spread** · `image/sample-spread` | `image: image, points: vec2*` → `colors: color*` | Read a spread of points from an image, one Color per point. One full-bitmap read amortised across all points — cheap for thousands. |
| **Crop** · `image/crop` | `image: image, sx, sy, sw, sh: number` → `image: image` | New image containing the source's `(sx, sy, sw, sh)` rectangle, sized `(sw, sh)`. Zero in `sw/sh` runs the rect to the image edge. |
| **Slicer** · `image/slicer` | `image: image, cols, rows: number` → `images: image*` | Split an image into a `cols × rows` grid of sub-images (row-major spread). Each sub-image is sized `(srcW/cols, srcH/rows)`. Capped at 256 cells. |
| **Resize** · `image/resize` | `image: image, w, h: number` → `image: image` | New image scaled to `(w, h)`. Zero on an axis keeps the source's dimension on that axis. |
| **Filter** · `image/filter` | `image: image, level: number` → `image: image` | Apply a filter. `mode` param: gray / invert / threshold / posterize / blur / dilate / erode / opaque. `level` tunes threshold / posterize / blur. |
| **Adjust** · `image/adjust` | `image: image, brightness, contrast, saturate, hue: number` → `image: image` | Colour-corrects a picture. `brightness`, `contrast` and `saturate` are multipliers: 1 (the default) leaves it alone, 0 gives black, flat grey and greyscale respectively, above 1 pushes further. `hue` turns every colour round the wheel by that many degrees (default 0). A new picture is made only when an input changes. |
| **Convolve** · `image/convolve` | `image: image, amount: number` → `image: image` | Runs a 3×3 kernel over the picture. `mode`: `edge` (outlines), `edge-sobel` (a smoother gradient edge map), `sharpen` or `emboss` (grey relief). `amount` 0..1 (default 1) mixes the original with the filtered result. Alpha is kept and the border pixels reuse their nearest neighbours. |
| **Pixelate** · `image/pixelate` | `image: image, size: number` → `image: image` | A chunky mosaic: the picture is shrunk by `size` (whole pixels, at least 1, default 8) and blown back up without smoothing, keeping its original dimensions. |
| **Blend** · `image/blend` | `a, b: image, sx, sy, sw, sh, dx, dy, dw, dh: number` → `image: image` | Composite B into A at `(dx, dy, dw, dh)` from B's `(sx, sy, sw, sh)` region with the chosen blend mode. `mode` param: normal / add / darken / lighten / difference / exclusion / multiply / screen / overlay / hard-light / soft-light / dodge / burn. Output sized to A. |
| **Rasterize** · `image/from-layer` | `layer: layer*, w, h: number` → `image: image` | Render a Layer (spread) into an offscreen picture and emit it as an `image`. The bridge from drawing to data — unlocks feedback loops and using patch output as a texture. |

## Pixel

Raw pixel access for data art: read an image as a list of colours, work on it with the list and colour nodes, and write it back. Pixels run row by row from the top left, and the picture keeps its size.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **GetPixels** · `pixel/get-pixels` | `image: image` → `pixels: color*, width, height: number` | Every pixel of the picture as a list of colours, row-major from the top left. `width` and `height` say where the rows break (zero while there is no picture). Big pictures give big lists, so Resize or Crop first. |
| **SetPixels** · `pixel/set-pixels` | `image: image, pixels: color*` → `image: image` | A copy of the picture with its pixels replaced, in the same order, by the colours in `pixels`. A short list changes only the start, and an empty entry leaves its pixel alone. The inverse of GetPixels. |
| **GetRow** · `pixel/get-row` | `image: image, y: number` → `pixels: color*` | One row of the picture as colours, left to right. `y` counts whole pixels from the top and is clamped into the picture. |
| **GetCol** · `pixel/get-col` | `image: image, x: number` → `pixels: color*` | One column of the picture as colours, top to bottom. `x` counts whole pixels from the left and is clamped into the picture. |
| **SetRow** · `pixel/set-row` | `image: image, y: number, pixels: color*` → `image: image` | A copy of the picture with row `y` overwritten by the colours, left to right. A list longer than the row is cut off; a shorter one writes only the start. |
| **SetCol** · `pixel/set-col` | `image: image, x: number, pixels: color*` → `image: image` | A copy of the picture with column `x` overwritten by the colours, top to bottom. A list longer than the column is cut off; a shorter one writes only the start. |
| **PixelSort** · `pixel/sort` | `image: image, lo, hi: number` → `image: image` | Glitch-style pixel sorting. Along every row (or column, `axis`), each run of neighbouring pixels whose `criterion` lies between `lo` and `hi` (0..1, defaults 0.25 and 0.8) is sorted by it, `ascending` or `descending`; pixels outside the range stay put and break the runs. `criterion`: brightness, hue, saturation, lightness, red, green or blue. |

## Video

Video files as images. Frames come out on the `image` wire like any other image, so `draw/image` and the Image / Pixel nodes take them directly. MP4 / MOV with H.264 plays everywhere; on macOS anything QuickTime plays (HEVC, ProRes, …). Audio tracks are ignored. The file is referenced by path (pick it on the node), not copied into the patch.

Both nodes also play **image sequences**, numbered PNG / JPEG frames, at their `fps` pin (default 30; ignored for video files): pick one frame of a run and the rest are found (`name-0001.png, name-0002.png, …`, as the PNG-sequence export writes them; zero-padded numbers match their own width, unpadded ones any), or give a folder (every PNG / JPEG in natural order), a pattern (`name_####.png`, `name_%04d.png`, `name_%d.png`) or a list of frames one per line (paths or `data:` URLs — how a patch can carry a short sequence inside itself). Frames decode on the playback thread into the same bounded cache as video frames.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **VideoFile** · `video/file` | `url: string, play, loop: boolean, speed, scrub, fps: number` → `image: image, time, duration, width, height: number, ready: boolean` | Play a video file. `speed` is a rate (negative plays backwards). Wiring a 0..1 number into `scrub` overrides playback and holds that position; unwire it and playback carries on from there. The playhead survives patch edits and restarts with the transport. `ready` is false until the first frame is decoded. |
| **VideoFrameAt** · `video/frame-at` | `url: string, time, fps: number` → `image: image, duration, width, height: number, ready: boolean` | One frame of a video at `time` seconds, without playback: random access for stills, contact sheets or sensor-driven scrubbing in seconds. |

## Style

Style nodes output a `Style` (a bundle of drawing settings: fill, stroke, text …). Chain them via `base`: the base style runs first, then the node's override.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Fill** · `style/fill` | `color: color, base: style` → `style: style` | Sets the fill colour for downstream shapes: wire a Color node into `color`. Honors alpha. |
| **Stroke** · `style/stroke` | `color: color, width: number, align: string, base: style` → `style: style` | Sets the outline. `color` (black while unwired) and `width` (default 1; zero or less leaves the width the `base` style had). `align` is `center` (default), `inside` or `outside` the outline; it applies to closed shapes. |
| **StrokeWeight** · `style/stroke-weight` | `w: number, base: style` → `style: style` | Set stroke thickness in pixels. |
| **StrokeCap** · `style/stroke-cap` | `cap: string, base: style` → `style: style` | Line end-cap shape: round / square / project. |
| **StrokeJoin** · `style/stroke-join` | `join: string, base: style` → `style: style` | Stroke join: miter / bevel / round. |
| **RectMode** · `style/rect-mode` | `mode: string, base: style` → `style: style` | How `(x, y, w, h)` is interpreted by Rect: corner / center / corners / radius. |
| **EllipseMode** · `style/ellipse-mode` | `mode: string, base: style` → `style: style` | How `(x, y, w, h)` is interpreted by Ellipse: corner / center / corners / radius. |
| **NoFill** · `style/no-fill` | `base: style` → `style: style` | Disable fill. |
| **NoStroke** · `style/no-stroke` | `base: style` → `style: style` | Disable stroke. |
| **Blur** · `style/blur` | `amount: number, base: style` → `style: style` | Softens everything drawn with this style (fill, stroke and text alike) with a Gaussian blur of `amount` pixels. 0, the default, is sharp, and negative values count as 0. To blur a whole layer or picture use Effect. |
| **FontVariations** · `style/font-variations` | `weight, width, slant, opticalSize: number, axis: string, value: number, base: style` → `style: style` | Variable-font axes (`wght`, `wdth`, `slnt`, `opsz`, plus any tag), animatable per frame. 0 leaves an axis at the font default; chain nodes for more custom axes. |
| **TextParams** · `style/text-params` | `font: string, bold, italic: boolean, align, baseline: string, kerning, lineSpacing: number, base: style` → `style: style` | Sets how downstream text is typeset. `font` is a family name (default `sans-serif`; wire Font's `font` output to use a font file), `bold` and `italic` switch the styles on, `align` is left / center / right, `baseline` is alphabetic / top / middle / bottom, `kerning` adds that many pixels between letters (0 or more, default 0) and `lineSpacing` is the line height as a multiple of the size (default 1.2). Chain through `base`. |
| **Gradient Fill** · `style/gradient` | `colors: color*, positions: number*`, geometry per `mode` (`start, end: vec2` for linear), `base: style` → `style: style` | Fill (or, with `target`, stroke) downstream shapes with a `linear` (start / end), `radial` (center / radius) or `conic` (center / angle, radians, clockwise from pointing right) gradient through the `colors` spread, evenly spaced unless `positions` (0..1) are given. `units`: `shape` places the geometry in each shape's own box (−1..1 across it, centre origin; particles and glyph runs get one each), `canvas` in scene units. `extend`: `pad`, `repeat` or `mirror` past the ends. Colours blend in premultiplied RGB. Drawn per pixel on the GPU. |

## Render

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Renderer** · `render/renderer` | `layer (layers): layer*, background: color, width, height: number` → `layer (layers): layer*, background: color` | The patch sink. Connect layers and (optionally) a background colour. Inline `width` / `height` params set the canvas size. |
| **Effect** · `gpu/effect` | `input: layer*`, pins per `mode` (`threshold, radius, intensity: number` for bloom) → `layer (out): layer` | A post-effect on a layer, on the GPU. `bloom` (threshold, radius px, intensity: bright parts glow), `grain` (amount, size px, seed: animated film grain), `displace` (amount px; noise scale / speed / seed, or a wired `map` layer whose red pushes right and green down), `dither` (levels, pixel size: ordered Bayer), `halftone` (cell size px, angle: dots sized by brightness, in the image's colours), `chromatic` (amount px at the frame edge: red out, blue in), `vignette` (amount, radius, softness, colour: darkens towards the corners, the background too). `blur`, by `kind`: gaussian (radius), lens (radius, highlights: a disc kernel, bright points open into bokeh), motion (amount px along angle), zoom (amount 0..1 towards `center`, streaks radially), spin (angle radians round `center`); `center` is a canvas point. `shadow` (offset, radius, colour, opacity, inner) and `glow` (`blend`, radius, colour, intensity, inner): of the layer's coverage, not its brightness like bloom; `inner` puts them inside the shape. Glow's `blend` `normal` lays it under the layer; `add` adds it as light, over the layer and whatever lies under the halo, so overlapping glows build up. `barrel` (amount: + barrel, − pincushion; zoom). `edges` (threshold, width px, colour, keep: Sobel on brightness and coverage, `keep` 1 leaves the layer under the lines). Colour: `posterize` (levels per channel), `threshold` (`channel` luma: level, softness, low / high colours; alpha: cuts the coverage at level — blur first for metaballs), `levels` (in black / white, gamma, out black / white), `curves` (five colour pins = the curve's points at 0, ¼, ½, ¾, 1 per channel, monotone cubic; defaults are a straight line), `lut` (`lut`: a `.cube` 3D or 1D table — a File node's `url`, a path or URL; amount; loads off the tick), `key` (chroma key: colour, tolerance, softness, spill). Text: `ascii` (size px: cell height, cells half as wide; chars: a ramp darkest first, default ` .:-=+*#%@`; colored: each character in its cell's colour, else `color`; background: behind the characters) — the layer as characters picked by each cell's brightness, from a glyph atlas the node rasterises from the bundled DejaVu Sans Mono. Covers the whole canvas, like Shader; it animates even when the patch is idle. |
| **Shader** · `gpu/shader` | `input: layer*`, one pin per `uniform` → `layer (out): layer` | A ShaderToy-style fragment shader (`mainImage`, `iTime`, `iResolution`, `iMouse`, `iFrame`, `iChannel0–3`), with optional Buffer A–D passes; edit it in the Shader panel. Wire a layer into `input` for post-effects, or leave it for generative work. `feedback` gives the Image pass its own last frame. `history` (frames, 0 = off, at most 240 and 256 MB) keeps the input's past frames for the passes, as Echo does: `iHistoryFrame(ago, uv)` reads the input `ago` frames back (0 = now; premultiplied), or sample the `iHistory` array with `iHistoryHead` / `iHistoryCount` / `iHistorySize`. |
| **Echo** · `gpu/echo` | `input: layer*`, pins per `mode` (`decay: number, reset: boolean` for trails) → `layer (out): layer` | Time effects on a layer, on the GPU. `trails` (decay: each frame over the last, which fades), `echo` (copies, spacing in frames, decay: earlier frames under the live one, each fainter), `delay` (frames: the layer as it was that many frames ago), `freeze` (while `freeze` is on, hold the last frame shown), `slit-scan` (frames, direction: each row or column from a different moment; the edge `direction` points away from is now). `reset` forgets the past. The renderer keeps past frames per node at canvas size, capped at 256 MB / 240 frames (32 frames at 1920×1080). Keeps running while the patch is idle. |

## Sound

Audio nodes run on the app's own audio engine. The `audio` wire type
carries persistent handles — each Source/Processor keeps its audio alive
across frames; per-frame compute updates params (freq, amp, cutoff, …)
without re-allocating. The audio chain terminates at **Output**, the sink.
Volume, mute and the output device are in the status bar. On macOS the
microphone asks for permission the first time an AudioIn node starts.

### Sinks

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Output** · `sound/output` | `audio (in): audio, volume: number` → — | Audio sink. Connect a chain here to hear it. `volume` sets master output gain. |

### Sources

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Oscillator** · `sound/oscillator` | `freq, amp: number` → `audio: audio` | Sine / triangle / saw / square wave. `type` param swaps shape live. |
| **Pulse** · `sound/pulse` | `freq, amp, width: number` → `audio: audio` | Square wave with adjustable PWM. `width` 0 = symmetric square; 1 = thin spike. |
| **Noise** · `sound/noise` | `amp: number` → `audio: audio` | White / pink / brown noise. `type` param swaps the spectrum. |
| **SoundFile** · `sound/sound-file` | `url: string, rate, pan, amp: number, loop, play: boolean` → `audio: audio, loaded: boolean, duration, position: number` | Load an audio file by URL (set as a node param). Rising edge of `play` starts playback. |
| **AudioIn** · `sound/audio-in` | `amp: number, start: boolean` → `audio: audio` | Microphone input. Rising edge of `start` opens the microphone; macOS asks for permission the first time, and if it is refused the node outputs nothing. |

### Processors

Processors take an `audio` input and re-emit. Chain freely.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Gain** · `sound/gain` | `audio (in): audio, level: number` → `audio: audio` | Audio volume control — multiplies the signal by `level`. |
| **Filter** · `sound/filter` | `audio (in): audio, freq, res: number` → `audio: audio` | Biquad filter. `type` param: lowpass / highpass / bandpass. |
| **Delay** · `sound/delay` | `audio (in): audio, time, feedback, filter: number` → `audio: audio` | Echo with feedback and lowpass damping. |
| **Reverb** · `sound/reverb` | `audio (in): audio, seconds, decay, wet: number` → `audio: audio` | Convolution reverb. `reverse` param flips the IR. IR only rebuilds when seconds/decay/reverse change. |
| **Distortion** · `sound/distortion` | `audio (in): audio, amount, wet: number` → `audio: audio` | Waveshaper. `oversample` param trades CPU for less aliasing. |
| **Compressor** · `sound/compressor` | `audio (in): audio, threshold, ratio, knee, attack, release: number` → `audio: audio` | Dynamic range compression. |
| **Envelope** · `sound/envelope` | `audio (in): audio, attack, decay, sustain, release, level: number, trigger: boolean` → `audio: audio` | ADSR gate — passes audio shaped by an envelope. Rising edge of `trigger` fires attack; falling edge fires release. |
| **Mix** · `sound/mix` | `audio (in1), audio (in2) … audio (inN): audio, level: number` → `audio: audio` | Sum several audio chains into one, × `level` (0..2) — so voices can share a delay, a reverb, an FFT or one Output. The header's `− n +` sets how many inputs (`slotCount`, up to 32); a chain wired twice counts once. |

### Synths

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **MonoSynth** · `sound/mono-synth` | `note, velocity, attack, decay, sustain, release: number, trigger: boolean` → `audio: audio` | Single-voice synth with built-in ADSR. `note` is a MIDI number; rising edge of `trigger` plays the note, falling edge releases it. |

### Analysis

Tap audio sources without consuming them — these bridge `audio` back into `number` / `number*` / `boolean` so the rest of the patch can react.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Amplitude** · `sound/amplitude` | `audio (in): audio, smoothing: number` → `level: number` | RMS level (0..1) of the input. Smoothing is EMA factor (0 = sample-true, ~0.9 = smooth). |
| **FFT** · `sound/fft` | `audio (in): audio, smoothing: number` → `spectrum, waveform: number*` | Frequency analyser. Spectrum is a spread of bin magnitudes (0..255); waveform is time-domain (-1..1). Bin count via `bins` param (32–1024). |
| **PeakDetect** · `sound/peak-detect` | `audio (in): audio, freq1, freq2, threshold: number` → `detected: boolean` | True while a peak above `threshold` is detected between freq1..freq2 Hz. |

### Utilities

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **MidiToFreq** · `sound/midi-to-freq` | `midi: number` → `freq: number` | MIDI note → frequency in Hz. A4 = 69 → 440. |
| **FreqToMidi** · `sound/freq-to-midi` | `freq: number` → `midi: number` | Frequency in Hz → MIDI note. 440 → 69. |
| **Scale** · `sound/scale` | `mode` picks the first pins: `degree: number*, root, octave` (degree), `note: number*, root` (quantize), `amount: number*, root, octaves` (range); `scale: string` (+ `steps: string` when custom) → `note, freq, index: number*` | Snap numbers to the notes of a musical scale, so mice, sliders, noise and sensors play in key. `degree`: steps of the scale up from `root` (a MIDI note; one more than the scale's length is an octave up, negative goes down) + `octave` octaves. `quantize`: any MIDI note moved to the nearest note of the scale (ties go up). `range`: 0..1 spread over `octaves` octaves of the scale from the root. Per element over the first pin's spread. `scale`: major, minor, harmonic-minor, dorian, phrygian, lydian, mixolydian, pentatonic, minor-pentatonic, blues, whole-tone, chromatic or custom (`steps`: semitones from the root, e.g. `0 3 5 7 10`). `note` is the MIDI note, `freq` its frequency in Hz, `index` its place in the scale (0 = the root) for colours. |

## Input

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Mouse** · `input/mouse` | — → `x, y: number, pressed: boolean` | Mouse position and button state from the preview canvas. |
| **Keyboard** · `input/keyboard` | `mode` picks the pins: — (last), `keys: string` (keys), `clear: boolean, max: number` (text) → `pressed: boolean, key: string, down: boolean*, index: number, text: string, hit: boolean, hits: boolean*` | Keys pressed in the preview. `last` (default): the last key pressed (`key`, lowercase letters, `" "` for space, `ArrowLeft` …) and whether any key is down (`pressed`). `keys`: which of the listed keys are held — `down`, one per key (`keys` is one character per key, or names split by commas / spaces, e.g. `ArrowLeft, ArrowRight, space`) — and `index`, the most recently pressed one still held (−1 none): a computer-keyboard piano, game controls. `text`: the `text` typed so far, case and punctuation as typed; Backspace deletes, Enter starts a new line, typing stops at `max` characters, a rising `clear` empties it; the text survives edits of the patch. `hit` is true on the frame a key goes down (a listed one in keys mode, any typing in text mode); in keys mode `hits*` says which — one boolean per listed key, true on the frame it went down (several at once when pressed together). Effectful. |
| **Point Tracker** · `image/track-points` | `image: image, points: vec2*, reset: boolean, window: number` → `points: vec2*, confidence: number*` | Follow points across the frames of a moving image (camera, video, a rendered layer) by pyramidal Lucas–Kanade optical flow, from the pixels alone. Points are in canvas units like the image nodes ((0, 0) = the image's centre, y down). Tracks start at `points`, and again while `reset` is on, when the number of points changes or when they're edited; a point can move about `window` px a frame. Each point keeps the look it started with and pulls every frame's track onto it, so tracks don't drift. `confidence` 0..1 (normalised correlation, cut when the backward track doesn't return or there's no texture); a lost point stays put at 0. Repeated frames are skipped; tracks survive patch edits. |
| **Camera** · `input/camera` | `start: boolean` → `image: image, ready: boolean, width, height: number` | The live camera as an image. Nothing opens until `start` goes on; on macOS the first time asks for camera permission. `ready` turns true with the first frame, and `width` / `height` are its pixel size (0 until then). Pick the camera on the node (`device`, empty is the default one); `mirror` flips the picture like a mirror. With no camera, or access refused, the outputs stay empty. |
| **Landmarks** · `input/landmarks` | `image: image, start: boolean, size: number, guides: boolean, style: style` → `points: vec2*, z, group: number*, color: color*, connA, connB: vec2*, connColor: color*, guides: layer*, people: vec2*, personColors: color*, detections, perGroup: number, detected: boolean` | Finds hands, faces or bodies in a picture (wire Camera's `image`, or a video) with the tracking models built into the app; turn `start` on to run it. `tracker`: hands, face, pose or all; `count` (1..6, default 2) is how many people to look for; `person` 0 reports everyone, n only person n; `mirror` (on by default) flips x like a mirror. Points are in canvas units, the picture mapped onto a `size` × `size` square (default 500) about the origin, and `z` is depth on the same scale. `group` and `color` say which hand / face / person each point belongs to. `connA` / `connB` / `connColor` are the skeleton lines, and `guides` (when on) draws them and the points as ready-made layers, restyled by `style`. `people` is one list of points per person with `personColors`, `detections` how many were found, `perGroup` the points in one hand / face / body (0 for `all`), and `detected` whether anything was seen. |
| **MidiCC** · `input/midi-cc` | — → `value, raw: number` | Tap a single MIDI Control Change. Set `channel` (0 = any, 1..16) and `cc` (0..127) on the node; `value` is normalised 0..1, `raw` is the original 0..127. Connected MIDI devices are picked up automatically, so knobs and sliders on any board light up without setup. |
| **MidiNote** · `input/midi-note` | — → `note, velocity: number, pressed: boolean, freq: number` | Last held MIDI note (stack-based mono). Pairs with MonoSynth via `note`→`note` and `pressed`→`trigger`. `freq` is the convenience Hz conversion. |

## IO

Interactive UI nodes — the inline widget IS the value source.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Slider** · `io/slider` | — → `value (number): number` | Drag to set a number. |
| **Toggle** · `io/toggle` | — → `bool: boolean` | On/off switch. |
| **ColorPicker** · `io/color-picker` | — → `color: color, r, g, b: number` | Hex color picker — outputs a Color plus r/g/b channels in 0..255. |
| **XYPad** · `io/xy-pad` | — → `x, y: number` | 2D draggable area — outputs x, y in [0, 1]. |
| **Button** · `io/button` | — → `bool: boolean` | `true` while you hold the mouse down on it, `false` once released. Toggle is the latching version. |
| **Bang** · `io/bang` | — → `bang: boolean` | A button that fires once: a click makes `bang` `true` for exactly one tick, then `false` again. Wire it into anything that reacts to a rising edge: a Counter's `inc` or `reset`, a Latch, a Trigger. |
| **Stepper** · `io/stepper` | — → `value (number): number` | A whole number you change with − and + buttons on the node. Each press moves it by `step` (default 1), kept between `min` and `max` (defaults 0 and 10). |
| **Select** · `io/select` | — → `value: string` | A drop-down menu on the node. Type the choices into `options`, separated by commas (`a,b,c`), and pick one: `value` is the chosen text. |
| **Latch** · `io/latch` | `set, reset, toggle: boolean*, initial: boolean` → `state, changed: boolean*` | A flip-flop per element: a rising edge on `set` turns it on, `reset` off, `toggle` flips (reset wins over set). Starts at `initial`; `changed` = the tick it flips. Replaces `hit && m ? 1 - s : s`, `p && !was`, `click && h >= 0 ? 1 - under : v` with a State loop. Effectful; state survives edits. |
| **Counter** · `io/counter` | `inc, reset: boolean, step, start: number, dec: boolean`; `limit` clamp / wrap adds `min, max: number` → `count: number` | `+step` on each rising edge of `inc`, `−step` on each rising edge of `dec`; `reset` returns to `start`. `limit` none (default): unbounded; clamp: within `[min, max]`; wrap: into `[min, max)` (min 0, max 4 counts 0 1 2 3 0 …). |
| **Metro** · `io/metro` | `interval: number, run: boolean` → `bang: boolean` | A metronome: `bang` is `true` for one tick every `interval` seconds (default 1, at least 0.01, set on the node) while `run` is on (default on). It doesn't fire on its first tick, and turning `run` off restarts the count. Drives periodic actions; pairs with Counter. |
| **Edge** · `io/edge` | `in: number, mode: string, threshold: number` → `bang: boolean` | Turns a changing number into a one-tick `true`. `mode` `rising` (default): `in` goes up past `threshold` (default 0.5). `falling`: it drops to or below it. `change`: it differs from the last tick. `mode` and `threshold` are set on the node, and the first tick never fires. Reads slot 0 of a spread (wrap it in a ForEach). |
| **Gate** · `io/gate` | `value: any, open: boolean` → `out: any` | Sample-and-hold. While `open` is on (it is when unwired) `value` passes straight through; when `open` goes off, `out` freezes on the last value that passed (empty if none has). Works on whole lists. |
| **ImageURL** · `io/image-url` | — → `image: image, loaded: boolean, width, height: number` | Load an image from a URL (set as the node's `url` param). `image` is null and `loaded` is false until the bytes arrive; downstream `draw/image*` nodes no-op during the load window. |
| **File** · `io/file` | — → `url: string, image: image, name: string, loaded: boolean, type: string` | Pick any file on the node (it opens a file dialog, or drop one on it). The file's bytes are stored inside the patch, so the patch stays one file but grows with it. `url` is the file as a `data:` URL (CSV File accepts it), `image` the decoded picture when it is an image, `name` its file name, `type` its MIME type and `loaded` is true once it is ready. For large pictures or video, ImageURL and VideoFile point at a path instead of copying. |
| **SVG** · `io/svg` | `position: vec2, scale: number, style: style` → `layer: layer, width, height: number, loaded: boolean` | Draws an SVG as a layer, rasterised crisply at the size it is drawn. `mode`: `file` (pick an .svg on the node), `url` (a web address or file path) or `markup` (paste the SVG source). `position` moves it (centre at the origin), `scale` multiplies its size (default 1). A wired `style` overrides the SVG's own fill and stroke on every shape. `width` / `height` are the SVG's own size and `loaded` is true once it is drawn. |

## Data

Numbers from tables.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **CSVFile** · `data/csv-file` | `url: string, column: number` → `values: number*, rows: number, loaded: boolean` | Reads the numbers in a CSV file into a list. `url` is a file path (a relative path is looked up beside the patch), a web address, or a File node's `url`. `column` picks one column (0 is the first; cells are separated by commas, semicolons or tabs); -1, the default, takes every number in the file. Text, headers and empty cells are skipped. `rows` counts the non-empty lines and `loaded` goes true once the file has been read. |
| **CSVInline** · `data/csv-inline` | — → `values: number*` | Numbers typed or pasted into the `data` box on the node (it starts with 16 sample values), separated by commas, spaces or line breaks. Anything that isn't a number is skipped. For small tables that don't need a file. |

## State

Cross-frame memory. The runtime breaks the topo cycle for these — their write input is gathered post-tick and emitted on the next frame.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **State** · `state/value` | `init, next: any` → `current: any` | Legacy (hidden from the node browser; old files still load): one slot of cross-frame state with `next` wired back into the same box. Use Write → Read instead. |
| **State Spread** · `state/spread` | `init, next: any*` → `current: any*` | Legacy (hidden), same for spreads. Use Write\* → Read\* instead. |
| **Read** · `state/read` | `init: any` → `current: any` | Start of a feedback pair. Emits the value its Write (same `slot`) stored last frame; `init` on frame 0. Params: `slot` (the pairing), `name` (optional, shared with its Write). |
| **Write** · `state/write` | `next: any` → — | End of a feedback pair. Stores `next` at end of frame; its Read emits it next frame. Params: `slot`, `name`. `Read*` / `Write*` (`state/read-spread`, `state/write-spread`) are the spread versions. |
| **Read*** · `state/read-spread` | `init: any*` → `current: any*` | The spread version of Read: emits the whole list its Write\* (same `slot`) stored last frame, or `init` on frame 0. For a feedback pair that carries a list: a trail, a grid of cells, a set of positions. Placing a Read\* places its Write\*. Params: `slot`, `name`. |
| **Write*** · `state/write-spread` | `next: any*` → — | The spread version of Write: stores the whole list `next` at the end of the frame for its Read\* to emit next frame. Placing a Write\* places its Read\*. Params: `slot`, `name`. |

**Feedback: Write → Read.** Patches flow left to right; the only way a value crosses frames is a Write at the end of the flow and its paired Read at the start (never a wire back into an earlier node, never a single State box with `next` wired back). The two halves are paired by their slot, kept underneath; the header shows `#slot`, or the pair's name once it has one (`Write · ball position`). In the editor, placing a Write also places its Read (to the left of the flow, at the same height) and vice versa, on a slot of its own, both selected; double-click either header (or "Name pair…" in its menu) to name the pair, which names both. Selecting a half draws a dashed arc to its partner, and hovering one lights the other. Deleting a Write deletes its Read(s); deleting a Read deletes its Write unless another Read of it stays. Duplicated or pasted halves get a fresh slot, and a lone copied half gets a new partner. The compiler warns about a Read with no Write, a Write with no Read, and two Writes on one slot.

## Region

Regions are scope containers. Inner nodes run with extra context (per-iteration values, conditions, cache hashes) supplied by the orchestrator. Ports are added on the frame's edges by dragging wires onto them; the compile step synthesises the matching splicer-in / splicer-out at runtime.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **ForEach** · `region/foreach` | `delay: number` → — | Iterate over a spread. Inner nodes run once per slot. |
| **If** · `region/if` | `condition: boolean` → — | Run the inner scope when `condition` is true; otherwise emit empties / forward accumulator inits unchanged. |
| **Cache** · `region/cache` | — → — | Memoise the inner scope by structural hash of its inputs. Re-runs only when inputs change. |
| **Section** · `meta/section` | — → — | A titled, coloured backdrop that groups nodes on the canvas. Purely visual: no ports and no scope — compile lifts its children into the enclosing scope, so the patch computes the same with or without it, and wires pass straight through. Nodes inside move with it; resize it like a region, double-click the title to rename it, right-click for its colour or to unwrap it. Params: `label`, `color` (hex), `width`, `height`. |
| **Subpatch** · `meta/subpatch` | — → — | A frame that folds part of a patch behind one boundary, with ports of your own, like a node you built. Drag wires onto its edges to add ports; inputs are on the left, outputs on the right, and what is inside runs as part of the patch. Usually made by selecting nodes and choosing Wrap in subpatch (⌘G). Right-click for Ungroup subpatch, or save it to the Library from the Subpatches panel. Params: `label`, `width`, `height`. |
| **Comment** · `meta/comment` | — → — | A free-floating text note on the canvas (right-click empty canvas → Add comment). It has no ports and doesn't affect the patch. Double-click to edit the text; drag the corner grip to resize. Params: `text`, `width`, `height`. |
| **Probe** · `meta/probe` | `in: any*` → `out: any` | Shows the data on a wire: a scrollable readout of the values that flow through it (the first 400), with their type and count, a size note for pictures and "layer" for drawings. `out` passes the data on unchanged, so the probe also reroutes the wire. Drag a wire out and Alt-drop it on empty canvas to place one. |
| **AccRead** · `region/accumulator-read/number` | `init: number` → `read: number` | Inside a ForEach: a number that carries over from one pass to the next. On the first pass `read` is `init` (the value wired to the frame's matching input); on every later pass it is what the paired AccWrite received on the pass before. Paired with its AccWrite by `slot`. Place it at the left of the loop body so wires run rightwards. |
| **AccRead** · `region/accumulator-read/boolean` | `init: boolean` → `read: boolean` | Inside a ForEach: a boolean that carries over from one pass to the next. On the first pass `read` is `init` (the value wired to the frame's matching input); on every later pass it is what the paired AccWrite received on the pass before. Paired with its AccWrite by `slot`. Place it at the left of the loop body so wires run rightwards. |
| **AccRead** · `region/accumulator-read/image` | `init: image` → `read: image` | Inside a ForEach: a image that carries over from one pass to the next. On the first pass `read` is `init` (the value wired to the frame's matching input); on every later pass it is what the paired AccWrite received on the pass before. Paired with its AccWrite by `slot`. Place it at the left of the loop body so wires run rightwards. |
| **AccWrite** · `region/accumulator-write/number` | `write: number` → `final: number` | Inside a ForEach: the number to carry to the next pass; the paired AccRead gives it back as `read`. When the loop has run, the last value written leaves the frame as `final`. Paired with its AccRead by `slot`. |
| **AccWrite** · `region/accumulator-write/boolean` | `write: boolean` → `final: boolean` | Inside a ForEach: the boolean to carry to the next pass; the paired AccRead gives it back as `read`. When the loop has run, the last value written leaves the frame as `final`. Paired with its AccRead by `slot`. |
| **AccWrite** · `region/accumulator-write/image` | `write: image` → `final: image` | Inside a ForEach: the image to carry to the next pass; the paired AccRead gives it back as `read`. When the loop has run, the last value written leaves the frame as `final`. Paired with its AccRead by `slot`. |

### Internal (hidden from palette)

These exist so the runtime has a node to schedule for each region frame port. Compile synthesises them; the user never drops one manually.

- `region/splicer-in/{number,layer,boolean}` — receive an outer spread; deliver one slot per iteration.
- `region/splicer-out/{number,layer,boolean}` — collect one value per iteration into an outer spread.


## Particles

Stateful particle simulation. New port types `world`, `force`, `collider`, `particle` are introduced. Several pins are CONSTANT (`C` badge) — read once at compile.

### Physics

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **World** · `physics/world` | `gravity: vec2, drag, timeScale: number, bounds: vec2, forces: force*, colliders: collider*, integrator: string` → `world: world` | Physics world handle. Global forces / colliders wire here once, every Emitter sharing the world inherits them. |

### Particles core

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Emitter** · `particles/emitter` | `world: world, positions: vec2*, rate, lifetime: number, initialVel: vec2, velSpread: number, color: color, size, maxCount: number, overflowMode: string, seed: number, localForces: force*, time: number` → `particles: particle` | The system root. Owns a fixed-capacity ParticleBuffer; spawns at `rate`/sec from `positions`; integrates each tick. |
| **FromLayer** · `particles/from-layer` | `layer: layer, mode: string, density: number` → `positions: vec2*` | Sample points on any shape (outline or fill). Wire into Emitter `positions` to emit from rects, ellipses, polygons, even text. |
| **Particle Shapes** · `particles/render-as` | `particles: particle, template: string, templateLayer: layer, style: style, tint: color, useBufferColor: boolean` → `layer: layer` | Convert a ParticleBuffer into a renderable Layer. Template is circle / rect / custom layer. |
| **Trail** · `particles/trail` | `particles: particle, segments: number, style: style, color: color, fadeOut, useBufferColor, tapered: boolean, tailWidth: number` → `layer: layer` | Render a polyline trail behind every alive particle. Ring buffer length fixed at compile. |
| **Bounds** · `particles/bounds` | `size, center: vec2, mode: string, restitution: number` → `collider: collider` | Wrap, kill, or bounce particles at the edges of a rectangle. Plug into a World's `colliders` input. |

### Movers

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Move** · `particles/move` | `position, velocity, acceleration: vec2*, drag, max speed (maxSpeed), radius: number*, edge, center, size: vec2` (+ `restitution: number*` in bounce, `reset: boolean*` in run mode), `time scale (timeScale): number` → `position, velocity: vec2*, hit: boolean*, normal: vec2*` | Kinematics on plain lists, no buffers: semi-implicit Euler with `acceleration`, `drag` (per second) and `maxSpeed`; the box (`size` 0 × 0 = the canvas) `wrap`s (things leave whole), `bounce`s (`restitution`), `clamp`s or not (`none`), allowing for each `radius`. `run` keeps its own state from the inputs (spread growth adds movers; `reset` per element respawns); `step` moves the inputs one frame (for a State loop). `hit` / `normal`: the wall touched this frame. dt capped at 0.1 s. Replaces `p + v * h` with `mod` wrap / hand-written wall bounces (`q.x < -353 ? abs(v.x) : …`). Effectful; state survives edits. |

### Forces

All output a `force` value plugged into a World's `forces` input (global) or an Emitter's `localForces` (per-emitter). Shaped forces also expose a `viz` layer output for debugging (toggle `showViz`).

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Gravity** · `force/gravity` | `direction: vec2, strength: number` → `force: force` | Uniform vector, scaled by particle mass. Use for *additional* gravity beyond what's on World. |
| **Drag** · `force/drag` | `strength: number` → `force: force` | Velocity-proportional damping. Per-particle, composable. |
| **Attractor** · `force/attractor` | `center: vec2, strength, radius: number, falloff: string, showViz: boolean` → `force: force, viz: layer` | Radial pull. Negative strength = repulsion. Falloff baked at compile. |
| **Repulsor** · `force/repulsor` | `center: vec2, strength, radius: number, falloff: string, showViz: boolean` → `force: force, viz: layer` | Radial push. (Attractor with inverted semantics.) |
| **Vortex** · `force/vortex` | `center: vec2, axisStrength, pull, radius: number, falloff: string, showViz: boolean` → `force: force, viz: layer` | Rotational + optional radial pull. Counter-clockwise when `axisStrength > 0`. |
| **Wind** · `force/wind` | `direction: vec2, strength, noise, seed: number` → `force: force` | Directional force with Perlin variation. Mass-independent. |
| **Turbulence** · `force/turbulence` | `scale, speed, strength, seed: number` → `force: force` | 2D Perlin noise field. Time drifts the field. ~5–10× the cost of gravity per particle. |

### Colliders

All output a `collider` value plugged into a World's `colliders` input. Modes: `bounce` (reflect velocity, scaled by `restitution`), `kill` (free the slot), `absorb` (zero velocity in place). `inverted` flips inside/outside.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Circle** · `collider/circle` | `center: vec2, radius: number, mode: string, restitution: number, inverted: boolean` → `collider: collider` | Analytic circle. Cheap. |
| **Rect** · `collider/rect` | `center, size: vec2, mode: string, restitution: number, inverted: boolean` → `collider: collider` | Axis-aligned rectangle. Cheap. |
| **FromLayer** · `collider/from-layer` | `layer: layer, mode: string, restitution: number, inverted: boolean` → `collider: collider` | Build a collider from any shape (rect / ellipse / triangle / polygon). Polygon bounce uses centroid-normal approximation. |

### Simulation

One stateful node with a `mode` dropdown; its pins follow the mode. It steps 60 times per second of frame time (at most 3 steps a frame), so the same seed and frame times give the same result. A rising edge on `reset` starts over; so does a new `seed` (and, per mode, a new size / count / grid). Growth and reaction keep their state through edits of the patch. Samples: `sim-flocking`, `sim-slime-mould`, `sim-dla-coral`, `sim-cloth`, `sim-differential-growth`, `sim-reaction-diffusion`.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Simulation** · `particles/simulation` | pins per `mode` (below; `count: number, size: vec2, maxSpeed, separation, alignment, cohesion, radius: number, target: vec2, food: vec2*, attract, scale: number, style: style, colors: color*, seed: number, reset: boolean` for boids) → `layer: layer, points: vec2*, values: number*, from, to: vec2*, image: image` | `layer` draws the simulation; `points` / `values` / `from`, `to` are there to draw your own; `image` is the physarum trail map or the reaction–diffusion picture (1×1 placeholder otherwise). `colors*` ramps shapes along `values`. |

| Mode | Inputs | Outputs |
|---|---|---|
| `flocking` | `count, size, maxSpeed, separation, alignment, cohesion, radius, target, food*, attract, scale, style, colors*, seed, reset` | Boids on a wrapping `size` field: darts on `layer`; `values` = heading (radians); `from`→`to` = 0.1 s of velocity. `attract` steers them to `target` or, while `food*` holds points, each boid to the food point nearest it (the short way round the field; every boid against every point, so keep food to hundreds). A changing `count` adds or drops boids without starting over, so it can be a population. |
| `physarum` | `agents, size, resolution, sensorAngle, sensorDistance, turnAngle, stepSize, deposit, decay, diffuse, food*, colors*, seed, reset` | Slime-mould agents on a `resolution`-cell trail map (angles in degrees, distances in cells; trail saturates at 1). `image` and `layer` show the map: white→clear with no `colors`, one colour→that colour, several→a ramp from empty to dense. `food*` points keep the trail full. |
| `dla` | `count, size, radius, rate, stickiness, seeds*, style, colors*, seed, reset` | Walkers stick to a cluster grown from `seeds*` (the origin if none), `rate` per second up to `count`. Lines from each particle to its parent; `values` = growth order 0..1. |
| `cloth` | `cols, rows, size, pos, pin, gravity, wind, stiffness, iterations, damping, tear, pointer, grab, style, colors*, reset` | Verlet spring mesh; `pin` = top / corners / left / none (pins follow `pos`). `tear` > 0 breaks springs stretched past `tear` × rest. Holding `grab` drags the point nearest `pointer`. `values` = stretch; springs on `layer` and `from`→`to`. |
| `growth` | `shape*, radius, size, maxNodes, repel, force, align, maxEdge, grow, speed, style, colors*, seed, reset` | Differential growth: a curve (the first outline of `shape`, closed or open, else a circle of `radius`) whose nodes push apart within `repel` (× `force`), pull `align` of the way to their neighbours' middle and move at most `speed` a step; `grow` new nodes a second go in at random edges and edges longer than `maxEdge` split, up to `maxNodes` (8000 at most), so it folds like coral or a brain. Stays inside `size`. One path on `layer` (the style's fill too); with `colors` the edges ramp by age. `values` = age 0..1; `from`→`to` = edges. |
| `reaction` | `size, resolution, preset, (feed, kill), rateA, rateB, speed, seed image, paint*, colors*, seed, reset` | Gray–Scott reaction–diffusion (Karl Sims's formulation) on a wrapping grid `resolution` cells along `size`'s long side (512 at most), on the CPU. `preset`: coral, mitosis, fingerprints, worms, maze, spots, waves, or custom (`feed`, `kill` pins). `rateA` / `rateB`: diffusion; `speed`: iterations a step (40 at most). B starts where `seed image` is bright (luminance × alpha > ½, stretched over `size`) or in a few random spots; `paint*` points keep adding it. `image` / `layer`: B's concentration, coloured like physarum's map. For full-screen resolution use the Shader node's feedback buffers. |

## Geometry

Generators and shape operations. The polygon nodes share their outputs: `shape` (all polygons in one layer), one layer per polygon, `points` (every ring's corners, flat) and `counts` (corners per ring — pair them with **AtGroup** to pick one ring). Layers are read as outlines: polygons, paths, lines, rects, ellipses, arcs, triangles, quads, glyphs (`text/glyphs`) and text (`draw/text`), with group transforms applied. A dropdown picks the mode; the pins follow it.

| Label · type | Inputs → Outputs | Description |
|---|---|---|
| **Scatter** · `geometry/scatter` | `center, size: vec2, shape: layer*` + mode pins (`distance` / `count` / `spacing, jitter` / `points`), `relax`, `seed` → `points: vec2*` | Points over a rectangle or inside a wired shape. Modes: `poisson` (no two closer than `distance`), `random`, `jittered-grid` (one per `spacing` cell, moved up to `jitter` of a cell), `halton` (low-discrepancy, shifted by the seed), `relax` (the wired `points`). `relax` runs that many passes of Lloyd relaxation (each point to the centre of its Voronoi cell, up to 100): clumps and gaps even out into an organic, even spread with an exact count. `stipple` (`density: image, count, gamma, invert`): `count` points gathered where the image (stretched over the area) is dark — light with `invert`, `gamma` > 1 for more contrast — then relaxed by weighted Lloyd passes (Secord's weighted Voronoi stippling) into an even, hand-stippled grain. Same seed, same points. |
| **Cells** · `geometry/cells` | `points: vec2*, center, size: vec2` (voronoi, subdivide), `depth, chance, minSize, ratio, quad, seed` (subdivide), `gap: number, colors: color*, style` → `layer (shape): layer, cells: layer*, points: vec2*, counts: number*, centers: vec2*, depth: number*` | Voronoi cells (clipped to the rectangle), Delaunay triangles or the convex hull of a points spread. `gap` leaves space between neighbours; `colors` fills polygon i with colour i (cycled). `subdivide`: recursive splits of the rectangle (Mondrian, quadtree) — in two across the longer side more often than not, or in four with `quad`; after the first split each part splits again with probability `chance`, down to `depth` levels, never thinner than `minSize`, cuts up to `ratio` (0..1) away from the middle; with `points` wired a part splits while it holds more than one of them (a quadtree / k-d tree of the points). Same `seed`, same splits; `colors` are picked per rectangle at random (repeat one to make it likelier); `depth` out = each rectangle's level (0 in the other modes). |
| **Path Ops** · `geometry/path-ops` | `points: vec2*, closed: boolean, path: layer*` + mode pins (`spacing` / `iterations` / `tolerance` / `distance, join`), `style` → `layer (shape): layer, layer (layers): layer*, points: vec2*, counts: number*` | Reshape a point list or every outline of a layer. Modes: `resample` (even spacing), `smooth` (Chaikin), `subdivide` (midpoints), `simplify` (Ramer–Douglas–Peucker), `offset` (grow / shrink areas with round, miter or bevel joins; open paths shift sideways), `round-corners` (radius: each corner an arc, limited by the edges), `thicken` (width, cap, join: a stroke's outline as filled polygons, in the stroke's colour), `wave` (amplitude, wavelength, phase: a sine along the normals, whole waves round closed shapes), `noise` (amount, scale, seed, phase: Perlin wobble), `twist` (angle, radius, center: a swirl that fades out at radius), `explode` (distance: every edge its own piece on `layers`, pushed out from the centre, in the fill's colour). wave / noise / twist resample every `detail` units first; their `phase` loops at whole numbers. Shapes from a layer keep their style unless `style` is wired. |
| **Shape Boolean** · `geometry/boolean` | `a, b: layer*, style: style` → `layer (shape): layer, layer (layers): layer*, points: vec2*, counts: number*` | `union`, `intersect`, `subtract` (a − b) or `xor` of the filled areas of two layers (nonzero winding; lines ignored), holes included. With only `a` wired, `union` merges its shapes. |
| **L-System** · `geometry/lsystem` | `axiom, rules: string, iterations, angle, step, fit: number, position: vec2, heading, jitter, seed: number, style: style` → `layer (shape): layer, layer (layers): layer*, points: vec2*, counts: number*` | Rewrite `axiom` by `rules` (`X=…`, separated by `;`) `iterations` times and draw it with a turtle: `F` `G` draw, `f` `g` move, `+` `-` turn by `angle`°, `\|` turns round, `[` `]` branch. `fit` > 0 scales the drawing into a `fit`-wide box centred on `position`. `jitter` randomises every turn (by `seed`). |
| **Trim Path** · `geometry/trim` | `points: vec2*, closed: boolean, path: layer*, start, end, offset: number, style: style` → `layer (shape): layer, layer (layers): layer*, points: vec2*, counts: number*, heads: vec2*, angles: number*` | Draw-on / write-on: only the part of each outline between `start` and `end` (0..1 of its arc length), slid round by `offset` (wraps; a closed path carries on round its start). Modes: `each` trims every outline at once; `sequence` lays them end to end and trims the run, so they draw one after another (handwriting glyph outlines). Pieces are lines: a filled shape strokes in its fill colour unless `style` is wired. `heads` / `angles`: where each drawing end is and which way it heads (one for `sequence`), for a pen tip or arrowhead. |
| **Shape Morph** · `geometry/morph` | `a, b: layer*, t, points: number, style: style` → `layer (shape): layer, layer (layers): layer*, points: vec2*, counts: number*` | Blend `a`'s outlines into `b`'s by `t` (0 = a, 1 = b). Outer shapes pair with outer shapes and holes with holes, in order; a contour with no partner shrinks to (grows from) its centre. Pairs are resampled to `points` each (0 = about one per 4 units) and lined up (same direction, best start point) so blends don't twist; open and closed mix. Fill / stroke colours and stroke weight blend too, unless `style` is wired. |
| **Connect Points** · `geometry/connect` | `points: vec2*` + mode pins (`distance, maxLinks` / `neighbours, distance`), `fade, taper: number, colors: color*, colorBy: string, style` → `layer (shape): layer, from, to: vec2*, strength: number*` | Lines between nearby points (plexus). `distance`: every pair closer than `distance`, each point keeping at most `maxLinks` (shortest first; 0 = no limit). `nearest`: each point to its `neighbours` nearest (within `distance` when > 0). `strength` falls from 1 (touching) to 0 (at the reach); `fade` takes that much off a line's alpha and `taper` off its width. A wired `colors` spread colours the lines: `colorBy` `points` gives point i colour i (cycled) and draws each line as a gradient between its ends' colours; `length` runs the colours as a palette from the shortest lines (first) to those at the reach (last). A uniform grid keeps thousands of points fast; at most 100 000 lines. |
| **Curve** · `geometry/curve` | `count, center, radius, phase, rotation` + mode pins (`a, b` / `n, d` / `R, r, d, laps` / `turns, inner` / `points, inner` / `m, n1, n2, n3`), `style` → `layer (shape): layer, points: vec2*, tangents: number*` | Parametric curves: `lissajous`, `rose`, `hypotrochoid`, `epitrochoid`, `spiral`, `log-spiral`, `star`, `superformula`. `count` points fitted to `radius` round `center` (Lissajous fills the square), turned by `rotation`; closed curves run round exactly once (period from the ratios, up to 64 turns) and close their path; spirals are open, `inner` → `radius`. Hypo- / epitrochoid `laps` (0, the default: one closed period) above 0 runs the pen round that many times instead, as an open path (point i at i / count of the laps) — a Spirograph drawn for as long as you like. `phase` 0..1 is one seamless animation cycle (figures morph, roses / stars / superformula turn one symmetry step, spirals one turn). `tangents`: direction of travel (radians). Replaces `vec2(cos(u * tau), sin(u * tau)) * (… abs(cos(petals / 2 * u * tau)))`, `vec2(sin(tau * l * 3), sin(tau * l * 4))` and Spirograph formulas. |
| **Plot** · `geometry/plot` | `values: number*, center, size: vec2, range` (+ `min, max` when `fixed`), `baseline` (area, bars), `gap` (bars), `style` → `layer (shape): layer, points, base: vec2*` | Values as a chart in a box: `line`, `area` (closed down to `baseline`: filled hills, mountain ridges) or `bars`. Larger values higher (y grows down). `range` `auto` / `zero` (includes 0) / `fixed`; a flat series sits mid-box; NaN on the baseline; `base` = the baseline point under each value. Replaces `vec2(-330 + 660 * index / 239, 292 - 108 * v)` chart points and the two extra corner points (`index == 100 ? vec2(400, 260) : …`) that close a ridge. |
| **Hit Test** · `geometry/hit-test` | `points: vec2*` + mode pins (`centers, sizes: vec2*` / `centers: vec2*, radii: number*` / `shapes: layer*`), `margin: number` → `inside: boolean*, index, hits: number*` | Points against `rects`, `circles` or a `layer`'s shapes (each drawn shape one; holes respected; lines by their stroke). `margin` grows (shrinks when negative) every shape. Per point `inside` and `index` (first shape, −1 none); per shape `hits` (points inside). Replaces `abs(c.x - q.x) <= 37 && abs(c.y - q.y) <= 17`, `distance(q, m) < 10`, grid-cell maths like `floor((m.y + 174) / 36) * 16 + floor((m.x + 288) / 36)`. |
| **Contours** · `geometry/contours` | mode pins (`values: number*, cols` / `points: vec2*, radius: number*, cell` / `image` / `shape: layer*, cell`), `center, size: vec2, threshold, levels, step: number, colors: color*, style` → `layer (shape): layer, layer (layers): layer*, points: vec2*, counts, levels: number*` | Isolines of a field as closed outlines (marching squares). `field`: a spread of `values`, `cols` a row, over the rectangle (e.g. noise on Grid positions: topographic lines). `points`: metaballs, Σ radius² / distance², sampled every `cell` units. `image`: brightness × alpha (0..1) of an image centred on `center` at `size` (0 = its pixel size). `shape`: distance from the outlines of a wired layer (text, shapes; open paths by distance only), sampled every `cell` units: the levels are distances, 0 = the outline, outward positive, negative inside, and outer rings are drawn underneath, so filled rings stack into contour-map bands round a word. Draws `levels` contours at `threshold`, `threshold + step`, …; outside the field counts as low, so every contour closes, running clockwise round high ground: each level fills as one polygon with its holes. `levels` out: each contour's level. A wired `colors` spread colours the levels (fill, else stroke) as a palette from the first level to the last. `flow` (`seeds, vectors: vec2*, cols, field, scale, seed, turn, spacing, step, maxLength, minLength`): streamlines instead — flow-field lines for plotter art. The field: wired `vectors`, `cols` a row over the rectangle (bilinear between them; e.g. Noise curl on Grid positions), or built in: `field` `angle` (Perlin noise as directions: lines gather into rivers) or `curl` (swirls that never cross) at `scale` / `seed`; `turn` radians turns every direction. Lines start at `seeds`, or unwired are spaced evenly (Jobard–Lefer: each line seeds the next `spacing` to either side, a shuffled grid fills the gaps), run both ways in `step`s (midpoint rule) until they leave the rectangle, reach `maxLength` or come within half a `spacing` of another line (`spacing` 0: they cross); shorter than `minLength` are dropped. `colors` picked per line at random; `levels` out = each line's length. At most 10 000 lines / 50 000 points. |
| **Layout** · `geometry/layout` | `count: number, center: vec2` + mode pins (`cols, spacing: vec2` / `radius, start, sweep` / `spacing` / `points: vec2*, closed, path: layer*`), `duration, stagger: number, easing: string, reset: boolean` → `positions: vec2*, angles, scales: number*, progress: number` | Arrange `count` items as a `grid` (`cols`, 0 = square-ish), a `ring` (a full turn, or an arc of `sweep` with items on both ends; angles point outwards), a phyllotaxis `spiral` (equal area per item) or evenly along a `path` (a layer's outlines end to end, or a `points` polyline; angles follow it). The point of it is the move between layouts: when `count`, the mode or `cols` changes, or the node is edited, every item eases to its new place over `duration` s (`easing` as Ease; `stagger` 0..1 starts later items later); new items grow in (`scales` 0 → 1), removed ones shrink away where they are, so the spreads run longer than `count` until the move ends. An interrupted move carries on from where the items are. Animated inputs are followed exactly; `reset` jumps; `progress` is 0..1 through a move (1 when settled). Effectful; its state survives patch edits. |
| **Tiles** · `geometry/tiles` | `cols, rows: number, size: number, center: vec2` + mode pins (`tiles, noise` / `sockets, closed`), `seed, colors: color*, style` → `layer (shape): layer, layer (layers): layer*, centers: vec2*, tiles, angles: number*` | A grid of tiles that read as one pattern (at most 64 a side). `truchet`: every cell takes a turn of a `tiles` set — `arcs` (two quarter circles: meandering loops), `diagonals` (10 PRINT maze), `triangles` (half squares) — at random, or with `noise` > 0 following Perlin noise at that scale per cell so turns clump. `wfc`: wave function collapse (simple tiled model) over edge-matched tiles: `sockets` lists them as four characters for the north, east, south, west edges (`0` = nothing, e.g. `1100` a corner; `*weight` makes one likelier); every tile comes in its four turns, neighbours must match where they meet, `closed` keeps the outer edges empty so no path runs off the grid; a contradiction restarts (12 tries, then a warning). Drawn as pipes: straight, round a corner, to the middle (a dot at a dead end). Same `seed`, same pattern. `colors`: colour k for tile k of the set (its turn in truchet). `centers` / `tiles` / `angles` (radians clockwise) let a ForEach draw its own tiles. |
