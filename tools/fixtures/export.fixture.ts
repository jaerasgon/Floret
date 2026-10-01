/**
 * Golden-fixture exporter for the native (Rust) port.
 *
 * Writes, from the TypeScript implementation (the reference):
 *   native/samples/<id>.flora        every built-in sample as a real patch file
 *   native/samples/catalogue.json    sample order / name / category / description
 *   native/fixtures/registry.json    every NodeDef: ports, defaults, flags
 *   native/fixtures/traces/<id>.json.gz  compile result + per-frame node outputs
 *
 * The Rust engine loads the same .flora, ticks the same frames and must
 * reproduce the same outputs. Everything here is deterministic: fixed time
 * steps, scripted mouse/keyboard, seeded Math.random, mocked clocks.
 *
 * Run: npm run fixtures:export
 */
import { it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

import '../../src/nodes';
import { nodeRegistry } from '../../src/nodes/registry';
import { getResolvedInputs, type NodeDef } from '../../src/nodes/types';
import { SAMPLES } from '../../src/persistence/samples';
import { serializePatch, deserializePatch } from '../../src/persistence/serialize';
import { migratePatch } from '../../src/persistence/migrate';
import type { PatchFile } from '../../src/persistence/schema';
import { compileGraph, parseLoopVarHandle } from '../../src/runtime/compile';
import { tick } from '../../src/runtime/tick';
import { paramStore } from '../../src/runtime/param-store';
import { NEUTRAL_STYLE } from '../../src/runtime/scene';
import type { FrameCtx, PortType } from '../../src/runtime/types';
import { parseHandle } from '../../src/editor/connection-rules';
import { toEditorRefs } from '../../src/editor/editor-state/util';
import { resetRegistries } from '../../src/editor/editor-state/registry-sync';
import type { FloretEdge, FloretNode } from '../../src/editor/editor-state';

const ROOT = path.resolve(__dirname, '../..');
const SAMPLES_DIR = path.join(ROOT, 'native/samples');
const FIXTURES_DIR = path.join(ROOT, 'native/fixtures');
const TRACES_DIR = path.join(FIXTURES_DIR, 'traces');

/** Format version of the trace files. Bump when the shape changes. */
const TRACE_FORMAT = 2;
/** Ticks run per sample (frame 0 .. TICKS-1). */
const TICKS = 61;
/** Frames whose outputs are written to the trace. */
const RECORDED = new Set([0, 1, 2, 10, 30, 60]);
const DT = 1 / 60;
const FIXED_DATE = '2026-01-01T00:00:00.000Z';

// ─── Determinism ────────────────────────────────────────────────────────────

function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

let clockMs = 0;
const realRandom = Math.random;
function installDeterminism(): void {
  let rng = mulberry32(0x5eed);
  Math.random = () => rng();
  (installDeterminism as { reseed?: () => void }).reseed = () => { rng = mulberry32(0x5eed); };
  const epoch = Date.parse(FIXED_DATE);
  globalThis.performance.now = () => clockMs;
  Date.now = () => epoch + clockMs;
}
function reseed(): void {
  (installDeterminism as { reseed?: () => void }).reseed?.();
  clockMs = 0;
}

/** Scripted input for frame i. Mouse is centre-origin, like the driver. */
function frameFor(i: number, width: number, height: number): FrameCtx {
  const t = i * DT;
  const keyDown = i >= 10 && i < 20;
  return {
    t,
    dt: DT,
    frameCount: i,
    width,
    height,
    mouseX: Math.round(200 * Math.cos(t * 2) * 1000) / 1000,
    mouseY: Math.round(150 * Math.sin(t * 3) * 1000) / 1000,
    mouseIsPressed: i % 40 >= 20,
    key: i >= 10 ? 'a' : '',
    keyIsPressed: keyDown,
  };
}

/** Canvas size the driver would use: renderer's paramStore width/height, clamped. */
function canvasSize(rendererId: string | null): { width: number; height: number } {
  const clamp = (v: number) => Math.max(32, Math.min(4096, Math.floor(v)));
  if (!rendererId) return { width: 1080, height: 1920 };
  const w = paramStore.getKey<number>(rendererId, 'width');
  const h = paramStore.getKey<number>(rendererId, 'height');
  return {
    width: clamp(typeof w === 'number' && Number.isFinite(w) ? w : 1080),
    height: clamp(typeof h === 'number' && Number.isFinite(h) ? h : 1920),
  };
}

// ─── Canonical value encoding ───────────────────────────────────────────────
//
// JSON can't carry everything a wire can: undefined vs null, NaN/Infinity,
// closures (Style), typed arrays, Maps, browser objects. Tagged objects keep
// those distinctions so the Rust side can compare exactly:
//   {"$u":1}                         undefined
//   {"$num":"NaN"|"Infinity"|"-Infinity"}
//   {"$style":{...ResolvedStyle}}    a Style closure, applied to NEUTRAL_STYLE
//   {"$fn":"name"}                   any other function
//   {"$ta":"Float32Array","len":n,"fnv":"hex","head":[...]}
//   {"$map":[[k,v],...]}  {"$set":[...]}
//   {"$opaque":"ClassName"}          browser/runtime resource (canvas, audio node)
//   {"$class":"Name", ...fields}     non-plain object instance
//
// Shared references. Scene trees are DAGs: one layer object is reachable
// from many outputs (draw → transform → group → renderer) and diamonds in
// the patch multiply that. Expanding every path is exponential, so an
// object (or array) reached more than once in one encoding scope is written
// in full the FIRST time, tagged {"$id":n,...} (arrays: {"$id":n,"$arr":[…]}),
// and every later occurrence is {"$ref":n}. A reference to an object still
// being written (a true cycle) is {"$cycle":1}.

const OPAQUE = /Canvas|Image|Audio|Context|Element|Bitmap|Video|Stream|Node$|Mock/;

function fnv1a(bytes: Uint8Array): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) {
    h ^= bytes[i]!;
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, '0');
}

