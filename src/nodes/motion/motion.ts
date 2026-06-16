import { registerNode } from '../registry';
import type { NodeParamContext, PortSpec, PortValue } from '../types';
import { frameCtxStore } from '../../runtime/frame-ctx';
import { applyEase, expSmooth, integrateSpring, EASING_NAMES } from './_shared';
import {
  adaptiveOutputs, portTypeForValueType, kindForValueType, identityValue,
  kindOf, componentsOf, fromComponents,
} from '../math/_adaptive';

/** dynamicInputs for a motion node: one adaptive `value` operand (follows
 *  valueType) followed by fixed number control inputs. */
function motionInputs(controls: ReadonlyArray<{ name: string; default: number }>) {
  return (params: Record<string, unknown>): ReadonlyArray<PortSpec> => {
    const kind = kindForValueType(params.valueType);
    return [
      { name: 'value', type: portTypeForValueType(params.valueType), default: identityValue(kind, 0) },
      ...controls.map((c) => ({ name: c.name, type: 'number' as const, default: c.default })),
    ];
  };
}

/**
 * Motion category — easing, springs, smoothing, and edge triggers. The
 * stateful ones (`smooth`, `spring`, `trigger`) are `effectful` so the runtime
 * never memoises them, and they read the per-frame `dt` from the ambient
 * frame store (populated at the top of `tick`) since `computeRaw` doesn't
 * receive `frame`. Per-node state is stashed on the persistent param context
 * (same pattern as particles/trail), so it survives across frames and clears
 * on recompile.
 */

