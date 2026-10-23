import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Envelope — ADSR audio gate built from a GainNode and a small
 * AudioParam scheduling routine. Rising edge on `trigger` fires
 * attack→decay→sustain; falling edge fires release. Pass-through
 * audio is modulated by the gain.
 */

interface EnvChain {
  gain: GainNode;
  lastTrigger: boolean;
}

const chains = new Map<string, EnvChain>();

export const envelopeNode = registerNode({
  type: 'sound/envelope',
  category: 'Sound',
  label: 'Envelope',
  description: 'Shape an audio signal with an ADSR envelope.',
  inputs: [
    { name: 'in', type: 'audio' },
    { name: 'attack', type: 'number', default: 0.05 },
    { name: 'decay', type: 'number', default: 0.1 },
    { name: 'sustain', type: 'number', default: 0.5 },
    { name: 'release', type: 'number', default: 0.5 },
    { name: 'level', type: 'number', default: 1 },
    { name: 'trigger', type: 'boolean', default: false },
  ],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const upstream = (inputs[0] ?? [])[0] as AudioHandle | null;
    const attack = ((inputs[1] ?? [])[0] as number | undefined) ?? 0.05;
    const decay = ((inputs[2] ?? [])[0] as number | undefined) ?? 0.1;
    const sustain = ((inputs[3] ?? [])[0] as number | undefined) ?? 0.5;
    const release = ((inputs[4] ?? [])[0] as number | undefined) ?? 0.5;
    const level = ((inputs[5] ?? [])[0] as number | undefined) ?? 1;
    const trigger = !!((inputs[6] ?? [])[0]);

    const gain = audioRegistry.ensure<GainNode>(
      params.nodeId, 'main', 'envelope',
      () => {
        const ctx = getAudioContext();
        const g = ctx.createGain();
        g.gain.value = 0;
        chains.set(params.nodeId, { gain: g, lastTrigger: false });
        return g;
      },
    );
    const chain = chains.get(params.nodeId)!;

    const a = Math.max(0, Math.min(10, attack));
    const d = Math.max(0, Math.min(10, decay));
    const s = Math.max(0, Math.min(1, sustain));
    const r = Math.max(0, Math.min(10, release));
    const peak = Math.max(0, Math.min(2, level));
    const ctx = getAudioContext();
    const t = ctx.currentTime;

    if (trigger && !chain.lastTrigger) {
      gain.gain.cancelScheduledValues(t);
      gain.gain.setValueAtTime(gain.gain.value, t);
      gain.gain.linearRampToValueAtTime(peak, t + a);
      gain.gain.linearRampToValueAtTime(peak * s, t + a + d);
    } else if (!trigger && chain.lastTrigger) {
      gain.gain.cancelScheduledValues(t);
      gain.gain.setValueAtTime(gain.gain.value, t);
      gain.gain.linearRampToValueAtTime(0, t + r);
    }
    chain.lastTrigger = trigger;

    if (upstream && upstream.node) {
      audioRegistry.connect(upstream, gain);
    }

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: gain, kind: 'envelope',
    };
    return [[handle]];
  },
});
