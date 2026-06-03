import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * AudioIn — microphone / line input via getUserMedia →
 * MediaStreamAudioSourceNode → GainNode chain.
 *
 *   getUserMedia() ─→ MediaStreamSource ─→ amp (GainNode) ─→ output
 *
 * `start` is edge-triggered: rising edge requests mic permission +
 * starts the stream; falling edge tears the stream down. Audio output
 * is null until permission is granted AND the stream is live.
 */

interface MicChain {
  output: GainNode;
  source: MediaStreamAudioSourceNode | null;
  stream: MediaStream | null;
  lastStart: boolean;
  started: boolean;
}

const chains = new Map<string, MicChain>();

function buildChain(): MicChain {
  const ctx = getAudioContext();
  const output = ctx.createGain();
  output.gain.value = 0.5;
  return { output, source: null, stream: null, lastStart: false, started: false };
}

async function startStream(chain: MicChain): Promise<void> {
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    if (!stream) return;
    const ctx = getAudioContext();
    const source = ctx.createMediaStreamSource(stream);
    source.connect(chain.output);
    chain.source = source;
    chain.stream = stream;
    chain.started = true;
  } catch (err) {
    // eslint-disable-next-line no-console
    console.warn('audio-in start failed:', err);
  }
}

function stopStream(chain: MicChain): void {
  if (chain.source) {
    try { chain.source.disconnect(); } catch { /* ignore */ }
    chain.source = null;
  }
  if (chain.stream) {
    for (const track of chain.stream.getTracks()) {
      try { track.stop(); } catch { /* ignore */ }
    }
    chain.stream = null;
  }
  chain.started = false;
}

export const audioInNode = registerNode({
  type: 'sound/audio-in',
  category: 'Sound',
  label: 'AudioIn',
  description: 'Read audio from the microphone.',
  inputs: [
    { name: 'amp', type: 'number', default: 0.5 },
    { name: 'start', type: 'boolean', default: false },
  ],
  outputs: [{ name: 'audio', type: 'audio' }],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const amp = ((inputs[0] ?? [])[0] as number | undefined) ?? 0.5;
    const start = !!((inputs[1] ?? [])[0]);

    const output = audioRegistry.ensure<GainNode>(
      params.nodeId, 'main', 'audio-in',
      () => {
        const chain = buildChain();
        chains.set(params.nodeId, chain);
        return chain.output;
      },
    );
    const chain = chains.get(params.nodeId)!;
    const t = getAudioContext().currentTime;
    output.gain.cancelScheduledValues(t);
    output.gain.linearRampToValueAtTime(Math.max(0, Math.min(1, amp)), t + 0.02);

    if (start && !chain.lastStart) void startStream(chain);
    else if (!start && chain.lastStart) stopStream(chain);
    chain.lastStart = start;

    if (!chain.started) return [[null]];

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: output, kind: 'audio-in',
    };
    return [[handle]];
  },
});
