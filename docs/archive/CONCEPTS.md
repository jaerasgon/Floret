# Floret — Core Concepts

The map. Read this before adding a new node category, refactoring the
runtime, or trying to understand why something is the way it is.

Each section is short by design: name, one-line summary, where it lives,
why it exists, gotchas. Code is the source of truth; this doc tells you
which file to open.

> **Note to Claude (or any AI assistant working on this codebase): always
> read this file at the start of any planning or coding session — both to
> ground decisions in the existing conventions and to add a section here
> whenever a new architectural concept is introduced. If a concept
> doesn't belong here yet, that's the signal to ask whether it should.**

---

## Ports & types

**Port types** are the value flavours that flow on wires: `number`,
`boolean`, `string`, `vec2`, `color`, `mat3`, `layer`, `style`, `audio`,
`image`, `any`.
- Defined in [src/runtime/types.ts](src/runtime/types.ts)
- `any` is the universal accept-all type used by `convert/*` nodes.

**Wire kinds** decide the *shape* of what flows on a port — orthogonal
to type:
- `single` (default) — one scalar value of `type`
- `spread` — one wire carrying `Spread<type>` (a 1-D list)
- `multi` (input-side only) — N wires gathered into a single `Spread<type>`
- See `PortSpec.kind` in [src/nodes/types.ts](src/nodes/types.ts)

Compatibility (enforced in
[src/editor/connection-rules.ts](src/editor/connection-rules.ts)):
- single → single ✓
- spread → spread ✓
- single → spread ✓ (auto-wraps as length-1 spread)
- single → multi ✓ (one element added to the gather)
- spread → multi ✓ (all elements concatenated)
- spread → single ✗ (must cross a region boundary via a splicer)

**Why a length-1 spread = scalar:** every wire physically carries
`ReadonlyArray<unknown>`. A "scalar" is just a spread of length 1. This
eliminates `is-this-a-scalar?` branching from every node. See
[src/runtime/types.ts](src/runtime/types.ts).

---

## Spreads, broadcast, auto-lift

Every input is a spread under the hood. A node's `compute(a, b)` (scalar
function) is automatically *lifted* to spreads by the runtime: if `a` has
length M and `b` has length N, the result has length `max(M, N)` and
shorter inputs cycle. See `liftCyclic` in
[src/runtime/spread.ts](src/runtime/spread.ts).

**Broadcasting across a region boundary** — when a node outside a region
wires into a node inside a region, the value is gathered once per outer
tick and treated as a constant inside the region's iteration loop. This
is how `time/seconds` reaches every particle in a ForEach without
limiting iteration count. Putting `time` on a region *port* would cap
iterations to 1 (port lengths bound the iteration count).

---

## Regions

Region nodes are *orchestrator* nodes whose body is its own scope of
child nodes. The runtime evaluates the body N times per outer tick, with
different splicer / accumulator state each iteration.

Kinds (see `regionKind` on `NodeDef`):
- `foreach` — classic iterator. Iterations = min length of spread inputs.
- `if` — conditional, evaluates body once or zero times.
- `cache` — memoises body output by input hash.
- `subpatch` — visual encapsulation; no semantic iteration.

Splicer nodes are the input/output bridges between outer and inner
scopes (`regionRole: 'splicer-in' | 'splicer-out'`). Accumulators
(`'accumulator-read' | 'accumulator-write'`) thread state across
iterations (running sums, scans, etc.). See
[src/runtime/tick.ts](src/runtime/tick.ts) `runRegion`.

---

## Dataflow direction — left to right, no cycles

Floret's evaluation is a topological sort of the patch graph each tick.
**Every wire flows left → right** (source → sink); no wire can feed back
into a node earlier in the same scope. The compiler rejects cycles.

How we still get feedback / cross-frame state:

- **`state/read` + `state/write` pair** is the only sanctioned loop. The
  Write node captures a value at the *end* of the tick; the Read node
  exposes the previous tick's value at the *start* of the next tick. The
  edge feeding `next` is excluded from topo, so the graph stays acyclic
  for that frame.
- Stateful single-node patterns (`state/value`, `state/spread`) wrap the
  pair into one node. Their `next` input is gathered in a post-eval pass.
- Particle emitters, Game of Life cells, trails, springs, etc. all build
  on this same primitive — never a literal cycle in the patch.

When you're tempted to wire an output back into an earlier input on the
same scope: reach for a state node instead. Keep flow strictly
left → right; the runtime guarantees it stays that way.

---

## Memoisation and the `effectful` flag

