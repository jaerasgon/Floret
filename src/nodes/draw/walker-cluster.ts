import { registerNode } from '../registry';
import type { Color, Layer, PortValue } from '../../runtime/types';
import type { SceneNode, ResolvedStyle } from '../../runtime/scene';
import { DEFAULT_STYLE, translateMat, scaleMat } from '../../runtime/scene';
import { toColor } from '../_shared/coerce';

/**
 * WalkerCluster — port of a generative random-walker sketch.
 *
 * Spawns N random walkers around an anchor point and runs each for
 * `steps` discrete jumps. Each step either extends a tapering line trail
 * (the previous step's position → the new one, with a thinner overlay
 * line in a second palette colour) or stamps two concentric circles. As
 * walkers age, their stroke weight and circle radius decay by 0.95 per
 * step — long-lived walkers fade to delicate filaments while the
 * short-lived ones lay down thick saturated blocks.
 *
 * The whole walker arena is a fixed internal `WORLD × WORLD` square,
 * scaled into the user's `size × size` cell at draw time and clipped
 * to that cell. That means tweaking `size` rescales the composition
 * uniformly — the walker drift, line weights, and overall density all
 * stay visually consistent across cell sizes.
 *
 * Use case: drop one inside a `region/foreach` over a `spread/grid` of
 * positions to tile a canvas with 36 unique cells (set `seed` from the
 * iteration index for per-cell variation).
 */

const WORLD = 250;
const COARSE_ANCHOR_STEP = 50;
const FINE_ANCHOR_STEP = 25;
const ATTEMPTS_PER_ANCHOR = 2;
const BASE_SPAWN_PROB = 0.25;
const INITIAL_WEIGHT_MAX = 100;
const DECAY = 0.95;
const WEIGHT_OVERLAY_THRESHOLD = 10;

/** Mulberry32 — tiny deterministic PRNG seeded by a uint32. */
function mulberry32(seed: number): () => number {
  let t = seed >>> 0;
  return () => {
    t = (t + 0x6d2b79f5) >>> 0;
    let r = Math.imul(t ^ (t >>> 15), t | 1);
    r ^= r + Math.imul(r ^ (r >>> 7), r | 61);
    return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
  };
}

/** Box-Muller transform — one standard-normal sample per call. */
function gaussian(rng: () => number): number {
  let u = 0;
  let v = 0;
  while (u === 0) u = rng();
  while (v === 0) v = rng();
  return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
}

function clamp01(v: number): number {
  return v < 0 ? 0 : v > 1 ? 1 : v;
}

function pickJitteredColor(rng: () => number, palette: ReadonlyArray<Color>): Color {
  if (palette.length === 0) return { r: 1, g: 1, b: 1, a: 1 };
  const base = palette[Math.floor(rng() * palette.length)]!;
  // Tiny RGB jitter — matches the original's gaussian * scale=1 on HSB
  // channels, just remapped to RGB. Visually the per-step variation is
  // subtle but breaks up flat colour blocks at the dense centres.
  const j = 0.012;
  return {
    r: clamp01(base.r + gaussian(rng) * j),
    g: clamp01(base.g + gaussian(rng) * j),
    b: clamp01(base.b + gaussian(rng) * j),
    a: 1,
  };
}

function colorToCss(c: Color): string {
  const r = Math.round(c.r * 255);
  const g = Math.round(c.g * 255);
  const b = Math.round(c.b * 255);
  return `rgb(${r},${g},${b})`;
}

function num(spread: PortValue | undefined, dflt: number): number {
  const v = spread?.[0];
  return typeof v === 'number' ? v : dflt;
}

/** Default palette from the source sketch — used when nothing is wired
 *  into the `colors` multi pin. Strong saturated stops in 7 hue families
 *  with light/mid/dark variants. */
