import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Compressor — dynamic-range compression via Web Audio's
 * DynamicsCompressorNode. Native AudioParams for everything; no
 * curve-rebuild gymnastics.
 */
export const compressorNode = registerNode({
  type: 'sound/compressor',
  category: 'Sound',
  label: 'Compressor',
  description: 'Compress audio dynamics with threshold, ratio, attack, and release.',
  inputs: [
    { name: 'in', type: 'audio' },
    { name: 'threshold', type: 'number', default: -24 },
    { name: 'ratio', type: 'number', default: 12 },
    { name: 'knee', type: 'number', default: 30 },
    { name: 'attack', type: 'number', default: 0.003 },
    { name: 'release', type: 'number', default: 0.25 },
  ],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const upstream = (inputs[0] ?? [])[0] as AudioHandle | null;
    const threshold = ((inputs[1] ?? [])[0] as number | undefined) ?? -24;
    const ratio = ((inputs[2] ?? [])[0] as number | undefined) ?? 12;
    const knee = ((inputs[3] ?? [])[0] as number | undefined) ?? 30;
    const attack = ((inputs[4] ?? [])[0] as number | undefined) ?? 0.003;
    const release = ((inputs[5] ?? [])[0] as number | undefined) ?? 0.25;

    const c = audioRegistry.ensure<DynamicsCompressorNode>(
      params.nodeId, 'main', 'compressor',
      () => getAudioContext().createDynamicsCompressor(),
    );
    const t = getAudioContext().currentTime;
    c.threshold.setValueAtTime(Math.max(-100, Math.min(0, threshold)), t);
    c.ratio.setValueAtTime(Math.max(1, Math.min(20, ratio)), t);
    c.knee.setValueAtTime(Math.max(0, Math.min(40, knee)), t);
    c.attack.setValueAtTime(Math.max(0, Math.min(1, attack)), t);
    c.release.setValueAtTime(Math.max(0, Math.min(1, release)), t);

    if (upstream && upstream.node) {
      audioRegistry.connect(upstream, c);
    }

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: c, kind: 'compressor',
    };
    return [[handle]];
  },
});
