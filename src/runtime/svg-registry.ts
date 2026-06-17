/**
 * SVG registry.
 *
 * Per-node-id cache of an SVG parsed into a Floret `layer` (a `group`
 * SceneNode of `polygon` shapes). Mirrors the image registry lifecycle:
 * `ensure()` parses on first sight of a source and reuses the result
 * across frames; a source change re-parses; `dispose`/`retain`/`reset`
 * clean up.
 *
 * Parsing leans on the BROWSER'S OWN SVG ENGINE rather than a hand-rolled
 * path-grammar parser: each geometry element is flattened to a polyline
 * via `getTotalLength()` + `getPointAtLength()` (so béziers/arcs/any `d`
 * grammar "just work"), its paint resolved via `getComputedStyle`, and
 * its ancestor transforms + viewBox baked in via `getScreenCTM()`. The
 * result is centred on the origin (Floret renders centre-origin).
 *
 * Deliberate v1 limits — the vector model can't express them: curves
 * become polylines (Floret polygons have no native béziers); even-odd
 * HOLES fill solid (each subpath is its own filled polygon); no
 * gradients, text, clip-paths or embedded raster.
 */

import { markDirty } from './dirty';
import { DEFAULT_STYLE, type ResolvedStyle, type SceneNode } from './scene';
import type { Vec2 } from './types';

interface SvgEntry {
  /** The source string this entry was parsed from (markup, data: URL, or
   *  remote URL) — re-parse only when this changes. */
  key: string;
  layer: SceneNode | null;
  width: number;
  height: number;
  loaded: boolean;
  error: string | null;
}

/** Max points sampled per subpath — keeps a giant rect/curve from
 *  exploding into thousands of collinear vertices. */
const MAX_SAMPLES = 256;
/** Approx. user-units between samples along a flattened outline. */
const SAMPLE_STEP = 2;

class SvgRegistry {
  private entries = new Map<string, SvgEntry>();
  /** Hidden, rendered host for getScreenCTM/getTotalLength (need layout). */
  private host: HTMLElement | null = null;

  /**
   * Drive a node's SVG. Returns the cached entry, kicking off a parse
   * when the source changed. For inline sources (raw markup or a
   * `data:` URL) the parse is synchronous and the entry is `loaded`
   * before this returns; for a remote URL the fetch is async and the
   * entry flips to `loaded` later (waking a tick via markDirty).
   */
  ensure(nodeId: string, key: string, isUrl: boolean): SvgEntry {
    let entry = this.entries.get(nodeId);
    if (entry && entry.key === key) return entry;

    entry = { key, layer: null, width: 0, height: 0, loaded: false, error: null };
    this.entries.set(nodeId, entry);

    if (isUrl) {
      this.loadUrl(nodeId, key, entry);
    } else {
      // Inline: a data: URL we decode, or raw <svg> markup as-is.
      const text = key.startsWith('data:') ? decodeDataUrl(key) : key;
      this.parseInto(entry, text);
    }
    return entry;
  }

  isLoaded(nodeId: string): boolean {
    return !!this.entries.get(nodeId)?.loaded;
  }

  error(nodeId: string): string | null {
    return this.entries.get(nodeId)?.error ?? null;
  }

  private async loadUrl(nodeId: string, url: string, entry: SvgEntry): Promise<void> {
    try {
      const res = await fetch(url);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const text = await res.text();
      // The node may have moved on to a different source while we waited.
      if (this.entries.get(nodeId) !== entry) return;
      this.parseInto(entry, text);
      markDirty(); // wake a tick so a static patch shows the result
    } catch (err) {
      entry.error = err instanceof Error ? err.message : String(err);
      entry.loaded = false;
    }
  }

  private parseInto(entry: SvgEntry, svgText: string): void {
    try {
      const parsed = parseSvg(svgText, this.getHost());
      entry.layer = parsed.layer;
      entry.width = parsed.width;
      entry.height = parsed.height;
      entry.loaded = true;
      entry.error = null;
    } catch (err) {
      entry.error = err instanceof Error ? err.message : String(err);
      entry.loaded = false;
      entry.layer = null;
    }
  }

