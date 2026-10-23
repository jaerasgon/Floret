import { registerNode } from '../registry';
import { frameCtxStore } from '../../runtime/frame-ctx';
import { consumeBang } from './bang-bus';
import { BangNode, ButtonNode, StepperNode, SelectNode } from '../../editor/nodes/IoControls';
import type { PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

// ─── Interactive widgets ──────────────────────────────────────────────────

/** Bang — momentary one-shot trigger (true for a single tick). */
export const bangNode = registerNode({
  type: 'io/bang',
  category: 'IO',
  compact: true,
  label: 'Bang',
  description: 'Momentary trigger — click to emit `true` for one tick, then `false`. Wire into a rising-edge input (e.g. a send/reset).',
  inputs: [],
  outputs: [{ name: 'bang', type: 'boolean' }],
  effectful: true,
  component: BangNode,
  computeRaw: (_inputs, params: NodeParamContext): PortValue[] => [[consumeBang(params.nodeId)]],
});

/** Button — momentary: true while held. */
export const buttonNode = registerNode({
  type: 'io/button',
  category: 'IO',
  compact: true,
  label: 'Button',
  description: 'Momentary boolean — `true` while pressed, `false` on release (vs Toggle, which latches).',
  inputs: [],
  outputs: [{ name: 'bool', type: 'boolean' }],
  defaultParams: { value: false },
  readFromContext: (_frame, params) => [[params.param<boolean>('value', false)]],
  component: ButtonNode,
});

/** Stepper — integer with +/− buttons. */
export const stepperNode = registerNode({
  type: 'io/stepper',
  category: 'IO',
  compact: true,
  label: 'Stepper',
  description: 'An integer you nudge with +/− buttons (clamped to min/max, by step).',
  inputs: [],
  outputs: [{ name: 'number', type: 'number' }],
  defaultParams: { value: 0, min: 0, max: 10, step: 1 },
  readFromContext: (_frame, params) => [[params.param<number>('value', 0)]],
  component: StepperNode,
});

/** Select — pick one of N comma-separated options. */
export const selectNode = registerNode({
  type: 'io/select',
  category: 'IO',
  compact: true,
  label: 'Select',
  description: 'Choose one of a list of options (comma-separated) — outputs the selected string.',
  inputs: [],
  outputs: [{ name: 'value', type: 'string' }],
  defaultParams: { options: 'a,b,c', value: 'a' },
  readFromContext: (_frame, params) => [[params.param<string>('value', '')]],
  component: SelectNode,
});

// ─── Timing / flow (logic — default node UI) ───────────────────────────────

interface MetroState { lastFire: number; inited: boolean; }
const metroState = new Map<string, MetroState>();

/** Metro — emit a bang every `interval` seconds while `run` is true. */
export const metroNode = registerNode({
  type: 'io/metro',
  category: 'IO',
  compact: true,
  label: 'Metro',
  description: 'Emit a one-tick bang every `interval` seconds while `run` is on. Drives periodic actions (pairs with Counter).',
  effectful: true,
  inputs: [
    { name: 'interval', type: 'number', default: 1, param: 'interval' },
    { name: 'run', type: 'boolean', default: true },
  ],
  outputs: [{ name: 'bang', type: 'boolean' }],
  defaultParams: { interval: 1 },
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const id = params.nodeId;
    const t = frameCtxStore.snapshot().t;
    const interval = Math.max(0.01, Number(params.param<number>('interval', 1)) || 1);
    const run = ((inputs[1] ?? [])[0] ?? true) !== false;
    let st = metroState.get(id);
    if (!st) { st = { lastFire: t, inited: true }; metroState.set(id, st); return [[false]]; }
    if (!run) { st.lastFire = t; return [[false]]; }
    if (t - st.lastFire >= interval) { st.lastFire = t; return [[true]]; }
    return [[false]];
  },
});

