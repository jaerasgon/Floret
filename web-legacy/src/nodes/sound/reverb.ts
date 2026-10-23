import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Reverb — convolution reverb via Web Audio's ConvolverNode + a
 * synthesised exponentially-decaying noise IR. Wet/dry mix is a
 * Gain split:
 *
 *   in ──┬─→ dryGain ──┐
 *        ↓             ↓
 *      convolver ─→ wetGain ─→ output
 *
 * Rebuilding the IR each frame would burn CPU; we cache by
 * (seconds, decay, reverse) and only re-synth when those change.
 */

interface ReverbChain {
  input: GainNode;
  dry: GainNode;
  wet: GainNode;
  convolver: ConvolverNode;
  output: GainNode;
  seconds: number;
  decay: number;
  reverse: boolean;
}

const chains = new Map<string, ReverbChain>();

function buildIR(ctx: BaseAudioContext, seconds: number, decay: number, reverse: boolean): AudioBuffer {
  const sampleRate = ctx.sampleRate;
  const len = Math.max(1, Math.floor(sampleRate * seconds));
  const buf = ctx.createBuffer(2, len, sampleRate);
  for (let ch = 0; ch < 2; ch++) {
    const data = buf.getChannelData(ch);
    for (let i = 0; i < len; i++) {
      const k = reverse ? (len - 1 - i) : i;
      const env = Math.pow(1 - k / len, decay);
      data[i] = (Math.random() * 2 - 1) * env;
    }
  }
  return buf;
}

function buildChain(seconds: number, decay: number, reverse: boolean): ReverbChain {
  const ctx = getAudioContext();
  const input = ctx.createGain();
  const dry = ctx.createGain();
  const wet = ctx.createGain();
  const convolver = ctx.createConvolver();
  const output = ctx.createGain();
  convolver.buffer = buildIR(ctx, seconds, decay, reverse);

  input.connect(dry);
  input.connect(convolver);
  convolver.connect(wet);
  dry.connect(output);
  wet.connect(output);

  return { input, dry, wet, convolver, output, seconds, decay, reverse };
}

export const reverbNode = registerNode({
  type: 'sound/reverb',
  category: 'Sound',
  label: 'Reverb',
  description: 'Add convolution reverb to an audio signal.',
  defaultParams: { reverse: false },
  inputs: [
    { name: 'in', type: 'audio' },
    { name: 'seconds', type: 'number', default: 3 },
    { name: 'decay', type: 'number', default: 2 },
    { name: 'wet', type: 'number', default: 0.5 },
  ],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const upstream = (inputs[0] ?? [])[0] as AudioHandle | null;
    const seconds = ((inputs[1] ?? [])[0] as number | undefined) ?? 3;
    const decay = ((inputs[2] ?? [])[0] as number | undefined) ?? 2;
    const wet = ((inputs[3] ?? [])[0] as number | undefined) ?? 0.5;
    const reverse = !!params.param<boolean>('reverse', false);
    const s = Math.max(0.001, Math.min(10, seconds));
    const dc = Math.max(0, Math.min(100, decay));
    const wt = Math.max(0, Math.min(1, wet));

    const output = audioRegistry.ensure<GainNode>(
      params.nodeId, 'main', 'reverb',
      () => {
        const chain = buildChain(s, dc, reverse);
        chains.set(params.nodeId, chain);
        return chain.output;
      },
    );
    const chain = chains.get(params.nodeId)!;
    if (chain.seconds !== s || chain.decay !== dc || chain.reverse !== reverse) {
      chain.convolver.buffer = buildIR(getAudioContext(), s, dc, reverse);
      chain.seconds = s; chain.decay = dc; chain.reverse = reverse;
    }
    const t = getAudioContext().currentTime;
    chain.wet.gain.cancelScheduledValues(t);
    chain.wet.gain.linearRampToValueAtTime(wt, t + 0.02);
    chain.dry.gain.cancelScheduledValues(t);
    chain.dry.gain.linearRampToValueAtTime(1 - wt, t + 0.02);

    if (upstream && upstream.node) {
      audioRegistry.connect(upstream, chain.input);
    }

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: output, kind: 'reverb',
    };
    return [[handle]];
  },
});
