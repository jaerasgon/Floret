import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Gain — chainable volume control via Web Audio's GainNode. Multiplies
 * the upstream signal by `level` and re-emits.
 */
export const gainNode = registerNode({
  type: 'sound/gain',
  category: 'Sound',
  label: 'Gain',
  description: 'Multiply an audio signal by `level`.',
  inputs: [
    { name: 'in', type: 'audio' },
    { name: 'level', type: 'number', default: 1 },
  ],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const upstream = (inputs[0] ?? [])[0] as AudioHandle | null;
    const level = ((inputs[1] ?? [])[0] as number | undefined) ?? 1;

    const gain = audioRegistry.ensure<GainNode>(
      params.nodeId, 'main', 'gain',
      () => getAudioContext().createGain(),
    );
    const t = getAudioContext().currentTime;
    gain.gain.cancelScheduledValues(t);
    gain.gain.linearRampToValueAtTime(Math.max(0, Math.min(2, level)), t + 0.02);

    if (upstream && upstream.node) {
      audioRegistry.connect(upstream, gain);
    }

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: gain, kind: 'gain',
    };
    return [[handle]];
  },
});
