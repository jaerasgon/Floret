import { registerNode } from '../registry';

/**
 * Select — ternary on `any` so you can switch between two layers, colours,
 * strings, vec2s, whatever. Returns `a` when condition is true, else `b`.
 */
export const selectNode = registerNode({
  type: 'logic/select',
  category: 'Logic',
  label: 'Select',
  description: 'Pick a when cond is true, otherwise b.',
  inputs: [
    { name: 'cond', type: 'boolean', default: false },
    { name: 'a', type: 'any' },
    { name: 'b', type: 'any' },
  ],
  outputs: [{ name: 'out', type: 'any' }],
  compute: (cond, a, b) => [cond ? a : b],
});
