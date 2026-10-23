import type { SceneNode } from '../runtime/scene';
import { sceneToSvg } from '../runtime/scene-svg';

/**
 * Output helpers — export the current (paused) frame as a file:
 *
 *   - PNG: a raster snapshot of the preview canvas (optionally upscaled
 *     for print/poster use).
 *   - SVG: a vector export built from the retained scene graph — sharp at
 *     any size, the right pick for line/shape-heavy patches.
 *
 * Both trigger a browser download; neither touches the network. Animated
 * or sequence output isn't here by design — that's expressed in the patch
 * with nodes, not the export button.
 */

function triggerDownload(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function dataUrlToBlob(dataUrl: string): Blob {
  const [head, b64] = dataUrl.split(',');
  const mime = /data:([^;]+)/.exec(head ?? '')?.[1] ?? 'application/octet-stream';
  const bin = atob(b64 ?? '');
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new Blob([bytes], { type: mime });
}

/**
 * Export the canvas as a PNG. `scale` > 1 upscales: we draw the source
 * canvas into a larger offscreen with smoothing off (crisp pixel-doubling)
 * — not a true higher-resolution re-render, but a clean way to get a
 * larger raster from a live sketch without re-running the graph.
 */
export function exportCanvasPng(canvas: HTMLCanvasElement, name: string, scale = 1): void {
  let source: HTMLCanvasElement = canvas;
  if (scale !== 1) {
    const off = document.createElement('canvas');
    off.width = Math.round(canvas.width * scale);
    off.height = Math.round(canvas.height * scale);
    const ctx = off.getContext('2d');
    if (ctx) {
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(canvas, 0, 0, off.width, off.height);
      source = off;
    }
  }
  triggerDownload(dataUrlToBlob(source.toDataURL('image/png')), `${name}.png`);
}

/** Export the current scene as a self-contained SVG document. */
export function exportSceneSvg(
  layers: ReadonlyArray<SceneNode>,
  width: number,
  height: number,
  background: string | null,
  name: string,
): void {
  const svg = sceneToSvg(layers, { width, height, background });
  triggerDownload(new Blob([svg], { type: 'image/svg+xml' }), `${name}.svg`);
}
