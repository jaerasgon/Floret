# Sample review sheet (beta)

All 175 Library samples, by category, to tick off by hand before the beta. Generated from `native/samples/catalogue.json`: open each one from File → Samples Library (Cmd+Shift+O) and fill in the columns.

- **Opens**: the sample opens with no error or warning.
- **Runs and looks right**: it animates smoothly and the picture matches its description.
- **Interaction works**: the mouse, sliders, keys, camera, microphone or MIDI do what the description says (write "n/a" if it has none).
- **Notes**: anything off, with a screenshot name if you took one.

Mark a column with `x` when it is fine.

| Category | Samples |
|---|---|
| Tutorial | 48 |
| Type | 11 |
| Particles | 11 |
| Curves | 7 |
| Motion | 5 |
| Fields | 9 |
| Nature | 9 |
| Shaders | 10 |
| Image | 9 |
| Pixel | 1 |
| Sound | 7 |
| Feedback | 3 |
| Data | 3 |
| Geometry | 13 |
| Compositing | 1 |
| Color | 3 |
| Systems | 15 |
| Interactive | 10 |

## Tutorial

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `tut-01-hello-rect` | 01 · Hello Rect | Tutorial | The smallest patch — one shape wired to the Renderer. Start here. |  |  |  |  |
| `tut-02-color-fill` | 02 · Colour & Fill | Tutorial | Colour a shape with the Color → Fill → Rect chain. |  |  |  |  |
| `tut-03-animate` | 03 · Animate with Time | Tutorial | Drive a shape's position with time/seconds → Math (sin) → Math (multiply). |  |  |  |  |
| `tut-04-mouse` | 04 · Follow the Mouse | Tutorial | Wire input/mouse into an ellipse — the shape tracks the cursor. |  |  |  |  |
| `tut-05-slider` | 05 · Slider Control | Tutorial | A slider drives the rect's width — the basic interactive control. |  |  |  |  |
| `tut-06-spread-foreach` | 06 · Spreads & ForEach | Tutorial | Generate a list with Range Spread, iterate over it with a ForEach to draw N shapes. |  |  |  |  |
| `tut-07-grid` | 07 · Grid of Dots | Tutorial | Grid Spread emits one vec2 per cell — each one wires straight into an ellipse → 8×8 grid. |  |  |  |  |
| `tut-08-palette` | 08 · Palette Colours | Tutorial | Palette + Cycle Spread → colour a grid of dots from a 4-stop palette. |  |  |  |  |
| `tut-09-state` | 09 · Memory (Write → Read) | Tutorial | A Write → Read pair keeps a counter from frame to frame; +1 every frame turns the hue. |  |  |  |  |
| `tut-11-polar` | 11 · Polar Coordinates | Tutorial | sin/cos × radius gives any point on a circle — the foundation of radial layouts. |  |  |  |  |
| `tut-12-circle` | 12 · Circle of Shapes | Tutorial | Ring Spread + ForEach lays N evenly-spaced shapes on a ring. |  |  |  |  |
| `tut-13-map` | 13 · Map: Remap Ranges | Tutorial | math/map converts one numeric range to another — mouseX → hue across the rainbow. |  |  |  |  |
| `tut-14-colorlerp` | 14 · Lerp Two Colours | Tutorial | color/lerp blends colour A → B by t. sin(time) gives a smooth oscillation between them. |  |  |  |  |
| `tut-15-style-chain` | 15 · Style Chains | Tutorial | Wire one style into another's `base` to layer fills + strokes on a single shape. |  |  |  |  |
| `tut-16-group-rotate` | 16 · Group + Transform | Tutorial | transform/group collapses many layers into one; transform/rotate spins the whole assembly. |  |  |  |  |
| `tut-17-perlin` | 17 · Perlin Noise | Tutorial | Smooth random — neighbouring inputs give neighbouring outputs. Drives y and hue per cell. |  |  |  |  |
| `tut-18-random-spread` | 18 · Random Spread | Tutorial | random/uniform seeded per-index → 80 deterministically-placed ellipses. Change seed to reroll. |  |  |  |  |
| `tut-19-if` | 19 · If Region (Conditional) | Tutorial | region/if runs its body only when its condition is true. sin(time) > 0 toggles it, so the rect blinks on and off. |  |  |  |  |
| `tut-20-subpatch` | 20 · Subpatch (Reusable Box) | Tutorial | meta/subpatch wraps a sub-graph behind custom ports — double-click to descend, edit, climb back out. |  |  |  |  |
| `tut-21-trails` | 21 · Trails (Write*/Read*) | Tutorial | Append mouse position to a running spread each frame; draw the buffer as a Path. |  |  |  |  |
| `tut-22-cache` | 22 · Cache Region | Tutorial | A Cache region skips recompute when inputs are stable — wrap expensive work inside. |  |  |  |  |
| `tut-23-polygon` | 23 · Polygon from Points | Tutorial | draw/polygon turns a Spread<vec2> into ONE shape — no ForEach needed. |  |  |  |  |
| `tut-24-path-tension` | 24 · Path with Tension | Tutorial | draw/path tension > 0 turns segments into smooth Catmull-Rom curves; sin(time) breathes between sharp and smooth. |  |  |  |  |
| `tut-25-spring` | 25 · Spring-follow Mouse | Tutorial | Two state slots (pos + vel) + Hooke + damping — the dot trails the cursor with springy delay. |  |  |  |  |
| `tut-26-rasterize` | 26 · Rasterize (Layer → Image) | Tutorial | image/from-layer converts a vector layer into a bitmap; the bitmap is then drawn back via draw/image. |  |  |  |  |
| `tut-27-imgsample` | 27 · Image Sampling Grid | Tutorial | image/sample-spread reads N colours at N points — recolour a 24×24 grid of squares from a rasterised gradient. |  |  |  |  |
| `tut-28-audio-react` | 28 · Audio-Reactive Visual | Tutorial | Mic → amplitude → drives circle size. Allow microphone access when asked. |  |  |  |  |
| `tut-29-oscillator` | 29 · Oscillator (mouse-pitched) | Tutorial | sound/oscillator + sound/output; mouseY → frequency. Move the cursor to slide the tone. |  |  |  |  |
| `tut-30-keyboard` | 30 · Keyboard Branching | Tutorial | Two If regions gated on input/keyboard.key — press R for red rect, B for blue ellipse. |  |  |  |  |
| `tut-31-vec2math` | 31 · Vec2 Math | Tutorial | Operate on vec2 as one unit with Vector Math (scale) — no splitting and recombining. |  |  |  |  |
| `tut-32-normalize` | 32 · Normalize + Polar | Tutorial | Vector Math's normalize gives a unit-vector direction; scale it to any length. |  |  |  |  |
| `tut-33-gradient` | 33 · Gradient Sampling | Tutorial | color/gradient interpolates between palette stops; mouseX picks the colour. |  |  |  |  |
| `tut-34-text` | 34 · Draw Text | Tutorial | draw/text is just a draw node — pos, size, style — with a string param. |  |  |  |  |
| `tut-35-wanderer` | 35 · Wanderer (random drift) | Tutorial | random/wanderer2d gives a vec2 that drifts smoothly with time — natural motion. |  |  |  |  |
| `tut-36-gaussian` | 36 · Gaussian Scatter | Tutorial | random/gaussian clusters samples around a mean — bell-curve placement. |  |  |  |  |
| `tut-37-image-url` | 37 · Image from URL | Tutorial | io/image-url fetches an image; draw/image displays it. Picsum returns a fresh photo per seed. |  |  |  |  |
| `tut-38-image-adjust` | 38 · Image Adjust | Tutorial | Image filters chain like styles — brightness, contrast, saturation, hue. |  |  |  |  |
| `tut-39-pixelate` | 39 · Pixelate | Tutorial | image/pixelate downsamples-then-upsamples for a chunky retro look. |  |  |  |  |
| `tut-40-pixel-sort` | 40 · Pixel Sort | Tutorial | pixel/sort reorders pixel runs by brightness — classic datamosh / glitch-art look. |  |  |  |  |
| `tut-41-sort-shuffle` | 41 · Spread Sort + Shuffle | Tutorial | Shuffle Spread, then Reorder set to sort, reorder a Spread<number>; visualised as a bar chart. |  |  |  |  |
| `tut-42-elementary-ca` | 42 · Elementary CA | Tutorial | spread/elementary-ca advances a 1D boolean row by one Wolfram tick (try rule 30, 90, 110). |  |  |  |  |
| `tut-43-gol` | 43 · Game of Life | Tutorial | spread/gol-step + Write*/Read* runs Conway across frames; classic cellular automaton. |  |  |  |  |
| `tut-44-accumulator` | 44 · Accumulator (Fold) | Tutorial | AccRead / AccWrite split pair threads state across ForEach iterations — running sum of 1..10, all wires LTR. |  |  |  |  |
| `tut-45-midi-cc` | 45 · MIDI CC | Tutorial | input/midi-cc reads a Control Change knob (already mapped 0..1). Drives rect size. |  |  |  |  |
| `tut-46-filter` | 46 · Sound Filter Sweep | Tutorial | sound/filter biquad on a saw; mouseX sweeps the cutoff frequency — classic synth tone. |  |  |  |  |
| `tut-47-envelope` | 47 · ADSR Envelope | Tutorial | sound/envelope shapes amplitude over time (attack / decay / sustain / release). Mouse press = gate. |  |  |  |  |
| `tut-48-pulse` | 48 · Pulse Width Drone | Tutorial | sound/pulse with sin-driven width creates moving timbres; circle breathes with amplitude. |  |  |  |  |
| `tut-49-fft` | 49 · FFT Spectrum Bars | Tutorial | sound/fft outputs a spectrum spread; 128 bars track mic input. Grant microphone permission. |  |  |  |  |

