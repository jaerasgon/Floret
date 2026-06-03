import { registerNode } from '../registry';
import { imageRegistry } from '../../runtime/image-registry';
import { FileNode } from '../../editor/nodes/FileNode';
import type { NodeParamContext, PortValue } from '../types';

/**
 * File — pick an image / audio / any binary file from disk and expose it
 * to the patch.
 *
 * Browsers can't read filesystem paths typed by a user (security); they
 * can only see file bytes selected through a file picker or drag-and-
 * drop. So this node is built around a custom React component that shows
 * a "Pick file…" button + a drag-and-drop landing zone. When a file is
 * chosen we read it as a base64 data URL and stash it in the node's
 * params, so the picked file survives reload / save without depending
 * on a blob: URL that gets revoked.
 *
 * Outputs:
 *   - `url`      data URL of the picked file (empty string when none)
 *   - `image`    if the file's MIME type begins with `image/`, this is
 *                the loaded image (via imageRegistry); otherwise null
 *   - `name`     filename the user picked (no path; browsers strip it)
 *   - `loaded`   true once the file's bytes are decoded and (for images)
 *                the image has finished decoding into a canvas
 *   - `type`     MIME type string (e.g. `image/png`, `audio/mpeg`)
 *
 * For audio: wire `url` into `sound/sound-file.url` (which is now both
 * a param AND an input — wire wins when connected).
 */
export const fileNode = registerNode({
  type: 'io/file',
  category: 'IO',
  label: 'File',
  description: 'Pick a file from disk — outputs a URL and image handle.',
  inputs: [],
  outputs: [
    { name: 'url',    type: 'string' },
    { name: 'image',  type: 'image' },
    { name: 'name',   type: 'string' },
    { name: 'loaded', type: 'boolean' },
    { name: 'type',   type: 'string' },
  ],
  defaultParams: {
    // Persisted file payload — base64 data URL. Heavy for large files
    // (the entire blob is encoded in the patch JSON) but survives reload.
    dataUrl: '',
    filename: '',
    mimeType: '',
  },
  component: FileNode,
  readFromContext: (_frame, params: NodeParamContext): PortValue[] => {
    const dataUrl = params.param<string>('dataUrl', '').trim();
    const filename = params.param<string>('filename', '');
    const mimeType = params.param<string>('mimeType', '');

    if (!dataUrl) {
      return [[''], [null], [filename], [false], [mimeType]];
    }

    // Image handle — only populate when the file actually IS an image.
    // For other types (audio, video, plain binary) the image output
    // stays null and the user should route via `url` instead.
    if (mimeType.startsWith('image/')) {
      const canvas = imageRegistry.ensure(params.nodeId, dataUrl);
      const loaded = imageRegistry.isLoaded(params.nodeId);
      return [[dataUrl], [canvas], [filename], [loaded], [mimeType]];
    }

    // Non-image file: data URL is ready immediately; image is null.
    return [[dataUrl], [null], [filename], [true], [mimeType]];
  },
});
