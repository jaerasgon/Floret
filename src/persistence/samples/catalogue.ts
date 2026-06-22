import type { LibraryEntry, PatchLibrary } from '../library';
import { serializePatch } from '../serialize';
import { spaced, type Sample } from './helpers';
import { buildGridArt, buildKaleidoscope, buildLissajous } from './builds/Curves';
import { buildForceField, buildNoiseWave, buildRippleField } from './builds/Fields';
import { buildGridRipple } from './builds/GridRipple';
import { buildCurlFlow } from './builds/CurlFlow';
import { buildMotionFollow } from './builds/MotionFollow';
import { buildPhyllotaxis } from './builds/Phyllotaxis';
import { buildEasedStagger } from './builds/EasedStagger';
import { buildSmoothedPath } from './builds/SmoothedPath';
import { buildLocalPeaks } from './builds/LocalPeaks';
import { buildFFTBloom, buildToneSurface } from './builds/Sound';
import { buildPaintTrail, buildRandomWalkTrail } from './builds/Feedback';
import { buildDatamosh, buildPhotoMosaic, buildWebcamMosaic, buildBodyTracking } from './builds/Image';
import { buildCSVBarChart } from './builds/Data';
import { buildPixelSort } from './builds/Pixel';
import { buildNocAttraction, buildNocDrag, buildNocFlowField, buildNocFractalTree, buildNocSeek, buildNocSpring, buildNocWindGravity, buildPendulum } from './builds/Nature';
import { buildTutAccumulator, buildTutAnimateWithTime, buildTutAudioReactive, buildTutCacheRegion, buildTutCircleOfShapes, buildTutColorAndFill, buildTutColorLerp, buildTutElementaryCA, buildTutEnvelope, buildTutFFT, buildTutGaussian, buildTutGoL, buildTutGradient, buildTutGrid, buildTutGroupTransform, buildTutHelloRect, buildTutIfRegion, buildTutImageAdjust, buildTutImageSample, buildTutImageUrl, buildTutKeyboardBranch, buildTutMap, buildTutMidiCC, buildTutMouse, buildTutNormalize, buildTutOscillator, buildTutPalette, buildTutPathTension, buildTutPerlin, buildTutPixelSort, buildTutPixelate, buildTutPolar, buildTutPolygon, buildTutPulse, buildTutRandomSpread, buildTutRasterize, buildTutSlider, buildTutSortShuffle, buildTutSoundFilter, buildTutSpreadForeach, buildTutSpringFollow, buildTutState, buildTutStyleChain, buildTutSubpatch, buildTutText, buildTutTrails, buildTutVec2Math, buildTutWanderer } from './builds/Tutorial';
import { buildKineticType, buildTypeDrift, buildTypeHalo, buildTypeRain, buildTypeRaven } from './builds/Type';
import { buildParticlesAttractor, buildParticlesFountain, buildParticlesPinball, buildParticlesSpiral, buildParticlesTextDust } from './builds/Particles';
import { buildEinkClock } from './builds/Eink';

/**
 * Sample categories — order here is the order the tabs appear in the
 * Samples menu. The first non-empty category is selected by default.
 */
export const SAMPLE_CATEGORIES = [
  'Tutorial',
  'Type',
  'Particles',
  'Motion',
  'Curves',
  'Fields',
  'Nature',
  'Image',
  'Pixel',
  'Sound',
  'Feedback',
  'Data',
] as const;

