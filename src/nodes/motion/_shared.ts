/**
 * Shared math for the Motion node category — pure, framework-free, and
 * unit-tested. Easing curves map t ∈ [0,1] → eased value (elastic/back
 * overshoot beyond [0,1] by design). The integrators advance a value toward
 * a target given a frame `dt`.
 */

export function clamp01(t: number): number {
  return t < 0 ? 0 : t > 1 ? 1 : t;
}

export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

// ── Easing curves ──────────────────────────────────────────────────────────
// Named so the Ease node can offer them via a param dropdown. Each is a pure
// (t) => v function; callers clamp t first.
const c4 = (2 * Math.PI) / 3;
const c5 = (2 * Math.PI) / 4.5;

export const EASING: Record<string, (t: number) => number> = {
  linear: (t) => t,
  inQuad: (t) => t * t,
  outQuad: (t) => 1 - (1 - t) * (1 - t),
  inOutQuad: (t) => (t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2),
  inCubic: (t) => t * t * t,
  outCubic: (t) => 1 - Math.pow(1 - t, 3),
  inOutCubic: (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2),
  inElastic: (t) =>
    t === 0 ? 0 : t === 1 ? 1 : -Math.pow(2, 10 * t - 10) * Math.sin((t * 10 - 10.75) * c4),
  outElastic: (t) =>
    t === 0 ? 0 : t === 1 ? 1 : Math.pow(2, -10 * t) * Math.sin((t * 10 - 0.75) * c4) + 1,
  inOutElastic: (t) =>
    t === 0
      ? 0
      : t === 1
        ? 1
        : t < 0.5
          ? -(Math.pow(2, 20 * t - 10) * Math.sin((20 * t - 11.125) * c5)) / 2
          : (Math.pow(2, -20 * t + 10) * Math.sin((20 * t - 11.125) * c5)) / 2 + 1,
};

/** Curve names in display order — used for the Ease node's param dropdown. */
export const EASING_NAMES = Object.keys(EASING) as ReadonlyArray<string>;

export function applyEase(name: string, t: number): number {
  return (EASING[name] ?? EASING.linear)!(clamp01(t));
}

// ── Integrators ──────────────────────────────────────────────────────────

/**
 * Exponential smoothing toward `target`. `rate` is the responsiveness
 * (higher = snappier); the step is frame-rate independent via `dt`. Returns
 * `current` unchanged when dt ≤ 0 (paused / first frame).
 */
export function expSmooth(current: number, target: number, dt: number, rate: number): number {
  if (!(dt > 0) || !(rate > 0)) return current;
  const k = 1 - Math.exp(-rate * dt);
  const next = current + (target - current) * k;
  return Number.isFinite(next) ? next : target;
}

/**
 * Semi-implicit Euler spring step toward `target`. `k` is stiffness, `damp`
 * the damping coefficient. Sub-steps when dt is large so stiff springs stay
 * stable. Returns the new {value, vel}.
 */
export function integrateSpring(
  value: number,
  target: number,
  vel: number,
  dt: number,
  k: number,
  damp: number,
): { value: number; vel: number } {
  if (!(dt > 0)) return { value, vel };
  // Cap the step and sub-step so high stiffness doesn't blow up.
  const MAX_STEP = 1 / 120;
  let remaining = Math.min(dt, 0.1);
  let v = value;
  let vv = vel;
  while (remaining > 0) {
    const h = Math.min(MAX_STEP, remaining);
    const accel = k * (target - v) - damp * vv;
    vv += accel * h;
    v += vv * h;
    remaining -= h;
  }
  if (!Number.isFinite(v) || !Number.isFinite(vv)) return { value: target, vel: 0 };
  return { value: v, vel: vv };
}
