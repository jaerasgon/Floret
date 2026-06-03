import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Pulse — square wave with adjustable duty cycle (PWM). Implemented
 * as two sawtooth OscillatorNodes phase-offset by `width` and
 * subtracted — a classic PWM trick that avoids the aliasing of a
 * naive square at high frequencies.
 *
 *   oscA (saw +1) ─┐
 *                  ├─→ inverter ─→ output (gain)
 *   oscB (saw -1, phase-shifted) ─┘
 *
 * In practice we use one OscillatorNode of type 'sawtooth' fed into
 * a WaveShaper that converts saw → square based on a threshold; we
 * shift the threshold by `width` to skew the duty cycle.
 *
 * Web Audio doesn't expose an oscillator phase, so we use the simpler
 * "saw → waveshape" approach.
 */

interface PulseChain {
  source: OscillatorNode;
  shaper: WaveShaperNode;
  output: GainNode;
  width: number;
}

const chains = new Map<string, PulseChain>();

function buildCurve(width: number): Float32Array {
  // 257 samples (odd so 0 is centred). For each input x in [-1, 1],
  // output +1 when x > thresh, else -1. thresh = width-mapped offset.
  const n = 257;
  const curve = new Float32Array(new ArrayBuffer(n * 4));
  const thresh = 2 * width - 1;  // width 0 → -1 (50% duty), width 0.99 → +0.98 (tiny duty)
  for (let i = 0; i < n; i++) {
    const x = (i / (n - 1)) * 2 - 1;
    curve[i] = x > thresh ? 1 : -1;
  }
  return curve;
}

function buildChain(freq: number, width: number): PulseChain {
  const ctx = getAudioContext();
  const source = ctx.createOscillator();
  source.type = 'sawtooth';
  source.frequency.value = freq;
  const shaper = ctx.createWaveShaper();
  shaper.curve = buildCurve(width) as Float32Array<ArrayBuffer>;
  const output = ctx.createGain();
  output.gain.value = 0;
  source.connect(shaper);
  shaper.connect(output);
  source.start();
  return { source, shaper, output, width };
}

export const pulseNode = registerNode({
  type: 'sound/pulse',
  category: 'Sound',
  label: 'Pulse',
  description: 'Generate a square wave with adjustable pulse width.',
  inputs: [
    { name: 'freq', type: 'number', default: 220 },
    { name: 'amp', type: 'number', default: 0.5 },
    { name: 'width', type: 'number', default: 0 },
  ],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const freq = ((inputs[0] ?? [])[0] as number | undefined) ?? 220;
    const amp = ((inputs[1] ?? [])[0] as number | undefined) ?? 0.5;
    const width = ((inputs[2] ?? [])[0] as number | undefined) ?? 0;
    const w = Math.max(0, Math.min(0.99, width));

    const output = audioRegistry.ensure<GainNode>(
      params.nodeId, 'main', 'pulse',
      () => {
        const chain = buildChain(freq, w);
        chains.set(params.nodeId, chain);
        return chain.output;
      },
    );
    const chain = chains.get(params.nodeId)!;
    if (chain.width !== w) {
      chain.shaper.curve = buildCurve(w) as Float32Array<ArrayBuffer>;
      chain.width = w;
    }
    const t = getAudioContext().currentTime;
    chain.source.frequency.cancelScheduledValues(t);
    chain.source.frequency.linearRampToValueAtTime(freq, t + 0.02);
    output.gain.cancelScheduledValues(t);
    output.gain.linearRampToValueAtTime(Math.max(0, Math.min(1, amp)), t + 0.02);

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: output, kind: 'pulse',
    };
    return [[handle]];
  },
});
