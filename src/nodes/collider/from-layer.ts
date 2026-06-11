import { registerNode } from '../registry';
import type { Collider, ParticleBuffer } from '../../runtime/particles';
import type { SceneNode } from '../../runtime/scene';
import { COLLIDER_MODES, coerceMode } from './_shared';

/**
 * collider/from-layer — extract the geometry of a wired layer (rect or
 * ellipse for v1; polygon support tracked under CONCEPTS.md → "Particle
 * architecture") and produce a collider from it. Wire any draw node's
 * output into `layer` and downstream particles will collide with that
 * shape.
 *
 * v1 limitations:
 *   - Only `rect`, `ellipse`, `triangle`, `polygon` are honoured.
 *   - Group transforms (translate/rotate on a parent Group) are NOT
 *     followed — the layer is treated in its local space. For
 *     transformed shapes, recreate them with analytic collider/rect or
 *     collider/circle for now.
 */
export const colliderFromLayerNode = registerNode({
  type: 'collider/from-layer',
  category: 'Collider',
  label: 'FromLayer',
  description:
    'Build a collider from any shape layer (rect, ellipse, polygon, triangle).',
  inputs: [
    { name: 'layer',       type: 'layer',   default: null },
    { name: 'mode',        type: 'string',  default: 'bounce', param: 'mode', constant: true },
    { name: 'restitution', type: 'number',  default: 0.8 },
    { name: 'inverted',    type: 'boolean', default: false, param: 'inverted', constant: true },
  ],
  outputs: [{ name: 'collider', type: 'collider' }],
  defaultParams: { mode: 'bounce', inverted: false },
  paramOptions: { mode: [...COLLIDER_MODES] },
  computeRaw: (inputs) => {
    const layer = inputs[0]?.[0] as SceneNode | null | undefined;
    const mode = coerceMode(inputs[1]?.[0]);
    const rest = Math.max(0, Math.min(1, Number(inputs[2]?.[0] ?? 0.8)));
    const inverted = Boolean(inputs[3]?.[0]);
    if (!layer) {
      // No-op collider, but still returns a valid descriptor so wires hold.
      return [[{ apply: () => {} } as Collider]];
    }
    const apply = buildApply(layer, mode, rest, inverted);
    const collider: Collider = { apply };
    return [[collider]];
  },
});

type Mode = 'bounce' | 'kill' | 'absorb';

function buildApply(
  layer: SceneNode, mode: Mode, rest: number, inverted: boolean,
): (buf: ParticleBuffer) => void {
  switch (layer.kind) {
    case 'rect': {
      let x = layer.x, y = layer.y, w = layer.w, h = layer.h;
      if (layer.mode === 'center')  { x -= w / 2; y -= h / 2; }
      else if (layer.mode === 'corners') { w = w - x; h = h - y; }
      else if (layer.mode === 'radius')  { x -= w; y -= h; w *= 2; h *= 2; }
      return rectApply(x, y, x + w, y + h, mode, rest, inverted);
    }
    case 'ellipse': {
      let cx = layer.x, cy = layer.y, rx = layer.w / 2, ry = layer.h / 2;
      if (layer.mode === 'corner') { cx += rx; cy += ry; }
      else if (layer.mode === 'radius') { rx = layer.w; ry = layer.h; }
      const avgR = (rx + ry) * 0.5;
      return circleApply(cx, cy, avgR, mode, rest, inverted);
    }
    case 'triangle': {
      const pts = [
        { x: layer.x1, y: layer.y1 },
        { x: layer.x2, y: layer.y2 },
        { x: layer.x3, y: layer.y3 },
      ];
      return polygonApply(pts, mode, rest, inverted);
    }
    case 'polygon':
      return polygonApply([...layer.points], mode, rest, inverted);
    default:
      return () => {};
  }
}

function rectApply(
  minX: number, minY: number, maxX: number, maxY: number,
  mode: Mode, rest: number, inverted: boolean,
) {
  return (buf: ParticleBuffer) => {
    const cap = buf.capacity;
    const alive = buf.alive;
    const px = buf.posX, py = buf.posY;
    const vx = buf.velX, vy = buf.velY;
    for (let i = 0; i < cap; i++) {
      if (!alive[i]) continue;
      const x = px[i] ?? 0, y = py[i] ?? 0;
      const inside = x > minX && x < maxX && y > minY && y < maxY;
      if (inverted ? inside : !inside) continue;
      if (mode === 'kill') { buf.kill(i); continue; }
      if (mode === 'absorb') { vx[i] = 0; vy[i] = 0; continue; }
      const dxL = x - minX, dxR = maxX - x;
      const dyT = y - minY, dyB = maxY - y;
      const minDx = Math.min(dxL, dxR);
      const minDy = Math.min(dyT, dyB);
      if (minDx < minDy) {
        vx[i] = -((vx[i] ?? 0) * rest);
        px[i] = dxL < dxR ? minX - 0.1 : maxX + 0.1;
      } else {
        vy[i] = -((vy[i] ?? 0) * rest);
        py[i] = dyT < dyB ? minY - 0.1 : maxY + 0.1;
      }
    }
  };
}

