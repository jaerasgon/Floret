import { registerNode } from '../registry';
import { toVec2, ZERO_VEC2 as ZERO } from '../_shared/coerce';

/**
 * Vec2 nodes — construct, decompose, and operate on 2D vectors.
 * Default for any unconnected `vec2` input is the origin (0, 0).
 */

// ─── Construct / decompose ─────────────────────────────────────────────────

export const makeNode = registerNode({
  type: 'vec2/make',
  category: 'Vec2',
  label: 'Vec2',
  description: 'Build a vec2 from (x, y).',
  inputs: [
    { name: 'x', type: 'number', default: 0 },
    { name: 'y', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'vector', type: 'vec2' }],
  compute: (x, y) => [{ x: x as number, y: y as number }],
});

export const splitNode = registerNode({
  type: 'vec2/split',
  category: 'Vec2',
  label: 'Split',
  description: 'Decompose a vec2 into x and y.',
  inputs: [{ name: 'v', type: 'vec2', default: ZERO }],
  outputs: [
    { name: 'x', type: 'number' },
    { name: 'y', type: 'number' },
  ],
  compute: (v) => {
    const p = toVec2(v);
    return [p.x, p.y];
  },
});

export const fromAngleNode = registerNode({
  type: 'vec2/from-angle',
  category: 'Vec2',
  label: 'FromAngle',
  description: 'Polar → cartesian: unit vector at `angle` radians, scaled by `mag` (radius).',
  inputs: [
    { name: 'angle', type: 'number', default: 0 },
    { name: 'mag', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'vector', type: 'vec2' }],
  compute: (angle, mag) => {
    const a = angle as number;
    const m = mag as number;
    return [{ x: Math.cos(a) * m, y: Math.sin(a) * m }];
  },
});

export const toPolarNode = registerNode({
  type: 'vec2/to-polar',
  category: 'Vec2',
  label: 'ToPolar',
  description: 'Cartesian → polar: decompose a vec2 into `angle` (radians) and `radius`.',
  inputs: [{ name: 'v', type: 'vec2', default: ZERO }],
  outputs: [
    { name: 'angle', type: 'number' },
    { name: 'radius', type: 'number' },
  ],
  compute: (v) => {
    const p = toVec2(v);
    return [Math.atan2(p.y, p.x), Math.hypot(p.x, p.y)];
  },
});

// ─── Binary ops ────────────────────────────────────────────────────────────

const ab = [
  { name: 'a', type: 'vec2' as const, default: ZERO },
  { name: 'b', type: 'vec2' as const, default: ZERO },
];

export const addNode = registerNode({
  type: 'vec2/add',
  category: 'Vec2',
  label: 'Add',
  description: 'Add two vec2s component-wise.',
  // Deprecated: folded into the adaptive math/add (valueType=vec2). Hidden
  // from the palette; kept registered so pre-migration patches still load.
  hidden: true,
  inputs: ab,
  outputs: [{ name: 'vector', type: 'vec2' }],
  compute: (a, b) => {
    const p = toVec2(a);
    const q = toVec2(b);
    return [{ x: p.x + q.x, y: p.y + q.y }];
  },
});

export const subNode = registerNode({
  type: 'vec2/sub',
  category: 'Vec2',
  label: 'Sub',
  description: 'Subtract b from a component-wise.',
  hidden: true, // Deprecated: folded into adaptive math/sub (valueType=vec2).
  inputs: ab,
  outputs: [{ name: 'vector', type: 'vec2' }],
  compute: (a, b) => {
    const p = toVec2(a);
    const q = toVec2(b);
    return [{ x: p.x - q.x, y: p.y - q.y }];
  },
});

export const mulNode = registerNode({
  type: 'vec2/mul',
  category: 'Vec2',
  label: 'Mul',
  description: 'Component-wise multiply.',
  hidden: true, // Deprecated: folded into adaptive math/mul (valueType=vec2).
  inputs: ab,
  outputs: [{ name: 'vector', type: 'vec2' }],
  compute: (a, b) => {
    const p = toVec2(a);
    const q = toVec2(b);
    return [{ x: p.x * q.x, y: p.y * q.y }];
  },
});

export const scaleNode = registerNode({
  type: 'vec2/scale',
  category: 'Vec2',
  label: 'Scale',
  description: 'Multiply a vec2 by a scalar.',
  inputs: [
    { name: 'v', type: 'vec2', default: ZERO },
    { name: 's', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'scaled', type: 'vec2' }],
  compute: (v, s) => {
    const p = toVec2(v);
    const k = s as number;
    return [{ x: p.x * k, y: p.y * k }];
  },
});

export const lerpNode = registerNode({
  type: 'vec2/lerp',
  category: 'Vec2',
  label: 'Lerp',
  description: 'Linear interpolate between a and b at t ∈ [0, 1] (component-wise).',
  hidden: true, // Deprecated: folded into adaptive math/lerp (valueType=vec2).
  inputs: [
    { name: 'a', type: 'vec2', default: ZERO },
    { name: 'b', type: 'vec2', default: ZERO },
    { name: 't', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'vector', type: 'vec2' }],
  compute: (a, b, t) => {
    const p = toVec2(a);
    const q = toVec2(b);
    const tv = t as number;
    return [{ x: p.x + (q.x - p.x) * tv, y: p.y + (q.y - p.y) * tv }];
  },
});

// ─── Magnitude / distance / dot / angle ────────────────────────────────────

export const magNode = registerNode({
  type: 'vec2/mag',
  category: 'Vec2',
  label: 'Mag',
  description: 'Length of the vector.',
  inputs: [{ name: 'v', type: 'vec2', default: ZERO }],
  outputs: [{ name: 'length', type: 'number' }],
  compute: (v) => {
    const p = toVec2(v);
    return [Math.hypot(p.x, p.y)];
  },
});

