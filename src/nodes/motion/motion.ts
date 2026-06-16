import { registerNode } from '../registry';
import type { NodeParamContext, PortValue } from '../types';
import { frameCtxStore } from '../../runtime/frame-ctx';
import { applyEase, expSmooth, integrateSpring, EASING_NAMES } from './_shared';

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
  description: 'Exponentially ease toward `value`. Higher `rate` = snappier. Frame-rate independent.',
  inputs: [
    { name: 'value', type: 'number', default: 0 },
    { name: 'rate', type: 'number', default: 8 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  effectful: true,
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const bag = params as unknown as { __smooth?: { current: number } };
    const target = Number(inputs[0]?.[0] ?? 0);
    const rate = Number(inputs[1]?.[0] ?? 8);
    const dt = frameCtxStore.snapshot().dt;
    if (!bag.__smooth || !Number.isFinite(bag.__smooth.current)) {
      bag.__smooth = { current: target };
      return [[target]];
    }
    const next = expSmooth(bag.__smooth.current, target, dt, rate);
    bag.__smooth.current = next;
    return [[next]];
  },
});

export const springNode = registerNode({
  type: 'motion/spring',
  category: 'Motion',
  label: 'Spring',
  description: 'Spring-follow `value` with stiffness + damping — overshoots and settles.',
  inputs: [
    { name: 'value', type: 'number', default: 0 },
    { name: 'stiffness', type: 'number', default: 120 },
    { name: 'damping', type: 'number', default: 14 },
  ],
  outputs: [{ name: 'result', type: 'number' }],
  effectful: true,
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const bag = params as unknown as { __spring?: { value: number; vel: number } };
    const target = Number(inputs[0]?.[0] ?? 0);
    const k = Number(inputs[1]?.[0] ?? 120);
    const damp = Number(inputs[2]?.[0] ?? 14);
    const dt = frameCtxStore.snapshot().dt;
    if (!bag.__spring || !Number.isFinite(bag.__spring.value)) {
      bag.__spring = { value: target, vel: 0 };
      return [[target]];
    }
    const next = integrateSpring(bag.__spring.value, target, bag.__spring.vel, dt, k, damp);
    bag.__spring = next;
    return [[next.value]];
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
