import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Distortion — waveshaper via Web Audio's WaveShaperNode. `amount`
 * (0..1) drives the curve aggressiveness; `oversample` trades CPU
 * for reduced aliasing.
 *
 * The curve is a classic "tan-based" soft-clipper, rebuilt only when
 * `amount` changes (256 samples — cheap, but no point thrashing).
 */

interface DistortionChain {
  input: GainNode;
  dry: GainNode;
  wet: GainNode;
  shaper: WaveShaperNode;
  output: GainNode;
  amount: number;
}

const chains = new Map<string, DistortionChain>();
const OVERSAMPLE = ['none', '2x', '4x'] as const;

function buildCurve(amount: number): Float32Array {
  const n = 256;
  const curve = new Float32Array(new ArrayBuffer(n * 4));
  const k = amount * 100 + 0.001;
  const deg = Math.PI / 180;
  for (let i = 0; i < n; i++) {
    const x = (i * 2) / n - 1;
    curve[i] = ((3 + k) * x * 20 * deg) / (Math.PI + k * Math.abs(x));
  }
  return curve;
}

function buildChain(amount: number, oversample: OverSampleType): DistortionChain {
  const ctx = getAudioContext();
  const input = ctx.createGain();
  const dry = ctx.createGain();
  const wet = ctx.createGain();
  const shaper = ctx.createWaveShaper();
  shaper.curve = buildCurve(amount) as Float32Array<ArrayBuffer>;
  shaper.oversample = oversample;
  const output = ctx.createGain();

  input.connect(dry);
  input.connect(shaper);
  shaper.connect(wet);
  dry.connect(output);
  wet.connect(output);

  return { input, dry, wet, shaper, output, amount };
}

export const distortionNode = registerNode({
  type: 'sound/distortion',
  category: 'Sound',
  label: 'Distortion',
  description: 'Waveshaper distortion driven by `amount`.',
  defaultParams: { oversample: 'none' },
  paramOptions: { oversample: OVERSAMPLE as unknown as ReadonlyArray<string> },
  inputs: [
    { name: 'in', type: 'audio' },
    { name: 'amount', type: 'number', default: 0.25 },
    { name: 'wet', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const upstream = (inputs[0] ?? [])[0] as AudioHandle | null;
    const amount = ((inputs[1] ?? [])[0] as number | undefined) ?? 0.25;
    const wet = ((inputs[2] ?? [])[0] as number | undefined) ?? 1;
    const oversample = params.param<string>('oversample', 'none') as OverSampleType;
    const a = Math.max(0, Math.min(1, amount));
    const wt = Math.max(0, Math.min(1, wet));

    const output = audioRegistry.ensure<GainNode>(
      params.nodeId, 'main', 'distortion',
      () => {
        const chain = buildChain(a, oversample);
        chains.set(params.nodeId, chain);
        return chain.output;
      },
    );
    const chain = chains.get(params.nodeId)!;
    if (chain.amount !== a) {
      chain.shaper.curve = buildCurve(a) as Float32Array<ArrayBuffer>;
      chain.amount = a;
    }
    if (chain.shaper.oversample !== oversample) {
      chain.shaper.oversample = oversample;
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
      nodeId: params.nodeId, slot: 'main', node: output, kind: 'distortion',
    };
    return [[handle]];
  },
});