Pure compute nodes are memoised across ticks — when their gathered
inputs are structurally equal to last tick's, the runtime skips
`compute` and returns the cached output. This makes static chains free.

Nodes that **hide state inside `runtimeState`** (or anywhere else) must
opt out by setting `effectful: true` on the NodeDef. Otherwise the memo
hits on the second tick and the state never advances.

The rule: if `compute(inputs)` can return a *different* output on a
second call with the *same* inputs, the node is effectful. Examples:
`particles/emitter` (buffer + RNG + dt stash), `particles/trail`
(ring-buffer history advance).

See `effectful?: boolean` in
[src/nodes/types.ts](src/nodes/types.ts) and the compile-time check
in [src/runtime/compile.ts](src/runtime/compile.ts) (around the
memoisable gate).

---

## Stateful nodes — cross-frame state

Nodes with `stateful: true` see their input `next` (port index
`statefulNextPort`, default 1) gathered in a post-eval pass *after* the
rest of the graph has computed for the frame. The value is stashed on
`runtimeState.value` and exposed as the node's output on the **next**
tick. The edge feeding `next` is excluded from topo-sort so the feedback
loop doesn't deadlock. See `state/value`, `state/spread` for the
canonical pattern.

---

## Constant pins

Input ports marked `constant: true` on `PortSpec` are **sampled once at
compile** and stashed on `RuntimeNode.constants[i]`. The tick-time
`gatherInputs` skips them entirely — zero per-frame cost.

- Defined on [`PortSpec`](src/nodes/types.ts) as `constant?: boolean`.
- Compile reads from the same priority chain as unwired inputs:
  `portValues → PortSpec.param → PortSpec.default`. See
  [src/runtime/compile.ts](src/runtime/compile.ts).
- Gather phase excludes constant indices entirely; see
  [src/runtime/tick.ts](src/runtime/tick.ts) `gatherInputs`.
- Editor renders the row label with a small `C` badge and dims the pin
  circle; see [GenericNode.tsx](src/editor/nodes/GenericNode.tsx) and
  the `.port-flag-constant` class in [styles.css](src/app/styles.css).

**Rule of thumb for marking a pin constant**: anything that *shapes
topology* or *allocates memory* — buffer capacity, RNG seeds, integrator
choice, render-template selection, palette LUT size, ring-buffer length.
Anything that's just a number you want to tweak live (rate, strength,
lifetime, position) stays a regular pin.

**Gotchas:**
- Editing the upstream of a constant pin requires the patch to recompile
  to take effect (the runtime is honest: it never re-reads). The editor
  bumps `runtimeVersion` on relevant edits, which triggers recompile via
  the same path as topology changes.
- Wires into a constant pin are honoured only via their resolved
  upstream output at the moment of compile. A wire that pipes
  `time` into a constant pin will "freeze" the time value at compile and
  never update — flag it as a smell during code review.
- Constants must be acyclic in the constant subgraph (the compiler is
  strict about this, since constant resolution is a topo walk before
  ticks begin).

---

## Variable stroke width

Two complementary approaches, each fit for a different use case:

### 1. Per-segment Canvas2D stroke (animated polylines)

Used by `particles/trail`. The per-segment loop that already powers
`fadeOut` is extended to vary `ctx.lineWidth` between segments. Cheap,
no geometry to allocate per frame; ideal for trails that recompute
every tick. Steppy under high contrast or coarse segmentation, but
motion blurs that out at 60fps.

Knobs: `tapered: boolean` + `tailWidth: number` (0..1). When tapered,
segment width = `style.strokeWeight × (1 − age × (1 − tailWidth))`.

### 2. Illustrator-style offset polygon (static drawing)

Used by `draw/path` and `draw/polygon` when a `widths: Spread<number>`
input is wired and the values vary. The polyline is walked, perpendicular
offsets at each vertex are sampled from the widths curve, and a closed
polygon is emitted by stitching the left bank forward + the right bank
backwards. The downstream renderer FILLS that polygon — no `ctx.stroke()`
at all. That's what gives smooth tapers and gradient ribbons.

- Width sampling: `widths` shorter than the polyline becomes a curve
  sampled evenly across t ∈ [0, 1] with linear interpolation; equal
  length acts per-vertex.
- Corners use the bisector direction with a miter clamp (default 4) to
  prevent spikes at sharp angles.
- Endpoints get butt caps (the polygon closes directly across); round
  and square caps are room to grow.
- Colour for the ribbon prefers the resolved style's `fill`, falling
  back to `stroke`, then white. The stroke is dropped on the emitted
  polygon so the ribbon doesn't get a thin outline around it.