## Type

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `type-kinetic-wave` | Kinetic Wave | Type | Cavalry-style traveling wave: rows × characters laid out with sin(time + charPhase − rowDelay). Per-word colour from a palette. |  |  |  |  |
| `type-halo` | Halo | Type | A phrase laid around three concentric rings that spin, the inner ones faster. Hue cycles per character and per time for a chromatic chase. |  |  |  |  |
| `type-raven` | The Raven | Type | A passage from Poe’s “The Raven” as a serif grid. Each letter fades in and out on its own Perlin walk in black & white, blurring as it dims. |  |  |  |  |
| `type-drift` | Noise Drift | Type | A passage as a monospace grid. A slow Perlin mask picks drifting clusters of letters; inside them a noise field shoves and tints the type while the rest stays readable. |  |  |  |  |
| `type-rain` | Type Rain | Type | Matrix-style falling characters — every column scrolls at its own noise-seeded speed, and letters dim as they fall down the canvas. |  |  |  |  |
| `type-weight-wave` | Weight Wave | Type | A wave of weight and width rolls through a stack of rows in a variable font (style/font-variations); every row reflows at its own axis values. |  |  |  |  |
| `type-spiral-path` | Spiral Type | Type | text/on-path: a phrase scrolls along a spiral and a ring, each glyph turned to the path’s tangent. |  |  |  |  |
| `type-glyph-particles` | Glyph Particles | Type | text/outline samples the letterforms’ contours; particles are born on them while the font’s weight breathes. |  |  |  |  |
| `trim-write-on` | Write-On Lettering | Type | Trim Path (sequence) writes a word's glyph outlines one after another, with a pen nib riding the drawing end. |  |  |  |  |
| `kinetic-type` | Kinetic Type | Type | Text Animate: letters rise, turn and fade in one after another; a second line rides a rolling sine wave. No ForEach. |  |  |  |  |
| `topo-type` | Topo Type | Type | Contours in `shape` mode: rings at even distances round a word, filled like the bands of a contour map, each level its own colour from a palette. |  |  |  |  |