function canonNumber(n: number): unknown {
  if (Number.isNaN(n)) return { $num: 'NaN' };
  if (n === Infinity) return { $num: 'Infinity' };
  if (n === -Infinity) return { $num: '-Infinity' };
  return Object.is(n, -0) ? 0 : n;
}

function ctorName(obj: object): string {
  return (obj as { constructor?: { name?: string } }).constructor?.name ?? 'Object';
}

function styleOf(fn: unknown): object | null {
  try {
    const r = (fn as (s: unknown) => unknown)(NEUTRAL_STYLE);
    if (r && typeof r === 'object' && 'strokeWeight' in r) return r;
  } catch { /* not a style */ }
  return null;
}

/** Children an object is encoded through (must match `encode`). */
function children(obj: object): Iterable<unknown> {
  if (ArrayBuffer.isView(obj)) return [];
  if (Array.isArray(obj)) return obj;
  if (obj instanceof Map) return [...obj].flat();
  if (obj instanceof Set) return obj;
  const ctor = ctorName(obj);
  if (ctor !== 'Object' && OPAQUE.test(ctor)) return [];
  return Object.values(obj);
}

/** One encoding scope: shared objects get ids that are only valid within it. */
function canon(root: unknown): unknown {
  const counts = new Map<object, number>();
  const count = (v: unknown): void => {
    if (v === null || typeof v !== 'object') return;
    const c = counts.get(v) ?? 0;
    counts.set(v, c + 1);
    if (c > 0) return;
    for (const ch of children(v)) count(ch);
  };
  count(root);

  const ids = new Map<object, number>();
  const open = new Set<object>();
  const encode = (v: unknown): unknown => {
    if (v === undefined) return { $u: 1 };
    if (v === null) return null;
    switch (typeof v) {
      case 'number': return canonNumber(v);
      case 'boolean':
      case 'string': return v;
      case 'bigint': return { $big: String(v) };
      case 'symbol': return { $sym: String(v) };
      case 'function': {
        const st = styleOf(v);
        return st ? { $style: canon(st) } : { $fn: (v as { name?: string }).name || 'anonymous' };
      }
    }
    const obj = v as object;
    if (open.has(obj)) return { $cycle: 1 };
    const seenId = ids.get(obj);
    if (seenId !== undefined) return { $ref: seenId };
    const shared = (counts.get(obj) ?? 0) > 1;
    let id: number | undefined;
    if (shared) { id = ids.size; ids.set(obj, id); }
    open.add(obj);
    try {
      let out: unknown;
      if (ArrayBuffer.isView(obj)) {
        const ta = obj as unknown as { length: number; buffer: ArrayBuffer; byteOffset: number; byteLength: number } & ArrayLike<number>;
        const head: unknown[] = [];
        for (let i = 0; i < Math.min(8, ta.length); i++) head.push(canonNumber(ta[i]!));
        out = { $ta: ctorName(obj), len: ta.length, fnv: fnv1a(new Uint8Array(ta.buffer, ta.byteOffset, ta.byteLength)), head };
      } else if (Array.isArray(obj)) {
        const arr = obj.map(encode);
        out = shared ? { $arr: arr } : arr;
      } else if (obj instanceof Map) {
        out = { $map: [...obj].map(([k, x]) => [encode(k), encode(x)]) };
      } else if (obj instanceof Set) {
        out = { $set: [...obj].map(encode) };
      } else {
        const ctor = ctorName(obj);
        if (ctor !== 'Object' && OPAQUE.test(ctor)) {
          out = { $opaque: ctor };
        } else {
          const o: Record<string, unknown> = ctor !== 'Object' ? { $class: ctor } : {};
          for (const k of Object.keys(obj)) o[k] = encode((obj as Record<string, unknown>)[k]);
          out = o;
        }
      }
      if (id !== undefined) return { $id: id, ...(out as Record<string, unknown>) };
      return out;
    } finally {
      open.delete(obj);
    }
  };
  return encode(root);
}

