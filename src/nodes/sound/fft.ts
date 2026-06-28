import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * FFT — frequency + waveform analyser via Web Audio's AnalyserNode.
 *
 *   - `spectrum`  bin magnitudes 0..255 (Web Audio's byte-frequency domain)
 *   - `waveform`  time-domain samples -1..1
 *
 * `bins` (param) sets FFT size; `smoothing` is the analyser's built-in
 * smoothing time constant.
 *
 * AnalyserNode.fftSize is the time-domain buffer length; frequency
 * bins = fftSize/2. We honour the historical `bins` param semantics
 * (output is `bins` long) by setting fftSize = bins * 2.
 */

const BIN_OPTIONS = ['32', '64', '128', '256', '512', '1024'] as const;

interface AnalyserBuffers {
  freqU8: Uint8Array;
  timeF32: Float32Array;
  bins: number;
}

const buffers = new Map<string, AnalyserBuffers>();

export const fftNode = registerNode({
  type: 'sound/fft',
  category: 'Sound',
  label: 'FFT',
  description: 'Analyse audio into frequency-bin and waveform spreads.',
  defaultParams: { bins: '128' },
  paramOptions: { bins: BIN_OPTIONS as unknown as ReadonlyArray<string> },
  inputs: [
    { name: 'in', type: 'audio' },
    { name: 'smoothing', type: 'number', default: 0.8 },
  ],
  outputs: [
    { name: 'spectrum', type: 'number', kind: 'spread' },
    { name: 'waveform', type: 'number', kind: 'spread' },
  ],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const upstream = (inputs[0] ?? [])[0] as AudioHandle | null;
    const smoothing = ((inputs[1] ?? [])[0] as number | undefined) ?? 0.8;
    const bins = parseInt(params.param<string>('bins', '128'), 10);

    const analyser = audioRegistry.ensure<AnalyserNode>(
      params.nodeId, 'main', 'fft',
      () => {
        const ctx = getAudioContext();
        const a = ctx.createAnalyser();
        a.fftSize = bins * 2;
        return a;
      },
    );
    // Resize buffers / re-set fftSize if `bins` changed (e.g. user
    // swapped dropdown).
    if (analyser.fftSize !== bins * 2) {
      analyser.fftSize = bins * 2;
    }
    analyser.smoothingTimeConstant = Math.max(0, Math.min(0.999, smoothing));
    let bufs = buffers.get(params.nodeId);
    if (!bufs || bufs.bins !== bins) {
      bufs = {
        freqU8: new Uint8Array(new ArrayBuffer(bins)),
        timeF32: new Float32Array(new ArrayBuffer(bins * 2 * 4)),
        bins,
      };
      buffers.set(params.nodeId, bufs);
    }
    if (upstream && upstream.node) {
      audioRegistry.tap(upstream, analyser);
    }
    analyser.getByteFrequencyData(bufs.freqU8 as Uint8Array<ArrayBuffer>);
    analyser.getFloatTimeDomainData(bufs.timeF32 as Float32Array<ArrayBuffer>);
    // Convert typed arrays to plain number arrays for spread outputs.
    const spectrum = Array.from(bufs.freqU8);
    const waveform = Array.from(bufs.timeF32);
    return [spectrum, waveform];
  },
});