## Particles

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `particles-fountain` | Fountain | Particles | The "hello, particles" patch — point emitter, gravity from the world, drag for terminal velocity, wrap-bounds keep the canvas cycling. |  |  |  |  |
| `particles-text-dust` | Text Dust | Particles | particles/from-layer samples points inside the wired text's glyph outlines — particles spawn from the actual letterforms. |  |  |  |  |
| `particles-attractor` | Attractor Field | Particles | Turbulence + drag + attractor wired into the world. The attractor's viz output draws its influence ring straight into the renderer. |  |  |  |  |
| `particles-pinball` | Polygon Pinball | Particles | An octagonal arena wired into BOTH the renderer (visible) AND collider/from-layer (deflective). Edge-normal bounce, not centroid approximation. |  |  |  |  |
| `particles-spiral` | Galactic Spiral | Particles | Verlet integrator + vortex + central pull = long stable spirals. Per-particle trails carry the buffer colour so each filament keeps its hue. |  |  |  |  |
| `sim-flocking` | Flocking | Particles | Simulation in flocking mode: 300 boids separate, align and cohere on a wrapping field. Sliders reshape the flock live; the mouse herds it. |  |  |  |  |
| `sim-slime-mould` | Slime Mould | Particles | Simulation in physarum mode: 20 000 agents lay and follow a trail map that grows into veins. The trail map is an image, coloured by a palette ramp; the mouse feeds it. |  |  |  |  |
| `sim-dla-coral` | DLA Coral | Particles | Simulation in dla mode: random walkers stick to a growing cluster and it branches like coral. Lines join each particle to its parent, coloured by growth order. |  |  |  |  |
| `sim-cloth` | Cloth | Particles | Simulation in cloth mode: a Verlet spring mesh hung from two corners, blown by gusts. Drag it with the mouse; pull hard and it tears. |  |  |  |  |
| `sim-differential-growth` | Differential Growth | Particles | The Simulation node's growth mode: a ring whose nodes repel and whose edges split as they stretch, folding like coral; edges coloured by age. |  |  |  |  |
| `sim-reaction-diffusion` | Reaction Diffusion | Particles | The Simulation node's reaction mode: Gray–Scott coral growing out of a rasterised word, with presets for mitosis, fingerprints, worms and more. |  |  |  |  |

## Curves

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `lissajous` | Lissajous Threads | Curves | A 240-dot Lissajous curve. Two sliders pick the X/Y frequency ratio; time drifts the phase. |  |  |  |  |
| `kaleidoscope` | Kaleidoscope | Curves | A drifting window into a photo, cut to a wedge and repeated as 12 round the centre, every other one mirrored (the Symmetry node in kaleidoscope mode). The window wanders with time. |  |  |  |  |
| `grid-art` | Grid Art | Curves | 6×6 grid of stochastic walker-cluster cells, each independently seeded — 36 unique compositions sharing one palette. Slider re-rolls the whole grid. The walker algorithm (spawn, drift, tapering trails) lives inside draw/walker-cluster. |  |  |  |  |
| `phyllotaxis` | Phyllotaxis | Curves | A sunflower spiral placed by the golden angle, driven entirely by the ForEach index / fraction loop vars + polar vec2/from-angle. Hue sweeps with fraction. |  |  |  |  |
| `smoothed-path` | Smoothed Path | Curves | A jagged noise polyline with a smoothed overlay — a 3-point box blur using spread/neighbor (broadcast the spread, read left/right via the index pin) inside a ForEach. |  |  |  |  |
| `rose-curve` | Rose Curve | Curves | A rose of 900 dots from the Curve node: a slider sets the petals, an LFO turns them, a Linear spread of hues runs round the colour wheel along the curve and a Sine Spread ripples the dot sizes. |  |  |  |  |
| `flow-lines` | Flow Lines | Curves | Contours' flow mode: evenly spaced streamlines through a noise field, each line a colour from the palette — the plotter-art look, slowly turning with time. |  |  |  |  |

