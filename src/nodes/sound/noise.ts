import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Noise — white / pink / brown via an AudioBufferSourceNode looping a
 * 2-second prebuilt noise buffer. (Web Audio has no built-in noise
 * generator.) Output is `AudioBufferSourceNode → GainNode`; we expose
 * the gain as the AudioHandle so amp updates don't touch the source.
 */

type NoiseType = 'white' | 'pink' | 'brown';
const TYPES: ReadonlyArray<NoiseType> = ['white', 'pink', 'brown'];

interface NoiseChain {
  source: AudioBufferSourceNode;
  output: GainNode;
  type: NoiseType;
}

const chains = new Map<string, NoiseChain>();

/** Build a 2-second mono buffer for the requested noise colour. */
function buildBuffer(ctx: BaseAudioContext, type: NoiseType): AudioBuffer {
  const seconds = 2;
  const buf = ctx.createBuffer(1, seconds * ctx.sampleRate, ctx.sampleRate);
  const data = buf.getChannelData(0);
  if (type === 'white') {
    for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
    return buf;
  }
  if (type === 'pink') {
    // Paul Kellet's pink-noise approximation — cheap, surprisingly good.
    let b0 = 0, b1 = 0, b2 = 0, b3 = 0, b4 = 0, b5 = 0, b6 = 0;
    for (let i = 0; i < data.length; i++) {
      const white = Math.random() * 2 - 1;
      b0 = 0.99886 * b0 + white * 0.0555179;
      b1 = 0.99332 * b1 + white * 0.0750759;
      b2 = 0.96900 * b2 + white * 0.1538520;
      b3 = 0.86650 * b3 + white * 0.3104856;
      b4 = 0.55000 * b4 + white * 0.5329522;
      b5 = -0.7616 * b5 - white * 0.0168980;
      const pink = b0 + b1 + b2 + b3 + b4 + b5 + b6 + white * 0.5362;
      b6 = white * 0.115926;
      data[i] = pink * 0.11;  // gain normalize
    }
    return buf;
  }
  // brown: integrate white, leak to keep DC bounded
  let last = 0;
  for (let i = 0; i < data.length; i++) {
    const white = Math.random() * 2 - 1;
    last = (last + 0.02 * white) / 1.02;
    data[i] = last * 3.5;
  }
  return buf;
}

function buildChain(type: NoiseType): NoiseChain {
  const ctx = getAudioContext();
  const source = ctx.createBufferSource();
  source.buffer = buildBuffer(ctx, type);
  source.loop = true;
  const output = ctx.createGain();
  output.gain.value = 0;
  source.connect(output);
  source.start();
  return { source, output, type };
}

export const noiseNode = registerNode({
  type: 'sound/noise',
  category: 'Sound',
  label: 'Noise',
  description: 'Generate white, pink, or brown noise.',
  defaultParams: { type: 'white' },
  paramOptions: { type: TYPES as unknown as ReadonlyArray<string> },
  inputs: [{ name: 'amp', type: 'number', default: 0.5 }],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const amp = ((inputs[0] ?? [])[0] as number | undefined) ?? 0.5;
    const type = (params.param<string>('type', 'white') as NoiseType);

    const output = audioRegistry.ensure<GainNode>(
      params.nodeId, 'main', 'noise',
      () => {
        const chain = buildChain(type);
        chains.set(params.nodeId, chain);
        return chain.output;
      },
    );
    const chain = chains.get(params.nodeId);
    if (chain && chain.type !== type) {
      try { chain.source.stop(); } catch { /* ignore */ }
      try { chain.source.disconnect(); } catch { /* ignore */ }
      const ctx = getAudioContext();
      const fresh = ctx.createBufferSource();
      fresh.buffer = buildBuffer(ctx, type);
      fresh.loop = true;
      fresh.connect(chain.output);
      fresh.start();
      chain.source = fresh;
      chain.type = type;
    }
    const t = getAudioContext().currentTime;
    output.gain.cancelScheduledValues(t);
    output.gain.linearRampToValueAtTime(Math.max(0, Math.min(1, amp)), t + 0.02);

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: output, kind: 'noise',
    };
    return [[handle]];
  },
});
