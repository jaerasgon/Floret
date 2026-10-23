/**
 * Camera registry.
 *
 * Per-node-id webcam / connected-camera capture. Mirrors the image and audio
 * registries: `ensure()` owns a long-lived `<video>` + capture `<canvas>` per
 * node, opens a MediaStream via getUserMedia on the rising edge of `start`,
 * draws each frame into the canvas, and hands that canvas out as the `image`
 * wire value. `stop`/dispose stop the tracks (turning the camera light off).
 *
 * Device enumeration mirrors the MIDI registry: a cached, identity-stable
 * `deviceOptions()` for the per-node dropdown, rebuilt on `devicechange` and
 * after permission is granted (labels are blank until then, per browser rule).
 */

interface CameraChain {
  video: HTMLVideoElement;
  /** Capture surface handed out as the `image` value — a `CanvasImageSource`
   *  the GL renderer textures directly. 1×1 until the first frame arrives. */
  canvas: HTMLCanvasElement;
  stream: MediaStream | null;
  deviceId: string;
  /** A getUserMedia call is in flight — prevents re-launching every tick. */
  starting: boolean;
  /** Stream is live and playing. */
  started: boolean;
  error: string | null;
}

class CameraRegistry {
  private chains = new Map<string, CameraChain>();
  private deviceOptionsCache: ReadonlyArray<{ value: string; label: string }> = [];
  private listeners = new Set<() => void>();
  private enumerated = false;

  // ── Device enumeration (MIDI-registry pattern) ──────────────────────────

  /** Identity-stable `{value: deviceId, label}` list for the device dropdown.
   *  Lazily kicks off enumeration on first read. */
  deviceOptions(): ReadonlyArray<{ value: string; label: string }> {
    this.ensureEnumerated();
    return this.deviceOptionsCache;
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => { this.listeners.delete(fn); };
  }

  private ensureEnumerated(): void {
    if (this.enumerated) return;
    if (typeof navigator === 'undefined' || !navigator.mediaDevices?.enumerateDevices) return;
    this.enumerated = true;
    void this.refreshDevices();
    navigator.mediaDevices.addEventListener?.('devicechange', () => void this.refreshDevices());
  }

  private async refreshDevices(): Promise<void> {
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      const cams = devices.filter((d) => d.kind === 'videoinput');
      this.deviceOptionsCache = [
        { value: '', label: 'Default camera' },
        ...cams.map((d, i) => ({ value: d.deviceId, label: d.label || `Camera ${i + 1}` })),
      ];
      for (const l of this.listeners) l();
    } catch {
      /* enumeration can fail before permission; ignore */
    }
  }

  // ── Per-node capture ────────────────────────────────────────────────────

  /**
   * Drive a node's camera. On the rising edge of `start` (or a device change
   * while running) opens the stream; on the falling edge stops it. While live,
   * draws the current video frame into the node's canvas (mirrored if asked)
   * and returns it. Returns a 1×1 placeholder until the first frame.
   */
  ensure(nodeId: string, deviceId: string, start: boolean, mirror: boolean): HTMLCanvasElement {
    let chain = this.chains.get(nodeId);
    if (!chain) {
      chain = this.build();
      this.chains.set(nodeId, chain);
    }
    const deviceChanged = chain.deviceId !== deviceId;
    chain.deviceId = deviceId;

    if (start && !chain.started && !chain.starting) {
      void this.startStream(chain, deviceId);
    } else if (start && chain.started && deviceChanged) {
      this.stopStream(chain);
      void this.startStream(chain, deviceId);
    } else if (!start && (chain.started || chain.starting)) {
      this.stopStream(chain);
    }

    const v = chain.video;
    if (chain.started && v.readyState >= 2 && v.videoWidth > 0) {
      const c = chain.canvas;
      if (c.width !== v.videoWidth || c.height !== v.videoHeight) {
        c.width = v.videoWidth;
        c.height = v.videoHeight;
      }
      const ctx = c.getContext('2d');
      if (ctx) {
        ctx.save();
        if (mirror) { ctx.translate(c.width, 0); ctx.scale(-1, 1); }
        ctx.drawImage(v, 0, 0);
        ctx.restore();
      }
    }
    return chain.canvas;
  }

  isReady(nodeId: string): boolean {
    const c = this.chains.get(nodeId);
    return !!c && c.started && c.video.videoWidth > 0;
  }

  error(nodeId: string): string | null {
    return this.chains.get(nodeId)?.error ?? null;
  }

  private build(): CameraChain {
    const video = document.createElement('video');
    video.muted = true;
    video.playsInline = true;
    video.autoplay = true;
    const canvas = document.createElement('canvas');
    canvas.width = 1;
    canvas.height = 1;
    return { video, canvas, stream: null, deviceId: '', starting: false, started: false, error: null };
  }

  private async startStream(chain: CameraChain, deviceId: string): Promise<void> {
    if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia) {
      chain.error = 'Camera API unavailable';
      return;
    }
    chain.starting = true;
    chain.error = null;
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        video: deviceId ? { deviceId: { exact: deviceId } } : true,
        audio: false,
      });
      chain.stream = stream;
      chain.video.srcObject = stream;
      await chain.video.play().catch(() => { /* autoplay quirks — frames still arrive */ });
      chain.started = true;
      // Labels become available once permission is granted.
      void this.refreshDevices();
    } catch (err) {
      chain.error = err instanceof Error ? err.message : String(err);
      chain.started = false;
      // eslint-disable-next-line no-console
      console.warn('camera start failed:', err);
    } finally {
      chain.starting = false;
    }
  }

  private stopStream(chain: CameraChain): void {
    if (chain.stream) {
      for (const track of chain.stream.getTracks()) {
        try { track.stop(); } catch { /* ignore */ }
      }
      chain.stream = null;
    }
    try { chain.video.srcObject = null; } catch { /* ignore */ }
    chain.started = false;
    chain.starting = false;
  }

  // ── Lifecycle (image/audio-registry pattern) ────────────────────────────

  dispose(nodeId: string): void {
    const c = this.chains.get(nodeId);
    if (!c) return;
    this.stopStream(c);
    this.chains.delete(nodeId);
  }

  retain(liveNodeIds: ReadonlySet<string>): void {
    for (const [id, c] of this.chains) {
      if (liveNodeIds.has(id)) continue;
      this.stopStream(c);
      this.chains.delete(id);
    }
  }

  reset(): void {
    for (const c of this.chains.values()) this.stopStream(c);
    this.chains.clear();
  }
}

export const cameraRegistry = new CameraRegistry();

if (typeof window !== 'undefined') {
  (window as unknown as { __cameraRegistry?: unknown }).__cameraRegistry = cameraRegistry;
}
