import { registerNode } from '../registry';
import { mediapipeRegistry, type TrackerMode } from '../../runtime/mediapipe-registry';
import type { Color, PortValue, Style, Vec2 } from '../../runtime/types';
import type { SceneNode } from '../../runtime/scene';
import { resolveStyle, colorToCss, LINE_DEFAULT_STYLE } from '../../runtime/scene';
import type { NodeParamContext } from '../types';

/** Fixed per-person palette (0..1 colour), cycled by stable id:
 *  yellow → green → blue → orange → magenta → cyan. */
const PALETTE: ReadonlyArray<Color> = [
  { r: 1.00, g: 0.84, b: 0.00, a: 1 }, // yellow
  { r: 0.24, g: 0.86, b: 0.35, a: 1 }, // green
  { r: 0.27, g: 0.51, b: 1.00, a: 1 }, // blue
  { r: 1.00, g: 0.55, b: 0.16, a: 1 }, // orange
  { r: 0.86, g: 0.31, b: 0.90, a: 1 }, // magenta
  { r: 0.16, g: 0.86, b: 0.90, a: 1 }, // cyan
];
function colorForId(id: number): Color {
  return PALETTE[((id % PALETTE.length) + PALETTE.length) % PALETTE.length]!;
}

/**
 * Landmarks — body/face/hand tracking via MediaPipe Tasks-Vision.
 *
 * Wire a camera (or any) `image` in and flip `start` on: the node lazily
 * loads the chosen tracker's model (self-hosted under public/mediapipe) and
 * runs detection each frame. Privacy/perf-safe — nothing loads until `start`.
 *
 * `tracker` picks hands / face / pose, or `all` (run all three at once).
 * `count` is the max detections per tracker (e.g. numPoses) — raise it to
 * track multiple people/hands. Each detection gets a STABLE id (tracked
 * frame-to-frame by the registry), surfaced as `group`, and an automatic
 * distinct `color` (parallel to `points`) / `connColor` (parallel to the
 * connection spreads) so every person is consistently coloured. In `all`
 * mode the id is unified across the three trackers (pose-anchored): each
 * face/hand inherits the nearest pose's person id, so one person's body,
 * face, and hands share a colour. `person` filters the output to a single
 * tracked id: 0 = everyone, 1 = person #1, etc.
 * Outputs every detected landmark as a `vec2` spread in CENTRED canvas coords
 * (scaled by `size`, optionally `mirror`ed), a parallel `z` depth spread, and
 * a parallel `group` spread giving the detection index (0,1,…) each point
 * belongs to — so you can partition the points per hand/face/person.
 * `detections` is how many were found; `perGroup` is the landmarks-per-
 * detection (21 hands / 478 face / 33 pose; 0 in `all` mode, since it's mixed).
 *
 * See mediapipe-registry for loading + v1 limits.
 */
