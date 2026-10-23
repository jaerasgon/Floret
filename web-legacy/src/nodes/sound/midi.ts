import { registerNode } from '../registry';

/**
 * MIDI ↔ frequency converters. Scalar, stateless — they don't need the
 * audio registry. Useful for driving MonoSynth's `note` input from a
 * Hz-based slider, or for reading frequency from a sequencer that emits
 * MIDI numbers.
 *
 * Formulas (standard MIDI tuning, A440 = MIDI 69):
 *   freq = 440 · 2^((midi − 69) / 12)
 *   midi = 69 + 12 · log2(freq / 440)
 */

export const midiToFreqNode = registerNode({
  type: 'sound/midi-to-freq',
  category: 'Sound',
  label: 'MidiToFreq',
  description: 'Convert a MIDI note number to frequency in Hz.',
  inputs: [{ name: 'midi', type: 'number', default: 69 }],
  outputs: [{ name: 'freq', type: 'number' }],
  compute: (midi: unknown) => {
    const n = typeof midi === 'number' ? midi : Number(midi) || 0;
    return [440 * Math.pow(2, (n - 69) / 12)];
  },
});

export const freqToMidiNode = registerNode({
  type: 'sound/freq-to-midi',
  category: 'Sound',
  label: 'FreqToMidi',
  description: 'Convert a frequency in Hz to a MIDI note number.',
  inputs: [{ name: 'freq', type: 'number', default: 440 }],
  outputs: [{ name: 'midi', type: 'number' }],
  compute: (freq: unknown) => {
    const f = typeof freq === 'number' ? freq : Number(freq) || 0;
    if (f <= 0) return [0];
    return [69 + 12 * Math.log2(f / 440)];
  },
});
