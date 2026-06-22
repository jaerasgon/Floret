/**
 * MediaPipe landmark registry.
 *
 * Per-node-id wrapper around MediaPipe Tasks-Vision landmarkers (hands /
 * face / pose, or `all` three at once). Mirrors the camera + svg registries:
 *
 *   - `@mediapipe/tasks-vision` is loaded with a DYNAMIC import, so none of
 *     it lands in the base bundle — the ~1 MB JS chunk + the WASM + the
 *     model file(s) are only fetched once a node's `start` goes high.
 *   - WASM and `.task` models are SELF-HOSTED under `public/mediapipe/`
 *     (vendored, Apache-2.0 — see public/mediapipe/NOTICE), so detection
 *     works offline with no CDN dependency.
 *   - rising edge of `start` loads the model(s); falling edge frees them
 *     (don't pay the download / keep a model resident until asked) — the
 *     same privacy/perf gate as the camera node.
 *   - `detectForVideo()` runs once per tick on the wired image; the latest
 *     normalised landmarks are cached for the node to read.
 *
 * `all` runs hands + face + pose simultaneously (≈3× the cost) and merges
 * their results — each tracker keeps its own connection table and group size.
 *
 * v1 limits: GPU video mode, default counts (2 hands / 1 face / 1 pose);
 * blendshapes / world coords / segmentation are not surfaced yet.
 */

import { markDirty } from './dirty';
import { isReadyImageCanvas } from '../nodes/_shared/coerce';

export type Tracker = 'hands' | 'face' | 'pose';
export type TrackerMode = Tracker | 'all';

/** Model file (under public/mediapipe/models) + landmarks-per-detection. */
const MODELS: Record<Tracker, { file: string; group: number }> = {
  hands: { file: 'hand_landmarker.task', group: 21 },
  face:  { file: 'face_landmarker.task', group: 478 },
  pose:  { file: 'pose_landmarker_lite.task', group: 33 },
};
const ALL_TRACKERS: Tracker[] = ['hands', 'face', 'pose'];
const trackersFor = (mode: TrackerMode): Tracker[] => (mode === 'all' ? ALL_TRACKERS : [mode]);

const WASM_PATH = '/mediapipe/wasm';
const MODEL_DIR = '/mediapipe/models';

interface Landmark { x: number; y: number; z: number; }
/** A skeleton edge: indices into a detection's landmark array. */
interface Connection { start: number; end: number; }
type Landmarker = { detectForVideo: (img: CanvasImageSource, ts: number) => unknown; close: () => void };

/** A tracked entity (person/hand/face), kept across frames for stable IDs. */
interface Tracked { id: number; cx: number; cy: number; miss: number; }
/** Squared distance below which a detection is the "same" entity as last frame. */
const MATCH_DIST2 = 0.25 * 0.25;
/** Frames an entity can go unseen before its ID is freed. */
const MAX_MISS = 12;

/** One active tracker within a node (a node in `all` mode has three). */
interface Unit {
  tracker: Tracker;
  landmarker: Landmarker;
  connections: ReadonlyArray<Connection>;
  /** Latest detections, each an array of normalised (0..1) landmarks. */
  dets: ReadonlyArray<ReadonlyArray<Landmark>>;
  /** Stable id per detection (parallel to `dets`) from frame-to-frame tracking. */
  detIds: number[];
  /** Tracked entities for identity persistence. */
  tracked: Tracked[];
}

interface Chain {
  mode: TrackerMode;
  /** Max detections per tracker (numHands / numFaces / numPoses). */
  count: number;
  units: Unit[];        // empty until loaded
  loading: boolean;
  started: boolean;     // all units have a landmarker
  error: string | null;
  lastTs: number;
}

/** What the node reads: one entry per active tracker. */
export interface LandmarkLayer {
  dets: ReadonlyArray<ReadonlyArray<Landmark>>;
  /** Stable id per detection (parallel to `dets`). */
  ids: ReadonlyArray<number>;
  group: number;
  connections: ReadonlyArray<Connection>;
}

class MediaPipeRegistry {
  private chains = new Map<string, Chain>();