export const SAMPLES: Sample[] = [
  // ── Tutorial (start here) ──────────────────────────────
  {
    id: 'tut-01-hello-rect',
    name: '01 · Hello Rect',
    category: 'Tutorial',
    description: 'The smallest patch — one shape wired to the Renderer. Start here.',
    build: () => spaced(buildTutHelloRect()),
  },
  {
    id: 'tut-02-color-fill',
    name: '02 · Colour & Fill',
    category: 'Tutorial',
    description: 'Colour a shape with the Color → Fill → Rect chain.',
    build: () => spaced(buildTutColorAndFill()),
  },
  {
    id: 'tut-03-animate',
    name: '03 · Animate with Time',
    category: 'Tutorial',
    description: 'Drive a shape\'s position with time/seconds → sin → multiply.',
    build: () => spaced(buildTutAnimateWithTime()),
  },
  {
    id: 'tut-04-mouse',
    name: '04 · Follow the Mouse',
    category: 'Tutorial',
    description: 'Wire input/mouse into an ellipse — the shape tracks the cursor.',
    build: () => spaced(buildTutMouse()),
  },
  {
    id: 'tut-05-slider',
    name: '05 · Slider Control',
    category: 'Tutorial',
    description: 'A slider drives the rect\'s width — the basic interactive control.',
    build: () => spaced(buildTutSlider()),
  },
  {
    id: 'tut-06-spread-foreach',
    name: '06 · Spreads & ForEach',
    category: 'Tutorial',
    description: 'Generate a list with RangeSpread, iterate over it with a ForEach to draw N shapes.',
    build: () => spaced(buildTutSpreadForeach()),
  },
  {
    id: 'tut-07-grid',
    name: '07 · Grid of Dots',
    category: 'Tutorial',
    description: 'GridSpread emits one vec2 per cell — split + ellipse → 8×8 grid.',
    build: () => spaced(buildTutGrid()),
  },
  {
    id: 'tut-08-palette',
    name: '08 · Palette Colours',
    category: 'Tutorial',
    description: 'Palette + CycleSpread → colour a grid of dots from a 4-stop palette.',
    build: () => spaced(buildTutPalette()),
  },
  {
    id: 'tut-09-state',
    name: '09 · State (per-frame memory)',
    category: 'Tutorial',
    description: 'A state/value counter incremented every frame drives a rotating hue.',
    build: () => spaced(buildTutState()),
  },
  {
    id: 'tut-11-polar',
    name: '11 · Polar Coordinates',
    category: 'Tutorial',
    description: 'sin/cos × radius gives any point on a circle — the foundation of radial layouts.',
    build: () => spaced(buildTutPolar()),
  },
  {
    id: 'tut-12-circle',
    name: '12 · Circle of Shapes',
    category: 'Tutorial',
    description: 'RingSpread + ForEach lays N evenly-spaced shapes on a ring.',
    build: () => spaced(buildTutCircleOfShapes()),
  },
  {
    id: 'tut-13-map',
    name: '13 · Map: Remap Ranges',
    category: 'Tutorial',
    description: 'math/map converts one numeric range to another — mouseX → hue across the rainbow.',
    build: () => spaced(buildTutMap()),
  },
  {
    id: 'tut-14-colorlerp',
    name: '14 · Lerp Two Colours',
    category: 'Tutorial',
    description: 'color/lerp blends colour A → B by t. sin(time) gives a smooth oscillation between them.',
    build: () => spaced(buildTutColorLerp()),
  },
  {
    id: 'tut-15-style-chain',
    name: '15 · Style Chains',
    category: 'Tutorial',
    description: 'Wire one style into another\'s `base` to layer fills + strokes on a single shape.',
    build: () => spaced(buildTutStyleChain()),
  },
  {
    id: 'tut-16-group-rotate',
    name: '16 · Group + Transform',
    category: 'Tutorial',
    description: 'transform/group collapses many layers into one; transform/rotate spins the whole assembly.',
    build: () => spaced(buildTutGroupTransform()),
  },
  {
    id: 'tut-17-perlin',
    name: '17 · Perlin Noise',
    category: 'Tutorial',
    description: 'Smooth random — neighbouring inputs give neighbouring outputs. Drives y and hue per cell.',
    build: () => spaced(buildTutPerlin()),
  },
  {
    id: 'tut-18-random-spread',
    name: '18 · Random Spread',
    category: 'Tutorial',
    description: 'random/uniform seeded per-index → 80 deterministically-placed ellipses. Change seed to reroll.',
    build: () => spaced(buildTutRandomSpread()),
  },
  {
    id: 'tut-19-if',
    name: '19 · If Region (Conditional)',
    category: 'Tutorial',
    description: 'region/if runs its body only when condition is true. Move past 150px from centre to reveal a rect.',
    build: () => spaced(buildTutIfRegion()),
  },
  {
    id: 'tut-20-subpatch',
    name: '20 · Subpatch (Reusable Box)',
    category: 'Tutorial',
    description: 'meta/subpatch wraps a sub-graph behind custom ports — double-click to descend, edit, climb back out.',
    build: () => spaced(buildTutSubpatch()),
  },
  {
    id: 'tut-21-trails',
    name: '21 · Trails (Write*/Read*)',
    category: 'Tutorial',
    description: 'Append mouse position to a running spread each frame; draw the buffer as a Path.',
    build: () => spaced(buildTutTrails()),
  },
  {
    id: 'tut-22-cache',
    name: '22 · Cache Region',
    category: 'Tutorial',
    description: 'A Cache region skips recompute when inputs are stable — wrap expensive work inside.',
    build: () => spaced(buildTutCacheRegion()),
  },
  {
    id: 'tut-23-polygon',
    name: '23 · Polygon from Points',
    category: 'Tutorial',
    description: 'draw/polygon turns a Spread<vec2> into ONE shape — no ForEach needed.',
    build: () => spaced(buildTutPolygon()),
  },
  {
    id: 'tut-24-path-tension',
    name: '24 · Path with Tension',
    category: 'Tutorial',
    description: 'draw/path tension > 0 turns segments into smooth Catmull-Rom curves; sin(time) breathes between sharp and smooth.',
    build: () => spaced(buildTutPathTension()),
  },
  {
    id: 'tut-25-spring',
    name: '25 · Spring-follow Mouse',
    category: 'Tutorial',
    description: 'Two state slots (pos + vel) + Hooke + damping — the dot trails the cursor with springy delay.',
    build: () => spaced(buildTutSpringFollow()),
  },
  {
    id: 'tut-26-rasterize',
    name: '26 · Rasterize (Layer → Image)',
    category: 'Tutorial',
    description: 'image/from-layer converts a vector layer into a bitmap; the bitmap is then drawn back via draw/image.',
    build: () => spaced(buildTutRasterize()),
  },
  {
    id: 'tut-27-imgsample',
    name: '27 · Image Sampling Grid',
    category: 'Tutorial',
    description: 'image/sample-spread reads N colours at N points — recolour a 24×24 dot grid from a rasterised source.',
    build: () => spaced(buildTutImageSample()),
  },
  {
    id: 'tut-28-audio-react',
    name: '28 · Audio-Reactive Visual',
    category: 'Tutorial',
    description: 'Mic → amplitude → drives circle size. Grant the browser microphone permission.',
    build: () => spaced(buildTutAudioReactive()),
  },
  {
    id: 'tut-29-oscillator',
    name: '29 · Oscillator (mouse-pitched)',
    category: 'Tutorial',
    description: 'sound/oscillator + sound/output; mouseY → frequency. Move the cursor to slide the tone.',
    build: () => spaced(buildTutOscillator()),
  },
  {
    id: 'tut-30-keyboard',
    name: '30 · Keyboard Branching',
    category: 'Tutorial',
    description: 'Two If regions gated on input/keyboard.key — press R for red rect, B for blue ellipse.',
    build: () => spaced(buildTutKeyboardBranch()),
  },
  {
    id: 'tut-31-vec2math',
    name: '31 · Vec2 Math',
    category: 'Tutorial',
    description: 'Operate on vec2 as one unit with vec2/add, vec2/scale — no splitting and recombining.',
    build: () => spaced(buildTutVec2Math()),
  },
  {
    id: 'tut-32-normalize',
    name: '32 · Normalize + Polar',
    category: 'Tutorial',
    description: 'vec2/normalize gives a unit-vector direction; scale it to any length.',
    build: () => spaced(buildTutNormalize()),
  },
  {
    id: 'tut-33-gradient',
    name: '33 · Gradient Sampling',
    category: 'Tutorial',
    description: 'color/gradient interpolates between palette stops; mouseX picks the colour.',
    build: () => spaced(buildTutGradient()),
  },
  {
    id: 'tut-34-text',
    name: '34 · Draw Text',
    category: 'Tutorial',
    description: 'draw/text is just a draw node — x, y, size, style — with a string param.',
    build: () => spaced(buildTutText()),
  },
  {
    id: 'tut-35-wanderer',
    name: '35 · Wanderer (random drift)',
    category: 'Tutorial',
    description: 'random/wanderer2d gives a vec2 that drifts smoothly with time — natural motion.',
    build: () => spaced(buildTutWanderer()),
  },
  {
    id: 'tut-36-gaussian',
    name: '36 · Gaussian Scatter',
    category: 'Tutorial',
    description: 'random/gaussian clusters samples around a mean — bell-curve placement.',
    build: () => spaced(buildTutGaussian()),
  },
  {
    id: 'tut-37-image-url',
    name: '37 · Image from URL',
    category: 'Tutorial',
    description: 'io/image-url fetches an image; draw/image displays it. Picsum returns a fresh photo per seed.',
    build: () => spaced(buildTutImageUrl()),
  },
  {
    id: 'tut-38-image-adjust',
    name: '38 · Image Adjust',
    category: 'Tutorial',
    description: 'Image filters chain like styles — brightness, contrast, saturation, hue.',
    build: () => spaced(buildTutImageAdjust()),
  },
  {
    id: 'tut-39-pixelate',
    name: '39 · Pixelate',
    category: 'Tutorial',
    description: 'image/pixelate downsamples-then-upsamples for a chunky retro look.',
    build: () => spaced(buildTutPixelate()),
  },
  {
    id: 'tut-40-pixel-sort',
    name: '40 · Pixel Sort',
    category: 'Tutorial',
    description: 'pixel/sort reorders pixel runs by brightness — classic datamosh / glitch-art look.',
    build: () => spaced(buildTutPixelSort()),
  },
  {
    id: 'tut-41-sort-shuffle',
    name: '41 · Spread Sort + Shuffle',
    category: 'Tutorial',
    description: 'SortSpread + ShuffleSpread reorder a Spread<number>; visualised as a bar chart.',
    build: () => spaced(buildTutSortShuffle()),
  },
  {
    id: 'tut-42-elementary-ca',
    name: '42 · Elementary CA',
    category: 'Tutorial',
    description: 'spread/elementary-ca advances a 1D boolean row by one Wolfram tick (try rule 30, 90, 110).',
    build: () => spaced(buildTutElementaryCA()),
  },
  {
    id: 'tut-43-gol',
    name: '43 · Game of Life',
    category: 'Tutorial',
    description: 'spread/gol-step + Write*/Read* runs Conway across frames; classic cellular automaton.',
    build: () => spaced(buildTutGoL()),
  },
  {
    id: 'tut-44-accumulator',
    name: '44 · Accumulator (Fold)',
    category: 'Tutorial',
    description: 'AccRead / AccWrite split pair threads state across ForEach iterations — running sum of 1..10, all wires LTR.',
    build: () => spaced(buildTutAccumulator()),
  },
  {
    id: 'tut-45-midi-cc',
    name: '45 · MIDI CC',
    category: 'Tutorial',
    description: 'input/midi-cc reads a Control Change knob (already mapped 0..1). Drives rect size.',
    build: () => spaced(buildTutMidiCC()),
  },
  {
    id: 'tut-46-filter',
    name: '46 · Sound Filter Sweep',
    category: 'Tutorial',
    description: 'sound/filter biquad on a saw; mouseX sweeps the cutoff frequency — classic synth tone.',
    build: () => spaced(buildTutSoundFilter()),
  },
  {
    id: 'tut-47-envelope',
    name: '47 · ADSR Envelope',
    category: 'Tutorial',
    description: 'sound/envelope shapes amplitude over time (attack / decay / sustain / release). Mouse press = gate.',
    build: () => spaced(buildTutEnvelope()),
  },
  {
    id: 'tut-48-pulse',
    name: '48 · Pulse Width Drone',
    category: 'Tutorial',
    description: 'sound/pulse with sin-driven width creates moving timbres; circle breathes with amplitude.',
    build: () => spaced(buildTutPulse()),
  },
  {
    id: 'tut-49-fft',
    name: '49 · FFT Spectrum Bars',
    category: 'Tutorial',
    description: 'sound/fft outputs a spectrum spread; 128 bars track mic input. Grant microphone permission.',
    build: () => spaced(buildTutFFT()),
  },
  {
    id: 'type-kinetic-wave',
    name: 'Kinetic Wave',
    category: 'Type',
    description:
      'Cavalry-style traveling wave: rows × characters laid out with sin(time + charPhase − rowDelay). Per-word colour from a palette.',
    build: () => spaced(buildKineticType()),
  },
  {
    id: 'type-halo',
    name: 'Halo',
    category: 'Type',
    description:
      'Phrase laid around a circle; the whole ring spins. Hue cycles per character and per time for a chromatic chase.',
    build: () => spaced(buildTypeHalo()),
  },
  {
    id: 'type-raven',
    name: 'The Raven',
    category: 'Type',
    description:
      'A passage from Poe’s “The Raven” as a serif grid. Each letter fades in and out on its own Perlin walk in black & white, blurring as it dims.',
    build: () => spaced(buildTypeRaven()),
  },
  {
    id: 'type-drift',
    name: 'Noise Drift',
    category: 'Type',
    description:
      'A passage as a monospace grid. A slow Perlin mask picks drifting clusters of letters; inside them a noise field shoves and tints the type while the rest stays readable.',
    build: () => spaced(buildTypeDrift()),
  },
  {
    id: 'type-rain',
    name: 'Type Rain',
    category: 'Type',
    description:
      'Matrix-style falling characters — every column scrolls at its own noise-seeded speed; brightness fades head → tail down the column.',
    build: () => spaced(buildTypeRain()),
  },
  // ── Particles ──────────────────────────────────────────
  {
    id: 'particles-fountain',
    name: 'Fountain',
    category: 'Particles',
    description:
      'The "hello, particles" patch — point emitter, gravity from the world, drag for terminal velocity, wrap-bounds keep the canvas cycling.',
    build: () => spaced(buildParticlesFountain()),
  },
  {
    id: 'particles-text-dust',
    name: 'Text Dust',
    category: 'Particles',
    description:
      'particles/from-layer rasterises the wired text shape and samples inside the glyph alpha mask — particles spawn from the actual letterforms.',
    build: () => spaced(buildParticlesTextDust()),
  },
  {
    id: 'particles-attractor',
    name: 'Attractor Field',
    category: 'Particles',
    description:
      'Turbulence + drag + attractor wired into the world. The attractor\'s viz output draws its influence ring straight into the renderer.',
    build: () => spaced(buildParticlesAttractor()),
  },
  {
    id: 'particles-pinball',
    name: 'Polygon Pinball',
    category: 'Particles',
    description:
      'An octagonal arena wired into BOTH the renderer (visible) AND collider/from-layer (deflective). Edge-normal bounce, not centroid approximation.',
    build: () => spaced(buildParticlesPinball()),
  },
  {
    id: 'particles-spiral',
    name: 'Galactic Spiral',
    category: 'Particles',
    description:
      'Verlet integrator + vortex + central pull = long stable spirals. Per-particle trails carry the buffer colour so each filament keeps its hue.',
    build: () => spaced(buildParticlesSpiral()),
  },

  // ── Curves ─────────────────────────────────────────────
  {
    id: 'lissajous',
    name: 'Lissajous Threads',
    category: 'Curves',
    description: 'A 240-dot Lissajous curve. Two sliders pick the X/Y frequency ratio; time drifts the phase.',
    build: () => spaced(buildLissajous()),
  },
  {
    id: 'kaleidoscope',
    name: 'Kaleidoscope',
    category: 'Curves',
    description: 'A mouse-controlled triangle mirrored 12 times around the centre. Hue cycles with time.',
    build: () => spaced(buildKaleidoscope()),
  },
  {
    id: 'grid-art',
    name: 'Grid Art',
    category: 'Curves',
    description: '6×6 grid of stochastic walker-cluster cells, each independently seeded — 36 unique compositions sharing one palette. Slider re-rolls the whole grid. The walker algorithm (spawn, drift, tapering trails) lives inside draw/walker-cluster.',
    build: () => spaced(buildGridArt()),
  },

  // ── Motion ─────────────────────────────────────────────
  {
    id: 'eink-clock',
    name: 'E-ink Clock',
    category: 'Motion',
    description: 'A bold sweeping clock hand fanned to the Renderer AND an io/eink node that POSTs the frame to a networked Waveshare e-paper panel every 25s (the toggle forces a send now). Set the node’s url to your panel.',
    build: () => spaced(buildEinkClock()),
  },
  {
    id: 'motion-follow',
    name: 'Motion Lab',
    category: 'Motion',
    description: 'A swaying target tracked by Smooth (lags), Spring (overshoots/resonates), and DelayLine (phase-shifted) in separate lanes against a shared guide — the difference is visible at a glance.',
    build: () => buildMotionFollow(),
  },
  {
    id: 'eased-stagger',
    name: 'Eased Stagger',
    category: 'Motion',
    description: 'A row of dots bobbing in a travelling wave — the travel comes from the ForEach delay pin (per-item clock offset), the bob from a Motion ease curve, and hue from the fraction loop var.',
    build: () => buildEasedStagger(),
  },

  // ── Curves ─────────────────────────────────────────────
  {
    id: 'phyllotaxis',
    name: 'Phyllotaxis',
    category: 'Curves',
    description: 'A sunflower spiral placed by the golden angle, driven entirely by the ForEach index / fraction loop vars + polar vec2/from-angle. Hue sweeps with fraction.',
    build: () => buildPhyllotaxis(),
  },
  {
    id: 'smoothed-path',
    name: 'Smoothed Path',
    category: 'Curves',
    description: 'A jagged noise polyline with a smoothed overlay — a 3-point box blur using spread/neighbor (broadcast the spread, read left/right via the index pin) inside a ForEach.',
    build: () => buildSmoothedPath(),
  },

  // ── Fields ─────────────────────────────────────────────
  {
    id: 'grid-ripple',
    name: 'Grid Ripple',
    category: 'Fields',
    description: 'A grid of stroked squares offset by a radial sin ripple, driven through a ForEach with per-iteration delay. Stroke colours cycle a palette across the cells.',
    // Authored from real editor coordinates — no spaced() rescale.
    build: () => buildGridRipple(),
  },
  {
    id: 'curl-flow',
    name: 'Curl Flow',
    category: 'Fields',
    description: 'A grid of streaks aligned to a divergence-free curl-noise field that drifts with time — the classic swirling flow-field look.',
    build: () => buildCurlFlow(),
  },
  {
    id: 'ripple-field',
    name: 'Ripple Field',
    category: 'Fields',
    description: 'A 25×25 grid of dots, each pulsed by sin(dist_from_center − time). Concentric ripples.',
    build: () => spaced(buildRippleField()),
  },
  {
    id: 'noise-wave',
    name: 'Noise Wave',
    category: 'Fields',
    description: 'A 32×32 grid of cells displaced and hue-shifted by aggressive multi-octave Perlin noise. Pure function of time.',
    build: () => spaced(buildNoiseWave()),
  },
  {
    id: 'force-field',
    name: 'Force Field Arrows',
    category: 'Fields',
    description: 'A 14×14 grid of arrows pointing along a Perlin angle field that drifts with time. Each tip dot is hue-coded.',
    build: () => spaced(buildForceField()),
  },

  // ── Nature (Nature of Code) ────────────────────────────
  {
    id: 'pendulum',
    name: 'Pendulum',
    category: 'Nature',
    description: 'Classic pendulum — angular acceleration = −(g/L)·sin(θ), velocity damped each tick. State<number> for θ and ω. NOC §3.',
    build: () => spaced(buildPendulum()),
  },
  {
    id: 'noc-wind-gravity',
    name: 'Wind & Gravity',
    category: 'Nature',
    description: 'Mouse-X drives wind; gravity pulls down. F = m·a, so heavier masses respond less to wind. Slide the mass to feel the asymmetry. NOC §2 — forces.',
    build: () => spaced(buildNocWindGravity()),
  },
  {
    id: 'noc-drag',
    name: 'Drag',
    category: 'Nature',
    description: 'Quadratic fluid drag (−c·|v|·v) opposes motion as the ball falls. The terminal-velocity hover is the punchline — gravity and drag balance at constant speed. NOC §2.',
    build: () => spaced(buildNocDrag()),
  },
  {
    id: 'noc-attraction',
    name: 'Mouse Attraction',
    category: 'Nature',
    description: 'A mover is pulled toward the mouse cursor via a gravity-like inverse-square force, clamped at close distances. Damping keeps it from blowing up. NOC §2.',
    build: () => spaced(buildNocAttraction()),
  },
  {
    id: 'noc-spring',
    name: 'Spring',
    category: 'Nature',
    description: "Hooke's law: F = −k·(y − restY), plus gravity and damping. State<number> for y and v_y. A bob oscillates beneath a hanging point. NOC §3.",
    build: () => spaced(buildNocSpring()),
  },
  {
    id: 'noc-seek',
    name: 'Seek',
    category: 'Nature',
    description: "Reynolds' classic seek: desired = normalize(target - pos)·maxSpeed, steer = limit(desired − vel, maxForce), vel += steer. Two sliders tune the agent. NOC §5 — autonomous agents.",
    build: () => spaced(buildNocSeek()),
  },
  {
    id: 'noc-flow-field',
    name: 'Flow Field',
    category: 'Nature',
    description: 'A 16×16 grid of arrows whose angles come from Perlin noise that drifts with time. Slider tunes noise scale. NOC §5 — fields driving motion.',
    build: () => spaced(buildNocFlowField()),
  },
  {
    id: 'noc-fractal-tree',
    name: 'Fractal Tree',
    category: 'Nature',
    description: 'A 2-level Pythagoras-style tree built from three lines (trunk + 2 branches), with the trunk rotated by sin(time) to give a gentle sway. NOC §8.',
    build: () => spaced(buildNocFractalTree()),
  },

  // ── Image ──────────────────────────────────────────────
  {
    id: 'datamosh',
    name: 'Datamosh',
    category: 'Image',
    description: 'Load an image, layer it 4× with RGB-tinted copies at time-driven offsets and blend modes (SCREEN / DIFFERENCE) for a chromatic-aberration glitch.',
    build: () => spaced(buildDatamosh()),
  },
  {
    id: 'photo-mosaic',
    name: 'Photo Mosaic',
    category: 'Image',
    description: 'Samples a 32×32 grid of pixels from a loaded image and draws each as a dot sized by luminance. Pointillist / Warhol effect that breathes with time.',
    build: () => spaced(buildPhotoMosaic()),
  },
  {
    id: 'webcam-mosaic',
    name: 'Webcam Mosaic',
    category: 'Image',
    description: 'A live, mirrored webcam halftone. Flip the Toggle into the camera\'s `start` pin and grant permission; each cell becomes a yellow circle where the image is light or a blue square where it is dark (luminance threshold via logic/select).',
    build: () => spaced(buildWebcamMosaic()),
  },
  {
    id: 'body-tracking',
    name: 'Body Tracking',
    category: 'Image',
    description: 'Live MediaPipe skeleton from the webcam. One Toggle starts the camera + the input/landmarks tracker; cyan lines draw the connection edges and yellow dots the points. Switch hands / face / pose on the Landmarks node.',
    build: () => spaced(buildBodyTracking()),
  },

  // ── Pixel ──────────────────────────────────────────────
  {
    id: 'pixel-sort',
    name: 'Pixel Sort',
    category: 'Pixel',
    description: 'Classic glitch art: sorts maximal runs of pixels along each row by brightness, masked by a (lo, hi) range. Drag the sliders to shift the streaks.',
    build: () => spaced(buildPixelSort()),
  },

  // ── Sound ──────────────────────────────────────────────
  {
    id: 'tone-surface',
    name: 'Tone Surface',
    category: 'Sound',
    description: 'Move the mouse — X picks pitch, Y picks amplitude. A 20×20 grid lights up around the cursor.',
    build: () => spaced(buildToneSurface()),
  },
  {
    id: 'fft-bloom',
    name: 'FFT Bloom',
    category: 'Sound',
    description: 'An oscillator → FFT → polar plot. Each of 64 bins becomes a dot on a ring; mouse Y drives the source pitch.',
    build: () => spaced(buildFFTBloom()),
  },

  // ── Feedback (state-driven) ────────────────────────────
  {
    id: 'random-walk',
    name: 'Random Walk Trail',
    category: 'Feedback',
    description: 'A walker drifts via hashed (time, seed) angles, leaving a 200-point colored trail. Hue cycles along the trail; oldest points fade. Uses state/spread.',
    build: () => spaced(buildRandomWalkTrail()),
  },
  {
    id: 'paint-trail',
    name: 'Paint Trail',
    category: 'Feedback',
    description: 'Move the mouse to paint. image/from-layer + state/value accumulate each brush stroke on an offscreen canvas — infinite paint with a hue that cycles over time.',
    build: () => spaced(buildPaintTrail()),
  },

  // ── Data ───────────────────────────────────────────────
  {
    id: 'csv-bar-chart',
    name: 'CSV Bar Chart',
    category: 'Data',
    description: 'Inline CSV → number spread → 16 colored bars. Edit the `data` param on the CSVInline node to load a new series.',
    build: () => spaced(buildCSVBarChart()),
  },
  {
    id: 'local-peaks',
    name: 'Local Peaks',
    category: 'Data',
    description: 'Noise bars where each lights green only if it is taller than BOTH neighbours — local-maximum detection with spread/neighbor inside a ForEach.',
    build: () => buildLocalPeaks(),
  },
];

