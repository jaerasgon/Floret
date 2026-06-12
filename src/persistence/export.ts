import type { SceneNode } from '../runtime/scene';
import { sceneToSvg } from '../runtime/scene-svg';

/**
 * Output helpers — get a sketch out of the editor as a file. Three forms:
 *
 *   - PNG: a raster snapshot of the live preview canvas (optionally
 *     upscaled for print/poster use).
 *   - SVG: a vector export built from the retained scene graph — sharp at
 *     any size, the right pick for line/shape-heavy patches.
 *   - WebM: a video capture of the running canvas over a fixed duration.
 *
 * All three trigger a browser download; none touch the network.
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

/**
 * Capture the canvas to a WebM video over `durationMs`. Resolves when the
 * recording stops and the file has been handed to the browser. Rejects if
 * MediaRecorder / captureStream isn't supported.
 */
export function exportCanvasWebm(
  canvas: HTMLCanvasElement,
  name: string,
  durationMs = 4000,
  fps = 30,
): Promise<void> {
  return new Promise((resolve, reject) => {
    if (typeof (canvas as { captureStream?: unknown }).captureStream !== 'function' ||
        typeof MediaRecorder === 'undefined') {
      reject(new Error('Video capture not supported in this browser.'));
      return;
    }
    // Prefer vp9, fall back to whatever the browser offers.
    const mime = ['video/webm;codecs=vp9', 'video/webm;codecs=vp8', 'video/webm']
      .find((t) => MediaRecorder.isTypeSupported(t)) ?? 'video/webm';
    const stream = (canvas as HTMLCanvasElement & { captureStream: (fps: number) => MediaStream })
      .captureStream(fps);
    const recorder = new MediaRecorder(stream, { mimeType: mime });
    const chunks: BlobPart[] = [];
    recorder.ondataavailable = (e) => { if (e.data.size > 0) chunks.push(e.data); };
    recorder.onerror = () => reject(new Error('Recording failed.'));
    recorder.onstop = () => {
      triggerDownload(new Blob(chunks, { type: mime }), `${name}.webm`);
      resolve();
    };
    recorder.start();
    setTimeout(() => recorder.stop(), durationMs);
  });
}