// ─── Oversized outputs ──────────────────────────────────────────────────────
//
// A few nodes emit millions of objects per frame (grid-art's walker cluster:
// ~22M). Those outputs are replaced by {"$summary": stats} where stats are
// computed over the value as a TREE (shared objects counted every time they
// are reached), so they don't depend on how an implementation shares memory:
//   objects  number of objects/arrays     numbers  number of numeric leaves
//   sum/abs  sum and absolute sum of finite numeric leaves
//   strings  number of string leaves      kinds    count of objects per `kind`

const SUMMARY_LIMIT = 200_000;

interface Stats { objects: number; numbers: number; sum: number; abs: number; strings: number; kinds: Record<string, number> }

function uniqueObjects(v: unknown, limit: number): number {
  const seen = new Set<object>();
  const stack: unknown[] = [v];
  while (stack.length) {
    const x = stack.pop();
    if (x === null || typeof x !== 'object' || seen.has(x)) continue;
    seen.add(x);
    if (seen.size > limit) return seen.size;
    for (const c of children(x)) stack.push(c);
  }
  return seen.size;
}

function makeSummarizer(): (v: unknown) => Stats {
  const memo = new Map<object, Stats>();
  const open = new Set<object>();
  const leaf = (v: unknown): Stats => {
    const s: Stats = { objects: 0, numbers: 0, sum: 0, abs: 0, strings: 0, kinds: {} };
    if (typeof v === 'number') {
      s.numbers = 1;
      if (Number.isFinite(v)) { s.sum = v; s.abs = Math.abs(v); }
    } else if (typeof v === 'string') s.strings = 1;
    return s;
  };
  const add = (a: Stats, b: Stats) => {
    a.objects += b.objects; a.numbers += b.numbers; a.sum += b.sum; a.abs += b.abs; a.strings += b.strings;
    for (const k in b.kinds) a.kinds[k] = (a.kinds[k] ?? 0) + b.kinds[k]!;
  };
  const visit = (v: unknown): Stats => {
    if (v === null || typeof v !== 'object') return leaf(v);
    const hit = memo.get(v);
    if (hit) return hit;
    if (open.has(v)) return leaf(undefined);
    open.add(v);
    const s: Stats = { objects: 1, numbers: 0, sum: 0, abs: 0, strings: 0, kinds: {} };
    const kind = (v as { kind?: unknown }).kind;
    if (typeof kind === 'string') s.kinds[kind] = 1;
    for (const c of children(v)) add(s, visit(c));
    open.delete(v);
    memo.set(v, s);
    return s;
  };
  return visit;
}

