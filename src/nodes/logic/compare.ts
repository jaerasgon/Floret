import { registerNode } from '../registry';

const ab = [
  { name: 'a', type: 'number' as const, default: 0 },
  { name: 'b', type: 'number' as const, default: 0 },
];
const out = [{ name: 'out', type: 'boolean' as const }];

export const gtNode = registerNode({
  type: 'logic/gt',
  category: 'Logic',
  label: 'Greater',
  description: 'True when a is greater than b.',
  inputs: ab,
  outputs: out,
  compute: (a, b) => [(a as number) > (b as number)],
});

export const ltNode = registerNode({
  type: 'logic/lt',
  category: 'Logic',
  label: 'Less',
  description: 'True when a is less than b.',
  inputs: ab,
  outputs: out,
  compute: (a, b) => [(a as number) < (b as number)],
});

export const gteNode = registerNode({
  type: 'logic/gte',
  category: 'Logic',
  label: 'GreaterEq',
  description: 'True when a is greater than or equal to b.',
  inputs: ab,
  outputs: out,
  compute: (a, b) => [(a as number) >= (b as number)],
});

export const lteNode = registerNode({
  type: 'logic/lte',
  category: 'Logic',
  label: 'LessEq',
  description: 'True when a is less than or equal to b.',
  inputs: ab,
  outputs: out,
  compute: (a, b) => [(a as number) <= (b as number)],
});

export const eqNode = registerNode({
  type: 'logic/eq',
  category: 'Logic',
  label: 'Equal',
  description: 'True when a equals b.',
  inputs: ab,
  outputs: out,
  compute: (a, b) => [(a as number) === (b as number)],
});

export const neqNode = registerNode({
  type: 'logic/neq',
  category: 'Logic',
  label: 'NotEqual',
  description: 'True when a does not equal b.',
  inputs: ab,
  outputs: out,
  compute: (a, b) => [(a as number) !== (b as number)],
});