Helper: [src/runtime/stroke-offset.ts](src/runtime/stroke-offset.ts)
(`buildOffsetPolygon`, `sampleWidthCurve`, `widthsAreVariable`). The
helper is reusable for future features — glows, inset/outset, brush
strokes — anything that needs the offset-bank geometry.

When the `widths` input is empty or uniform, the draw nodes fall
through to the existing Canvas2D stroke path (fast, single
`ctx.stroke()` per polyline). No perf regression for the common case.

---

## Retained-mode rendering

Draw nodes don't mutate a canvas — they return a tree of `SceneNode`
data describing what to draw. The renderer walks the tree and emits
Canvas2D calls (the native app's GPU renderer walks the same tree).

- Tree types in [src/runtime/scene.ts](src/runtime/scene.ts)
- Renderer in [src/runtime/scene-renderer.ts](src/runtime/scene-renderer.ts)

Three reasons retained > immediate:
1. **One description, any renderer** — swapping Canvas2D for the GPU
   renderer is one new visitor, not a per-node double implementation.
2. **Hit-testing** — "which shape did the user click?" is a tree walk.
3. **Smart rendering** — GPU batching, worker offload, occlusion all
   easier when the renderer sees the whole scene declaratively.

Every `SceneNode` carries its own `ResolvedStyle` (immutable). No
implicit `p.fill()` state machine. Transforms compose via
`{ kind: 'group', children, transform }`.

---

## Styles (immutable composition)

A `Style` is a *pure function* `(base: ResolvedStyle) => ResolvedStyle`.
Style nodes chain via a `base` input:

```
Fill(red) → Stroke(blue, 2) → NoFill → shape
= shape.style = NoFill(Stroke(blue,2)(Fill(red)(DEFAULT)))
```

`ResolvedStyle` carries structured typography fields
(`fontSize`, `fontFamily`, `fontWeight`, `fontStyle`,
`letterSpacing`, `lineHeight`, …). The CSS `font` shorthand is composed
at render time via `composeFontCss(style)`.

See [src/runtime/scene.ts](src/runtime/scene.ts).

---

## Dynamic inputs / outputs

Nodes can change their port shape based on params or what's wired in:

- `dynamicInputs(params)` — used by `color/color` (mode picker
  switches between rgb/hsl/hex), `style/fill` legacy, `color/palette`
  (stopCount → N stops).
- `dynamicOutputs(params, upstreamTypes)` — used by `Repeat`, `Sort`,
  `Reverse`, etc. so output type echoes input type when wired (a Repeat
  of colours has a `color` output, not `any`).

Ports must be returned in deterministic order — compile uses array
index for runtime slots.

See `getResolvedInputs` / `getResolvedOutputs` in
[src/nodes/types.ts](src/nodes/types.ts).

---

## paramStore vs portValues vs default

Three sources of "the value at an unconnected pin," in priority order:

1. **Connected wire** — always wins.
2. **`portValues`** (per-instance, on `node.data`) — what `node(…)` in
   `samples.ts` writes; lets a sample seed a specific value without
   recompile.
3. **`params` + `param: 'name'`** (per-port) — `PortSpec.param` binds an
   inline editor widget to the pin. The runtime reads `paramStore` each
   tick *only when no wire is connected and no `portValues` override is
   set*. Editing the slider doesn't recompile.
4. **`default`** on `PortSpec` — the bottom fallback.

`defaultParams` on `NodeDef` is the initial value of the params dict
when a new instance is created.

---

## Particle architecture

Particle systems run on a perf-first runtime layered into Floret's
existing dataflow model. Headline ideas:

- **Four new port types**: `world`, `force`, `collider`, `particle`.
  See [src/runtime/types.ts](src/runtime/types.ts) and the swatches in
  [styles.css](src/app/styles.css) (`--port-world`, `--port-force`, …).
- **`particle` wires carry one `ParticleBuffer` handle**, not a JS array
  of Particle objects. The "spread" on the wire is always length 1; the
  buffer represents thousands of particles internally. Zero per-frame
  wire allocations.
- **Struct-of-Arrays storage** in `Emitter.runtimeState.__emitter`.
  `Float32Array` for pos/vel/age/size/color/etc.; `Uint8Array` for
  alive flags; free-slot linked list for O(1) spawn and kill.
  See [src/runtime/particles.ts](src/runtime/particles.ts).
- **Force = mutation contract**: every `Force` is
  `(buf, dt, world) => void` and mutates `buf.forceX` / `buf.forceY`
  in-place. No per-particle allocations, no return values.
- **Colliders**: same mutation contract via `Collider.apply(...)`.
  Applied after integration each tick.
- **CPU instancing at render**: new SceneNode variants
  `ParticleBatch` and `ParticleTrailBatch`. The renderer walks them
  with one tight loop over the buffer's typed arrays — one path
  primitive (arc / rect / sub-tree) re-used per particle. Files:
  [scene.ts](src/runtime/scene.ts),
  [scene-renderer.ts](src/runtime/scene-renderer.ts).

### Tick order (inside Emitter)

```
1. Spawn N new particles from rate · dt
2. Zero force accumulators
3. Apply world.forces[*] then localForces[*]
4. Integrate (Euler, current impl)
5. Apply world.colliders[*]
6. Age + cull
```

Pure mutation; no allocations after the first tick.

### What's a Constant pin in this category

`maxCount`, `seed`, `overflowMode` on the Emitter, `template` /
`useBufferColor` on Render-As, `segments` on Trail, `mode` / `inverted`
on every collider, `falloff` / `showViz` on every shaped force. The
rule: anything that *shapes topology* (buffer size, render dispatch,
ring length) or *bakes a numeric path* (which falloff function each
particle multiplies through) is constant.

### Where to extend

- New force: drop a file in [src/nodes/force/](src/nodes/force/),
  register, and reuse `_shared.ts` for falloff + viz boilerplate.
- New collider: same pattern under
  [src/nodes/collider/](src/nodes/collider/).
- New template: add a `ParticleTemplate` variant in
  [particles.ts](src/runtime/particles.ts) and a render case in
  [scene-renderer.ts](src/runtime/scene-renderer.ts).
- New per-particle state (e.g. species id): add a Float32Array /
  Uint8Array to `ParticleBuffer`, initialise it in `initSlot`, expose
  it on any node that reads or writes it.

### Integrators

Two are wired:

- **`euler`** (default) — symplectic / semi-implicit Euler. One pass
  over the buffer, one force evaluation per tick. Stable for typical
  visual workloads.
- **`verlet`** — velocity Verlet. Two-pass: half-velocity → new
  position → re-apply forces at new positions → complete velocity.
  ~2× the per-tick force cost; pays off when long-running accuracy
  matters (orbital particles, dense attractor fields, springs).

Choice lives on the World's `integrator` constant pin and dispatches
once per tick in the Emitter. See
[integrateEuler / integrateVerlet](src/runtime/particles.ts).

### Text rasterisation in `particles/from-layer`

Text shapes are rasterised to an `OffscreenCanvas`, pixels with alpha
above a threshold are stored as inside / outline coordinate buffers,
and the result is cached by composed font + text + alignment +
letter-spacing key (capped at 64 entries). Sampling at tick time is
`pickRandom(insidePixels)` — cheap. Outline mode picks pixels with at
least one outside 4-neighbour. Bounding-box fallback runs if the
runtime has no `OffscreenCanvas`.

### Collider broad-phase

Colliders expose an optional `bbox` (axis-aligned bounding rectangle in
world coords). Polygon and triangle from-layer colliders precompute
edge normals + bbox at construction and use them to: (a) AABB-reject
particles outside the polygon's bbox cheaply, (b) bounce across the
closest edge's normal, with position correction along that normal.
Trail and trail colour both honour per-particle buffer colour
(`useBufferColor`) so fading particles take their trails with them.

### Broad-phase

Collision is O(N · K) — N particles × K colliders — with no spatial
acceleration structure. Polygon colliders short-circuit via an AABB
reject before the point-in-polygon test, so the worst case is cheap.
Analytic circle/rect colliders are a handful of float ops each and
don't benefit from further broad-phase, so this is by design.

---

## Where to look when…

| You want to… | Open |
|---|---|
| Add a new node | [src/nodes/](src/nodes/), register via `registerNode` |
| Change rendering | [src/runtime/scene.ts](src/runtime/scene.ts), [scene-renderer.ts](src/runtime/scene-renderer.ts) |
| Change tick semantics | [src/runtime/tick.ts](src/runtime/tick.ts) |
| Change compile / topology | [src/runtime/compile.ts](src/runtime/compile.ts) |
| Change connection rules | [src/editor/connection-rules.ts](src/editor/connection-rules.ts) |
| Change the inline node UI | [src/editor/nodes/GenericNode.tsx](src/editor/nodes/GenericNode.tsx) |
| Add a sample patch | [src/persistence/samples.ts](src/persistence/samples.ts) |
| Catalog of every node | [NODES.md](NODES.md) |
