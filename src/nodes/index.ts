/**
 * Barrel: importing this triggers registration of every built-in NodeDef
 * via the side effects of each module's `registerNode(...)` call.
 *
 * Keep imports sorted by category for easy scanning.
 */

// Value
import './value/number';
import './value/string';
import './value/string-ops';

// Time
import './time/time';

// Math
import './math/binops';
import './math/unary';
import './math/map';
import './math/aggregate';

// Motion (easing / spring / smooth / trigger)
import './motion/motion';

// Logic
import './logic/compare';
import './logic/bool';
import './logic/select';

// Random
import './random/noise';
import './random/random';
import './random/wanderer';

// Vec2
import './vec2/vec2';

// Color
import './color/color';
import './color/palette';

// Convert
import './convert/convert';

// Spread ops
import './spread-ops/generators';
import './spread-ops/vec2-generators';
import './spread-ops/ops';
import './spread-ops/slice-mutation';
import './spread-ops/combine';
import './spread-ops/filter';
import './spread-ops/reorder';
import './spread-ops/partition';
import './spread-ops/gol-step';
import './spread-ops/walk-step';
import './spread-ops/elementary-ca';

// Data
import './data/csv';

// State (cross-frame persistence)
import './state/state';

// Transform
import './transform/translate';
import './transform/affine';
import './transform/blend';
import './transform/group';
import './transform/cache';

// Draw
import './draw/rect';
import './draw/text';
import './draw/shapes';
import './draw/image';
import './draw/image-region';
import './draw/image-tile';
import './draw/walker-cluster';

// Image (manipulation — read dims/pixels, derive new images)
import './image/dims';
import './image/sample';
import './image/sample-spread';
import './image/crop';
import './image/resize';
import './image/filter';
import './image/blend';
import './image/from-layer';
import './image/slicer';
import './image/pixelate';
import './image/convolve';
import './image/adjust';

// Pixel (per-pixel / per-row / per-col access; specialised pixel-sort)
import './pixel/pixels';
import './pixel/row-col';
import './pixel/sort';

// Style
import './style/fill';
import './style/modes';
import './style/text-params';

// Render
import './render/renderer';

// Input
import './input/mouse';
import './input/keyboard';
import './input/midi-cc';
import './input/midi-note';
import './input/camera';

// Meta
import './meta/comment';
import './meta/subpatch';

// IO Boxes
import './io/slider';
import './io/toggle';
import './io/color-picker';
import './io/xy-pad';
import './io/image-url';
import './io/file';
import './io/svg';
import './io/save-svg';

// Regions
import './region/foreach';
import './region/splicers';
import './region/accumulators';
import './region/if-cache';

// Sound — every node uses the same `audio` port type + audioRegistry
// lifecycle established in Stage 0. Categories of sound nodes:
//   - Sinks:      output
//   - Sources:    oscillator, pulse, noise, sound-file, audio-in
//   - Processors: gain, filter, delay, reverb, distortion, compressor, envelope
//   - Synths:     mono-synth
//   - Analysis:   amplitude, fft, peak-detect (audio → number/spread/boolean taps)
//   - Utilities:  midi-to-freq, freq-to-midi (pure scalar)
import './sound/output';
import './sound/oscillator';
import './sound/pulse';
import './sound/noise';
import './sound/sound-file';
import './sound/audio-in';
import './sound/gain';
import './sound/filter';
import './sound/delay';
import './sound/reverb';
import './sound/distortion';
import './sound/compressor';
import './sound/envelope';
import './sound/mono-synth';
import './sound/amplitude';
import './sound/fft';
import './sound/peak-detect';
import './sound/midi';

// Particles (Phase 1 + 2) — see CONCEPTS.md "Particle architecture"
import './physics/world';
import './particles/emitter';
import './particles/from-layer';
import './particles/render-as';
import './particles/trail';
import './particles/bounds';
import './force/gravity';
import './force/attractor';
import './force/repulsor';
import './force/drag';
import './force/turbulence';
import './force/wind';
import './force/vortex';
import './collider/circle';
import './collider/rect';
import './collider/from-layer';

export { nodeRegistry, registerNode } from './registry';
export type { NodeDef, PortSpec, NodeParamContext } from './types';
