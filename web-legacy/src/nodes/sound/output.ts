import { registerNode } from '../registry';
import { audioRegistry } from '../../runtime/audio';
import type { AudioHandle, PortValue } from '../../runtime/types';

/**
 * The audio sink. Connect any audio chain here for it to reach the
 * speakers. Without an Output node in the patch, nothing makes a sound.
 *
 * The first connected wire is routed to the master output. Master
 * volume is controllable via the `volume` input (0..1, default 1).
 *
 * Mirrors the Renderer's role for visuals — every audio graph
 * terminates here.
 */
export const soundOutputNode = registerNode({
  type: 'sound/output',
  category: 'Sound',
  label: 'Output',
  description: 'Send an audio chain to the speakers.',
  inputs: [
    { name: 'in', type: 'audio' },
    { name: 'volume', type: 'number', default: 1 },
  ],
  outputs: [],
  computeRaw: (inputs: PortValue[]) => {
    const handle = (inputs[0] ?? [])[0] as AudioHandle | null;
    const vol = (inputs[1] ?? [])[0] as number | undefined;
    audioRegistry.setMasterVolume(typeof vol === 'number' ? vol : 1);
    if (handle && handle.node) {
      audioRegistry.connect(handle, audioRegistry.masterOut());
    }
    return [];
  },
});
