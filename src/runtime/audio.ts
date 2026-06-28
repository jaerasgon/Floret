import type { AudioHandle, AudioNodeKind } from './types';

/**
 * Audio registry — native Web Audio.
 *
 * Web Audio nodes (OscillatorNode, BiquadFilterNode, GainNode, …) are
 * persistent objects that run continuously on the audio thread.
 * Recreating them every frame would cause clicks + CPU spikes, so we
 * keep them alive across frames keyed by `${nodeId}:${slot}`.
 *
 * Lifecycle:
 *   1. A sound-node's `computeRaw` calls `audioRegistry.ensure(nodeId,
 *      slot, kind, factory)` each tick. On first call the factory runs
 *      and the AudioNode is cached; subsequent calls return the cache.
 *   2. The compute function updates parameters (freq, gain, cutoff, …)
 *      via the cached node's AudioParam objects and returns an
 *      AudioHandle wire value.
 *   3. Node deletion → `audioRegistry.dispose(nodeId)` from the editor
 *      delete path tears down every slot.
 *   4. Patch reload → `audioRegistry.reset()` blows the whole map.
 *
 * Connections between nodes are managed by sinks (Output) which walk
 * the upstream chain and re-wire only when topology changes.
 *
 * Browser autoplay policy: the AudioContext starts suspended until a
 * user gesture. `installAudioStartHook()` adds one-shot listeners that
 * resume it on the first pointerdown/keydown.
 */

interface Entry {
  kind: AudioNodeKind;
  node: AudioNode;
  /** Last destination this node was connected to, used by sinks to
   *  avoid reconnect spam. `null` = unconnected. `'master'` = destination. */
  connectedTo: AudioNode | 'master' | null;
}

let _ctx: AudioContext | null = null;

/**
 * The shared AudioContext. Created lazily on first use so SSR / Node
 * tests don't crash importing this module.
 */
export function getAudioContext(): AudioContext {
  if (_ctx) return _ctx;
  const Ctor = (globalThis as unknown as {
    AudioContext: typeof AudioContext;
    webkitAudioContext?: typeof AudioContext;
  }).AudioContext
    ?? (globalThis as unknown as { webkitAudioContext: typeof AudioContext })
        .webkitAudioContext;
  _ctx = new Ctor();
  return _ctx;
}

class AudioRegistry {
  private entries = new Map<string, Entry>();
  /** Master gain — every sink (Output) routes through this so the user
   *  can set volume globally without touching every node's gain. */
  private _master: GainNode | null = null;
  private started = false;

  private keyOf(nodeId: string, slot: string): string {
    return `${nodeId}:${slot}`;
  }

  /** Build (lazily) the master gain → destination chain. */
  private ensureMaster(): GainNode {
    if (this._master) return this._master;
    const ctx = getAudioContext();
    const g = ctx.createGain();
    g.gain.value = 1;
    g.connect(ctx.destination);
    this._master = g;
    return g;
  }

  /**
   * Get or create the audio node for (nodeId, slot, kind). If an entry
   * exists with a different `kind`, dispose + recreate — this happens
   * when the user swaps a dropdown like Filter type.
   */
  ensure<T extends AudioNode = AudioNode>(
    nodeId: string,
    slot: string,
    kind: AudioNodeKind,
    factory: () => T,
  ): T {
    const key = this.keyOf(nodeId, slot);
    const existing = this.entries.get(key);
    if (existing && existing.kind === kind) {
      return existing.node as T;
    }
    if (existing) this.disposeEntry(existing);
    const node = factory();
    this.entries.set(key, { kind, node, connectedTo: null });
    return node;
  }

  get(nodeId: string, slot: string = 'main'): Entry | undefined {
    return this.entries.get(this.keyOf(nodeId, slot));
  }

  /** Build an AudioHandle wire value. Caller must have `ensure`d the slot first. */
  handle(nodeId: string, slot: string = 'main'): AudioHandle | null {
    const e = this.entries.get(this.keyOf(nodeId, slot));
    if (!e) return null;
    return { nodeId, slot, node: e.node, kind: e.kind };
  }

