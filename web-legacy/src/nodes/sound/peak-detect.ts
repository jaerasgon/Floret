import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * PeakDetect — boolean true while a frequency-band peak exceeds a
 * normalised threshold. Uses an AnalyserNode for spectrum data and a
 * simple "above threshold + minimum frame gap" heuristic — a standard
 * running-threshold peak detector, good enough for typical
 * beat-detection use.
 *
 * `freq1` / `freq2` bound the listening band (Hz). `threshold` (0..1)
 * is the minimum normalised energy required to register a peak.
 */

interface PeakState {
  analyser: AnalyserNode;
  freqBuf: Uint8Array;
  framesSincePeak: number;
}

const states = new Map<string, PeakState>();
const MIN_FRAMES_BETWEEN_PEAKS = 20;

export const peakDetectNode = registerNode({
  type: 'sound/peak-detect',
  category: 'Sound',
  label: 'PeakDetect',
  description: 'Detect amplitude peaks within a frequency band.',
  inputs: [
    { name: 'in', type: 'audio' },
    { name: 'freq1', type: 'number', default: 40 },
    { name: 'freq2', type: 'number', default: 120 },
    { name: 'threshold', type: 'number', default: 0.9 },
  ],
  outputs: [{ name: 'detected', type: 'boolean' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const upstream = (inputs[0] ?? [])[0] as AudioHandle | null;
    const freq1 = ((inputs[1] ?? [])[0] as number | undefined) ?? 40;
    const freq2 = ((inputs[2] ?? [])[0] as number | undefined) ?? 120;
    const threshold = ((inputs[3] ?? [])[0] as number | undefined) ?? 0.9;

    const analyser = audioRegistry.ensure<AnalyserNode>(
      params.nodeId, 'main', 'peak-detect',
      () => {
        const ctx = getAudioContext();
        const a = ctx.createAnalyser();
        a.fftSize = 512;
        a.smoothingTimeConstant = 0.5;
        states.set(params.nodeId, {
          analyser: a,
          freqBuf: new Uint8Array(new ArrayBuffer(a.frequencyBinCount)),
          framesSincePeak: 0,
        });
        return a;
      },
    );
    const state = states.get(params.nodeId)!;
    if (upstream && upstream.node) {
      audioRegistry.tap(upstream, analyser);
    }
    analyser.getByteFrequencyData(state.freqBuf as Uint8Array<ArrayBuffer>);

    // Frequency bins are linearly spaced across [0, sampleRate / 2].
    const sampleRate = getAudioContext().sampleRate;
    const binHz = sampleRate / analyser.fftSize;
    const i1 = Math.max(0, Math.floor(freq1 / binHz));
    const i2 = Math.min(state.freqBuf.length - 1, Math.ceil(freq2 / binHz));

    let max = 0;
    for (let i = i1; i <= i2; i++) {
      const v = state.freqBuf[i] ?? 0;
      if (v > max) max = v;
    }
    const normalised = max / 255;
    state.framesSincePeak++;
    let detected = false;
    if (normalised >= threshold && state.framesSincePeak >= MIN_FRAMES_BETWEEN_PEAKS) {
      detected = true;
      state.framesSincePeak = 0;
    }
    return [[detected]];
  },
});