export const landmarksNode = registerNode({
  type: 'input/landmarks',
  category: 'Input',
  label: 'Landmarks',
  description: 'Hand / face / pose tracking (MediaPipe). Wire an image + flip `start`; outputs landmark points, depth, and per-detection grouping.',
  defaultParams: { tracker: 'hands', mirror: true, count: 2, person: 0 },
  paramOptions: { tracker: ['hands', 'face', 'pose', 'all'], mirror: ['false', 'true'] },
  inputs: [
    { name: 'image', type: 'image' },
    { name: 'start', type: 'boolean', default: false },
    { name: 'size', type: 'number', default: 500 },
    // Toggle the ready-drawn `guides` overlay layer on/off (data outputs are
    // unaffected — only the rendered skeleton layer is suppressed when off).
    { name: 'guides', type: 'boolean', default: true },
    // Style for the `guides` overlay (e.g. a Stroke node to drop the line
    // width). Per-person colour is kept — only weight/cap/etc. apply.
    { name: 'style', type: 'style' },
  ],
  outputs: [
    { name: 'points', type: 'vec2', kind: 'spread' },
    { name: 'z', type: 'number', kind: 'spread' },
    { name: 'group', type: 'number', kind: 'spread' },
    // Auto per-person colour, parallel to `points` (a distinct stable hue per
    // tracked id) — wire it into the fill so each person is coloured.
    { name: 'color', type: 'color', kind: 'spread' },
    // Skeleton edges as two parallel vec2 spreads: segment i runs connA[i]→connB[i].
    // Iterate them in lockstep through a ForEach and draw a line per pair.
    { name: 'connA', type: 'vec2', kind: 'spread' },
    { name: 'connB', type: 'vec2', kind: 'spread' },
    // Per-segment colour, parallel to connA/connB (same per-person hue).
    { name: 'connColor', type: 'color', kind: 'spread' },
    // Ready-drawn overlay: a coloured line per skeleton edge + a dot per
    // landmark, using each person's hue. Toggle with `guides`, restyle (e.g.
    // thinner stroke) with `style`. Wire straight into the renderer.
    { name: 'guides', type: 'layer', kind: 'spread' },
    // Spread-of-spreads: people[i] is person i's own point spread (length =
    // `count`, indexed by stable id; absent people are empty). Feed into a
    // spread/ungroup node to fan each person out to its own pin.
    { name: 'people', type: 'vec2', kind: 'spread' },
    // One palette colour per person slot (parallel to people).
    { name: 'personColors', type: 'color', kind: 'spread' },
    { name: 'detections', type: 'number' },
    { name: 'perGroup', type: 'number' },
    { name: 'detected', type: 'boolean' },
  ],
  effectful: true,
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const image = (inputs[0] ?? [])[0] ?? null;
    const start = !!((inputs[1] ?? [])[0]);
    const size = Number((inputs[2] ?? [])[0] ?? 500) || 500;
    const tracker = params.param<string>('tracker', 'hands') as TrackerMode;
    const mirror = params.param<string | boolean>('mirror', true) === true
      || params.param<string | boolean>('mirror', true) === 'true';
    const count = Math.max(1, Math.min(6, Math.floor(Number(params.param<number>('count', 2))) || 1));
    const person = Math.max(0, Math.floor(Number(params.param<number>('person', 0))) || 0);
    const guidesOn = ((inputs[3] ?? [])[0] ?? true) !== false;
    const styleFn = ((inputs[4] ?? [])[0] ?? null) as Style | null;

    mediapipeRegistry.ensure(params.nodeId, image, start, tracker, count);
    const r = mediapipeRegistry.result(params.nodeId);
    const perGroup = mediapipeRegistry.groupSize(params.nodeId);

    const toCanvas = (lm: { x: number; y: number }): Vec2 => ({
      x: ((mirror ? 1 - lm.x : lm.x) - 0.5) * size,
      y: (lm.y - 0.5) * size,
    });

    // Flatten every detection across every active tracker into parallel
    // spreads, keyed by each detection's STABLE id (so colour stays with the
    // person). `person` > 0 keeps only that id.
    const points: Vec2[] = [];
    const zs: number[] = [];
    const group: number[] = [];
    const color: Color[] = [];
    const connA: Vec2[] = [];
    const connB: Vec2[] = [];
    const connColor: Color[] = [];
    // Spread-of-spreads: one sub-spread per person slot (indexed by stable id).
    const people: Vec2[][] = Array.from({ length: count }, () => []);
    const personColors: Color[] = Array.from({ length: count }, (_, i) => colorForId(i));
    let detCount = 0;
    for (const layer of r?.layers ?? []) {
      layer.dets.forEach((det, di) => {
        const id = layer.ids[di] ?? di;
        if (person > 0 && id !== person - 1) return;
        detCount++;
        const col = colorForId(id);
        const bucket = id < count ? people[id]! : null;
        for (const lm of det) {
          const p = toCanvas(lm);
          points.push(p);
          zs.push(lm.z * size);
          group.push(id);
          color.push(col);
          if (bucket) bucket.push(p);
        }
        for (const c of layer.connections) {
          const a = det[c.start];
          const b = det[c.end];
          if (a && b) { connA.push(toCanvas(a)); connB.push(toCanvas(b)); connColor.push(col); }
        }
      });
    }

    // Ready-drawn overlay layer: a coloured line per edge + a dot per point,
    // styled by the `style` input (per-person colour preserved). Empty when
    // the `guides` toggle is off.
    const guides: SceneNode[] = [];
    if (guidesOn) {
      const base = resolveStyle(styleFn, { ...LINE_DEFAULT_STYLE, strokeWeight: 2 });
      for (let i = 0; i < connA.length; i++) {
        const a = connA[i]!, b = connB[i]!;
        guides.push({ kind: 'line', x1: a.x, y1: a.y, x2: b.x, y2: b.y, style: { ...base, stroke: colorToCss(connColor[i]!) } });
      }
      for (let i = 0; i < points.length; i++) {
        guides.push({ kind: 'point', x: points[i]!.x, y: points[i]!.y, style: { ...base, stroke: colorToCss(color[i]!) } });
      }
    }

    return [points, zs, group, color, connA, connB, connColor, guides, people, personColors, [detCount], [perGroup], [points.length > 0]];
  },
});