  private getHost(): HTMLElement {
    if (this.host && this.host.isConnected) return this.host;
    const host = document.createElement('div');
    host.setAttribute('aria-hidden', 'true');
    // Off-screen but rendered (NOT display:none) so getScreenCTM /
    // getTotalLength have layout. visibility:hidden keeps it invisible.
    host.style.cssText = 'position:absolute;left:-99999px;top:0;visibility:hidden;';
    document.body.appendChild(host);
    this.host = host;
    return host;
  }

  // ── Lifecycle (image/camera-registry pattern) ───────────────────────────

  dispose(nodeId: string): void {
    this.entries.delete(nodeId);
  }

  retain(liveNodeIds: ReadonlySet<string>): void {
    for (const id of this.entries.keys()) {
      if (!liveNodeIds.has(id)) this.entries.delete(id);
    }
  }

  reset(): void {
    this.entries.clear();
  }
}

// ── Parsing ────────────────────────────────────────────────────────────────

const GEOMETRY = 'path,rect,circle,ellipse,line,polygon,polyline';

function parseSvg(
  svgText: string,
  host: HTMLElement,
): { layer: SceneNode; width: number; height: number } {
  // Parse via innerHTML so the HTML parser builds properly-namespaced
  // SVG elements (DOMParser elements adopted into the HTML doc lose their
  // SVG prototype → no getScreenCTM/getTotalLength).
  host.innerHTML = svgText;
  const svg = host.querySelector('svg') as SVGSVGElement | null;
  if (!svg) { host.innerHTML = ''; throw new Error('No <svg> root'); }
  try {
    const { x: vbX, y: vbY, w: vbW, h: vbH } = readViewBox(svg);
    const cx = vbX + vbW / 2;
    const cy = vbY + vbH / 2;

    const rootMatrix = svg.getScreenCTM();
    const rootInv = rootMatrix ? rootMatrix.inverse() : null;

    const children: SceneNode[] = [];
    const elements = svg.querySelectorAll(GEOMETRY);
    for (const el of Array.from(elements)) {
      const geo = el as SVGGeometryElement;
      // Map element user-space → root (viewBox) user-space, baking in every
      // ancestor <g> transform. Falls back to identity if CTMs unavailable.
      const screen = geo.getScreenCTM();
      const toRoot = rootInv && screen ? rootInv.multiply(screen) : null;
      const style = resolvePaint(geo);

      // getScreenCTM yields a legacy SVGMatrix in some engines (no
      // transformPoint), so apply the affine via its a..f components —
      // present on both SVGMatrix and DOMMatrix.
      const m = toRoot;
      for (const sub of flatten(geo)) {
        const pts: Vec2[] = sub.points.map((p) => {
          const x = m ? m.a * p.x + m.c * p.y + m.e : p.x;
          const y = m ? m.b * p.x + m.d * p.y + m.f : p.y;
          return { x: x - cx, y: y - cy };
        });
        if (pts.length < 2) continue;
        children.push({ kind: 'polygon', points: pts, closed: sub.closed, style });
      }
    }

    return { layer: { kind: 'group', children }, width: vbW, height: vbH };
  } finally {
    host.innerHTML = '';
  }
}

function readViewBox(svg: SVGSVGElement): { x: number; y: number; w: number; h: number } {
  const vb = svg.viewBox?.baseVal;
  if (vb && vb.width > 0 && vb.height > 0) {
    return { x: vb.x, y: vb.y, w: vb.width, h: vb.height };
  }
  const w = parseFloat(svg.getAttribute('width') ?? '') || 100;
  const h = parseFloat(svg.getAttribute('height') ?? '') || 100;
  return { x: 0, y: 0, w, h };
}