// ─── Built-in libraries (samples grouped by category) ─────────────────────
//
// Each `SAMPLE_CATEGORIES` entry becomes its own in-memory `PatchLibrary`
// flagged `builtIn: true` and grouped under the "Samples" folder. The
// LibraryMenu renders these alongside user-imported libraries; loading a
// patch from a built-in library reuses the regular `loadLibraryPatch`
// path because each sample's `build()` output is wrapped into a
// `PatchFile` via `serializePatch`.
//
// Built once per session and cached — `SAMPLES` is hardcoded so there's
// no reason to rebuild on every menu open.

let BUILT_IN_LIBRARIES_CACHE: PatchLibrary[] | null = null;

export function buildBuiltInSampleLibraries(): PatchLibrary[] {
  if (BUILT_IN_LIBRARIES_CACHE) return BUILT_IN_LIBRARIES_CACHE;

  const byCategory = new Map<string, LibraryEntry[]>();
  for (const cat of SAMPLE_CATEGORIES) byCategory.set(cat, []);

  for (const sample of SAMPLES) {
    const cat = sample.category && SAMPLE_CATEGORIES.includes(sample.category as typeof SAMPLE_CATEGORIES[number])
      ? sample.category
      : null;
    if (!cat) continue; // Skip uncategorised — none should exist after the trim.
    const { nodes, edges } = sample.build();
    const patchFile = serializePatch({ nodes, edges, name: sample.name });
    byCategory.get(cat)!.push({
      id: sample.id,
      name: sample.name,
      description: sample.description,
      patch: patchFile,
    });
  }

  const libs: PatchLibrary[] = [];
  for (const cat of SAMPLE_CATEGORIES) {
    const patches = byCategory.get(cat) ?? [];
    if (patches.length === 0) continue;
    libs.push({
      kind: 'floret.library',
      version: 1,
      name: cat,
      patches,
      builtIn: true,
      folder: 'Samples',
    });
  }

  BUILT_IN_LIBRARIES_CACHE = libs;
  return libs;
}
