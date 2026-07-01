import type { FloretNode, FloretEdge } from './editor-state';
import { rfNodeBox } from './canvas/region-hit';

/**
 * Left-to-right layered ("Sugiyama-lite") auto-layout.
 *
 * Per scope (root + each region interior):
 *   1. Layer nodes by longest-path depth → columns (data flows left→right).
 *   2. Order nodes within each column by the MEDIAN of their neighbours' rows,
 *      swept down/up a few times — the standard crossing-minimisation heuristic
 *      that makes the flow read cleanly instead of criss-crossing.
 *   3. Place with REAL node sizes: each column is as wide as its widest node,
 *      each node's Y is the barycentre of its inputs (so a node sits beside what
 *      feeds it), with overlaps pushed apart by a fixed gap.
 *
 * Regions are laid out interior-first (post-order) and resized to fit their
 * tidied children, so a region reserves the right amount of room among its
 * siblings. Comment nodes are parked in a far-right column.
 */

const H_GAP = 40;   // horizontal space between columns (tighter → less horizontal spread)
const V_GAP = 58;   // vertical space between stacked nodes (roomier → more vertical spread)
const REGION_PAD = 30;   // padding on the sides / bottom inside a region
const REGION_TOP = 50;   // top clearance for the region header + padding
const FALLBACK = { w: 200, h: 100 };