// ─── Editor-side normalisation (what loading a sample in the app does) ──────

/** Mirror of compile-pipeline.ts `reconcileAdaptiveTypes` (the valueType latch). */
function reconcileAdaptiveTypes(nodes: FloretNode[], edges: FloretEdge[]): FloretNode[] {
  const byId = new Map(nodes.map((n) => [n.id, n] as const));
  const portConcreteType = (nodeId: string, handle: string | null | undefined): PortType | null => {
    const parsed = parseHandle(handle);
    if (!parsed) return null;
    const n = byId.get(nodeId);
    const def = n ? nodeRegistry.get(n.data.defType) : undefined;
    if (!n || !def) return null;
    let t: PortType | undefined;
    if (def.adaptiveType) {
      const vt = n.data.params.valueType;
      t = vt === 'vec2' || vt === 'color' || vt === 'number' ? vt : undefined;
    } else if (def.regionKind) {
      if (parseLoopVarHandle(parsed.name)) t = 'number';
      else t = n.data.ports?.find((p) => p.id === parsed.name)?.type
        ?? def.inputs.find((p) => p.name === parsed.name)?.type;
    } else if (parsed.kind === 'out') {
      t = def.outputs.find((p) => p.name === parsed.name)?.type;
    } else {
      t = getResolvedInputs(def, n.data.params).find((p) => p.name === parsed.name)?.type;
    }
    if (!t || t === 'any') return null;
    if (t === 'boolean') return 'number';
    return t === 'number' || t === 'vec2' || t === 'color' ? t : null;
  };
  return nodes.map((n) => {
    const def = nodeRegistry.get(n.data.defType);
    if (!def?.adaptiveType) return n;
    const cur = (n.data.params.valueType as string | undefined) ?? 'any';
    const insA = getResolvedInputs(def, { ...n.data.params, valueType: 'number' });
    const insB = getResolvedInputs(def, { ...n.data.params, valueType: 'vec2' });
    const adaptiveIns = new Set<string>();
    insA.forEach((p, i) => { if (insB[i] && insB[i]!.type !== p.type) adaptiveIns.add(p.name); });
    const touching = edges.filter((e) => {
      if (e.source === n.id) return true;
      if (e.target === n.id) {
        const p = parseHandle(e.targetHandle);
        return !!p && adaptiveIns.has(p.name);
      }
      return false;
    });
    let next = cur;
    if (touching.length === 0) next = 'any';
    else if (cur === 'any') {
      for (const e of touching) {
        const peer = e.source === n.id
          ? portConcreteType(e.target, e.targetHandle)
          : portConcreteType(e.source, e.sourceHandle);
        if (peer) { next = peer; break; }
      }
    }
    if (next === cur) return n;
    return { ...n, data: { ...n.data, params: { ...n.data.params, valueType: next } } };
  });
}

function stamp(patch: PatchFile, name: string): PatchFile {
  return { ...patch, meta: { name, createdAt: FIXED_DATE, modifiedAt: FIXED_DATE } };
}

/** Load a .flora exactly like the app does: migrate → fix-ups → latch. */
function loadPatch(patch: PatchFile): { nodes: FloretNode[]; edges: FloretEdge[] } {
  const migrated = migratePatch(patch).patch;
  const { nodes, edges } = deserializePatch(migrated);
  return { nodes: reconcileAdaptiveTypes(nodes, edges), edges };
}

// ─── Registry manifest ──────────────────────────────────────────────────────