const DEFAULT_PALETTE: ReadonlyArray<Color> = [
  { r: 0.949, g: 0.922, b: 0.541, a: 1 }, // #f2eb8a — pale yellow
  { r: 0.996, g: 0.816, b: 0.000, a: 1 }, // #fed000 — saturated yellow
  { r: 0.988, g: 0.518, b: 0.020, a: 1 }, // #fc8405 — orange
  { r: 0.929, g: 0.212, b: 0.102, a: 1 }, // #ed361a — red
  { r: 0.886, g: 0.941, b: 0.953, a: 1 }, // #e2f0f3 — pale blue
  { r: 0.702, g: 0.863, b: 0.878, a: 1 }, // #b3dce0 — mid blue
  { r: 0.267, g: 0.392, b: 0.631, a: 1 }, // #4464a1 — deep blue
  { r: 0.125, g: 0.188, b: 0.318, a: 1 }, // #203051 — navy
  { r: 1.000, g: 0.773, b: 0.780, a: 1 }, // #ffc5c7 — pale pink
  { r: 0.953, g: 0.596, b: 0.765, a: 1 }, // #f398c3 — pink
  { r: 0.812, g: 0.220, b: 0.584, a: 1 }, // #cf3895 — magenta
  { r: 0.427, g: 0.208, b: 0.541, a: 1 }, // #6d358a — purple
  { r: 0.024, g: 0.706, b: 0.690, a: 1 }, // #06b4b0 — teal
  { r: 0.294, g: 0.541, b: 0.373, a: 1 }, // #4b8a5f — green
];

interface DrawCmd {
  /** 0 = line (ox,oy → x,y); 1 = circle pair centred at (x, y). */
  kind: 0 | 1;
  ox: number;
  oy: number;
  x: number;
  y: number;
  /** For lines: stroke weight. For circles: outer diameter. */
  size: number;
  primary: string;
  secondary: string;
}

interface Walker {
  x: number;
  y: number;
  step: number;
  w: number;
  rad: number;
  shape: 0 | 1;
  c1: string;
  c2: string;
}

