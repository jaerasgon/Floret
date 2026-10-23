import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Filter — biquad filter via Web Audio's BiquadFilterNode.
 * `type` selects the response shape; freq sets cutoff (Hz); res sets
 * resonance (Q, 0.0001..50 clamped). The `.type` property can change
 * live without artifacts so no recreate needed.
 */

const TYPES = ['lowpass', 'highpass', 'bandpass'] as const;

export const filterNode = registerNode({
  type: 'sound/filter',
  category: 'Sound',
  label: 'Filter',
  description: 'Biquad filter — lowpass, highpass, or bandpass.',
  defaultParams: { type: 'lowpass' },
  paramOptions: { type: TYPES as unknown as ReadonlyArray<string> },
  inputs: [
    { name: 'in', type: 'audio' },
    { name: 'freq', type: 'number', default: 1000 },
    { name: 'res', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const upstream = (inputs[0] ?? [])[0] as AudioHandle | null;
    const freq = ((inputs[1] ?? [])[0] as number | undefined) ?? 1000;
    const res = ((inputs[2] ?? [])[0] as number | undefined) ?? 1;
    const type = params.param<string>('type', 'lowpass') as BiquadFilterType;

    const f = audioRegistry.ensure<BiquadFilterNode>(
      params.nodeId, 'main', 'filter',
      () => {
        const node = getAudioContext().createBiquadFilter();
        node.type = type;
        return node;
      },
    );
    if (f.type !== type) f.type = type;
    const t = getAudioContext().currentTime;
    f.frequency.cancelScheduledValues(t);
    f.frequency.linearRampToValueAtTime(
      Math.max(20, Math.min(20000, freq)), t + 0.02,
    );
    f.Q.cancelScheduledValues(t);
    f.Q.linearRampToValueAtTime(
      Math.max(0.0001, Math.min(50, res)), t + 0.02,
    );

    if (upstream && upstream.node) {
      audioRegistry.connect(upstream, f);
    }

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: f, kind: 'filter',
    };
    return [[handle]];
  },
});