export function autoLayoutPatch(
  nodes: ReadonlyArray<FloretNode>,
  edges: ReadonlyArray<FloretEdge>,
): FloretNode[] {
  const nodeById = new Map(nodes.map((n) => [n.id, n]));
  const byParent = new Map<string | null, FloretNode[]>();
  for (const n of nodes) {
    const k = n.parentId ?? null;
    (byParent.get(k) ?? byParent.set(k, []).get(k)!).push(n);
  }

  const positions = new Map<string, { x: number; y: number }>();
  const regionSize = new Map<string, { w: number; h: number }>();  // tidied region sizes

  const sizeOf = (id: string): { w: number; h: number } => {
    const r = regionSize.get(id);
    if (r) return r;
    const n = nodeById.get(id);
    if (!n) return FALLBACK;
    const b = rfNodeBox(n).size;
    return { w: b.w || FALLBACK.w, h: b.h || FALLBACK.h };
  };

  const median = (xs: number[]): number => {
    if (xs.length === 0) return -1;
    const s = [...xs].sort((a, b) => a - b);
    const m = s.length >> 1;
    return s.length % 2 ? s[m]! : (s[m - 1]! + s[m]!) / 2;
  };

  /** Lay out one scope; return its content extent (max right / bottom) so the
   *  caller can size an enclosing region. */
  function layoutScope(parentId: string | null): { right: number; bottom: number } {
    const scope = byParent.get(parentId) ?? [];
    if (scope.length === 0) return { right: 0, bottom: 0 };
    const ids = new Set(scope.map((n) => n.id));
    const E = edges.filter((e) => ids.has(e.source) && ids.has(e.target) && e.source !== e.target);

    const outs = new Map<string, string[]>();
    const ins = new Map<string, string[]>();
    for (const id of ids) { outs.set(id, []); ins.set(id, []); }
    for (const e of E) { outs.get(e.source)!.push(e.target); ins.get(e.target)!.push(e.source); }

    // ── 1. longest-path layering (Kahn) ──
    const indeg = new Map<string, number>();
    for (const id of ids) indeg.set(id, ins.get(id)!.length);
    const depth = new Map<string, number>();
    const queue: string[] = [];
    for (const id of ids) if (indeg.get(id) === 0) { depth.set(id, 0); queue.push(id); }
    while (queue.length) {
      const id = queue.shift()!;
      const d = depth.get(id)!;
      for (const t of outs.get(id)!) {
        depth.set(t, Math.max(depth.get(t) ?? 0, d + 1));
        indeg.set(t, indeg.get(t)! - 1);
        if (indeg.get(t) === 0) queue.push(t);
      }
    }
    for (const id of ids) if (!depth.has(id)) depth.set(id, 0);  // cycle / detached → col 0
    let maxD = 0;
    for (const d of depth.values()) maxD = Math.max(maxD, d);
    // comments → their own far-right column
    for (const n of scope) if (n.data.defType === 'meta/comment') depth.set(n.id, maxD + 1);
    maxD = 0;
    for (const d of depth.values()) maxD = Math.max(maxD, d);

    const cols: string[][] = Array.from({ length: maxD + 1 }, () => []);
    for (const [id, d] of depth) cols[d]!.push(id);
    // seed order by current Y (keeps hand-tuned layouts roughly stable)
    for (const c of cols) c.sort((a, b) =>
      (nodeById.get(a)!.position.y - nodeById.get(b)!.position.y) ||
      (nodeById.get(a)!.position.x - nodeById.get(b)!.position.x));

    // ── 2. crossing minimisation: median sweeps ──
    for (let sweep = 0; sweep < 6; sweep++) {
      const down = sweep % 2 === 0;
      const rank = new Map<string, number>();
      cols.forEach((c) => c.forEach((id, i) => rank.set(id, i)));
      const order = down
        ? Array.from({ length: maxD }, (_, i) => i + 1)               // 1..maxD, using inputs
        : Array.from({ length: maxD }, (_, i) => maxD - 1 - i);       // maxD-1..0, using outputs
      for (const d of order) {
        const neigh = down ? ins : outs;
        cols[d] = cols[d]!
          .map((id, i) => {
            const m = median(neigh.get(id)!.map((nid) => rank.get(nid)!).filter((v) => v >= 0));
            return { id, key: m >= 0 ? m : i };   // no neighbours → keep current row
          })
          .sort((a, b) => a.key - b.key)
          .map((o) => o.id);
      }
    }

    // ── 3. coordinates ──
    const colW = cols.map((c) => Math.max(FALLBACK.w, ...c.map((id) => sizeOf(id).w)));
    const colX: number[] = [];
    let x = 0;
    for (let d = 0; d <= maxD; d++) { colX[d] = x; x += colW[d]! + H_GAP; }

    const centerY = new Map<string, number>();
    for (let d = 0; d <= maxD; d++) {
      let prevBottom = -Infinity;
      for (const id of cols[d]!) {
        const h = sizeOf(id).h;
        const bary = ins.get(id)!.map((nid) => centerY.get(nid)).filter((v): v is number => v != null);
        let cy = bary.length ? bary.reduce((a, b) => a + b, 0) / bary.length
          : (prevBottom === -Infinity ? h / 2 : prevBottom + V_GAP + h / 2);
        if (cy - h / 2 < prevBottom + V_GAP) cy = prevBottom + V_GAP + h / 2;   // resolve overlap
        centerY.set(id, cy);
        prevBottom = cy + h / 2;
      }
    }

    const ox = parentId ? REGION_PAD : 0;
    const oy = parentId ? REGION_TOP : 0;
    // normalise so the topmost node sits at the scope origin
    let minTop = Infinity;
    for (const id of ids) minTop = Math.min(minTop, centerY.get(id)! - sizeOf(id).h / 2);
    const shift = oy - (Number.isFinite(minTop) ? minTop : 0);

    let right = 0, bottom = 0;
    for (let d = 0; d <= maxD; d++) {
      const cw = colW[d]!;
      for (const id of cols[d]!) {
        const b = sizeOf(id);
        const px = ox + colX[d]! + (cw - b.w) / 2;   // centre node within its column
        const py = centerY.get(id)! - b.h / 2 + shift;
        positions.set(id, { x: px, y: py });
        right = Math.max(right, px + b.w);
        bottom = Math.max(bottom, py + b.h);
      }
    }
    return { right, bottom };
  }

  // Post-order: lay out (and size) inner regions before their parent scope.
  function process(parentId: string | null): { right: number; bottom: number } {
    for (const n of byParent.get(parentId) ?? []) {
      if (byParent.has(n.id)) {
        const ext = process(n.id);
        regionSize.set(n.id, { w: ext.right + REGION_PAD, h: ext.bottom + REGION_PAD });
      }
    }
    return layoutScope(parentId);
  }
  process(null);

  return nodes.map((n) => {
    const p = positions.get(n.id);
    let out = p ? { ...n, position: p } : n;
    const rs = regionSize.get(n.id);
    if (rs) out = { ...out, style: { ...n.style, width: rs.w, height: rs.h } };
    return out;
  });
}