function circleApply(
  cx: number, cy: number, r: number,
  mode: Mode, rest: number, inverted: boolean,
) {
  const r2 = r * r;
  return (buf: ParticleBuffer) => {
    const cap = buf.capacity;
    const alive = buf.alive;
    const px = buf.posX, py = buf.posY;
    const vx = buf.velX, vy = buf.velY;
    for (let i = 0; i < cap; i++) {
      if (!alive[i]) continue;
      const dx = (px[i] ?? 0) - cx;
      const dy = (py[i] ?? 0) - cy;
      const d2 = dx * dx + dy * dy;
      const hit = inverted ? d2 > r2 : d2 < r2;
      if (!hit) continue;
      if (mode === 'kill') { buf.kill(i); continue; }
      if (mode === 'absorb') { vx[i] = 0; vy[i] = 0; continue; }
      const dist = Math.sqrt(d2) || 1e-6;
      const nx = (inverted ? -dx : dx) / dist;
      const ny = (inverted ? -dy : dy) / dist;
      const vDotN = (vx[i] ?? 0) * nx + (vy[i] ?? 0) * ny;
      if (vDotN < 0) {
        vx[i] = ((vx[i] ?? 0) - 2 * vDotN * nx) * rest;
        vy[i] = ((vy[i] ?? 0) - 2 * vDotN * ny) * rest;
      }
      const overlap = inverted ? (dist - r) : (r - dist);
      px[i] = (px[i] ?? 0) + nx * overlap;
      py[i] = (py[i] ?? 0) + ny * overlap;
    }
  };
}

function polygonApply(
  pts: { x: number; y: number }[],
  mode: Mode, rest: number, inverted: boolean,
) {
  // Precompute edges and inward-pointing normals once at construction.
  // For each edge a→b the outward normal (for a counter-clockwise polygon
  // in screen-space-Y-down) is `(dy, -dx) / |e|`. We store both edges
  // and normals in flat Float32Arrays for cache-friendly inner loops.
  const n = pts.length;
  const edgeX = new Float32Array(n);
  const edgeY = new Float32Array(n);
  const edgeLen = new Float32Array(n);
  const normX = new Float32Array(n);
  const normY = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    const a = pts[i]!;
    const b = pts[(i + 1) % n]!;
    const ex = b.x - a.x;
    const ey = b.y - a.y;
    const len = Math.hypot(ex, ey) || 1e-9;
    edgeX[i] = ex; edgeY[i] = ey; edgeLen[i] = len;
    // Outward normal — flipped sign of the screen-space rotation 90°CW.
    normX[i] = ey / len;
    normY[i] = -ex / len;
  }
  // Local AABB for cheap broad-phase rejection inside the apply —
  // particles outside the bounding box can't possibly be inside the
  // polygon, so we skip the O(edges) point-in-polygon test.
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const p of pts) {
    if (p.x < minX) minX = p.x;
    if (p.y < minY) minY = p.y;
    if (p.x > maxX) maxX = p.x;
    if (p.y > maxY) maxY = p.y;
  }

  return (buf: ParticleBuffer) => {
    const cap = buf.capacity;
    const alive = buf.alive;
    const px = buf.posX, py = buf.posY;
    const vx = buf.velX, vy = buf.velY;
    for (let i = 0; i < cap; i++) {
      if (!alive[i]) continue;
      const x = px[i] ?? 0, y = py[i] ?? 0;
      // Broad-phase AABB reject. For the non-inverted case, particles
      // outside the bbox can't possibly be inside the polygon, so skip.
      if (!inverted && (x < minX || x > maxX || y < minY || y > maxY)) continue;
      const inPoly = pointInPolygon(x, y, pts);
      const hit = inverted ? !inPoly : inPoly;
      if (!hit) continue;
      if (mode === 'kill') { buf.kill(i); continue; }
      if (mode === 'absorb') { vx[i] = 0; vy[i] = 0; continue; }

      // Find the closest edge and bounce across its normal.
      let bestDist = Infinity;
      let bestNx = 0, bestNy = -1;
      let bestProjX = x, bestProjY = y;
      for (let e = 0; e < n; e++) {
        const a = pts[e]!;
        const ex = edgeX[e]!, ey = edgeY[e]!, el = edgeLen[e]!;
        const tRaw = ((x - a.x) * ex + (y - a.y) * ey) / (el * el);
        const t = tRaw < 0 ? 0 : tRaw > 1 ? 1 : tRaw;
        const cx2 = a.x + ex * t, cy2 = a.y + ey * t;
        const ddx = x - cx2, ddy = y - cy2;
        const d2 = ddx * ddx + ddy * ddy;
        if (d2 < bestDist) {
          bestDist = d2;
          bestNx = normX[e]!;
          bestNy = normY[e]!;
          bestProjX = cx2;
          bestProjY = cy2;
        }
      }
      // Inverted polygons (particle outside polygon, "stay inside"
      // arena): flip the normal we'll reflect across.
      const nx = inverted ? -bestNx : bestNx;
      const ny = inverted ? -bestNy : bestNy;
      const vDotN = (vx[i] ?? 0) * nx + (vy[i] ?? 0) * ny;
      if (vDotN < 0) {
        vx[i] = ((vx[i] ?? 0) - 2 * vDotN * nx) * rest;
        vy[i] = ((vy[i] ?? 0) - 2 * vDotN * ny) * rest;
      }
      // Position correction — push the particle out to the surface
      // along the edge normal (move it to the projection point, then a
      // hair further to escape the inside).
      px[i] = bestProjX + nx * 0.5;
      py[i] = bestProjY + ny * 0.5;
    }
  };
}

function pointInPolygon(
  x: number, y: number,
  pts: ReadonlyArray<{ x: number; y: number }>,
): boolean {
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const xi = pts[i]!.x, yi = pts[i]!.y;
    const xj = pts[j]!.x, yj = pts[j]!.y;
    const intersect = ((yi > y) !== (yj > y)) &&
      (x < (xj - xi) * (y - yi) / (yj - yi || 1e-9) + xi);
    if (intersect) inside = !inside;
  }
  return inside;
}