## Motion

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `motion-follow` | Motion Lab | Motion | A swaying target tracked by Smooth (lags), Spring (overshoots/resonates), and DelayLine (phase-shifted) in separate lanes against a shared guide — the difference is visible at a glance. |  |  |  |  |
| `eased-stagger` | Eased Stagger | Motion | A row of dots bobbing in a travelling wave — the travel comes from the ForEach delay pin (per-item clock offset), the bob from a Motion ease curve, and hue from the fraction loop var. |  |  |  |  |
| `lfo-motion` | LFO Motion | Motion | LFOs animating size, colour and position: one LFO with a count of 12 ripples a row of bars, a triangle sweeps the hue, a smooth-random wanders a dot. |  |  |  |  |
| `loop-bloom` | Loop Bloom | Motion | A ring of dots breathing in a travelling wave, built on the Time node's loop phase so it repeats perfectly every 5 s — export one loop as MP4 or GIF (File → Export Video / GIF…) and it plays on repeat without a seam. |  |  |  |  |
| `layout-bloom` | Layout Bloom | Motion | Layout arranges a sunflower spiral whose count steps every 1.8 s: the seeds ease to their new places, new ones grow in and the extra ones shrink away, staggered from the centre out. |  |  |  |  |

## Fields

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `grid-ripple` | Grid Ripple | Fields | A grid of stroked squares offset by a radial sin ripple, driven through a ForEach with per-iteration delay. Stroke colours cycle a palette across the cells. |  |  |  |  |
| `curl-flow` | Curl Flow | Fields | A grid of streaks aligned to a divergence-free curl-noise field that drifts with time — the classic swirling flow-field look. |  |  |  |  |
| `ripple-field` | Ripple Field | Fields | A 25×25 grid of dots, each pulsed by sin(dist_from_center − time). Concentric ripples. |  |  |  |  |
| `noise-wave` | Noise Wave | Fields | A 32×32 grid of cells displaced and hue-shifted by aggressive multi-octave Perlin noise. Pure function of time. |  |  |  |  |
| `force-field` | Force Field Arrows | Fields | A 14×14 grid of arrows pointing along a Perlin angle field that drifts with time. Each tip dot is hue-coded. |  |  |  |  |
| `variations` | Variations & Presets | Fields | A seeded flow field of strokes. Vary (Ctrl/⌘+Shift+R) re-rolls the patch seed for a new picture; the Presets panel recalls Calm or Storm, or morphs between them. |  |  |  |  |
| `noise-cellular-warp` | Cellular Warp | Fields | One Noise node samples a whole 64×64 grid at once: cellular (Worley) noise, F2−F1 for the cell walls, domain-warped by a second noise field and drifting with time, coloured through an OKLab gradient. Switch its mode to perlin, simplex or value. |  |  |  |  |
| `topographic` | Topographic | Fields | Contours traces drifting noise on a grid at 14 levels: the lines of a topographic map, slowly shifting. |  |  |  |  |
| `noise-patterns` | Marble, Wood & Op Art | Fields | Noise's pattern modes, domain-warped: stripes → marble, rings → wood grain, checker → op art. The panels go through blur, posterize and edges for a topographic map. |  |  |  |  |

