import { registerNode } from '../registry';

/**
 * String ops — small utilities that turn strings into spreads or back
 * the other way. Pair them with ForEach to do per-character or per-word
 * work (kinetic typography, typewriter reveals, etc.).
 */

export const stringSplitNode = registerNode({
  type: 'string/split',
  category: 'IO',
  label: 'Split',
  description:
    'Split a string into a Spread<string>. With `by=""` (default) you get one item per character; with `by=" "` one item per word.',
  inputs: [
    { name: 'text', type: 'string', default: '', param: 'text' },
    { name: 'by', type: 'string', default: '', param: 'by' },
  ],
  outputs: [{ name: 'parts', type: 'string', kind: 'spread' }],
  defaultParams: { text: '', by: '' },
  computeRaw: (inputs) => {
    const text = String(inputs[0]?.[0] ?? '');
    const by = String(inputs[1]?.[0] ?? '');
    // `''` triggers per-character; anything else is a literal delimiter.
    // Array.from handles surrogate pairs / emoji better than ''.split('').
    const out = by === '' ? Array.from(text) : text.split(by);
    return [out];
  },
});

export const stringConcatNode = registerNode({
  type: 'string/concat',
  category: 'IO',
  label: 'Concat',
  description:
    'Concatenate any number of strings into one. Wire multiple strings into `parts` (the pin gathers them); `separator` goes between each.',
  inputs: [
    // `multi` gathers every wired source into a single Spread<string> —
    // the same gather semantics used by Group/Combine. Lets the user
    // plug 2, 5, or N strings into one pin without N input ports.
    { name: 'parts', type: 'string', kind: 'multi', default: '' },
    { name: 'separator', type: 'string', default: '', param: 'separator' },
  ],
  outputs: [{ name: 'text', type: 'string' }],
  defaultParams: { separator: '' },
  computeRaw: (inputs) => {
    const parts = (inputs[0] ?? []).map((v) => String(v ?? ''));
    const sep = String(inputs[1]?.[0] ?? '');
    return [[parts.join(sep)]];
  },
});

export const stringWordIndicesNode = registerNode({
  type: 'string/word-indices',
  category: 'IO',
  label: 'WordIndices',
  description:
    'For each character of `text`, output the word index that character belongs to. Words are separated by `by` (default " "). Separators themselves take the index of the word that just ended, so per-char colouring stays continuous.',
  inputs: [
    { name: 'text', type: 'string', default: '', param: 'text' },
    { name: 'by',   type: 'string', default: ' ', param: 'by' },
  ],
  outputs: [{ name: 'indices', type: 'number', kind: 'spread' }],
  defaultParams: { text: '', by: ' ' },
  computeRaw: (inputs) => {
    const text = String(inputs[0]?.[0] ?? '');
    const by   = String(inputs[1]?.[0] ?? ' ');
    if (text.length === 0) return [[]];
    const chars = Array.from(text);
    const out = new Array<number>(chars.length);
    let word = 0;
    let inWord = false;
    const isSep = (c: string) => by === '' ? false : by.includes(c);
    for (let i = 0; i < chars.length; i++) {
      const c = chars[i]!;
      if (isSep(c)) {
        out[i] = word;
        if (inWord) { word++; inWord = false; }
      } else {
        out[i] = word;
        inWord = true;
      }
    }
    return [out];
  },
});

export const stringLengthNode = registerNode({
  type: 'string/length',
  category: 'IO',
  label: 'Length',
  description: 'Number of characters in the input string.',
  inputs: [{ name: 'text', type: 'string', default: '' }],
  outputs: [{ name: 'length', type: 'number' }],
  compute: (text) => [Array.from(String(text ?? '')).length],
});
