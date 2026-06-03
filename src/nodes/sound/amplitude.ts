import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Amplitude — RMS level (0..1) of the upstream audio. Tap-style: the
 * upstream isn't consumed, just observed. `smoothing` is an EMA factor
 * applied per-tick (0 = sample-true, ~0.9 = smooth).
 *
 * Implemented as an AnalyserNode + getFloatTimeDomainData() each tick.
 */

const buffers = new Map<string, Float32Array>();
const prevLevels = new Map<string, number>();

export const amplitudeNode = registerNode({
  type: 'sound/amplitude',
  category: 'Sound',
  label: 'Amplitude',
  description: 'Measure the RMS level of an audio signal.',
  inputs: [
    { name: 'in', type: 'audio' },
    { name: 'smoothing', type: 'number', default: 0.4 },
  ],
  outputs: [{ name: 'level', type: 'number' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const upstream = (inputs[0] ?? [])[0] as AudioHandle | null;
    const smoothing = ((inputs[1] ?? [])[0] as number | undefined) ?? 0.4;

    const analyser = audioRegistry.ensure<AnalyserNode>(
      params.nodeId, 'main', 'amplitude',
      () => {
        const ctx = getAudioContext();
        const a = ctx.createAnalyser();
        a.fftSize = 2048;
        a.smoothingTimeConstant = 0;  // we handle smoothing in JS for level
        buffers.set(params.nodeId, new Float32Array(new ArrayBuffer(a.fftSize * 4)));
        return a;
      },
    );
    if (upstream && upstream.node) {
      audioRegistry.tap(upstream, analyser);
    }
    const buf = buffers.get(params.nodeId)!;
    analyser.getFloatTimeDomainData(buf as Float32Array<ArrayBuffer>);
    let sum = 0;
    for (let i = 0; i < buf.length; i++) sum += buf[i]! * buf[i]!;
    const rms = Math.sqrt(sum / buf.length);
    const k = Math.max(0, Math.min(0.999, smoothing));
    const prev = prevLevels.get(params.nodeId) ?? 0;
    const smoothed = prev * k + rms * (1 - k);
    prevLevels.set(params.nodeId, smoothed);
    return [[smoothed]];
  },
});
