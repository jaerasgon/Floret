import { registerNode } from '../registry';
import { audioRegistry, getAudioContext } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * SoundFile — fetch + decodeAudioData → AudioBufferSourceNode chain.
 *
 * Web Audio's source nodes are single-shot (you `start()` and `stop()`,
 * but can't reuse) so each rising edge of `play` creates a fresh
 * AudioBufferSourceNode wired into the persistent output GainNode.
 *
 *   AudioBuffer (cached) → BufferSource (per play) → StereoPanner → output
 */

interface SoundFileChain {
  buffer: AudioBuffer | null;
  panner: StereoPannerNode;
  output: GainNode;
  source: AudioBufferSourceNode | null;
  loaded: boolean;
  duration: number;
  startedAtCtxTime: number;   // ctx.currentTime when the latest source started
  startedAtPosition: number;  // seconds offset within buffer when start fired
  url: string;
  lastPlay: boolean;
  loop: boolean;
  rate: number;
  // Token guards against stale fetches: increment when URL changes;
  // an in-flight decode that finishes for an older URL is discarded.
  loadToken: number;
}

const chains = new Map<string, SoundFileChain>();

function buildChain(url: string): SoundFileChain {
  const ctx = getAudioContext();
  const panner = ctx.createStereoPanner();
  const output = ctx.createGain();
  panner.connect(output);
  const chain: SoundFileChain = {
    buffer: null,
    panner,
    output,
    source: null,
    loaded: false,
    duration: 0,
    startedAtCtxTime: 0,
    startedAtPosition: 0,
    url,
    lastPlay: false,
    loop: false,
    rate: 1,
    loadToken: 0,
  };
  void loadBuffer(chain, url);
  return chain;
}

async function loadBuffer(chain: SoundFileChain, url: string): Promise<void> {
  const token = ++chain.loadToken;
  try {
    const res = await fetch(url);
    const bytes = await res.arrayBuffer();
    const ctx = getAudioContext();
    const buf = await ctx.decodeAudioData(bytes);
    if (token !== chain.loadToken) return;  // URL changed mid-load
    chain.buffer = buf;
    chain.duration = buf.duration;
    chain.loaded = true;
  } catch (err) {
    // eslint-disable-next-line no-console
    console.warn('SoundFile load failed:', url, err);
  }
}

function startPlayback(chain: SoundFileChain, offsetSeconds: number): void {
  if (!chain.buffer) return;
  if (chain.source) {
    try { chain.source.stop(); } catch { /* ignore */ }
    try { chain.source.disconnect(); } catch { /* ignore */ }
  }
  const ctx = getAudioContext();
  const src = ctx.createBufferSource();
  src.buffer = chain.buffer;
  src.loop = chain.loop;
  src.playbackRate.value = chain.rate;
  src.connect(chain.panner);
  const startOffset = Math.max(0, Math.min(chain.duration, offsetSeconds));
  src.start(0, startOffset);
  chain.source = src;
  chain.startedAtCtxTime = ctx.currentTime;
  chain.startedAtPosition = startOffset;
}

function stopPlayback(chain: SoundFileChain): void {
  if (chain.source) {
    try { chain.source.stop(); } catch { /* ignore */ }
    try { chain.source.disconnect(); } catch { /* ignore */ }
    chain.source = null;
  }
}

export const soundFileNode = registerNode({
  type: 'sound/sound-file',
  category: 'Sound',
  label: 'SoundFile',
  description: 'Load and play an audio file from URL.',
  defaultParams: { url: '' },
  inputs: [
    { name: 'url', type: 'string', param: 'url', default: '' },
    { name: 'rate', type: 'number', default: 1 },
    { name: 'pan', type: 'number', default: 0 },
    { name: 'amp', type: 'number', default: 1 },
    { name: 'loop', type: 'boolean', default: false },
    { name: 'play', type: 'boolean', default: false },
  ],
  outputs: [
    { name: 'audio', type: 'audio' },
    { name: 'loaded', type: 'boolean' },
    { name: 'duration', type: 'number' },
    { name: 'position', type: 'number' },
  ],
  computeRaw: (inputs: PortValue[], params: NodeParamContext) => {
    const urlRaw = (inputs[0] ?? [])[0];
    let url = (typeof urlRaw === 'string' ? urlRaw : '').trim();
    if (url.startsWith('data:') && !url.startsWith('data:audio/')) url = '';

    const rate = ((inputs[1] ?? [])[0] as number | undefined) ?? 1;
    const pan = ((inputs[2] ?? [])[0] as number | undefined) ?? 0;
    const amp = ((inputs[3] ?? [])[0] as number | undefined) ?? 1;
    const loop = !!((inputs[4] ?? [])[0]);
    const play = !!((inputs[5] ?? [])[0]);

    if (!url) {
      audioRegistry.dispose(params.nodeId);
      chains.delete(params.nodeId);
      return [[null], [false], [0], [0]];
    }

    const output = audioRegistry.ensure<GainNode>(
      params.nodeId, 'main', 'sound-file',
      () => {
        const chain = buildChain(url);
        chains.set(params.nodeId, chain);
        return chain.output;
      },
    );
    const chain = chains.get(params.nodeId)!;
    if (chain.url !== url) {
      stopPlayback(chain);
      chain.buffer = null;
      chain.loaded = false;
      chain.duration = 0;
      chain.url = url;
      void loadBuffer(chain, url);
    }

    if (!chain.loaded) {
      return [[null], [false], [0], [0]];
    }

    // Live params.
    chain.loop = loop;
    chain.rate = rate;
    if (chain.source) {
      chain.source.loop = loop;
      chain.source.playbackRate.value = rate;
    }
    const t = getAudioContext().currentTime;
    chain.panner.pan.cancelScheduledValues(t);
    chain.panner.pan.linearRampToValueAtTime(Math.max(-1, Math.min(1, pan)), t + 0.02);
    output.gain.cancelScheduledValues(t);
    output.gain.linearRampToValueAtTime(Math.max(0, Math.min(2, amp)), t + 0.02);

    // Edge-detect play.
    if (play && !chain.lastPlay) startPlayback(chain, 0);
    else if (!play && chain.lastPlay) stopPlayback(chain);
    chain.lastPlay = play;

    let position = 0;
    if (chain.source) {
      const elapsed = (t - chain.startedAtCtxTime) * chain.rate;
      position = (chain.startedAtPosition + elapsed) % Math.max(0.001, chain.duration);
      if (position < 0) position = 0;
    }

    const handle: AudioHandle = {
      nodeId: params.nodeId, slot: 'main', node: output, kind: 'sound-file',
    };
    return [[handle], [true], [chain.duration], [position]];
  },
});
