import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Delay — feedback echo with a one-pole lowpass on the recirculation
 * path. Built from primitive Web Audio nodes:
 *
 *   in ──┬──────────────────────────── out (wet+dry mixed at output)
 *        ↓
 *      delayLine ─→ lowpass ─→ feedbackGain ─┐
 *        ↑                                    │
 *        └────────────────────────────────────┘
 *
 * The audio handle returned points at the output GainNode.
 */

interface DelayChain {
  input: GainNode;     // accepts upstream
  delay: DelayNode;
  feedback: GainNode;
  damping: BiquadFilterNode;
  output: GainNode;    // mixed wet+dry, handed out as the AudioHandle
}

const chains = new Map<string, DelayChain>();

function buildChain(): DelayChain {
  const ctx = getAudioContext();
  const input = ctx.createGain();
  const delay = ctx.createDelay(2);  // max 2s
  const feedback = ctx.createGain();
  const damping = ctx.createBiquadFilter();
  damping.type = 'lowpass';
  const output = ctx.createGain();

  input.connect(output);                     // dry path
  input.connect(delay);                      // tap into delay
  delay.connect(damping);
  damping.connect(feedback);
  feedback.connect(delay);                   // recirculation loop
  damping.connect(output);                   // wet path

  return { input, delay, feedback, damping, output };
}

export const delayNode = registerNode({
  type: 'sound/delay',
  category: 'Sound',
  label: 'Delay',
  description: 'Echo an audio signal with feedback and lowpass damping.',
  inputs: [
    { name: 'in', type: 'audio' },
    { name: 'time', type: 'number', default: 0.25 },
    { name: 'feedback', type: 'number', default: 0.4 },
    { name: 'filter', type: 'number', default: 2300 },
  ],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const upstream = (inputs[0] ?? [])[0] as AudioHandle | null;
    const time = ((inputs[1] ?? [])[0] as number | undefined) ?? 0.25;
    const feedback = ((inputs[2] ?? [])[0] as number | undefined) ?? 0.4;
    const filter = ((inputs[3] ?? [])[0] as number | undefined) ?? 2300;

    const output = audioRegistry.ensure<GainNode>(
      params.nodeId, 'main', 'delay',
      () => {
        const chain = buildChain();
        chains.set(params.nodeId, chain);
        return chain.output;
      },
    );
    const chain = chains.get(params.nodeId)!;
    const t = getAudioContext().currentTime;
    chain.delay.delayTime.cancelScheduledValues(t);
    chain.delay.delayTime.linearRampToValueAtTime(
      Math.max(0, Math.min(2, time)), t + 0.02,
    );
    chain.feedback.gain.cancelScheduledValues(t);
    chain.feedback.gain.linearRampToValueAtTime(
      Math.max(0, Math.min(0.99, feedback)), t + 0.02,
    );
    chain.damping.frequency.cancelScheduledValues(t);
    chain.damping.frequency.linearRampToValueAtTime(
      Math.max(20, Math.min(20000, filter)), t + 0.02,
    );

    if (upstream && upstream.node) {
      audioRegistry.connect(upstream, chain.input);
    }

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: output, kind: 'delay',
    };
    return [[handle]];
  },
});
