import { registerNode } from '../registry';
import { renderScene } from '../../runtime/scene-renderer';
import { colorToCss, type Color, type PortValue } from '../../runtime/types';
import { isReadyImageCanvas } from '../_shared/coerce';
import { markDirty } from '../../runtime/dirty';
import type { SceneNode } from '../../runtime/scene';
import type { NodeParamContext } from '../types';

/**
 * Generic Send / Receive over the in-app channel bus (a dev-server relay,
 * see floret-bus.vite.mjs). The browser POSTs/polls same-origin localhost, so
 * macOS/CORS LAN gates never apply; anything else on the LAN (another Floret
 * tab, a Raspberry Pi, …) pulls the channel.
 *
 * Payloads are auto-typed:
 *   - layers (SceneNodes) → rasterised to a PNG (width × height, background)
 *   - an image (camera/raster) → PNG
 *   - any other value (number / vec2 / color / string / spread) → JSON
 *
 * This replaces the e-ink-specific node: the e-ink flow is now just
 * `layers → Send(channel:"eink")` with a Pi polling that channel.
 */

const BUS = '/__floret/bus/';

function isSceneNode(v: unknown): v is SceneNode {
  return !!v && typeof v === 'object' && typeof (v as { kind?: unknown }).kind === 'string';
}

// ─── Send ──────────────────────────────────────────────────────────────────

interface SendState { canvas: HTMLCanvasElement | null; prevSend: boolean; lastSentMs: number; busy: boolean; ok: boolean; }
const sendState = new Map<string, SendState>();

export const sendNode = registerNode({
  type: 'io/send',
  category: 'IO',
  label: 'Send',
  description: 'Transmit data on a named channel — POSTs to the in-app bus on the `send` edge (and every `interval`s). Values go as JSON; layers/images as a PNG. Set `channel`; the receiver (another Floret, a Pi, …) pulls it.',
  effectful: true,
  inputs: [
    { name: 'data', type: 'any', kind: 'spread' },
    { name: 'channel', type: 'string', default: 'main', param: 'channel' },
    { name: 'send', type: 'boolean', default: false },
    { name: 'interval', type: 'number', default: 0, param: 'interval' },
    { name: 'width', type: 'number', default: 540, param: 'width' },
    { name: 'height', type: 'number', default: 540, param: 'height' },
    { name: 'background', type: 'color', default: { r: 1, g: 1, b: 1, a: 1 } },
  ],
  outputs: [{ name: 'ok', type: 'boolean' }],
  defaultParams: { channel: 'main', interval: 0, width: 540, height: 540 },
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const id = params.nodeId;
    let st = sendState.get(id);
    if (!st) { st = { canvas: null, prevSend: false, lastSentMs: 0, busy: false, ok: false }; sendState.set(id, st); }

    const channel = String(params.param<string>('channel', 'main') ?? 'main').trim() || 'main';
    const interval = Math.max(0, Number(params.param<number>('interval', 0)) || 0);
    const w = Math.max(1, Math.floor(Number(params.param<number>('width', 540)) || 540));
    const h = Math.max(1, Math.floor(Number(params.param<number>('height', 540)) || 540));
    const send = !!(inputs[2] ?? [])[0];
    const bg = ((inputs[6] ?? [])[0] as Color | null) ?? { r: 1, g: 1, b: 1, a: 1 };
    const data = (inputs[0] ?? []) as ReadonlyArray<unknown>;

    const now = typeof performance !== 'undefined' ? performance.now() : Date.now();
    const rising = send && !st.prevSend;
    st.prevSend = send;
    const due = interval > 0 && now - st.lastSentMs >= interval * 1000;
    if (!((rising || due) && !st.busy) || typeof document === 'undefined') return [[st.ok]];

    st.lastSentMs = now;

    // Classify the payload.
    const layers: SceneNode[] = [];
    for (const item of data) {
      if (Array.isArray(item)) { for (const s of item) if (isSceneNode(s)) layers.push(s); }
      else if (isSceneNode(item)) layers.push(item);
    }
    const firstImage = data.length && isReadyImageCanvas(data[0]) ? data[0] as CanvasImageSource & { width: number; height: number } : null;

    const post = (body: BodyInit, type: string) => {
      st!.busy = true;
      fetch(BUS + encodeURIComponent(channel), { method: 'POST', headers: { 'Content-Type': type }, body })
        .then((r) => { st!.ok = r.ok; })
        .catch(() => { st!.ok = false; })
        .finally(() => { st!.busy = false; });
    };

    if (layers.length || firstImage) {
      // Render to PNG.
      if (!st.canvas) st.canvas = document.createElement('canvas');
      const canvas = st.canvas;
      if (canvas.width !== w) canvas.width = w;
      if (canvas.height !== h) canvas.height = h;
      const ctx = canvas.getContext('2d');
      if (!ctx) return [[st.ok]];
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.fillStyle = colorToCss(bg);
      ctx.fillRect(0, 0, w, h);
      if (firstImage) {
        const sw = firstImage.width, sh = firstImage.height;
        const scale = Math.min(w / sw, h / sh);
        try { ctx.drawImage(firstImage, (w - sw * scale) / 2, (h - sh * scale) / 2, sw * scale, sh * scale); } catch { /* tainted */ }
      } else {
        ctx.translate(w / 2, h / 2);
        for (const layer of layers) { ctx.save(); try { renderScene(ctx, layer); } finally { ctx.restore(); } }
      }
      canvas.toBlob((blob) => { if (blob) post(blob, 'image/png'); else { st!.busy = false; } }, 'image/png');
    } else {
      // Any other value → JSON (round-trips the wire's PortValue).
      let json: string;
      try { json = JSON.stringify(data); } catch { json = '[]'; }
      post(json, 'application/json');
    }
    return [[st.ok]];
  },
});