export const walkerClusterNode = registerNode({
  type: 'draw/walker-cluster',
  category: 'Draw',
  label: 'WalkerCluster',
  description: 'Spawn a cluster of random walkers that paint trails and circles.',
  inputs: [
    // Spread input — wire ONE color spread into it (e.g. from a future
    // palette builder, or a single-color wire which auto-wraps to a
    // length-1 spread). Falls back to the built-in palette when unwired.
    { name: 'colors', type: 'color', kind: 'spread' },
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
    { name: 'size', type: 'number', default: 140 },
    { name: 'density', type: 'number', default: 1 },
    { name: 'steps', type: 'number', default: 200, integer: true },
    { name: 'seed', type: 'number', default: 0, integer: true },
  ],
  outputs: [{ name: 'out', type: 'layer' }],
  computeRaw: (inputs): PortValue[] => {
    const rawColors = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const palette: ReadonlyArray<Color> = rawColors.length > 0
      ? rawColors.map((c) => toColor(c))
      : DEFAULT_PALETTE;
    const x = num(inputs[1], 0);
    const y = num(inputs[2], 0);
    const size = Math.max(1, num(inputs[3], 140));
    const density = Math.max(0, Math.min(2, num(inputs[4], 1)));
    const steps = Math.max(1, Math.floor(num(inputs[5], 200)));
    const seed = Math.floor(num(inputs[6], 0)) >>> 0;

    const rng = mulberry32(seed);
    const spawnProb = BASE_SPAWN_PROB * density;
    const renderScale = size / WORLD;

    // Spawn walkers across two anchor grids — coarse with tiny steps,
    // fine with one of three larger step sizes. Matches the original.
    const walkers: Walker[] = [];
    const spawn = (ax: number, ay: number, step: number) => {
      walkers.push({
        x: ax,
        y: ay,
        step,
        w: rng() * INITIAL_WEIGHT_MAX,
        rad: step / 3,
        shape: rng() < 0.5 ? 0 : 1,
        c1: colorToCss(pickJitteredColor(rng, palette)),
        c2: colorToCss(pickJitteredColor(rng, palette)),
      });
    };

    for (let ax = 0; ax <= WORLD; ax += COARSE_ANCHOR_STEP) {
      for (let ay = 0; ay <= WORLD; ay += COARSE_ANCHOR_STEP) {
        for (let i = 0; i < ATTEMPTS_PER_ANCHOR; i++) {
          if (rng() < spawnProb) spawn(ax, ay, WORLD * 0.025);
        }
      }
    }
    for (let ax = 0; ax <= WORLD; ax += FINE_ANCHOR_STEP) {
      for (let ay = 0; ay <= WORLD; ay += FINE_ANCHOR_STEP) {
        for (let i = 0; i < ATTEMPTS_PER_ANCHOR; i++) {
          if (rng() < spawnProb) {
            const r = rng();
            const step = r < 1 / 3 ? WORLD * 0.05 : r < 2 / 3 ? WORLD * 0.1 : WORLD * 0.2;
            spawn(ax, ay, step);
          }
        }
      }
    }

    // Replay each walker for `steps` iterations, recording every draw
    // command. Each step picks one of {-1, 0, 1} for each axis. We
    // bake commands at compute time so the per-frame Layer call is a
    // straight playback loop — no PRNG calls inside the render path.
    const cmds: DrawCmd[] = [];
    for (const wlk of walkers) {
      for (let it = 0; it < steps; it++) {
        const dx = Math.floor(rng() * 3) - 1;
        const dy = Math.floor(rng() * 3) - 1;
        const ox = wlk.x;
        const oy = wlk.y;
        wlk.x += wlk.step * dx;
        wlk.y += wlk.step * dy;
        wlk.w *= DECAY;
        wlk.rad *= DECAY;
        cmds.push({
          kind: wlk.shape,
          ox,
          oy,
          x: wlk.x,
          y: wlk.y,
          size: wlk.shape === 0 ? wlk.w : wlk.rad,
          primary: wlk.c2,
          secondary: wlk.c1,
        });
      }
    }

    // Bake the recorded commands into SceneNodes. Each line becomes a
    // line node (plus a thinner overlay line when the weight is heavy);
    // each "circle" walker becomes a pair of nested ellipse nodes.
    // The whole batch lives inside a Group with translate+scale+clip
    // so the WORLD coordinate space maps cleanly into the user's cell.
    const children: SceneNode[] = [];
    for (const c of cmds) {
      if (c.kind === 0) {
        const primaryStyle: ResolvedStyle = {
          ...DEFAULT_STYLE,
          fill: null,
          stroke: c.primary,
          strokeWeight: c.size,
        };
        children.push({ kind: 'line', x1: c.ox, y1: c.oy, x2: c.x, y2: c.y, style: primaryStyle });
        if (c.size > WEIGHT_OVERLAY_THRESHOLD) {
          const overlay: ResolvedStyle = {
            ...DEFAULT_STYLE,
            fill: null,
            stroke: c.secondary,
            strokeWeight: c.size / 2,
          };
          children.push({ kind: 'line', x1: c.ox, y1: c.oy, x2: c.x, y2: c.y, style: overlay });
        }
      } else {
        children.push({
          kind: 'ellipse',
          x: c.x, y: c.y, w: c.size, h: c.size,
          mode: 'center',
          style: { ...DEFAULT_STYLE, fill: c.primary, stroke: null },
        });
        children.push({
          kind: 'ellipse',
          x: c.x, y: c.y, w: c.size / 2, h: c.size / 2,
          mode: 'center',
          style: { ...DEFAULT_STYLE, fill: c.secondary, stroke: null },
        });
      }
    }
    // Scale + clip group: clip first so children outside WORLD are
    // discarded; the translate/scale come from the wrapping group's
    // transform.
    const scaled: SceneNode = {
      kind: 'group',
      children,
      clip: { x: 0, y: 0, w: WORLD, h: WORLD },
    };
    // Compose translate then scale. We use a Group nesting (outer
    // translate, inner scale) since SceneNode `transform` is a single
    // affine per group — easy to read, matches the original push/pop
    // ordering of the p5 version.
    const node: Layer = {
      kind: 'group',
      children: [
        {
          kind: 'group',
          children: [scaled],
          transform: scaleMat(renderScale, renderScale),
        },
      ],
      transform: translateMat(x - size / 2, y - size / 2),
    };
    return [[node]];
  },
});