  /**
   * Drive a node's landmarker(s). Loads on the rising edge of `start`,
   * reloads on a mode change, frees on the falling edge, and runs one
   * detection per active tracker per call when live.
   */
  ensure(nodeId: string, image: unknown, start: boolean, mode: TrackerMode, count: number): void {
    let chain = this.chains.get(nodeId);
    if (!chain) {
      chain = { mode, count, units: [], loading: false, started: false, error: null, lastTs: 0 };
      this.chains.set(nodeId, chain);
    }

    // Mode or detection-count changes are create-time options → reload.
    if (chain.mode !== mode || chain.count !== count) {
      chain.mode = mode;
      chain.count = count;
      this.teardown(chain);
    }

    if (!start) {
      if (chain.units.length || chain.loading || chain.started) this.teardown(chain);
      return;
    }

    if (chain.units.length === 0 && !chain.loading) {
      void this.load(nodeId, chain, mode, count);
      return;
    }
    if (!chain.started) return; // still loading

    if (!isReadyImageCanvas(image)) return;
    const img = image as CanvasImageSource;
    const ts = Math.max(performance.now(), chain.lastTs + 1);
    chain.lastTs = ts;
    for (const u of chain.units) {
      try {
        const res = u.landmarker.detectForVideo(img, ts) as { landmarks?: Landmark[][]; faceLandmarks?: Landmark[][] };
        u.dets = res.landmarks ?? res.faceLandmarks ?? [];
        this.track(u);
      } catch (err) {
        chain.error = err instanceof Error ? err.message : String(err);
      }
    }
    if (chain.mode === 'all') this.unify(chain);
  }

  /**
   * Unify person ids across trackers in `all` mode (pose-anchored). The three
   * models number their detections independently, so a person's hand, face,
   * and body would otherwise get unrelated ids/colours. Here POSE is the
   * canonical person (its stable tracked id), and each face / hand inherits
   * the id of the nearest pose: a face matches the closest pose's nose
   * (landmark 0), a hand the closest pose's wrist (landmarks 15/16). With no
   * pose detected there's nothing to anchor to, so the per-tracker ids stand.
   */
  private unify(chain: Chain): void {
    const pose = chain.units.find((u) => u.tracker === 'pose');
    if (!pose || pose.dets.length === 0) return;
    const persons = pose.dets.map((det, di) => ({
      id: pose.detIds[di] ?? di,
      nose: det[0],
      wristL: det[15],
      wristR: det[16],
    }));
    for (const u of chain.units) {
      if (u.tracker === 'pose') continue;
      const isHand = u.tracker === 'hands';
      u.detIds = u.dets.map((det) => {
        // anchor point on this detection: hand wrist (lm 0) or face centroid.
        let px: number, py: number;
        if (isHand) { px = det[0]!.x; py = det[0]!.y; }
        else {
          let sx = 0, sy = 0;
          for (const lm of det) { sx += lm.x; sy += lm.y; }
          const n = det.length || 1; px = sx / n; py = sy / n;
        }
        let best = persons[0]!.id, bestD = Infinity;
        for (const p of persons) {
          const targets = isHand ? [p.wristL, p.wristR] : [p.nose];
          for (const t of targets) {
            if (!t) continue;
            const dx = px - t.x, dy = py - t.y;
            const d = dx * dx + dy * dy;
            if (d < bestD) { bestD = d; best = p.id; }
          }
        }
        return best;
      });
    }
  }

  /**
   * Assign a stable id to each detection by matching its centroid to the
   * previous frame's tracked entities (greedy nearest within MATCH_DIST2).
   * Unmatched detections get the smallest free id; entities unseen for
   * MAX_MISS frames are dropped (freeing their id). Keeps each person's id —
   * and therefore their colour — stable as they move, enter, and leave.
   */
  private track(u: Unit): void {
    const cs = u.dets.map((det) => {
      let sx = 0, sy = 0;
      for (const lm of det) { sx += lm.x; sy += lm.y; }
      const n = det.length || 1;
      return { x: sx / n, y: sy / n };
    });
    const ids = new Array<number>(cs.length).fill(-1);
    const matched = new Set<number>();
    const orig = u.tracked.length;
    for (let i = 0; i < cs.length; i++) {
      let best = -1, bestD = MATCH_DIST2;
      for (let t = 0; t < orig; t++) {
        if (matched.has(t)) continue;
        const dx = cs[i]!.x - u.tracked[t]!.cx, dy = cs[i]!.y - u.tracked[t]!.cy;
        const d = dx * dx + dy * dy;
        if (d < bestD) { bestD = d; best = t; }
      }
      if (best >= 0) {
        matched.add(best);
        const tr = u.tracked[best]!;
        tr.cx = cs[i]!.x; tr.cy = cs[i]!.y; tr.miss = 0;
        ids[i] = tr.id;
      }
    }
    for (let i = 0; i < ids.length; i++) {
      if (ids[i]! >= 0) continue;
      const taken = new Set(u.tracked.map((t) => t.id));
      let nid = 0; while (taken.has(nid)) nid++;
      u.tracked.push({ id: nid, cx: cs[i]!.x, cy: cs[i]!.y, miss: 0 });
      ids[i] = nid;
    }
    for (let t = 0; t < orig; t++) { if (!matched.has(t)) u.tracked[t]!.miss++; }
    u.tracked = u.tracked.filter((t) => t.miss <= MAX_MISS);
    u.detIds = ids;
  }