function portJson(p: NodeDef['inputs'][number]): unknown {
  const out: Record<string, unknown> = { name: p.name, type: p.type, kind: p.kind ?? 'single' };
  if (p.label !== undefined) out.label = p.label;
  if (p.default !== undefined) out.default = canon(p.default);
  if (p.param !== undefined) out.param = p.param;
  if (p.integer) out.integer = true;
  if (p.constant) out.constant = true;
  return out;
}

function defJson(def: NodeDef): unknown {
  const flags: Record<string, unknown> = {};
  for (const k of ['stateful', 'effectful', 'statefulNextPort', 'regionKind', 'regionRole', 'hidden', 'compact', 'adaptiveType'] as const) {
    if (def[k] !== undefined) flags[k] = def[k];
  }
  const evalKind = def.readFromContext ? 'context'
    : def.computeRaw ? 'raw'
    : def.compute ? 'scalar'
    : 'none';
  return {
    type: def.type,
    category: def.category,
    label: def.label,
    description: def.description ?? null,
    inputs: def.inputs.map(portJson),
    outputs: def.outputs.map(portJson),
    eval: evalKind,
    dynamicInputs: !!def.dynamicInputs,
    dynamicOutputs: !!def.dynamicOutputs,
    customUi: !!def.component,
    defaultParams: def.defaultParams ? canon(def.defaultParams) : null,
    paramOptions: def.paramOptions ?? null,
    ...flags,
  };
}

// ─── Export ─────────────────────────────────────────────────────────────────

function writeJson(file: string, data: unknown, pretty: boolean): void {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(data, null, pretty ? 2 : undefined) + '\n');
}

/** Traces are large; store gzipped (`<id>.json.gz`). */
function writeTrace(id: string, data: unknown): void {
  fs.mkdirSync(TRACES_DIR, { recursive: true });
  const json = Buffer.from(JSON.stringify(data));
  fs.writeFileSync(path.join(TRACES_DIR, `${id}.json.gz`), zlib.gzipSync(json, { level: 9 }));
}

