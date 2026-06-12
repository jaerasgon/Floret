import type { FloretEdge, FloretNode } from '../editor/editor-state';
import { compileGraph } from '../runtime/compile';
import { tick } from '../runtime/tick';
import { renderScene } from '../runtime/scene-renderer';
import { colorToCss, type SceneNode } from '../runtime/scene';
import type { Color } from '../runtime/types';
import { nodeRegistry } from '../nodes/registry';
import { parseHandle } from '../editor/connection-rules';
import { loadJSON, saveJSON } from './local-store';

/**
 * Headless thumbnail rendering. Compiles a patch off-screen, ticks a few
 * warmup frames (so stateful/particle patches settle into something
 * representative), renders the resulting scene onto a small canvas, and
 * returns a PNG data URL. Mirrors the render worker's recipe exactly —
 * solid background, centre-origin translate, then `renderScene` per layer
 * — so a thumbnail matches what the live Preview shows.
 *
 * Results are cached in memory and localStorage keyed by a hash of the
 * patch, so the 81 built-in samples render once and stick. Generation is
 * opportunistic (callers fire it from an idle callback) — the grid view
 * shows a tinted placeholder until the real image resolves.
 */

const RENDER_SIZE = 320;     // square off-screen render resolution
const WARMUP_FRAMES = 12;
const CACHE_LS_KEY = 'floret.thumbs.v1';

const memCache = new Map<string, string>();
let diskCache: Record<string, string> | null = null;

function disk(): Record<string, string> {
  if (!diskCache) diskCache = loadJSON<Record<string, string>>(CACHE_LS_KEY, {});
  return diskCache;
}

/** Cheap stable hash of a patch's structure for cache keying. */
export function patchHash(nodes: ReadonlyArray<FloretNode>, edges: ReadonlyArray<FloretEdge>): string {
  let h = 2166136261;
  const mix = (s: string) => {
    for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); }
  };
  for (const n of nodes) { mix(n.id); mix(n.data.defType); mix(JSON.stringify(n.data.params ?? {})); }
  for (const e of edges) { mix(e.source); mix(e.target); mix(e.sourceHandle ?? ''); mix(e.targetHandle ?? ''); }
  return (h >>> 0).toString(36);
}

function getCached(key: string): string | null {
  if (memCache.has(key)) return memCache.get(key)!;
  const d = disk()[key];
  if (d) { memCache.set(key, d); return d; }
  return null;
}

function setCached(key: string, dataUrl: string): void {
  memCache.set(key, dataUrl);
  const d = disk();
  d[key] = dataUrl;
  saveJSON(CACHE_LS_KEY, d);
}

function flatten(layers: ReadonlyArray<SceneNode | SceneNode[]>): SceneNode[] {
  const out: SceneNode[] = [];
  for (const l of layers) {
    if (Array.isArray(l)) { for (const n of l) if (n) out.push(n); }
    else if (l) out.push(l);
  }
  return out;
}

/**
 * Render a patch to a PNG data URL, or null if it has no Renderer (e.g.
 * an audio-only patch) or fails to produce a scene. Cached by patch hash.
 */
export async function renderPatchThumbnail(
  nodes: ReadonlyArray<FloretNode>,
  edges: ReadonlyArray<FloretEdge>,
  thumbW = 160,
  thumbH = 100,
): Promise<string | null> {
  const key = patchHash(nodes, edges);
  const cached = getCached(key);
  if (cached) return cached;

  try {
    const graph = compileGraph(
      nodes.map((n) => ({
        id: n.id, defType: n.data.defType, params: n.data.params,
        parentId: n.parentId, ports: n.data.ports,
      })),
      edges
        .map((e) => {
          const src = parseHandle(e.sourceHandle);
          const dst = parseHandle(e.targetHandle);
          if (!src || !dst) return null;
          return { fromNode: e.source, fromPort: src.name, toNode: e.target, toPort: dst.name };
        })
        .filter((x): x is NonNullable<typeof x> => x !== null),
      {
        registry: nodeRegistry,
        paramReader: (id) => nodes.find((n) => n.id === id)?.data.params ?? {},
        portValueReader: (id, port) => nodes.find((n) => n.id === id)?.data.portValues?.[port],
      },
    );
    const rendererId = graph.graph.rendererId;
    if (!rendererId) return null;

    let layers: SceneNode[] = [];
    let bg = '#0a0a12';
    for (let i = 0; i < WARMUP_FRAMES; i++) {
      const result = tick(graph.graph, {
        t: i * 0.016, dt: 0.016, frameCount: i, width: RENDER_SIZE, height: RENDER_SIZE,
        mouseX: RENDER_SIZE / 2, mouseY: RENDER_SIZE / 2, mouseIsPressed: false, key: '', keyIsPressed: false,
      });
      const raw = (result.outputs.get(rendererId)?.[1] as ReadonlyArray<unknown> | undefined)?.[0];
      if (raw && typeof raw === 'object' && 'r' in raw) bg = colorToCss(raw as Color);
      layers = flatten((result.layersByRenderer.get(rendererId) ?? result.layers) as ReadonlyArray<SceneNode | SceneNode[]>);
    }

    // Render at full size, then downscale to the thumbnail for antialiasing.
    const full = document.createElement('canvas');
    full.width = RENDER_SIZE; full.height = RENDER_SIZE;
    const fctx = full.getContext('2d');
    if (!fctx) return null;
    fctx.fillStyle = bg;
    fctx.fillRect(0, 0, RENDER_SIZE, RENDER_SIZE);
    fctx.translate(RENDER_SIZE / 2, RENDER_SIZE / 2);
    for (const layer of layers) {
      fctx.save();
      try { renderScene(fctx, layer); } catch { /* one bad layer shouldn't kill the thumb */ }
      fctx.restore();
    }

    const thumb = document.createElement('canvas');
    thumb.width = thumbW; thumb.height = thumbH;
    const tctx = thumb.getContext('2d');
    if (!tctx) return null;
    tctx.imageSmoothingEnabled = true;
    tctx.imageSmoothingQuality = 'high';
    // Cover-fit the square render into the thumbnail rect.
    const scale = Math.max(thumbW / RENDER_SIZE, thumbH / RENDER_SIZE);
    const dw = RENDER_SIZE * scale, dh = RENDER_SIZE * scale;
    tctx.drawImage(full, (thumbW - dw) / 2, (thumbH - dh) / 2, dw, dh);

    const dataUrl = thumb.toDataURL('image/png');
    setCached(key, dataUrl);
    return dataUrl;
  } catch {
    return null;
  }
}
