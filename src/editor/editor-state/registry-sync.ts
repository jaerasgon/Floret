/**
 * Single home for the per-node runtime registries' lifecycle fan-out. The
 * compile pass `retain`s entries for the live node set; `loadPatch` `reset`s
 * them all. Centralising it here keeps the two call sites from drifting as
 * new registries are added.
 */
import { audioRegistry } from '../../runtime/audio';
import { imageRegistry } from '../../runtime/image-registry';
import { derivedImageRegistry } from '../../runtime/derived-image-registry';
import { cameraRegistry } from '../../runtime/camera-registry';
import { svgRegistry } from '../../runtime/svg-registry';
import { mediapipeRegistry } from '../../runtime/mediapipe-registry';

/** Drop registry entries for any node id no longer present in the graph. */
export function retainRegistries(liveIds: ReadonlySet<string>): void {
  audioRegistry.retain(liveIds);
  imageRegistry.retain(liveIds);
  derivedImageRegistry.retain(liveIds);
  cameraRegistry.retain(liveIds);
  svgRegistry.retain(liveIds);
  mediapipeRegistry.retain(liveIds);
}

/** Tear down all registry-owned resources (oscillators, streams, images). */
export function resetRegistries(): void {
  audioRegistry.reset();
  imageRegistry.reset();
  derivedImageRegistry.reset();
  cameraRegistry.reset();
  svgRegistry.reset();
  mediapipeRegistry.reset();
}