// ─── Receive ─────────────────────────────────────────────────────────────────

interface RecvState { value: PortValue; version: number; lastPoll: number; busy: boolean; }
const recvState = new Map<string, RecvState>();

export const receiveNode = registerNode({
  type: 'io/receive',
  category: 'IO',
  label: 'Receive',
  description: 'Receive data from a named channel — polls the in-app bus and outputs the latest value. JSON arrives as a value/spread; a PNG arrives as an image. Set `channel` to match a Send.',
  effectful: true,
  inputs: [
    { name: 'channel', type: 'string', default: 'main', param: 'channel' },
    { name: 'interval', type: 'number', default: 0.5, param: 'interval' },
  ],
  outputs: [{ name: 'data', type: 'any' }],
  defaultParams: { channel: 'main', interval: 0.5 },
  computeRaw: (_inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const id = params.nodeId;
    let st = recvState.get(id);
    if (!st) { st = { value: [], version: -1, lastPoll: 0, busy: false }; recvState.set(id, st); }
    if (typeof fetch === 'undefined') return [st.value];

    const channel = String(params.param<string>('channel', 'main') ?? 'main').trim() || 'main';
    const interval = Math.max(0.05, Number(params.param<number>('interval', 0.5)) || 0.5);
    const now = typeof performance !== 'undefined' ? performance.now() : Date.now();

    if (!st.busy && now - st.lastPoll >= interval * 1000) {
      st.busy = true; st.lastPoll = now;
      const base = BUS + encodeURIComponent(channel);
      fetch(base + '/poll')
        .then((r) => r.json())
        .then((p: { version: number; has: boolean; contentType: string | null }) => {
          if (!p.has || p.version === st!.version) return null;
          st!.version = p.version;
          return fetch(base).then(async (r) => {
            const ct = r.headers.get('content-type') || '';
            if (ct.includes('json')) {
              const v = await r.json();
              st!.value = Array.isArray(v) ? (v as PortValue) : [v];
            } else if (ct.includes('image') && typeof createImageBitmap === 'function') {
              const bmp = await createImageBitmap(await r.blob());
              const c = document.createElement('canvas');
              c.width = bmp.width; c.height = bmp.height;
              c.getContext('2d')?.drawImage(bmp, 0, 0);
              st!.value = [c];
            } else {
              st!.value = [await r.text()];
            }
          });
        })
        .catch(() => { /* relay absent / not yet published */ })
        .finally(() => { st!.busy = false; markDirty(); });
    }
    return [st.value];
  },
});
