import { registerNode } from '../registry';

/**
 * Cross-frame state nodes.
 *
 * The canonical pattern is the split **Read** / **Write** pair: drop
 * Read on the left of your patch, Write on the right, pair them by slot
 * number, and the feedback loop reads strictly left-to-right — no
 * back-edge wires obscuring the graph. The pair comes in two flavours:
 *
 *   `Read`  / `Write`  — scalar (single value)
 *   `Read*` / `Write*` — spread (trails, grids, particle arrays)
 *
 * The `state/value` node is the single-node alternative (init + next +
 * current on one box). Useful for compact patches but produces a
 * back-edge. `state/spread` is the spread analogue and is hidden from
 * the palette — Read* / Write* superseded it.
 *
 * Pairing is by slot. The editor auto-picks the smallest unused slot
 * per (kind, type) when you drop a new Read or Read*. Same-kind slots
 * share a namespace; scalar Read at slot 1 pairs with scalar Write at
 * slot 1, and that's independent of Read* / Write* at slot 1.
 */

// ─── Single-node forms ─────────────────────────────────────────────────────

export const stateValueNode = registerNode({
  type: 'state/value',
  category: 'State',
  label: 'State',
  description: 'Hold a single value across frames — emit last tick\'s `next` as `current`.',
  inputs: [
    { name: 'init', type: 'any' },
    { name: 'next', type: 'any' },
  ],
  outputs: [{ name: 'current', type: 'any' }],
  stateful: true,
});

/**
 * Legacy spread variant. Hidden from the palette — superseded by the
 * Read* / Write* pair. Kept in the registry so existing patches that
 * reference `state/spread` still load and run.
 */
export const stateSpreadNode = registerNode({
  type: 'state/spread',
  category: 'State',
  label: 'StateSpread',
  description: 'Legacy: use Read* / Write* instead for left-to-right spread feedback.',
  inputs: [
    { name: 'init', type: 'any', kind: 'spread' },
    { name: 'next', type: 'any', kind: 'spread' },
  ],
  outputs: [{ name: 'current', type: 'any', kind: 'spread' }],
  stateful: true,
  hidden: true,
});

// ─── Split Read / Write pair (scalar) ──────────────────────────────────────

export const stateWriteNode = registerNode({
  type: 'state/write',
  category: 'State',
  label: 'Write',
  description: 'Stash a value at end-of-tick; the matching Read emits it next tick.',
  inputs: [{ name: 'next', type: 'any' }],
  outputs: [],
  stateful: true,
  statefulNextPort: 0,
  defaultParams: { slot: 1 },
});

export const stateReadNode = registerNode({
  type: 'state/read',
  category: 'State',
  label: 'Read',
  description: 'Emit the value last written under `slot`, or `init` on frame 0.',
  inputs: [{ name: 'init', type: 'any' }],
  outputs: [{ name: 'current', type: 'any' }],
  defaultParams: { slot: 1 },
});

// ─── Split Read / Write pair (spread) ──────────────────────────────────────

export const stateWriteSpreadNode = registerNode({
  type: 'state/write-spread',
  category: 'State',
  label: 'Write*',
  description: 'Stash a spread at end-of-tick; the matching Read* emits it next tick.',
  inputs: [{ name: 'next', type: 'any', kind: 'spread' }],
  outputs: [],
  stateful: true,
  statefulNextPort: 0,
  defaultParams: { slot: 1 },
});

export const stateReadSpreadNode = registerNode({
  type: 'state/read-spread',
  category: 'State',
  label: 'Read*',
  description: 'Emit the spread last written under `slot`, or `init` on frame 0.',
  inputs: [{ name: 'init', type: 'any', kind: 'spread' }],
  outputs: [{ name: 'current', type: 'any', kind: 'spread' }],
  defaultParams: { slot: 1 },
});
