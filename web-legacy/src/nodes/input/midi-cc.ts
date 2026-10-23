import { registerNode } from '../registry';
import { midiRegistry } from '../../runtime/midi';
import type { NodeParamContext } from '../types';

/**
 * MidiCC — tap a single CC (Control Change) message from a connected
 * MIDI device. The common parameter-control workflow:
 *
 *   knob on board → MidiCC node → Map node → freq / cutoff / etc.
 *
 * Params:
 *   - `device`   MIDIInput.id, or empty for "any device". The dropdown
 *                live-updates as boards connect / disconnect — pick a
 *                specific board if you want to route different
 *                controllers to different nodes.
 *   - `channel`  1..16, or 0 for "any channel" (default 0).
 *   - `cc`       0..127, the controller number you want to tap.
 *
 * Outputs:
 *   - `value`    normalized 0..1 (the usual node-wire range).
 *   - `raw`      0..127 (the original MIDI value, for downstream
 *                logic that wants the precise integer).
 *
 * Until a CC value has actually arrived, both outputs are 0.
 */
export const midiCCNode = registerNode({
  type: 'input/midi-cc',
  category: 'Input',
  label: 'MidiCC',
  description: 'Read a single MIDI Control Change message.',
  defaultParams: { device: '', channel: 0, cc: 0 },
  // device gets a live dropdown of attached MIDI boards; channel + cc
  // remain free-form numeric inputs.
  dynamicOptions: {
    device: {
      getOptions: () => midiRegistry.deviceOptions(),
      subscribe: (fn) => midiRegistry.subscribe(fn),
    },
  },
  inputs: [],
  outputs: [
    { name: 'value', type: 'number' },
    { name: 'raw', type: 'number' },
  ],
  readFromContext: (_frame, params: NodeParamContext) => {
    const device = params.param<string>('device', '');
    const channel = clampChannel(params.param<number>('channel', 0));
    const cc = clampCC(params.param<number>('cc', 0));
    const raw = midiRegistry.getCC(device, channel, cc);
    if (raw < 0) return [[0], [0]];
    return [[raw / 127], [raw]];
  },
});

function clampChannel(c: number): number {
  if (!Number.isFinite(c)) return 0;
  const n = Math.floor(c);
  return n < 0 ? 0 : n > 16 ? 16 : n;
}

function clampCC(c: number): number {
  if (!Number.isFinite(c)) return 0;
  const n = Math.floor(c);
  return n < 0 ? 0 : n > 127 ? 127 : n;
}