export const distNode = registerNode({
  type: 'vec2/dist',
  category: 'Vec2',
  label: 'Dist',
  description: 'Distance between two points.',
  inputs: ab,
  outputs: [{ name: 'distance', type: 'number' }],
  compute: (a, b) => {
    const p = toVec2(a);
    const q = toVec2(b);
    return [Math.hypot(q.x - p.x, q.y - p.y)];
  },
});

export const dotNode = registerNode({
  type: 'vec2/dot',
  category: 'Vec2',
  label: 'Dot',
  description: 'Dot product of a and b.',
  inputs: ab,
  outputs: [{ name: 'dot', type: 'number' }],
  compute: (a, b) => {
    const p = toVec2(a);
    const q = toVec2(b);
    return [p.x * q.x + p.y * q.y];
  },
});

export const headingNode = registerNode({
  type: 'vec2/heading',
  category: 'Vec2',
  label: 'Heading',
  description: 'Angle of the vector in radians.',
  inputs: [{ name: 'v', type: 'vec2', default: ZERO }],
  outputs: [{ name: 'angle', type: 'number' }],
  compute: (v) => {
    const p = toVec2(v);
    return [Math.atan2(p.y, p.x)];
  },
});

export const angleBetweenNode = registerNode({
  type: 'vec2/angle-between',
  category: 'Vec2',
  label: 'AngleBetween',
  description: 'Signed angle from a to b, in radians.',
  inputs: ab,
  outputs: [{ name: 'angle', type: 'number' }],
  compute: (a, b) => {
    const p = toVec2(a);
    const q = toVec2(b);
    return [Math.atan2(q.y, q.x) - Math.atan2(p.y, p.x)];
  },
});

// ─── Direction-preserving transforms ───────────────────────────────────────

export const normalizeNode = registerNode({
  type: 'vec2/normalize',
  category: 'Vec2',
  label: 'Normalize',
  description: 'Unit vector in the same direction. Zero vectors stay zero.',
  inputs: [{ name: 'v', type: 'vec2', default: ZERO }],
  outputs: [{ name: 'normalized', type: 'vec2' }],
  compute: (v) => {
    const p = toVec2(v);
    const m = Math.hypot(p.x, p.y);
    if (m === 0) return [ZERO];
    return [{ x: p.x / m, y: p.y / m }];
  },
});

export const setMagNode = registerNode({
  type: 'vec2/set-mag',
  category: 'Vec2',
  label: 'SetMag',
  description: 'Rescale the vector to the given magnitude.',
  inputs: [
    { name: 'v', type: 'vec2', default: ZERO },
    { name: 'mag', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'vector', type: 'vec2' }],
  compute: (v, mag) => {
    const p = toVec2(v);
    const target = mag as number;
    const m = Math.hypot(p.x, p.y);
    if (m === 0) return [ZERO];
    const k = target / m;
    return [{ x: p.x * k, y: p.y * k }];
  },
});

export const limitNode = registerNode({
  type: 'vec2/limit',
  category: 'Vec2',
  label: 'Limit',
  description: 'Clamp the magnitude to at most `max`.',
  inputs: [
    { name: 'v', type: 'vec2', default: ZERO },
    { name: 'max', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'vector', type: 'vec2' }],
  compute: (v, max) => {
    const p = toVec2(v);
    const cap = max as number;
    const m = Math.hypot(p.x, p.y);
    if (m === 0 || m <= cap) return [p];
    const k = cap / m;
    return [{ x: p.x * k, y: p.y * k }];
  },
});

export const rotateNode = registerNode({
  type: 'vec2/rotate',
  category: 'Vec2',
  label: 'Rotate',
  description: 'Rotate the vector by `angle` radians around the origin.',
  inputs: [
    { name: 'v', type: 'vec2', default: ZERO },
    { name: 'angle', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'rotated', type: 'vec2' }],
  compute: (v, angle) => {
    const p = toVec2(v);
    const a = angle as number;
    const c = Math.cos(a);
    const s = Math.sin(a);
    return [{ x: p.x * c - p.y * s, y: p.x * s + p.y * c }];
  },
});

export const reflectNode = registerNode({
  type: 'vec2/reflect',
  category: 'Vec2',
  label: 'Reflect',
  description: 'Reflect `v` across the surface with `normal` (normal is normalised internally).',
  inputs: [
    { name: 'v', type: 'vec2', default: ZERO },
    { name: 'normal', type: 'vec2', default: { x: 0, y: 1 } },
  ],
  outputs: [{ name: 'reflected', type: 'vec2' }],
  compute: (v, normal) => {
    const i = toVec2(v);
    const n0 = toVec2(normal);
    const len = Math.hypot(n0.x, n0.y);
    if (len === 0) return [i];
    const n = { x: n0.x / len, y: n0.y / len };
    const d = i.x * n.x + i.y * n.y;
    return [{ x: i.x - 2 * d * n.x, y: i.y - 2 * d * n.y }];
  },
});

export const projectNode = registerNode({
  type: 'vec2/project',
  category: 'Vec2',
  label: 'Project',
  description: 'Vector projection of `a` onto `b`.',
  inputs: ab,
  outputs: [{ name: 'projected', type: 'vec2' }],
  compute: (a, b) => {
    const p = toVec2(a);
    const q = toVec2(b);
    const denom = q.x * q.x + q.y * q.y;
    if (denom === 0) return [ZERO];
    const k = (p.x * q.x + p.y * q.y) / denom;
    return [{ x: q.x * k, y: q.y * k }];
  },
});
