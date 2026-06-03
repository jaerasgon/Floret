import { registerNode } from '../registry';
import { midiRegistry } from '../../runtime/midi';
import type { NodeParamContext } from '../types';

/**
 * MidiNote — last-held-note state from any connected MIDI device.
 *
 * Stack-based monophonic semantics: pressing C, then E, then releasing
 * E leaves `note` at C (most recently pressed note that's still held).
 * `pressed` is true while at least one note is held. Pairs naturally
 * with MonoSynth — wire `note` → synth.note, `pressed` → synth.trigger.
 *
 * Params:
 *   - `channel`  1..16, or 0 for "any channel" (default 0).
 *
 * Outputs:
 *   - `note`     MIDI note number (0..127) of the last held note, or
 *                0 when nothing is held.
 *   - `velocity` Normalised 0..1 velocity of that note, or 0.
 *   - `pressed`  True while any note is held on the channel.
 *   - `freq`     Convenience: `note` converted to Hz via standard MIDI
 *                tuning (A4 = 69 → 440).
 */
export const midiNoteNode = registerNode({
  type: 'input/midi-note',
  category: 'Input',
  label: 'MidiNote',
  description: 'Read the last MIDI note held (monophonic).',
  defaultParams: { device: '', channel: 0 },
  dynamicOptions: {
    device: {
      getOptions: () => midiRegistry.deviceOptions(),
      subscribe: (fn) => midiRegistry.subscribe(fn),
    },
  },
  inputs: [],
  outputs: [
    { name: 'note', type: 'number' },
    { name: 'velocity', type: 'number' },
    { name: 'pressed', type: 'boolean' },
    { name: 'freq', type: 'number' },
  ],
  readFromContext: (_frame, params: NodeParamContext) => {
    const device = params.param<string>('device', '');
    const channel = clampChannel(params.param<number>('channel', 0));
    const note = midiRegistry.getLastHeldNote(device, channel);
    const pressed = midiRegistry.isAnyNoteHeld(device, channel);
    if (note < 0) {
      return [[0], [0], [pressed], [0]];
    }
    const vel = midiRegistry.getNoteVelocity(device, channel, note);
    const freq = 440 * Math.pow(2, (note - 69) / 12);
    return [[note], [vel / 127], [pressed], [freq]];
  },
});

function clampChannel(c: number): number {
  if (!Number.isFinite(c)) return 0;
  const n = Math.floor(c);
  return n < 0 ? 0 : n > 16 ? 16 : n;
}