export const smoothNode = registerNode({
  type: 'motion/smooth',
  category: 'Motion',
  label: 'Smooth',
  description: 'Exponentially ease toward `value` (number, vec2, or color). Higher `rate` = snappier; frame-rate independent.',
  inputs: [
    { name: 'value', type: 'number', default: 0 },
    { name: 'rate', type: 'number', default: 8 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  defaultParams: { valueType: 'any' },
  adaptiveType: true,
  dynamicInputs: motionInputs([{ name: 'rate', default: 8 }]),
  dynamicOutputs: adaptiveOutputs,
  effectful: true,
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const bag = params as unknown as { __smooth?: { current: number[] } };
    const target = inputs[0]?.[0];
    const rate = Number(inputs[1]?.[0] ?? 8);
    const dt = frameCtxStore.snapshot().dt;
    const kind = kindOf(target) ?? 'number';
    const tc = componentsOf(target, kind);
    if (!bag.__smooth || bag.__smooth.current.length !== tc.length) {
      bag.__smooth = { current: tc.slice() };
      return [[fromComponents(tc, kind)]];
    }
    const next = bag.__smooth.current.map((c, i) => expSmooth(c, tc[i] ?? 0, dt, rate));
    bag.__smooth.current = next;
    return [[fromComponents(next, kind)]];
  },
});

export const springNode = registerNode({
  type: 'motion/spring',
  category: 'Motion',
  label: 'Spring',
  description: 'Spring-follow `value` (number, vec2, or color) with stiffness + damping — overshoots and settles.',
  inputs: [
    { name: 'value', type: 'number', default: 0 },
    { name: 'stiffness', type: 'number', default: 120 },
    { name: 'damping', type: 'number', default: 14 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  defaultParams: { valueType: 'any' },
  adaptiveType: true,
  dynamicInputs: motionInputs([{ name: 'stiffness', default: 120 }, { name: 'damping', default: 14 }]),
  dynamicOutputs: adaptiveOutputs,
  effectful: true,
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const bag = params as unknown as { __spring?: { value: number[]; vel: number[] } };
    const target = inputs[0]?.[0];
    const k = Number(inputs[1]?.[0] ?? 120);
    const damp = Number(inputs[2]?.[0] ?? 14);
    const dt = frameCtxStore.snapshot().dt;
    const kind = kindOf(target) ?? 'number';
    const tc = componentsOf(target, kind);
    if (!bag.__spring || bag.__spring.value.length !== tc.length) {
      bag.__spring = { value: tc.slice(), vel: new Array(tc.length).fill(0) };
      return [[fromComponents(tc, kind)]];
    }
    const value = new Array<number>(tc.length);
    const vel = new Array<number>(tc.length);
    for (let i = 0; i < tc.length; i++) {
      const step = integrateSpring(bag.__spring.value[i]!, tc[i] ?? 0, bag.__spring.vel[i]!, dt, k, damp);
      value[i] = step.value;
      vel[i] = step.vel;
    }
    bag.__spring = { value, vel };
    return [[fromComponents(value, kind)]];
  },
});

export const delayLineNode = registerNode({
  type: 'motion/delay-line',
  category: 'Motion',
  label: 'DelayLine',
  description: 'Echo `value` as it was `delay` seconds ago (linearly interpolated). The history clears on recompile.',
  inputs: [
    { name: 'value', type: 'number', default: 0 },
    { name: 'delay', type: 'number', default: 0.5 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  effectful: true,
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const bag = params as unknown as { __delay?: { hist: Array<{ t: number; v: number }> } };
    const value = Number(inputs[0]?.[0] ?? 0);
    const delay = Math.max(0, Number(inputs[1]?.[0] ?? 0.5));
    const t = frameCtxStore.snapshot().t;
    if (!bag.__delay) bag.__delay = { hist: [] };
    const hist = bag.__delay.hist;
    hist.push({ t, v: value });
    // Drop samples older than we could ever need, but keep the one bracketing
    // sample just before the cutoff so interpolation stays continuous. Hard
    // cap guards against unbounded growth when delay is large.
    const cutoff = t - delay - 0.001;
    while (hist.length > 2 && hist[1]!.t < cutoff) hist.shift();
    if (hist.length > 4096) hist.shift();

    const target = t - delay;
    if (target <= hist[0]!.t) return [[hist[0]!.v]];
    for (let i = hist.length - 1; i >= 0; i--) {
      if (hist[i]!.t <= target) {
        const a = hist[i]!;
        const b = hist[i + 1] ?? a;
        const span = b.t - a.t;
        const f = span > 0 ? (target - a.t) / span : 0;
        return [[a.v + (b.v - a.v) * f]];
      }
    }
    return [[hist[0]!.v]];
  },
});

export const easeNode = registerNode({
  type: 'motion/ease',
  category: 'Motion',
  label: 'Ease',
  description: 'Remap t ∈ [0,1] through an easing curve. Pick the curve from the dropdown.',
  inputs: [{ name: 't', type: 'number', default: 0 }],
  outputs: [{ name: 'result', type: 'number' }],
  defaultParams: { curve: 'inOutQuad' },
  paramOptions: { curve: EASING_NAMES },
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const t = Number(inputs[0]?.[0] ?? 0);
    const curve = params.param<string>('curve', 'inOutQuad');
    return [[applyEase(curve, t)]];
  },
});

export const triggerNode = registerNode({
  type: 'motion/trigger',
  category: 'Motion',
  label: 'Trigger',
  description: 'Emit a single-frame `true` pulse: on any change, a rising edge, or crossing `threshold` upward.',
  inputs: [
    { name: 'value', type: 'number', default: 0 },
    { name: 'threshold', type: 'number', default: 0.5 },
  ],
  outputs: [{ name: 'pulse', type: 'boolean' }],
  defaultParams: { mode: 'rising' },
  paramOptions: { mode: ['change', 'rising', 'threshold'] },
  effectful: true,
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const bag = params as unknown as { __trigger?: { prev: number } };
    const value = Number(inputs[0]?.[0] ?? 0);
    const threshold = Number(inputs[1]?.[0] ?? 0.5);
    const mode = params.param<string>('mode', 'rising');
    if (!bag.__trigger) {
      bag.__trigger = { prev: value };
      return [[false]];
    }
    const prev = bag.__trigger.prev;
    let fire = false;
    if (mode === 'change') fire = value !== prev;
    else if (mode === 'threshold') fire = prev < threshold && value >= threshold;
    else fire = prev < 0.5 && value >= 0.5; // rising edge (boolean-ish)
    bag.__trigger.prev = value;
    return [[fire]];
  },
});
