import { registerNode } from '../registry';

export const timeSecondsNode = registerNode({
  type: 'time/seconds',
  category: 'Time',
  label: 'Time',
  description: 'Output seconds elapsed since the patch started.',
  inputs: [],
  outputs: [{ name: 't', type: 'number', label: 't' }],
  readFromContext: (frame) => [[frame.t]],
});

export const timeFrameNode = registerNode({
  type: 'time/frame',
  category: 'Time',
  label: 'Frame',
  description: 'Output the integer frame counter.',
  inputs: [],
  outputs: [{ name: 'frame', type: 'number' }],
  readFromContext: (frame) => [[frame.frameCount]],
});