it('exports samples, registry and traces', () => {
  installDeterminism();
  const only = process.env.FIXTURE_ONLY?.split(',');
  if (!only) {
    fs.rmSync(TRACES_DIR, { recursive: true, force: true });
    fs.rmSync(SAMPLES_DIR, { recursive: true, force: true });
  }

  // Registry manifest.
  const defs = nodeRegistry.all().slice().sort((a, b) => a.type.localeCompare(b.type));
  writeJson(path.join(FIXTURES_DIR, 'registry.json'), {
    format: TRACE_FORMAT,
    count: defs.length,
    nodes: defs.map(defJson),
  }, true);

  const catalogue: unknown[] = [];
  const summary: string[] = [];

  for (const sample of SAMPLES) {
    if (only && !only.includes(sample.id)) continue;
    reseed();
    resetRegistries();

    // 1. Build → serialise → load → re-serialise: the normalised .flora.
    const built = sample.build();
    const raw = serializePatch({ nodes: built.nodes, edges: built.edges, name: sample.name });
    const normal = loadPatch(raw);
    const flora = stamp(serializePatch({ ...normal, name: sample.name }), sample.name);
    const floraText = JSON.stringify(flora, null, 2) + '\n';
    fs.mkdirSync(SAMPLES_DIR, { recursive: true });
    fs.writeFileSync(path.join(SAMPLES_DIR, `${sample.id}.flora`), floraText);
    catalogue.push({
      id: sample.id, name: sample.name, category: sample.category ?? 'Misc',
      description: sample.description, file: `${sample.id}.flora`,
    });

    // 2. Load the file we just wrote (round-trip proves it's self-contained).
    const { nodes, edges } = loadPatch(JSON.parse(floraText) as PatchFile);
    paramStore.replaceAll(nodes.map((n) => [n.id, n.data.params] as const));
    const refs = toEditorRefs({ nodes, edges });
    const nodeById = new Map(nodes.map((n) => [n.id, n] as const));

    const errors: string[] = [];
    const realWarn = console.warn;
    console.warn = (...args: unknown[]) => {
      const msg = args.map((a) => (a instanceof Error ? a.message : String(a))).join(' ');
      if (!errors.includes(msg)) errors.push(msg);
    };

    const trace: Record<string, unknown> = {
      format: TRACE_FORMAT, sample: sample.id, dt: DT, ticks: TICKS,
    };
    try {
      let result;
      try {
        result = compileGraph(refs.nodes, refs.edges, {
          registry: nodeRegistry,
          paramReader: (id) => nodeById.get(id)?.data.params ?? {},
          portValueReader: (id, port) => nodeById.get(id)?.data.portValues?.[port],
        });
      } catch (err) {
        trace.compileError = err instanceof Error ? err.message : String(err);
        writeTrace(sample.id, trace);
        summary.push(`${sample.id}: COMPILE ERROR ${trace.compileError}`);
        continue;
      }
      const g = result.graph;
      trace.compile = {
        warnings: result.warnings.map((w) => w.message),
        alwaysAnimate: !!g.alwaysAnimate,
        rendererIds: g.rendererIds,
        scopes: [...g.scopes].map(([key, s]) => ({
          parent: key, order: s.order, splicerIn: s.splicerInIds,
          splicerOut: s.splicerOutIds, accumulators: s.accumulatorIds,
        })),
        nodes: [...g.nodes.values()].map((n) => ({
          id: n.id, type: n.type, parent: n.parentId ?? null,
          inArity: n.inArity, outArity: n.outArity,
          defaults: canon(n.defaults),
          constants: n.constantInPorts ? [...n.constantInPorts] : [],
          constantValues: n.constants ? canon(n.constants) : null,
          multi: n.multiInPorts ? [...n.multiInPorts] : [],
          memoisable: !!n.memoisable,
          stateful: !!n.stateful,
          role: n.splicerRole ?? null,
          regionKind: n.regionKind ?? null,
        })),
        edges: [...g.edgesByTarget.values()].flat(),
      };

      const frames: unknown[] = [];
      const inputs: unknown[] = [];
      trace.inputs = inputs;
      for (let i = 0; i < TICKS; i++) {
        const { width, height } = canvasSize(g.rendererId);
        const frame = frameFor(i, width, height);
        inputs.push(canon(frame));
        clockMs = frame.t * 1000;
        let out;
        try {
          out = tick(g, frame);
        } catch (err) {
          // Source nodes (readFromContext) aren't wrapped by safeCompute, so
          // a browser-only resource (Image, getUserMedia) aborts the whole
          // tick. Record it; the sample is browser-dependent.
          trace.tickError = { frame: i, message: err instanceof Error ? err.message : String(err) };
          break;
        }
        if (!RECORDED.has(i)) continue;
        // One encoding scope per frame so layers shared between nodes'
        // outputs are written once.
        const summarize = makeSummarizer();
        const recorded: Record<string, unknown> = {};
        for (const [id, ports] of out.outputs) {
          recorded[id] = uniqueObjects(ports, SUMMARY_LIMIT) > SUMMARY_LIMIT
            ? { $summary: summarize(ports) }
            : ports;
        }
        frames.push({
          frame: canon(frame),
          outputs: canon(recorded),
          regionStats: Object.fromEntries([...out.regionStats].map(([id, s]) => [id, canon(s)])),
        });
      }
      trace.frames = frames;
      trace.errors = errors;
      writeTrace(sample.id, trace);
      const tickErr = trace.tickError ? ` TICK ERROR @${(trace.tickError as { frame: number }).frame}` : '';
      summary.push(`${sample.id}: ${g.nodes.size} nodes, ${result.warnings.length} warnings, ${errors.length} runtime errors${tickErr}`);
    } finally {
      console.warn = realWarn;
    }
  }

  if (!only) writeJson(path.join(SAMPLES_DIR, 'catalogue.json'), { samples: catalogue }, true);
  Math.random = realRandom;
  // eslint-disable-next-line no-console
  console.log(summary.join('\n'));
});