  /** Latest detections per active tracker (null until started). */
  result(nodeId: string): { layers: LandmarkLayer[] } | null {
    const c = this.chains.get(nodeId);
    if (!c || !c.started) return null;
    return {
      layers: c.units.map((u) => ({
        dets: u.dets, ids: u.detIds, group: MODELS[u.tracker].group, connections: u.connections,
      })),
    };
  }

  /** Landmarks-per-detection for a single-tracker node; 0 for `all` (mixed). */
  groupSize(nodeId: string): number {
    const mode = this.chains.get(nodeId)?.mode ?? 'hands';
    return mode === 'all' ? 0 : MODELS[mode].group;
  }

  error(nodeId: string): string | null {
    return this.chains.get(nodeId)?.error ?? null;
  }

  private async load(nodeId: string, chain: Chain, mode: TrackerMode, count: number): Promise<void> {
    chain.loading = true;
    chain.error = null;
    const stale = () => this.chains.get(nodeId) !== chain || chain.mode !== mode || chain.count !== count;
    const n = Math.max(1, Math.min(6, Math.floor(count) || 1));
    try {
      const vision = await import('@mediapipe/tasks-vision');
      const fileset = await vision.FilesetResolver.forVisionTasks(WASM_PATH);
      if (stale()) { chain.loading = false; return; }

      const units: Unit[] = [];
      for (const t of trackersFor(mode)) {
        const base = {
          baseOptions: { modelAssetPath: `${MODEL_DIR}/${MODELS[t].file}`, delegate: 'GPU' as const },
          runningMode: 'VIDEO' as const,
        };
        let landmarker: Landmarker;
        let connections: ReadonlyArray<Connection>;
        if (t === 'hands') {
          landmarker = await vision.HandLandmarker.createFromOptions(fileset, { ...base, numHands: n }) as Landmarker;
          connections = vision.HandLandmarker.HAND_CONNECTIONS;
        } else if (t === 'face') {
          landmarker = await vision.FaceLandmarker.createFromOptions(fileset, { ...base, numFaces: n }) as Landmarker;
          connections = vision.FaceLandmarker.FACE_LANDMARKS_TESSELATION;
        } else {
          landmarker = await vision.PoseLandmarker.createFromOptions(fileset, { ...base, numPoses: n }) as Landmarker;
          connections = vision.PoseLandmarker.POSE_CONNECTIONS;
        }
        if (stale()) { landmarker.close(); units.forEach((u) => u.landmarker.close()); chain.loading = false; return; }
        units.push({ tracker: t, landmarker, connections, dets: [], detIds: [], tracked: [] });
      }
      chain.units = units;
      chain.started = true;
      chain.loading = false;
      markDirty(); // wake a tick so detection begins
    } catch (err) {
      chain.error = err instanceof Error ? err.message : String(err);
      chain.loading = false;
      // eslint-disable-next-line no-console
      console.warn('mediapipe load failed:', err);
    }
  }

  private teardown(chain: Chain): void {
    for (const u of chain.units) { try { u.landmarker.close(); } catch { /* ignore */ } }
    chain.units = [];
    chain.loading = false;
    chain.started = false;
  }

  // ── Lifecycle (image/camera-registry pattern) ────────────────────────────

  dispose(nodeId: string): void {
    const c = this.chains.get(nodeId);
    if (!c) return;
    this.teardown(c);
    this.chains.delete(nodeId);
  }

  retain(liveNodeIds: ReadonlySet<string>): void {
    for (const [id, c] of this.chains) {
      if (liveNodeIds.has(id)) continue;
      this.teardown(c);
      this.chains.delete(id);
    }
  }

  reset(): void {
    for (const c of this.chains.values()) this.teardown(c);
    this.chains.clear();
  }
}

export const mediapipeRegistry = new MediaPipeRegistry();

if (typeof window !== 'undefined') {
  (window as unknown as { __mediapipeRegistry?: unknown }).__mediapipeRegistry = mediapipeRegistry;
}
