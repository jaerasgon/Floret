import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * MonoSynth — single-voice synth: one OscillatorNode shaped by an
 * ADSR-modulated GainNode. Rising edge of `trigger` plays the note;
 * falling edge releases. `note` is a MIDI number; we convert to Hz
 * via the standard 12-TET formula on each trigger so live note changes
 * during sustain track the input.
 */

interface SynthChain {
  source: OscillatorNode;
  gain: GainNode;     // ADSR envelope target + handed-out output
  lastTrigger: boolean;
}

const chains = new Map<string, SynthChain>();

function midiToHz(midi: number): number {
  return 440 * Math.pow(2, (midi - 69) / 12);
}

function buildChain(initialHz: number): SynthChain {
  const ctx = getAudioContext();
  const source = ctx.createOscillator();
  source.type = 'sawtooth';
  source.frequency.value = initialHz;
  const gain = ctx.createGain();
  gain.gain.value = 0;
  source.connect(gain);
  source.start();
  return { source, gain, lastTrigger: false };
}

export const monoSynthNode = registerNode({
  type: 'sound/mono-synth',
  category: 'Sound',
  label: 'MonoSynth',
  description: 'Play a single MIDI-note voice with an ADSR envelope.',
  inputs: [
    { name: 'note', type: 'number', default: 60 },
    { name: 'velocity', type: 'number', default: 0.7 },
    { name: 'attack', type: 'number', default: 0.02 },
    { name: 'decay', type: 'number', default: 0.25 },
    { name: 'sustain', type: 'number', default: 0.05 },
    { name: 'release', type: 'number', default: 0.35 },
    { name: 'trigger', type: 'boolean', default: false },
  ],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const note = ((inputs[0] ?? [])[0] as number | undefined) ?? 60;
    const velocity = ((inputs[1] ?? [])[0] as number | undefined) ?? 0.7;
    const attack = ((inputs[2] ?? [])[0] as number | undefined) ?? 0.02;
    const decay = ((inputs[3] ?? [])[0] as number | undefined) ?? 0.25;
    const sustain = ((inputs[4] ?? [])[0] as number | undefined) ?? 0.05;
    const release = ((inputs[5] ?? [])[0] as number | undefined) ?? 0.35;
    const trigger = !!((inputs[6] ?? [])[0]);

    const gain = audioRegistry.ensure<GainNode>(
      params.nodeId, 'main', 'mono-synth',
      () => {
        const chain = buildChain(midiToHz(note));
        chains.set(params.nodeId, chain);
        return chain.gain;
      },
    );
    const chain = chains.get(params.nodeId)!;
    const a = Math.max(0, Math.min(10, attack));
    const d = Math.max(0, Math.min(10, decay));
    const s = Math.max(0, Math.min(1, sustain));
    const r = Math.max(0, Math.min(10, release));
    const v = Math.max(0, Math.min(1, velocity));
    const ctx = getAudioContext();
    const t = ctx.currentTime;

    // Always track note → frequency, even during sustain.
    const hz = midiToHz(Math.max(0, Math.min(127, note)));
    chain.source.frequency.cancelScheduledValues(t);
    chain.source.frequency.linearRampToValueAtTime(hz, t + 0.01);

    if (trigger && !chain.lastTrigger) {
      gain.gain.cancelScheduledValues(t);
      gain.gain.setValueAtTime(gain.gain.value, t);
      gain.gain.linearRampToValueAtTime(v, t + a);
      gain.gain.linearRampToValueAtTime(v * s, t + a + d);
    } else if (!trigger && chain.lastTrigger) {
      gain.gain.cancelScheduledValues(t);
      gain.gain.setValueAtTime(gain.gain.value, t);
      gain.gain.linearRampToValueAtTime(0, t + r);
    }
    chain.lastTrigger = trigger;

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: gain, kind: 'mono-synth',
    };
    return [[handle]];
  },
});
