import { registerNode } from '../registry';
import { cameraRegistry } from '../../runtime/camera-registry';
import type { PortValue } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * Camera — live webcam / connected-camera feed as an `image`.
 *
 * `device` is a param picked from a live dropdown of `videoinput` devices.
 * `start` is an edge-triggered boolean input: rising edge requests camera
 * permission and opens the stream; falling edge stops it (camera light off).
 * `mirror` flips the feed horizontally (selfie view).
 *
 * Outputs the captured frame as `image` (an HTMLCanvasElement, same as
 * io/image-url) plus `ready` / `width` / `height`. The image is a 1×1
 * placeholder until the first frame arrives — downstream draw/image and the
 * image filters already guard with isReadyImageCanvas.
 */
export const cameraNode = registerNode({
  type: 'input/camera',
  category: 'Input',
  label: 'Camera',
  description: 'Live webcam / connected-camera feed as an image. Flip `start` on to open the camera.',
  defaultParams: { device: '', mirror: false },
  paramOptions: { mirror: ['false', 'true'] },
  dynamicOptions: {
    device: {
      getOptions: () => cameraRegistry.deviceOptions(),
      subscribe: (fn) => cameraRegistry.subscribe(fn),
    },
  },
  inputs: [
    { name: 'start', type: 'boolean', default: false },
  ],
  outputs: [
    { name: 'image', type: 'image' },
    { name: 'ready', type: 'boolean' },
    { name: 'width', type: 'number' },
    { name: 'height', type: 'number' },
  ],
  effectful: true,
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const start = !!((inputs[0] ?? [])[0]);
    const device = params.param<string>('device', '');
    const mirror = params.param<string | boolean>('mirror', false) === true
      || params.param<string | boolean>('mirror', false) === 'true';
    const canvas = cameraRegistry.ensure(params.nodeId, device, start, mirror);
    const ready = cameraRegistry.isReady(params.nodeId);
    return [
      [canvas],
      [ready],
      [ready ? canvas.width : 0],
      [ready ? canvas.height : 0],
    ];
  },
});
