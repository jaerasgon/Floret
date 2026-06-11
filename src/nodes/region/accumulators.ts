import { registerNode } from '../registry';
import type { PortType } from '../../runtime/types';

/**
 * Accumulator Read / Write pair — threads a stateful value across the
 * iterations of a ForEach region. Iron-rule compliant: every wire flows
 * left-to-right; the per-iteration feedback (write → next read) is an
 * implicit runtime pairing keyed by `slot`, not a back-edge wire.
 *
 *   Read   inputs: [init]     outputs: [read]      regionRole: 'accumulator-read'
 *   Write  inputs: [write]    outputs: [final]     regionRole: 'accumulator-write'
 *
 * Pairing scope is per-region: a (Read, Write) sharing the same `slot`
 * inside the same ForEach form one logical accumulator. Slot defaults
 * to 1. Drop multiple pairs for independent accumulators (slot 1 / 2 /
 * 3 / …) inside the same region.
 *
 * Typed per port-type so the editor's connection rules can disallow
 * cross-type wiring (number Write can't drive an image Read, etc.).
 */
function makeRead(portType: PortType) {
  return registerNode({
    type: `region/accumulator-read/${portType}`,
    category: 'Region',
    label: 'Read',
    description: 'Per-iteration accumulator read — paired with Write by slot.',
    inputs: [{ name: 'init', type: portType, kind: 'single' }],
    outputs: [{ name: 'read', type: portType, kind: 'single' }],
    regionRole: 'accumulator-read',
    defaultParams: { slot: 1 },
  });
}

function makeWrite(portType: PortType) {
  return registerNode({
    type: `region/accumulator-write/${portType}`,
    category: 'Region',
    label: 'Write',
    description: 'Per-iteration accumulator write — final emits after the loop.',
    inputs: [{ name: 'write', type: portType, kind: 'single' }],
    outputs: [{ name: 'final', type: portType, kind: 'single' }],
    regionRole: 'accumulator-write',
    defaultParams: { slot: 1 },
  });
}

export const accumulatorReadNumber  = makeRead('number');
export const accumulatorReadBool    = makeRead('boolean');
export const accumulatorReadImage   = makeRead('image');
export const accumulatorWriteNumber = makeWrite('number');
export const accumulatorWriteBool   = makeWrite('boolean');
export const accumulatorWriteImage  = makeWrite('image');
