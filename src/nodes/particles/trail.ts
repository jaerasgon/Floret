import { registerNode } from '../registry';
import type { ParticleBuffer } from '../../runtime/particles';
import type { ParticleTrailBatch, ResolvedStyle, SceneNode } from '../../runtime/scene';
import { DEFAULT_STYLE, resolveStyle } from '../../runtime/scene';
import type { Color, PortValue, Style } from '../../runtime/types';
import { colorToCss } from '../../runtime/types';
import type { NodeParamContext } from '../types';

/**
 * particles/trail — append a polyline trail behind every alive
 * particle. The ring buffer of past positions lives in this node's
 * runtimeState; each tick we write the current pos to the head index
 * and emit a ParticleTrailBatch for the renderer to walk.
 *
 * `segments` is CONSTANT — buffer length is fixed at compile, and
 * changing it would have to nuke trail history. Default 8; cap at 64
 * because trail cost = N × segments per frame.
 */
interface TrailState {
  buffer: ParticleBuffer | null;
  history: Float32Array;
  segments: number;
  headIdx: number;
  capacity: number;
}

export const particlesTrailNode = registerNode({
  type: 'particles/trail',
  category: 'Particles',
  label: 'Trail',
  description:
    'Render a polyline trail behind every alive particle. Ring-buffer length is fixed (CONSTANT).',
  // Hidden state in `runtimeState.__trail` — opt out of per-tick memo
  // so the history buffer actually advances each frame.
  effectful: true,
  inputs: [
    { name: 'particles', type: 'particle', default: null },
    { name: 'segments',  type: 'number',   default: 8, integer: true, param: 'segments', constant: true },
    { name: 'style',     type: 'style' },
    { name: 'color',     type: 'color',    default: null },
    { name: 'fadeOut',   type: 'boolean',  default: true },
    { name: 'useBufferColor', type: 'boolean', default: true, param: 'useBufferColor', constant: true },
    { name: 'tapered',   type: 'boolean',  default: false },
    { name: 'tailWidth', type: 'number',   default: 0.1 },
  ],
  outputs: [{ name: 'layer', type: 'layer' }],
  defaultParams: { segments: 8 },
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const stateBag = params as unknown as { __trail?: TrailState };
    const buf = inputs[0]?.[0] as ParticleBuffer | null | undefined;
    const segments = Math.max(2, Math.min(64, Math.floor(Number(inputs[1]?.[0] ?? 8))));
    if (!buf) {
      return [[{ kind: 'group', children: [] } as SceneNode]];
    }
    let state = stateBag.__trail;
    if (
      !state ||
      state.capacity !== buf.capacity ||
      state.segments !== segments ||
      state.buffer !== buf
    ) {
      state = {
        buffer: buf,
        segments,
        capacity: buf.capacity,
        history: new Float32Array(buf.capacity * segments * 2).fill(NaN),
        headIdx: 0,
      };
      stateBag.__trail = state;
    }
    // Advance head, write current positions for alive particles. Dead
    // slots keep their old NaN values so the renderer breaks the line.
    state.headIdx = (state.headIdx + 1) % segments;
    const hist = state.history;
    const head = state.headIdx;
    const segs = state.segments;
    const cap = buf.capacity;
    for (let i = 0; i < cap; i++) {
      const base = i * segs * 2 + head * 2;
      if (buf.alive[i]) {
        hist[base]     = buf.posX[i] ?? 0;
        hist[base + 1] = buf.posY[i] ?? 0;
      } else {
        hist[base] = NaN;
        hist[base + 1] = NaN;
      }
    }

    const userStyle = inputs[2]?.[0] as Style | null | undefined;
    const tint = inputs[3]?.[0] as Color | null | undefined;
    const fadeOut = Boolean(inputs[4]?.[0] ?? true);
    const useBufferColor = Boolean(inputs[5]?.[0] ?? true);
    const tapered = Boolean(inputs[6]?.[0] ?? false);
    const tailWidth = Math.max(0, Math.min(1, Number(inputs[7]?.[0] ?? 0.1)));
    let style: ResolvedStyle = resolveStyle(userStyle ?? null, DEFAULT_STYLE);
    if (tint) style = { ...style, stroke: colorToCss(tint), strokeWeight: style.strokeWeight || 1 };
    // Trails always need a stroke colour — fall back to white if the
    // wired style didn't set one. Preserve any user-wired strokeWeight.
    if (!style.stroke) style = { ...style, stroke: '#ffffff', strokeWeight: style.strokeWeight || 1 };

    const layer: ParticleTrailBatch = {
      kind: 'particle-trail-batch',
      buffer: buf,
      history: hist,
      segments: segs,
      headIdx: head,
      style,
      fadeOut,
      useBufferColor,
      tapered,
      tailWidth,
    };
    return [[layer]];
  },
});
