import { registerNode } from '../registry';
import { isReadyImageCanvas } from '../_shared/coerce';

/**
 * Read an image's pixel dimensions.
 *
 * `io/image-url` already exposes width/height on its own outputs, but
 * once an `image` wire passes through a `image/crop` or `image/resize`
 * those numbers no longer match the derived image. This node lets any
 * downstream patch sample the dims of whatever image it currently sees.
 *
 * Returns `0` while the source is still loading (the registry's 1×1
 * placeholder counts as "not loaded yet" — same rule as the draw nodes).
 */
export const imageDimsNode = registerNode({
  type: 'image/dims',
  category: 'Image',
  label: 'Dims',
  description: 'Read an image\'s pixel dimensions and aspect ratio.',
  inputs: [{ name: 'image', type: 'image' }],
  outputs: [
    { name: 'width', type: 'number' },
    { name: 'height', type: 'number' },
    { name: 'aspect', type: 'number' },
  ],
  compute: (image) => {
    if (!isReadyImageCanvas(image)) {
      return [0, 0, 0];
    }
    return [image.width, image.height, image.height > 0 ? image.width / image.height : 0];
  },
});
