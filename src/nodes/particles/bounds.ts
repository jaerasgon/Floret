import { registerNode } from '../registry';
import type { Collider, ParticleBuffer } from '../../runtime/particles';
import type { Vec2 } from '../../runtime/types';

/**
 * particles/bounds — wrap, kill, or bounce particles at the edges of a
 * rectangular boundary. Like collider/rect but inverted by default
 * (particles INSIDE the bounds stay inside) and exposes a `wrap` mode.
 *
 * `mode` is CONSTANT — the three modes do structurally different things
 * to the buffer (wrap teleports, kill frees a slot, bounce just
 * reflects), worth choosing at compile.
 */
type BoundsMode = 'wrap' | 'kill' | 'bounce';

export const particlesBoundsNode = registerNode({
  type: 'particles/bounds',
  category: 'Particles',
  label: 'Bounds',
  description:
    'Wrap, kill, or bounce particles at the edges of a rectangular boundary.',
  inputs: [
    { name: 'size',        type: 'vec2',   default: { x: 600, y: 400 } },
    { name: 'center',      type: 'vec2',   default: { x: 0, y: 0 } },
    { name: 'mode',        type: 'string', default: 'wrap', param: 'mode', constant: true },
    { name: 'restitution', type: 'number', default: 0.6 },
  ],
  outputs: [{ name: 'collider', type: 'collider' }],
  defaultParams: { mode: 'wrap' as BoundsMode },
  paramOptions: { mode: ['wrap', 'kill', 'bounce'] },
  computeRaw: (inputs) => {
    const size = (inputs[0]?.[0] as Vec2 | null) ?? { x: 600, y: 400 };
    const center = (inputs[1]?.[0] as Vec2 | null) ?? { x: 0, y: 0 };
    const mode: BoundsMode = (() => {
      const v = inputs[2]?.[0];
      return v === 'kill' || v === 'bounce' ? v : 'wrap';
    })();
    const rest = Math.max(0, Math.min(1, Number(inputs[3]?.[0] ?? 0.6)));
    const hw = Math.abs(size.x) * 0.5;
    const hh = Math.abs(size.y) * 0.5;
    const minX = center.x - hw, maxX = center.x + hw;
    const minY = center.y - hh, maxY = center.y + hh;
    const w = maxX - minX, h = maxY - minY;

    const apply = (buf: ParticleBuffer): void => {
      const cap = buf.capacity;
      const alive = buf.alive;
      const px = buf.posX, py = buf.posY;
      const vx = buf.velX, vy = buf.velY;
      for (let i = 0; i < cap; i++) {
        if (!alive[i]) continue;
        let x = px[i] ?? 0, y = py[i] ?? 0;
        const outX = x < minX || x > maxX;
        const outY = y < minY || y > maxY;
        if (!outX && !outY) continue;
        if (mode === 'kill') { buf.kill(i); continue; }
        if (mode === 'wrap') {
          if (x < minX) x += w;
          else if (x > maxX) x -= w;
          if (y < minY) y += h;
          else if (y > maxY) y -= h;
          px[i] = x; py[i] = y;
          continue;
        }
        // bounce
        if (outX) {
          vx[i] = -((vx[i] ?? 0) * rest);
          px[i] = x < minX ? minX + 0.1 : maxX - 0.1;
        }
        if (outY) {
          vy[i] = -((vy[i] ?? 0) * rest);
          py[i] = y < minY ? minY + 0.1 : maxY - 0.1;
        }
      }
    };
    const collider: Collider = { apply };
    return [[collider]];
  },
});