## Nature

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `pendulum` | Pendulum | Nature | Classic pendulum — angular acceleration = −(g/L)·sin(θ), velocity damped each tick. Write → Read pairs keep θ and ω from frame to frame. NOC §3. |  |  |  |  |
| `noc-wind-gravity` | Wind & Gravity | Nature | Mouse-X drives wind; gravity pulls down. F = m·a, so heavier masses respond less to wind. Slide the mass to feel the asymmetry. NOC §2 — forces. |  |  |  |  |
| `noc-drag` | Drag | Nature | Quadratic fluid drag (−c·\|v\|·v) opposes motion as the ball falls. The terminal-velocity hover is the punchline — gravity and drag balance at constant speed. NOC §2. |  |  |  |  |
| `noc-attraction` | Mouse Attraction | Nature | A mover is pulled toward the mouse cursor via a gravity-like inverse-square force, clamped at close distances. Damping keeps it from blowing up. NOC §2. |  |  |  |  |
| `noc-spring` | Spring | Nature | Hooke's law: F = −k·y (at rest at y = 0), plus gravity and damping. Write → Read pairs keep y and v_y from frame to frame. A bob oscillates beneath a hanging point. NOC §3. |  |  |  |  |
| `noc-seek` | Seek | Nature | Reynolds' classic seek: desired = normalize(target - pos)·maxSpeed, steer = limit(desired − vel, maxForce), vel += steer. Two sliders tune the agent. NOC §5 — autonomous agents. |  |  |  |  |
| `noc-flow-field` | Flow Field | Nature | A 16×16 grid of arrows whose angles come from Perlin noise that drifts with time. Slider tunes noise scale. NOC §5 — fields driving motion. |  |  |  |  |
| `fractal-tree-generative` | Fractal Tree | Nature | The recursion lives IN the graph — no baked coordinates. One ForEach walks 8 root-to-tip paths × 4 levels with three scalar accumulators (angle, x, y) as a turtle: back to the root at each trunk, turning ±0.5 rad by the path's bits, 0.7× shorter per level. A Sublist set to keep-where keeps each of the 15 branches once. Edit the turn (Map), the shrink (0.7) or the trunk (140) and the tree reshapes. |  |  |  |  |
| `lsystem-plant` | L-System Plant | Nature | A bracketed L-system (X → F+[[X]-X]-F[-FX]+X, F → FF) grown five times and drawn by a turtle; jitter makes it organic, an LFO sways it. |  |  |  |  |

## Shaders

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `shader-raymarch` | Shader · Raymarched Gyroid | Shaders | What a single fragment shader can do: a raymarched 3D scene with melting metaballs carved by a gyroid lattice, soft shadows, ambient occlusion, iridescent fresnel and a glowing grid floor. The Scale slider sets the lattice density. |  |  |  |  |
| `shader-plasma` | Shader · Plasma | Shaders | A generative GPU fragment shader (gpu/shader) — a classic animated plasma driven by iTime, with a `scale` uniform steered by a slider. Select the node and edit the GLSL in the side panel. |  |  |  |  |
| `shader-feedback` | Shader · Feedback Ripples | Shaders | A self-feedback fragment shader: each frame diffuses + fades its previous output (iChannel1) and injects a moving source — reaction-diffusion-style ripples that accumulate. Shows the GPU feedback buffer. |  |  |  |  |
| `shader-reaction-diffusion` | Shader · Reaction-Diffusion | Shaders | Gray-Scott reaction-diffusion on a multi-buffer shader: Buffers A–D each advance the chemical field one step (A reads D’s previous frame, so four steps a frame), the Image pass colour-maps it. The canonical multi-pass GPU case — give it a few seconds to bloom. |  |  |  |  |
| `neon-bloom` | Neon Bloom | Shaders | Post-effects on the GPU: two counter-rotating neon polygons (conic gradient strokes) through a chain of Effect nodes — bloom, chromatic aberration, animated film grain and a vignette. Switch any Effect's mode to try displace, dither or halftone. |  |  |  |  |
| `effect-metaballs` | Metaballs | Shaders | Gooey metaballs from Effect modes: blur (gaussian), then threshold on the coverage (channel alpha) so nearby circles melt together, a coverage glow and an inner shadow for depth. Orbiting circles merge and split. |  |  |  |  |
| `effect-rainy-night` | Rainy Night | Shaders | Lens blur turns scattered lights into bokeh discs, motion blur streaks falling rain along its angle, and barrel distortion bulges the frame like a wide lens. |  |  |  |  |
| `effect-warp-speed` | Warp Speed | Shaders | A starfield through spin blur (arcs round the centre) and zoom blur (streaks from it), the zoom pulsing with an LFO for a jump to hyperspace. |  |  |  |  |
| `shader-time-smear` | Time Smear | Shaders | A Shader with `history` on reads the input's past frames: each band of the picture shows a spinning star as it was a different moment ago, a rolling datamosh time smear. |  |  |  |  |
| `neon-orbits` | Neon Orbits | Shaders | Effect's glow with `blend` add: three neon orbits cross over a dark grid, and where their halos overlap the light adds up instead of covering what's beneath. |  |  |  |  |

