/**
 * Dev-only: publish the live render frame to the e-ink relay (the Vite
 * middleware in eink-relay.vite.mjs), which a Raspberry Pi pulls and pushes
 * to a Waveshare e-paper panel. See tools/eink/.
 *
 * E-ink refreshes every few minutes, so this just grabs the render canvas
 * at a slow interval and POSTs it as a PNG. The canvas backing store is the
 * renderer's native resolution; the Pi resizes + dithers to the panel.
 *
 * No UI yet — drive it from the console:
 *   __floretEink.start()                 // default: every 60s
 *   __floretEink.start({ intervalMs: 30000 })
 *   __floretEink.stop()
 */

export interface EinkPublishOptions {
  /** Publish cadence in ms (min 2000). Default 60000. */
  intervalMs?: number;
  /** Relay POST endpoint. Default '/__floret/frame'. */
  endpoint?: string;
}

let timer: number | null = null;

/** The first mounted preview's render canvas (driver appends it to .preview-canvas). */
function findRenderCanvas(): HTMLCanvasElement | null {
  return (
    document.querySelector<HTMLCanvasElement>('.preview-canvas canvas') ??
    document.querySelector<HTMLCanvasElement>('canvas')
  );
}

export function startEinkPublisher(opts: EinkPublishOptions = {}): void {
  const intervalMs = Math.max(2000, opts.intervalMs ?? 60_000);
  const endpoint = opts.endpoint ?? '/__floret/frame';
  stopEinkPublisher();

  const publish = async (): Promise<void> => {
    const canvas = findRenderCanvas();
    if (!canvas) return;
    const blob: Blob | null = await new Promise((resolve) =>
      canvas.toBlob((b) => resolve(b), 'image/png'),
    );
    if (!blob) return;
    try {
      await fetch(endpoint, { method: 'POST', headers: { 'Content-Type': 'image/png' }, body: blob });
    } catch {
      // Relay absent (e.g. a production build) — silently skip.
    }
  };

  void publish();
  timer = window.setInterval(() => void publish(), intervalMs);
  // eslint-disable-next-line no-console
  console.info(`[floret-eink] publishing render every ${Math.round(intervalMs / 1000)}s → ${endpoint}`);
}

export function stopEinkPublisher(): void {
  if (timer != null) {
    clearInterval(timer);
    timer = null;
    // eslint-disable-next-line no-console
    console.info('[floret-eink] stopped');
  }
}

// Manual console control until there's a UI toggle.
if (typeof window !== 'undefined') {
  (window as unknown as { __floretEink?: unknown }).__floretEink = {
    start: startEinkPublisher,
    stop: stopEinkPublisher,
  };
}
