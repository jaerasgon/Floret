import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Oscillator — periodic waveform via Web Audio's OscillatorNode.
 *
 * `type` (param, dropdown): sine / triangle / sawtooth / square.
 * Inputs: freq (Hz), amp (0..1). Output: audio.
 *
 * Internally builds `OscillatorNode → GainNode` per node. The Gain is
 * what we hand out as the AudioHandle: downstream consumers tap into
 * it and we control amplitude there. OscillatorNode by itself has no
 * amplitude control.
 *
 * Wave type can't change on an existing OscillatorNode without an
 * artifact; we rebuild on a kind swap. The registry keys oscillator
 * entries by `oscillator` (kind) so a `type` param swap is detected via
 * a stashed string check below and triggers a controlled recreate.
 */

interface OscillatorChain {
  /** The exported node — the gain that downstream consumes. */
  output: GainNode;
  /** Internal source — modulated each tick. */
  source: OscillatorNode;
  /** Last-applied wave type, so we can detect a type-param swap. */
  type: OscillatorType;
}

const TYPES: ReadonlyArray<OscillatorType> = ['sine', 'triangle', 'sawtooth', 'square'];

// Per-node chain stash so we can introspect the previous wave type
// across ticks (audio registry only knows about the public AudioNode).
const chains = new Map<string, OscillatorChain>();

function buildChain(initialFreq: number, initialType: OscillatorType): OscillatorChain {
  const ctx = getAudioContext();
  const source = ctx.createOscillator();
  source.type = initialType;
  source.frequency.value = initialFreq;
  const output = ctx.createGain();
  output.gain.value = 0;  // silence at construction to avoid DC pop
  source.connect(output);
  source.start();
  return { source, output, type: initialType };
}

export const oscillatorNode = registerNode({
  type: 'sound/oscillator',
  category: 'Sound',
  label: 'Oscillator',
  description: 'Generate a sine, triangle, saw, or square waveform.',
  defaultParams: { type: 'sine' },
  paramOptions: { type: TYPES as unknown as ReadonlyArray<string> },
  inputs: [
    { name: 'freq', type: 'number', default: 440 },
    { name: 'amp', type: 'number', default: 0.5 },
  ],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const freq = ((inputs[0] ?? [])[0] as number | undefined) ?? 440;
    const amp = ((inputs[1] ?? [])[0] as number | undefined) ?? 0.5;
    const type = (params.param<string>('type', 'sine') as OscillatorType);

    const output = audioRegistry.ensure<GainNode>(
      params.nodeId, 'main', 'oscillator',
      () => {
        const chain = buildChain(freq, type);
        chains.set(params.nodeId, chain);
        return chain.output;
      },
    );

    // Detect a wave-type swap and rebuild the source side without
    // touching the GainNode (downstream connections stay intact).
    const chain = chains.get(params.nodeId);
    if (chain && chain.type !== type) {
      try { chain.source.stop(); } catch { /* ignore */ }
      try { chain.source.disconnect(); } catch { /* ignore */ }
      const ctx = getAudioContext();
      const fresh = ctx.createOscillator();
      fresh.type = type;
      fresh.frequency.value = freq;
      fresh.connect(chain.output);
      fresh.start();
      chain.source = fresh;
      chain.type = type;
    }

    // Live parameter updates. Linear ramps to the current frame's
    // sample time keep sweeps smooth without clicks.
    if (chain) {
      const t = getAudioContext().currentTime;
      chain.source.frequency.cancelScheduledValues(t);
      chain.source.frequency.linearRampToValueAtTime(freq, t + 0.02);
    }
    const t = getAudioContext().currentTime;
    output.gain.cancelScheduledValues(t);
    output.gain.linearRampToValueAtTime(Math.max(0, Math.min(1, amp)), t + 0.02);

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: output, kind: 'oscillator',
    };
    return [[handle]];
  },
});
