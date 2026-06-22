import { registerNode } from '../registry';
import { renderScene } from '../../runtime/scene-renderer';
import { colorToCss, type Color, type PortValue } from '../../runtime/types';
import type { SceneNode } from '../../runtime/scene';
import type { NodeParamContext } from '../types';

/**
 * io/eink — send the wired drawing to a networked e-ink panel.
 *
 * Renders its `layers` exactly like the Renderer (centre-origin, bg fill)
 * to an offscreen canvas, encodes a PNG, and HTTP-POSTs it to a small
 * receiver on the Pi (which fits / dithers / rotates for the panel). The
 * panel becomes a first-class output of the patch — update it by editing
 * the graph.
 *
 * E-ink is slow and wears with refreshes, so sends are gated:
 *   - rising edge of `send` (wire a button / key) → push one frame now;
 *   - `interval` > 0 → also auto-push at most every `interval` seconds.
 * A single in-flight request at a time (the panel takes seconds to draw);
 * overlapping sends are dropped rather than queued.
 */

interface EinkState {
  canvas: HTMLCanvasElement | null;
  prevSend: boolean;
  prevClear: boolean;
  lastSentMs: number;
  busy: boolean;
  ok: boolean;
}
const states = new Map<string, EinkState>();

function isSceneNode(v: unknown): v is SceneNode {
  return !!v && typeof v === 'object' && typeof (v as { kind?: unknown }).kind === 'string';
}

export const einkNode = registerNode({
  type: 'io/eink',
  category: 'IO',
  label: 'E-ink Send',
  description:
    'Send the wired layers to a networked e-ink panel. Renders the layers and POSTs the PNG to the e-ink relay (the Pi pulls from it). Pushes on the `send` rising edge, and optionally every `interval` seconds. `url` defaults to the same-origin relay; `clear` blanks the panel.',
  effectful: true,
  inputs: [
    { name: 'layers', type: 'layer', kind: 'spread' },
    { name: 'url', type: 'string', default: '/__floret/eink/frame', param: 'url' },
    { name: 'send', type: 'boolean', default: false },
    { name: 'interval', type: 'number', default: 0, param: 'interval' },
    { name: 'width', type: 'number', default: 540, param: 'width' },
    { name: 'height', type: 'number', default: 540, param: 'height' },
    { name: 'background', type: 'color', default: { r: 1, g: 1, b: 1, a: 1 } },
    { name: 'clear', type: 'boolean', default: false },
  ],
  outputs: [{ name: 'ok', type: 'boolean' }],
  defaultParams: { url: '/__floret/eink/frame', interval: 0, width: 540, height: 540 },
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const id = params.nodeId;
    let st = states.get(id);
    if (!st) { st = { canvas: null, prevSend: false, prevClear: false, lastSentMs: 0, busy: false, ok: false }; states.set(id, st); }

    const url = String(params.param<string>('url', '') ?? '').trim();
    const interval = Math.max(0, Number(params.param<number>('interval', 0)) || 0);
    const w = Math.max(1, Math.floor(Number(params.param<number>('width', 540)) || 540));
    const h = Math.max(1, Math.floor(Number(params.param<number>('height', 540)) || 540));
    const send = !!(inputs[2] ?? [])[0];
    const bg = ((inputs[6] ?? [])[0] as Color | null) ?? { r: 1, g: 1, b: 1, a: 1 };
    const clear = !!(inputs[7] ?? [])[0];

    // Flatten the gathered layer input to a flat list of SceneNodes.
    const raw = (inputs[0] ?? []) as ReadonlyArray<unknown>;
    const layers: SceneNode[] = [];
    for (const item of raw) {
      if (Array.isArray(item)) { for (const s of item) if (isSceneNode(s)) layers.push(s); }
      else if (isSceneNode(item)) layers.push(item);
    }

    // Gate: rising edge of `send`, or throttled interval. `clear` is its own
    // rising-edge trigger that blanks the panel (takes priority over a send).
    const now = typeof performance !== 'undefined' ? performance.now() : Date.now();
    const risingSend = send && !st.prevSend;
    const risingClear = clear && !st.prevClear;
    st.prevSend = send;
    st.prevClear = clear;

    if (risingClear && !!url && !st.busy && typeof document !== 'undefined') {
      st.busy = true;
      // Derive the clear endpoint from the frame url (works for relative paths
      // and absolute URLs alike): …/frame → …/clear.
      const clearUrl = url.endsWith('/frame') ? `${url.slice(0, -'/frame'.length)}/clear` : url;
      fetch(clearUrl, { method: 'POST' })
        .then((r) => { st!.ok = r.ok; })
        .catch(() => { st!.ok = false; })
        .finally(() => { st!.busy = false; });
      return [[st.ok]];
    }

    const intervalDue = interval > 0 && now - st.lastSentMs >= interval * 1000;
    const shouldSend = (risingSend || intervalDue) && !!url && !st.busy;

    if (shouldSend && typeof document !== 'undefined') {
      st.lastSentMs = now;
      st.busy = true;

      // Render the layers exactly like the Renderer (render-worker): clear,
      // fill background, centre the origin, draw each layer.
      if (!st.canvas) st.canvas = document.createElement('canvas');
      const canvas = st.canvas;
      if (canvas.width !== w) canvas.width = w;
      if (canvas.height !== h) canvas.height = h;
      const ctx = canvas.getContext('2d');
      if (ctx) {
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.fillStyle = colorToCss(bg);
        ctx.fillRect(0, 0, w, h);
        ctx.translate(w / 2, h / 2);
        for (const layer of layers) {
          ctx.save();
          try { renderScene(ctx, layer); } finally { ctx.restore(); }
        }
        canvas.toBlob((blob) => {
          if (!blob) { st!.busy = false; st!.ok = false; return; }
          fetch(url, { method: 'POST', headers: { 'Content-Type': 'image/png' }, body: blob })
            .then((r) => { st!.ok = r.ok; })
            .catch(() => { st!.ok = false; })
            .finally(() => { st!.busy = false; });
        }, 'image/png');
      } else {
        st.busy = false;
      }
    }

    return [[st.ok]];
  },
});