  /**
   * Re-route a source's output to the given destination. No-op when
   * the connection is unchanged. Disconnects the previous destination
   * first. `dest === 'master'` connects through the master gain.
   */
  connect(source: AudioHandle, dest: AudioNode | 'master'): void {
    const e = this.entries.get(this.keyOf(source.nodeId, source.slot));
    if (!e) return;
    if (e.connectedTo === dest) return;
    try {
      e.node.disconnect();
    } catch { /* already-disconnected throws; ignore */ }
    try {
      if (dest === 'master') e.node.connect(this.ensureMaster());
      else e.node.connect(dest);
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn(`audio connect failed for ${source.nodeId}:`, err);
    }
    e.connectedTo = dest;
  }

  /**
   * Tap connection for analysers — wire source into an analyser node
   * without disturbing the source's main routing. Safe to call every
   * frame: AnalyserNode is the destination so subsequent calls re-
   * connect the same pair (Web Audio dedups internally per (out, in)).
   */
  tap(source: AudioHandle, analyser: AnalyserNode): void {
    if (!source.node) return;
    try {
      source.node.connect(analyser);
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn(`audio tap failed for ${source.nodeId}:`, err);
    }
  }

  /** Disconnect a source from whatever it was routed to. */
  disconnect(source: AudioHandle): void {
    const e = this.entries.get(this.keyOf(source.nodeId, source.slot));
    if (!e) return;
    if (e.connectedTo === null) return;
    try { e.node.disconnect(); } catch { /* see connect */ }
    e.connectedTo = null;
  }

  /** Drop entries whose nodeId is no longer present in the live set. */
  retain(liveNodeIds: ReadonlySet<string>): void {
    for (const [key, e] of this.entries) {
      const [nodeId] = key.split(':');
      if (!nodeId || liveNodeIds.has(nodeId)) continue;
      this.disposeEntry(e);
      this.entries.delete(key);
    }
  }

  /** Drop all slots for a single node. */
  dispose(nodeId: string): void {
    const prefix = `${nodeId}:`;
    for (const [key, e] of this.entries) {
      if (!key.startsWith(prefix)) continue;
      this.disposeEntry(e);
      this.entries.delete(key);
    }
  }

  /** Tear down everything. Called on patch reload and HMR teardown. */
  reset(): void {
    for (const e of this.entries.values()) this.disposeEntry(e);
    this.entries.clear();
    if (this._master) {
      try { this._master.disconnect(); } catch { /* ignore */ }
      this._master = null;
    }
    this.started = false;
  }

  /**
   * Resume the AudioContext. Browsers suspend it until a user gesture;
   * call from a click / keydown handler. Idempotent.
   */
  async start(): Promise<void> {
    if (this.started) return;
    const ctx = getAudioContext();
    try {
      if (ctx.state === 'suspended') await ctx.resume();
      this.started = true;
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn('audio start failed:', err);
    }
  }

  isStarted(): boolean {
    return this.started;
  }

  audioContext(): AudioContext {
    return getAudioContext();
  }

  /** Master gain output — pass to `connect(handle, masterOut())`. */
  masterOut(): 'master' {
    return 'master';
  }

  /** Set master volume (0..2, clamped). */
  setMasterVolume(v: number): void {
    const master = this.ensureMaster();
    const t = getAudioContext().currentTime;
    master.gain.setValueAtTime(Math.max(0, Math.min(2, v)), t);
  }

  private disposeEntry(e: Entry): void {
    const n = e.node as AudioNode & { stop?: (when?: number) => void };
    try { n.stop?.(); } catch { /* ignore */ }
    try { n.disconnect(); } catch { /* ignore */ }
  }
}

export const audioRegistry = new AudioRegistry();

if (typeof window !== 'undefined') {
  (window as unknown as { __audioRegistry?: unknown }).__audioRegistry = audioRegistry;
}

/**
 * Browser autoplay policy hook. Attaches one-shot listeners for the
 * first pointerdown / keydown / touchstart, resumes the AudioContext,
 * then itself stays installed (cheap — just resume() guarded by a flag).
 * Idempotent.
 */
let _audioHookInstalled = false;
export function installAudioStartHook(): void {
  if (_audioHookInstalled) return;
  if (typeof window === 'undefined') return;
  _audioHookInstalled = true;
  const handler = () => { void audioRegistry.start(); };
  window.addEventListener('pointerdown', handler);
  window.addEventListener('keydown', handler);
  window.addEventListener('touchstart', handler);
}