/** Flatten one geometry element to one-or-more polylines (subpaths). */
function flatten(geo: SVGGeometryElement): Array<{ points: Vec2[]; closed: boolean }> {
  const tag = geo.tagName.toLowerCase();
  // Multi-subpath <path>: split on move commands so disjoint pieces / holes
  // don't get joined by a stray segment. Each temp path inherits the same
  // parent (→ same transform context) for sampling.
  if (tag === 'path') {
    const d = geo.getAttribute('d') ?? '';
    const subs = splitSubpaths(d);
    if (subs.length <= 1) {
      return [{ points: sample(geo), closed: /[zZ]/.test(d) }];
    }
    const out: Array<{ points: Vec2[]; closed: boolean }> = [];
    const parent = geo.parentNode;
    for (const sd of subs) {
      const tmp = geo.cloneNode(false) as SVGGeometryElement;
      tmp.setAttribute('d', sd);
      parent?.appendChild(tmp);
      try {
        out.push({ points: sample(tmp), closed: /[zZ]/.test(sd) });
      } finally {
        parent?.removeChild(tmp);
      }
    }
    return out;
  }
  const open = tag === 'line' || tag === 'polyline';
  return [{ points: sample(geo), closed: !open }];
}

/** Sample points along an element's outline via the browser's geometry API. */
function sample(geo: SVGGeometryElement): Vec2[] {
  let len = 0;
  try {
    len = geo.getTotalLength();
  } catch {
    return [];
  }
  if (!(len > 0)) return [];
  const n = Math.max(2, Math.min(MAX_SAMPLES, Math.ceil(len / SAMPLE_STEP)));
  const pts: Vec2[] = new Array(n);
  for (let i = 0; i < n; i++) {
    const p = geo.getPointAtLength((i / (n - 1)) * len);
    pts[i] = { x: p.x, y: p.y };
  }
  return pts;
}

/** Split a path `d` string into subpaths at each move (M/m) command. */
function splitSubpaths(d: string): string[] {
  const matches = d.match(/[Mm][^Mm]*/g);
  return matches ? matches.map((s) => s.trim()).filter(Boolean) : [];
}

/** Read an element's resolved fill / stroke into a ResolvedStyle. */
function resolvePaint(geo: SVGGeometryElement): ResolvedStyle {
  const cs = getComputedStyle(geo);
  const globalAlpha = clamp01(num(cs.opacity, 1));
  const fillAlpha = clamp01(num(cs.fillOpacity, 1)) * globalAlpha;
  const strokeAlpha = clamp01(num(cs.strokeOpacity, 1)) * globalAlpha;

  const fill = paint(cs.fill, fillAlpha, '#000000');
  const stroke = paint(cs.stroke, strokeAlpha, null);
  const strokeWeight = num(cs.strokeWidth, 1);

  return {
    ...DEFAULT_STYLE,
    fill,
    stroke,
    strokeWeight: strokeWeight > 0 ? strokeWeight : 1,
  };
}

/** Resolve one paint value to a CSS colour string (or null for "none"). */
function paint(value: string, alpha: number, fallback: string | null): string | null {
  const v = (value || '').trim();
  if (!v || v === 'none' || v === 'transparent') return v === '' ? fallback : null;
  if (alpha >= 0.999) return v;
  return applyAlpha(v, alpha);
}

/** Multiply an existing alpha into an `rgb()/rgba()` string. */
function applyAlpha(css: string, alpha: number): string {
  const m = css.match(/rgba?\(([^)]+)\)/i);
  if (!m) return css;
  const parts = m[1]!.split(',').map((s) => s.trim());
  const [r, g, b] = parts;
  const a = parts.length >= 4 ? num(parts[3]!, 1) : 1;
  return `rgba(${r}, ${g}, ${b}, ${a * alpha})`;
}

function num(v: string, fallback: number): number {
  const n = parseFloat(v);
  return Number.isFinite(n) ? n : fallback;
}

function clamp01(v: number): number {
  return v < 0 ? 0 : v > 1 ? 1 : v;
}

function decodeDataUrl(dataUrl: string): string {
  const comma = dataUrl.indexOf(',');
  if (comma < 0) return '';
  const meta = dataUrl.slice(5, comma); // strip "data:"
  const payload = dataUrl.slice(comma + 1);
  try {
    if (/;base64/i.test(meta)) return atob(payload);
    return decodeURIComponent(payload);
  } catch {
    return '';
  }
}

export const svgRegistry = new SvgRegistry();

if (typeof window !== 'undefined') {
  (window as unknown as { __svgRegistry?: unknown }).__svgRegistry = svgRegistry;
}
