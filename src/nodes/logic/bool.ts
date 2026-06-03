import { registerNode } from '../registry';

const ab = [
  { name: 'a', type: 'boolean' as const, default: false },
  { name: 'b', type: 'boolean' as const, default: false },
];
const out = [{ name: 'out', type: 'boolean' as const }];

export const andNode = registerNode({
  type: 'logic/and',
  category: 'Logic',
  label: 'And',
  description: 'True when both a and b are true.',
  inputs: ab,
  outputs: out,
  compute: (a, b) => [Boolean(a) && Boolean(b)],
});

export const orNode = registerNode({
  type: 'logic/or',
  category: 'Logic',
  label: 'Or',
  description: 'True when either a or b is true.',
  inputs: ab,
  outputs: out,
  compute: (a, b) => [Boolean(a) || Boolean(b)],
});

export const notNode = registerNode({
  type: 'logic/not',
  category: 'Logic',
  label: 'Not',
  description: 'Invert a boolean.',
  inputs: [{ name: 'x', type: 'boolean', default: false }],
  outputs: out,
  compute: (x) => [!x],
});