interface CounterState { count: number; prevInc: boolean; prevReset: boolean; inited: boolean; }
const counterState = new Map<string, CounterState>();

/** Counter — count bangs on `inc`, zero on `reset`. */
export const counterNode = registerNode({
  type: 'io/counter',
  category: 'IO',
  compact: true,
  label: 'Counter',
  description: 'Add `step` to a running count on each rising edge of `inc`; `reset` returns it to `start`.',
  effectful: true,
  inputs: [
    { name: 'inc', type: 'boolean', default: false },
    { name: 'reset', type: 'boolean', default: false },
    { name: 'step', type: 'number', default: 1, param: 'step' },
    { name: 'start', type: 'number', default: 0, param: 'start' },
  ],
  outputs: [{ name: 'count', type: 'number' }],
  defaultParams: { step: 1, start: 0 },
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const id = params.nodeId;
    const inc = !!(inputs[0] ?? [])[0];
    const reset = !!(inputs[1] ?? [])[0];
    const step = Number(params.param<number>('step', 1)) || 1;
    const start = Number(params.param<number>('start', 0)) || 0;
    let st = counterState.get(id);
    if (!st) { st = { count: start, prevInc: false, prevReset: false, inited: true }; counterState.set(id, st); }
    if (reset && !st.prevReset) st.count = start;
    if (inc && !st.prevInc) st.count += step;
    st.prevInc = inc; st.prevReset = reset;
    return [[st.count]];
  },
});

const edgeState = new Map<string, { prev: number; inited: boolean }>();

/** Edge — bang when the input crosses a threshold (rising/falling) or changes. */
export const edgeNode = registerNode({
  type: 'io/edge',
  category: 'IO',
  compact: true,
  label: 'Edge',
  description: 'Emit a one-tick bang when `in` rises past / falls below `threshold`, or whenever it changes. Turns a continuous signal into a trigger.',
  effectful: true,
  inputs: [
    { name: 'in', type: 'number', default: 0 },
    { name: 'mode', type: 'string', default: 'rising', param: 'mode' },
    { name: 'threshold', type: 'number', default: 0.5, param: 'threshold' },
  ],
  outputs: [{ name: 'bang', type: 'boolean' }],
  defaultParams: { mode: 'rising', threshold: 0.5 },
  paramOptions: { mode: ['rising', 'falling', 'change'] },
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const id = params.nodeId;
    const v = Number((inputs[0] ?? [])[0] ?? 0) || 0;
    const mode = params.param<string>('mode', 'rising');
    const th = Number(params.param<number>('threshold', 0.5));
    let st = edgeState.get(id);
    if (!st) { st = { prev: v, inited: true }; edgeState.set(id, st); return [[false]]; }
    let fire = false;
    if (mode === 'rising') fire = st.prev <= th && v > th;
    else if (mode === 'falling') fire = st.prev > th && v <= th;
    else fire = v !== st.prev; // change
    st.prev = v;
    return [[fire]];
  },
});

const gateState = new Map<string, PortValue>();

/** Gate — pass `value` through while `open`; otherwise hold the last value. */
export const gateNode = registerNode({
  type: 'io/gate',
  category: 'IO',
  compact: true,
  label: 'Gate',
  description: 'Pass `value` through while `open` is true; when closed, hold the last value that passed. Flow control / sample-and-hold.',
  effectful: true,
  inputs: [
    { name: 'value', type: 'any' },
    { name: 'open', type: 'boolean', default: true },
  ],
  outputs: [{ name: 'out', type: 'any' }],
  dynamicOutputs: (_params, up) => [{ name: 'out', type: up.value ?? 'any' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const id = params.nodeId;
    const open = ((inputs[1] ?? [])[0] ?? true) !== false;
    const value = (inputs[0] ?? []) as PortValue;
    if (open) { gateState.set(id, value); return [value]; }
    return [gateState.get(id) ?? []];
  },
});
