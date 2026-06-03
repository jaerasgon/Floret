import { registerNode } from '../registry';
import type { Color, PortValue } from '../../runtime/types';
import type { NodeParamContext, PortSpec } from '../types';
import { PaletteNode } from '../../editor/nodes/PaletteNode';
import { hexToColor01 } from '../_shared/color-conv';
import { toColor, WHITE_COLOR } from '../_shared/coerce';

/**
 * Palette — author a literal list of colours. Each stop has its own
 * pin (so you can wire a colour-producing node in for that slot) AND
 * an inline colour-picker swatch that stores a hex string in the
 * node's params (used when the stop's pin is unwired).
 *
 * Output is a `Spread<Color>` ready to feed any node with a spread-
 * colour input — `walker-cluster`, `color/gradient` for interpolation,
 * `image/sample-spread`, etc.
 *
 * `+` / `−` buttons in the header grow / shrink the stop count, same
 * model as `transform/group`'s slot controls.
 */

const MIN_STOPS = 1;
const MAX_STOPS = 64;
const DEFAULT_STOP_COUNT = 4;

// A pleasant default palette so a fresh node has something to show.
// First `DEFAULT_STOP_COUNT` entries seed the inline pickers; later
// stops added via `+` fall back to white.
const DEFAULT_STOPS: ReadonlyArray<string> = [
  '#fed000', '#fc8405', '#ed361a', '#4464a1', '#cf3895', '#6d358a', '#06b4b0', '#4b8a5f',
];

function clampStops(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_STOP_COUNT;
  const f = Math.floor(n);
  return Math.max(MIN_STOPS, Math.min(MAX_STOPS, f));
}

/** Build the stop input ports from the current stopCount param. Each
 *  port carries one Color stop; when no wire is connected the inline
 *  picker's hex param is used. */
function buildStopPorts(params: Record<string, unknown>): ReadonlyArray<PortSpec> {
  const n = clampStops(Number(params.stopCount) || DEFAULT_STOP_COUNT);
  const ports: PortSpec[] = [];
  for (let i = 0; i < n; i++) {
    ports.push({
      name: `c${i}`,
      label: `${i + 1}`,
      type: 'color',
      default: null,
    });
  }
  return ports;
}

/** Build the default param record. Pre-populate ALL bundled default
 *  hexes (even past the initial stop count) so when the user clicks
 *  `+` the new stops have sensible seed colours instead of plain
 *  white. */
function buildDefaultParams(): Record<string, unknown> {
  const out: Record<string, unknown> = { stopCount: DEFAULT_STOP_COUNT };
  for (let i = 0; i < DEFAULT_STOPS.length; i++) out[`c${i}`] = DEFAULT_STOPS[i]!;
  return out;
}

export const paletteNode = registerNode({
  type: 'color/palette',
  category: 'Color',
  label: 'Palette',
  description: 'Author a list of colours by clicking swatches.',
  inputs: [
    // Static fallback — the dynamic version always replaces this when
    // present, but a placeholder is required so the registry sees a
    // well-formed inputs list.
    { name: 'c0', label: '1', type: 'color', default: null },
  ],
  outputs: [
    { name: 'colors', label: 'colors', type: 'color', kind: 'spread' },
  ],
  defaultParams: buildDefaultParams(),
  dynamicInputs: buildStopPorts,
  component: PaletteNode,
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const stopCount = clampStops(params.param<number>('stopCount', DEFAULT_STOP_COUNT));
    const stops: Color[] = new Array(stopCount);
    for (let i = 0; i < stopCount; i++) {
      const wiredRaw = inputs[i]?.[0];
      if (wiredRaw && typeof wiredRaw === 'object' && 'r' in (wiredRaw as object)) {
        stops[i] = toColor(wiredRaw);
        continue;
      }
      // No upstream wire — fall back to the inline hex picker.
      const fallback = i < DEFAULT_STOPS.length ? DEFAULT_STOPS[i]! : '#ffffff';
      const hex = params.param<string>(`c${i}`, fallback);
      try {
        stops[i] = hexToColor01(hex);
      } catch {
        stops[i] = WHITE_COLOR;
      }
    }
    return [stops];
  },
});