## Image

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `datamosh` | Datamosh | Image | Load an image, layer it 4× with RGB-tinted copies at time-driven offsets and blend modes (add / subtract) for a chromatic-aberration glitch. |  |  |  |  |
| `photo-mosaic` | Photo Mosaic | Image | Samples a 32×32 grid of pixels from a loaded image and draws each as a dot sized by luminance. Pointillist / Warhol effect that breathes with time. |  |  |  |  |
| `webcam-mosaic` | Webcam Mosaic | Image | A live, mirrored webcam halftone. Flip the Toggle into the camera's `start` pin and grant permission; each cell becomes a yellow circle where the image is light or a blue square where it is dark (luminance threshold via logic/select). |  |  |  |  |
| `body-tracking` | Body Tracking | Image | Live MediaPipe skeleton from the webcam. One Toggle starts the camera + the input/landmarks tracker; cyan lines draw the connection edges and yellow dots the points. Switch hands / face / pose on the Landmarks node. |  |  |  |  |
| `video-pixelate` | Video Pixelate | Image | A looping video file (video/file) through image/pixelate into draw/image. Click Replace… on the Video File node to play your own MP4 / MOV; wire a 0..1 number into `scrub` to scrub. |  |  |  |  |
| `flipbook-sequence` | Flipbook Sequence | Image | Video plays image sequences too: 16 PNG frames of a flower, carried inside the patch, loop at 12 fps, onion-skinned by Echo. Pick one frame of your own numbered run (Floret's PNG-sequence export, say) to play the lot. |  |  |  |  |
| `point-tracker` | Point Tracker | Image | Point Tracker follows three points across a drifting field of dots, frame by frame (pyramidal Lucas–Kanade), from the picture alone; each marker's colour is its confidence, and Echo trails show the paths. |  |  |  |  |
| `camera-slit-scan` | Camera Slit-Scan | Image | The webcam through Echo's slit-scan: each row of the picture comes from a moment further back, so anything that moves stretches and bends. Without a camera, a swinging stand-in shows the effect. |  |  |  |  |
| `ascii-cells` | ASCII Cells | Image | The Effect node's ascii mode: reaction–diffusion fingerprint ridges redrawn as coloured characters picked by brightness. |  |  |  |  |

## Pixel

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `pixel-sort` | Pixel Sort | Pixel | Classic glitch art: sorts maximal runs of pixels along each row by brightness, masked by a (lo, hi) range. Drag the sliders to shift the streaks. |  |  |  |  |

## Sound

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `tone-surface` | Tone Surface | Sound | Move the mouse — X picks pitch, Y picks amplitude. A 20×20 grid lights up around the cursor. |  |  |  |  |
| `fft-bloom` | FFT Bloom | Sound | An oscillator → FFT → polar plot. Each of 64 bins becomes a dot on a ring; mouse Y drives the source pitch. |  |  |  |  |
| `sound-step-sequencer` | Step Sequencer | Sound | A 16 × 8 tone matrix: click or drag cells, the playhead plays each lit column through four MonoSynth voices, Mix, filter, delay and reverb. Tempo, tone, scale. |  |  |  |  |
| `sound-keyboard-synth` | Keyboard Synth | Sound | Play the computer keyboard like a piano (A–K, W E T Y U), Z / X for octaves, mouse for the filter, and an arpeggiator for held chords; sparks and a scope. |  |  |  |  |
| `sound-hand-theremin` | Hand Theremin | Sound | Raise a hand to the webcam (or hold the mouse): left–right is pitch, snapped to a scale or gliding, up–down is loudness; rings and a string vibrate with it. |  |  |  |  |
| `sound-mic-bloom` | Mic Bloom | Sound | A flower that listens: the microphone's spectrum becomes its petals, bass swells the core, beats send rings out. Plays its own demo loop until the mic is on. |  |  |  |  |
| `sound-music-box` | Music Box | Sound | Pins on a turning disc pluck a comb: a noise-composed tune in a scale, three bell voices. Click the disc to add or remove pins, N for a new tune. |  |  |  |  |

## Feedback

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `random-walk` | Random Walk Trail | Feedback | A walker drifts via hashed (time, seed) angles, leaving a 200-point colored trail. Hue cycles along the trail; oldest points fade. Uses state/spread. |  |  |  |  |
| `paint-trail` | Paint Trail | Feedback | Move the mouse to paint. image/from-layer + state/value accumulate each brush stroke on an offscreen canvas — infinite paint with a hue that cycles over time. |  |  |  |  |
| `echo-trails` | Echo Trails | Feedback | Echo keeps the last frames on the GPU: a swinging star leaves ten fading copies. Try trails, delay, slit-scan, freeze. |  |  |  |  |

## Data

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `csv-bar-chart` | CSV Bar Chart | Data | Inline CSV → number spread → 16 colored bars. Edit the `data` param on the CSVInline node to load a new series. |  |  |  |  |
| `local-peaks` | Local Peaks | Data | Noise bars where each lights green only if it is taller than BOTH neighbours — local-maximum detection with spread/neighbor inside a ForEach. |  |  |  |  |
| `number-counters` | Number Counters | Data | Format Number: an eased count-up as money, percent and compact 12.3k, and a m:ss.ff clock. |  |  |  |  |

## Geometry

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `voronoi-mosaic` | Voronoi Mosaic | Geometry | Poisson-disc points → Voronoi cells with gaps, filled from a palette → corners smoothed by Path Ops. A new seed, a new mosaic. |  |  |  |  |
| `contour-blobs` | Contour Blobs | Geometry | Random circles merged by Shape Boolean, then offset in and out by Path Ops inside a ForEach — topographic rings around one blob. |  |  |  |  |
| `plexus` | Plexus | Geometry | Connect Points joins nearby boids with lines that fade and thin as they stretch, a few links per bird. |  |  |  |  |
| `morph-tunnel` | Morph Tunnel | Geometry | Shape Morph blends a circle into a star; 22 copies at rolling t, shrinking and turning, breathe from round to spiky. |  |  |  |  |
| `relaxed-cells` | Relaxed Cells | Geometry | The same random points before and after Scatter's relax (Lloyd relaxation): clumps and gaps become an even mosaic. |  |  |  |  |
| `path-ops-shapes` | Path Ops Shapes | Geometry | Six Path Ops modes: round corners, thicken (a spiral stroke outlined), wave, noise and twist deformers (phases looping with Time), and explode (a hexagon's edges pushed apart). |  |  |  |  |
| `plexus-colours` | Plexus Colours | Geometry | Connect Points colours each line from its two birds: every boid takes a hue from where it flies, and a line blends from one end's colour to the other's. |  |  |  |  |
| `stippled-sphere` | Stippled Sphere | Geometry | Scatter's `stipple` mode shades a lit sphere and its shadow with 2600 ink dots: they gather where the picture is dark and relax into an even, hand-stippled grain. |  |  |  |  |
| `mondrian-splits` | Mondrian Splits | Geometry | Cells' subdivide mode: a square split recursively by chance, black gaps between the parts, colours picked at random from a mostly-white palette. |  |  |  |  |
| `truchet-tiles` | Truchet Tiles | Geometry | The Tiles node's truchet mode: quarter-circle tiles turned by noise, so the arcs clump into long meandering loops in two colours. |  |  |  |  |
| `wfc-circuits` | WFC Circuits | Geometry | The Tiles node's wfc mode: wave function collapse over edge-matched pipe tiles, none running off the grid, each tile type in its own colour. |  |  |  |  |
| `kaleido-slime` | Kaleido Slime | Geometry | The Symmetry node's kaleidoscope mode: a slime mould's trail map cut to a wedge and mirrored six times round the centre, a growing lace mandala. |  |  |  |  |
| `wallpaper-groups` | Wallpaper Groups | Geometry | The Symmetry node's wallpaper mode: a small turning motif repeated by the p6m plane group on a hexagonal lattice. Pick any of the 17 groups. |  |  |  |  |

## Compositing

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `gradient-sunset` | Gradient Sunset | Compositing | Gradient fills and a mask: a linear sky, a sun graded across its own box, hard-edged repeating gradient stripes that slide down and mask the sun by luminance, and a turning conic-gradient ring. |  |  |  |  |

## Color

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `color-spaces` | Colour Spaces | Color | The same red → blue blend in RGB, OKLab and OKLCH (Lerp's space option), a cosine palette, and a tetradic harmony around a base colour whose hue turns with time (Palette's generator modes). |  |  |  |  |
| `effect-colour-grade` | Colour Grade | Color | Chroma key, then a grade: petals on a green backdrop keyed onto a gradient sky (with spill suppression), then Levels, Curves (colour pins = a curve per channel: cool shadows, warm highlights) and a 3D LUT from a .cube file in a File node. |  |  |  |  |
| `palette-from-image` | Palette From Image | Color | The Palette node's image mode: a photo's six dominant colours (k-means in OKLab), used to colour a Mondrian beside it. |  |  |  |  |

## Systems

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `sys-ecosystem` | Ecosystem | Systems | Boids graze on food that regrows: each steers for its nearest ripe item (Simulation's `food`), meals grow the population and hunger thins it — boom and bust in a live chart. Hold the mouse to grow food; sliders for hunger, speed, regrowth, metabolism. |  |  |  |  |
| `sys-epidemic` | Epidemic (SIR) | Systems | 320 wanderers, susceptible → infected → recovered: each catches it from the nearest infected person (Lookup's `nearest` op) within a radius. Click to infect; sliders for radius, roaming, recovery, waning immunity; lockdown toggle; presets; live chart. |  |  |  |  |
| `sys-orbits` | Orbital Sandbox | Systems | Bodies orbiting a binary star, integrated in Read* / Write* state (symplectic Euler). Drag and release to slingshot new bodies in; time-warp and star-separation sliders; Echo trails. |  |  |  |  |
| `sys-weather` | Weather Map | Systems | A drifting pressure field drawn as filled bands and isobars (Contours), with geostrophic wind feeding flow lines, 320 advected particles and a grid of arrows; H and L at the extremes. XY pad moves the storm; sliders for its depth and the wind. |  |  |  |  |
| `sys-clock` | Living Clock | Systems | The real time as a picture (the Clock node): second ticks, minute and hour arcs, the year's days on a spiral, the Moon's actual phase and the date spelled out, over a sky that follows the hour. Time-travel slider, 12/24-hour toggle. |  |  |  |  |
| `sys-rd-topo` | Topographic Reaction | Systems | Reaction–diffusion you paint with the mouse, read as terrain by Contours (filled elevation bands and isolines) and printed on grainy paper. Presets switch the chemistry; a toggle shows the raw chemicals. |  |  |  |  |
| `sys-plotter` | Plotter Pipeline | Systems | Flow-field streamlines in three inks, drawn line after line by Trim Path with a pen riding the tip, on paper with a shadow and grain. Sliders for pen width, spacing, field scale and plot speed; Replot; a clean-output toggle for export. |  |  |  |  |
| `sys-landscape` | Parallax Valley | Systems | Five noise ridges sliding past at parallax speeds, fading into the sky by distance, with fog bands, a sun and moon on an arc, stars at night and a small flock of birds. Time-of-day and travel sliders; move the mouse to look around. |  |  |  |  |
| `sys-city` | Night City | Systems | A city from one subdivision: per-block heights, roofs and sun shadows in a ForEach, windows scattered on the roofs, cars as Trim Path pieces circling the blocks and lamps along the streets, blooming after dark. Time-of-day and density sliders; Vary. |  |  |  |  |
| `sys-garden` | Seasons Garden | Systems | L-system plants that grow (Trim Path), sway in the wind and turn with the seasons; autumn leaves and winter snow fall from particle emitters in a windy world. Season and wind sliders, auto-year toggle, Replant. |  |  |  |  |
| `sys-tile-world` | Tile World | Systems | An island from noise elevation (coloured Cells), wave-function-collapse roads masked to land, Symmetry rosettes for forests and villages, drifting metaball clouds with shadows and a day/night cycle with village lights. Sea-level and time sliders; Vary. |  |  |  |  |
| `sys-data-story` | Data Story | Systems | An animated bar-chart race from inline CSV: Partition picks the year, two sorts give the ranks, state eases the bars, a ForEach sums each year for a line revealed by Trim Path, and captions animate in. Play toggle, year stepper, speed slider. |  |  |  |  |
| `sys-music-viz` | Generative Band | Systems | A self-playing trio — noise-walk melody (MonoSynth, delay, reverb), sawtooth bass, noise hats — and the same numbers drawn: a 16-step ring, note bursts, the melody's spectrum and waveform, through trails and bloom. Sliders for bpm, density, volume; major/minor. |  |  |  |  |
| `sys-glitch-tracker` | Glitch Tracker | Systems | Generated footage tracked by optical flow (Point Tracker) with a HUD of boxes and links, while the picture goes through Echo slit-scan, chromatic aberration and displacement bursts. Hold the mouse to re-seed the trackers; glitch slider; slit-scan toggle. |  |  |  |  |
| `sys-poster` | Generative Poster | Systems | A Swiss-style poster as rules: Layout grid modules picking shapes, turns and harmony colours from seeded noise, an animated title and a grain or halftone finish. Colour picker, columns slider (modules ease to the new grid), Vary, and presets to morph between. |  |  |  |  |

## Interactive

| Id | Name | Category | Description | Opens | Runs and looks right | Interaction works | Notes |
|---|---|---|---|---|---|---|---|
| `play-brush-studio` | Brush Studio | Interactive | Paint with the mouse on a canvas fed back to itself: keys 1–4 switch ink, ribbon, spray and rainbow brushes, M mirrors every stroke eight ways, C clears. |  |  |  |  |
| `play-physics-sandbox` | Slingshot Sandbox | Interactive | Press, pull back and let go to fling balls round bumpers, with an aiming arc; G cycles gravity, W bouncing or wrapping walls, C clears, R rain. |  |  |  |  |
| `play-flock-herder` | Flock Herder | Interactive | Hold the mouse to lure a flock of boids (F turns the lure into a scare); keys 1–3 change its character, S scatters it. Plexus lines and trails. |  |  |  |  |
| `play-breakout` | Breakout | Interactive | A complete game from nodes: the paddle follows the mouse, click to launch, bricks break into sparks, score, lives and levels. Plays itself until you click. |  |  |  |  |
| `play-voronoi-editor` | Voronoi Editor | Interactive | Drag points to reshape Voronoi cells, click to add, X to remove, hold L to relax them; D shows the Delaunay triangles, H the hull. Nearest-point picking from Expression + Statistics. |  |  |  |  |
| `play-typing-toy` | Typing Toy | Interactive | Type and each letter drops in with a bounce, a spin and sparks, then bobs on a wave under a rainbow; Keyboard's text mode keeps the string. |  |  |  |  |
| `play-kaleidoscope` | Kaleidoscope Toy | Interactive | Beads tumble in a round chamber seen through mirrors: drag sideways to turn the tube (gravity turns too) and drop beads; keys 3–9 set the mirrors. |  |  |  |  |
| `play-reaction-painter` | Reaction Painter | Interactive | Drag to paint chemical into a live reaction–diffusion dish; keys 1–7 switch coral, mitosis, fingerprints, worms, maze, spots and waves while it runs. |  |  |  |  |
| `play-midi-garden` | MIDI Garden | Interactive | Every MIDI note plants a flower and plays it; CC knobs turn hue, petals, spin and bloom. No MIDI: sliders, keys A–K and clicks stand in. |  |  |  |  |
| `play-preset-morph` | Preset Morph | Interactive | A spirograph to perform: six presets to recall or morph between in the Presets panel, Vary for new phases, the XY pad and a drag to fan the layers. |  |  |  |  |
