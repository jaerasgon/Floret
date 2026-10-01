/**
 * Golden SVG fixtures for the native (Rust) SVG exporter.
 *
 * Runs the TypeScript exporter (`src/runtime/scene-svg.ts`, the reference)
 * over scenes and writes {scene, expected SVG} pairs to
 * `native/fixtures/svg/*.json.gz`. The Rust test (`floret-engine/tests/svg.rs`)
 * decodes each scene into `SceneNode`s and must reproduce the SVG byte for
 * byte.
 *
 * Scenes come from two places:
 *  - `samples/<id>.json.gz`: each renderer's flattened layers in the
 *    recorded traces (`native/fixtures/traces`, so run `fixtures:export`
 *    first), at frames 0 and 60, exactly as the canvas driver hands them to
 *    the exporter (same flattening, same background).
 *  - `edge-cases.json.gz`: hand-built scenes for features and corner cases
 *    the samples don't reach.
 *
 * Scene kinds the Rust scene model doesn't have yet (`image`,
 * `particle-batch`, `particle-trail-batch`, `shader`) are removed from the
 * scene before export; the fixture lists how many were dropped.
 *
 * Run: npm run fixtures:svg
 */
import { it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

import { sceneToSvg } from '../../src/runtime/scene-svg';
import {
  DEFAULT_STYLE,
  LINE_DEFAULT_STYLE,
  NEUTRAL_STYLE,
  colorToCss,
  type ResolvedStyle,
  type SceneNode,
} from '../../src/runtime/scene';
import type { Color } from '../../src/runtime/types';

const ROOT = path.resolve(__dirname, '../..');
const TRACES_DIR = path.join(ROOT, 'native/fixtures/traces');
const OUT_DIR = path.join(ROOT, 'native/fixtures/svg');
const FORMAT = 1;
/** Recorded trace frames to export. */
const FRAMES = [0, 60];
/** Kinds the Rust scene model can't represent yet. */
const UNSUPPORTED = new Set(['image', 'particle-batch', 'particle-trail-batch', 'shader']);

// ─── Trace decoding (inverse of export.fixture.ts `canon`) ──────────────────

class Tagged {
  constructor(readonly tag: string) {}
}

function decodeTrace(root: unknown): unknown {
  const refs = new Map<number, unknown>();
  const dec = (j: unknown): unknown => {
    if (j === null || typeof j !== 'object') return j;
    if (Array.isArray(j)) return j.map(dec);
    const o = j as Record<string, unknown>;
    if ('$ref' in o) return refs.get(o.$ref as number);
    if ('$cycle' in o) return undefined;
    if ('$u' in o) return undefined;
    if ('$num' in o) return Number(o.$num);
    const id = o.$id as number | undefined;
    if ('$arr' in o) {
      const arr: unknown[] = [];
      if (id !== undefined) refs.set(id, arr);
      for (const x of o.$arr as unknown[]) arr.push(dec(x));
      return arr;
    }
    const tag = Object.keys(o).find((k) => k.startsWith('$') && k !== '$id' && k !== '$class');
    if (tag) {
      const t = new Tagged(tag);
      if (id !== undefined) refs.set(id, t);
      return t;
    }
    const out: Record<string, unknown> = {};
    if (id !== undefined) refs.set(id, out);
    for (const [k, v] of Object.entries(o)) if (k !== '$id' && k !== '$class') out[k] = dec(v);
    return out;
  };
  return dec(root);
}

// ─── Fixture encoding ───────────────────────────────────────────────────────
//
// Plain JSON in the TS object shape, plus:
//   {"$num":"NaN"|"Infinity"|"-Infinity"|"-0"}   non-JSON numbers
//   {"$u":1}                                     undefined
//   {"$id":n,…} / {"$id":n,"$arr":[…]}, {"$ref":n}  shared objects (DAGs)

function encode(root: unknown): unknown {
  const counts = new Map<object, number>();
  const count = (v: unknown): void => {
    if (v === null || typeof v !== 'object') return;
    const c = counts.get(v) ?? 0;
    counts.set(v, c + 1);
    if (c === 0) for (const ch of Object.values(v)) count(ch);
  };
  count(root);
  const ids = new Map<object, number>();
  const enc = (v: unknown): unknown => {
    if (v === undefined) return { $u: 1 };
    if (typeof v === 'number') {
      if (Number.isNaN(v)) return { $num: 'NaN' };
      if (v === Infinity) return { $num: 'Infinity' };
      if (v === -Infinity) return { $num: '-Infinity' };
      if (Object.is(v, -0)) return { $num: '-0' };
      return v;
    }
    if (v === null || typeof v !== 'object') return v;
    const seen = ids.get(v);
    if (seen !== undefined) return { $ref: seen };
    let id: number | undefined;
    if ((counts.get(v) ?? 0) > 1) { id = ids.size; ids.set(v, id); }
    if (Array.isArray(v)) {
      const arr = v.map(enc);
      return id === undefined ? arr : { $id: id, $arr: arr };
    }
    const out: Record<string, unknown> = id === undefined ? {} : { $id: id };
    for (const [k, x] of Object.entries(v)) out[k] = enc(x);
    return out;
  };
  return enc(root);
}

// ─── Scene preparation ──────────────────────────────────────────────────────

/** Drop unsupported kinds (recursively), keeping shared subtrees shared. */
function supportedOnly(layers: SceneNode[], dropped: Record<string, number>): SceneNode[] {
  const memo = new Map<object, SceneNode | null>();
  const visit = (n: SceneNode): SceneNode | null => {
    const hit = memo.get(n);
    if (hit !== undefined) return hit;
    let out: SceneNode | null = n;
    const kind = (n as { kind: string }).kind;
    if (UNSUPPORTED.has(kind)) {
      dropped[kind] = (dropped[kind] ?? 0) + 1;
      out = null;
    } else if (n.kind === 'group') {
      const kids = n.children.map(visit).filter((c): c is SceneNode => !!c);
      const same = kids.length === n.children.length && kids.every((c, i) => c === n.children[i]);
      out = same ? n : { ...n, children: kids };
    }
    memo.set(n, out);
    return out;
  };
  return layers.map(visit).filter((c): c is SceneNode => !!c);
}

/** Anything left that the fixture format can't carry (tagged trace values). */
function hasTagged(v: unknown, seen = new Set<object>()): boolean {
  if (v instanceof Tagged) return true;
  if (v === null || typeof v !== 'object' || seen.has(v)) return false;
  seen.add(v);
  return Object.values(v).some((x) => hasTagged(x, seen));
}

interface Case {
  name: string;
  width: number;
  height: number;
  background?: string | null;
  layers: SceneNode[];
  dropped?: Record<string, number>;
}

function caseJson(c: Case): unknown {
  const svg = sceneToSvg(c.layers, { width: c.width, height: c.height, background: c.background });
  return {
    name: c.name,
    width: encode(c.width),
    height: encode(c.height),
    background: encode(c.background),
    ...(c.dropped && Object.keys(c.dropped).length ? { dropped: c.dropped } : {}),
    layers: encode(c.layers),
    svg,
  };
}

function writeCases(file: string, cases: Case[]): void {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const json = Buffer.from(JSON.stringify({ format: FORMAT, cases: cases.map(caseJson) }));
  fs.writeFileSync(file, zlib.gzipSync(json, { level: 9 }));
}

// ─── Samples (from the recorded traces) ─────────────────────────────────────

function sampleCases(id: string, summary: string[]): Case[] {
  const trace = JSON.parse(zlib.gunzipSync(fs.readFileSync(path.join(TRACES_DIR, `${id}.json.gz`))).toString()) as {
    compile?: { rendererIds: string[] };
    frames?: Array<{ frame: { frameCount: number; width: number; height: number }; outputs: unknown }>;
  };
  const cases: Case[] = [];
  for (const fr of trace.frames ?? []) {
    if (!FRAMES.includes(fr.frame.frameCount)) continue;
    const outputs = decodeTrace(fr.outputs) as Record<string, unknown>;
    for (const rid of trace.compile?.rendererIds ?? []) {
      const name = `${id}@${fr.frame.frameCount}/${rid}`;
      const ports = outputs[rid] as unknown[] | Tagged | undefined;
      if (!Array.isArray(ports)) { summary.push(`${name}: skipped (no renderer output)`); continue; }
      const scene = (ports[0] ?? []) as unknown;
      if (!Array.isArray(scene)) { summary.push(`${name}: skipped (layers not recorded)`); continue; }
      // canvas-driver.ts: flatten one level, drop falsy layers.
      const flat: SceneNode[] = [];
      for (const layer of scene as Array<SceneNode | SceneNode[] | null>) {
        if (Array.isArray(layer)) { for (const x of layer) if (x) flat.push(x); }
        else if (layer) flat.push(layer);
      }
      const dropped: Record<string, number> = {};
      const layers = supportedOnly(flat, dropped);
      if (hasTagged(layers)) { summary.push(`${name}: skipped (unrecorded values in scene)`); continue; }
      const bgRaw = (ports[1] as unknown[] | undefined)?.[0];
      const background = bgRaw && typeof bgRaw === 'object' && 'r' in bgRaw ? colorToCss(bgRaw as Color) : '#000000';
      cases.push({ name, width: fr.frame.width, height: fr.frame.height, background, layers, dropped });
      summary.push(`${name}: ${layers.length} layers${Object.keys(dropped).length ? ` (dropped ${JSON.stringify(dropped)})` : ''}`);
    }
  }
  return cases;
}

// ─── Hand-built edge cases ──────────────────────────────────────────────────

/** A style with overrides. Loosely typed: edge cases use off-spec strings. */
const st = (o: Partial<Record<keyof ResolvedStyle, unknown>> = {}): ResolvedStyle =>
  ({ ...DEFAULT_STYLE, ...o }) as ResolvedStyle;
const RED = st({ fill: '#ff0000' });
const BLUE_STROKE = st({ fill: null, stroke: '#0000ff', strokeWeight: 3 });

function edgeCases(): Case[] {
  const doc = (name: string, layers: SceneNode[], extra: Partial<Case> = {}): Case =>
    ({ name, width: 200, height: 100, background: null, layers, ...extra });
  const rect = (o: Record<string, unknown> = {}): SceneNode =>
    ({ kind: 'rect', x: 0, y: 0, w: 10, h: 20, mode: 'corner', style: RED, ...o }) as SceneNode;
  const tri = (style: ResolvedStyle): SceneNode =>
    ({ kind: 'triangle', x1: 0, y1: 0, x2: 10, y2: 0, x3: 5, y3: 10, style });
  const pts = (...xy: number[]) => Array.from({ length: xy.length / 2 }, (_, i) => ({ x: xy[2 * i]!, y: xy[2 * i + 1]! }));
  const text = (t: string, style: Parameters<typeof st>[0] = {}, x = 0, y = 0): SceneNode =>
    ({ kind: 'text', text: t, x, y, style: st(style) });
  const shared = rect({ x: 1, y: 2 });

  return [
    // Document wrapper.
    doc('empty', []),
    doc('background', [], { background: '#abcdef' }),
    doc('background-escaped', [], { background: 'url("a")&<b>' }),
    doc('background-empty-string', [], { background: '' }),
    doc('background-undefined', [], { background: undefined }),
    doc('size-fractional-odd', [rect()], { width: 101.9, height: 33.2 }),
    doc('size-clamped', [], { width: 0, height: -50 }),
    doc('size-nan', [rect()], { width: NaN, height: Infinity }),
    doc('size-huge', [], { width: 1e21, height: 2 ** 31 + 1 }),
    // Numbers.
    doc('numbers', [
      rect({ x: NaN, y: Infinity, w: 10, h: -Infinity }),
      rect({ x: 1.347e-14, y: 12.3456789, w: -0, h: -0.0004 }),
      rect({ x: 1.0005, y: -1.0005, w: 2.0005, h: 0.0005 }),
      rect({ x: 1e300, y: -1e306, w: 123456789.123456, h: 5e-7 }),
      rect({ x: 0.1 + 0.2, y: 1 / 3, w: -2 / 3, h: 1e21 / 1000 }),
    ]),
    // Rect / ellipse / arc box modes.
    doc('rect-modes', ['corner', 'center', 'corners', 'radius', 'bogus', ''].map((mode, i) =>
      rect({ x: 10 + i, y: 20, w: 50 - i, h: 80, mode }))),
    doc('rect-negative', [rect({ x: 5, y: 5, w: -10, h: -4 }), rect({ x: 50, y: 60, w: 10, h: 20, mode: 'corners' })]),
    doc('ellipses', ['corner', 'center', 'corners', 'radius'].flatMap((mode) => [
      { kind: 'ellipse', x: 100, y: 100, w: 40, h: 21, mode, style: RED },
      { kind: 'ellipse', x: -3, y: 7, w: -40, h: -21, mode, style: BLUE_STROKE },
    ] as SceneNode[])),
    doc('arcs', [
      [0, Math.PI / 2], [0, Math.PI], [0, Math.PI + 0.001], [Math.PI / 2, 0], [0, -4],
      [-1, 5], [0, 2 * Math.PI], [0.3, 0.3], [NaN, 1], [1, Infinity], [100, 100.5],
    ].map(([start, stop], i) => ({
      kind: 'arc', x: i * 7, y: -i * 3, w: 60 + i, h: 40 - i, start, stop,
      mode: i % 2 ? 'center' : 'corner', style: i % 3 ? RED : BLUE_STROKE,
    } as SceneNode))),
    // Lines.
    doc('lines', [
      { kind: 'line', x1: 0, y1: 0, x2: 100, y2: 100, style: LINE_DEFAULT_STYLE },
      { kind: 'line', x1: 0, y1: 0, x2: 100, y2: 100, style: st({ stroke: null }) },
      { kind: 'line', x1: 0, y1: 0, x2: 100, y2: 100, style: st({ stroke: '' }) },
      { kind: 'line', x1: 1, y1: 2, x2: 3, y2: 4, style: st({ stroke: 'rgba(10,20,30,0.250)', strokeWeight: 2.5, strokeCap: 'round', strokeJoin: 'round' }) },
      { kind: 'line', x1: 1, y1: 2, x2: 3, y2: 4, style: st({ stroke: '#000', strokeCap: 'square', strokeJoin: 'bevel', strokeWeight: NaN }) },
    ]),
    // Triangles / quads / points.
    doc('triangles-quads', [
      tri(RED), tri(BLUE_STROKE), tri(st({ fill: null })),
      { kind: 'quad', x1: 0, y1: 0, x2: 10.12345, y2: 0, x3: 10, y3: 10, x4: 0, y4: -10, style: RED },
    ]),
    doc('points', [
      { kind: 'point', x: 5, y: 5, style: st({ stroke: '#123456', strokeWeight: 4 }) },
      { kind: 'point', x: 5, y: 5, style: st({ stroke: null, fill: 'rgba(1,2,3,0.5)', strokeWeight: 0.2 }) },
      { kind: 'point', x: 5, y: 5, style: st({ stroke: null, fill: null }) },
      { kind: 'point', x: 5, y: 5, style: st({ stroke: '', fill: '#fff' }) },
      { kind: 'point', x: 5, y: 5, style: st({ stroke: null, fill: '' }) },
      { kind: 'point', x: -2.5, y: 7, style: st({ stroke: '#000', strokeWeight: NaN }) },
    ] as SceneNode[]),
    // Polygons, straight and curved.
    doc('polygons', [
      { kind: 'polygon', points: [], closed: true, style: RED },
      { kind: 'polygon', points: pts(1, 2), closed: false, style: BLUE_STROKE },
      { kind: 'polygon', points: pts(0, 0, 10, 0, 10, 10), closed: true, style: RED },
      { kind: 'polygon', points: pts(0, 0, 10, 0, 10, 10), closed: false, style: BLUE_STROKE },
      { kind: 'polygon', points: pts(0, 0, 10, 0), closed: false, style: BLUE_STROKE, tension: 1 },
      { kind: 'polygon', points: pts(0, 0, 10, 0, 10, 10, 0, 10), closed: false, style: BLUE_STROKE, tension: 0.5 },
      { kind: 'polygon', points: pts(0, 0, 10, 0, 10, 10, 0, 10), closed: true, style: RED, tension: 1 },
      { kind: 'polygon', points: pts(0, 0, 10, 0, 10, 10), closed: true, style: RED, tension: 0 },
      { kind: 'polygon', points: pts(0, 0, 10, 0, 10, 10), closed: true, style: RED, tension: -1 },
      { kind: 'polygon', points: pts(0, 0, 10, 0, 10, 10), closed: true, style: RED, tension: undefined },
      { kind: 'polygon', points: pts(0, 0, 10, 0, 10, 10), closed: false, style: RED, tension: NaN },
      { kind: 'polygon', points: pts(1.1, 2.2, 3.3, -4.4, 5.55555, 6, -7, 8), closed: true, style: BLUE_STROKE, tension: 0.37 },
      { kind: 'polygon', points: pts(0, 0, 3, 1, 0, 2), closed: false, style: st({ fill: 'rgba(0,0,0,0.000)', stroke: '#fff' }), tension: 2 },
    ] as SceneNode[]),
    // Text.
    doc('text', [
      text('hello', { textAlign: 'center', textBaseline: 'middle' }, 50, 50),
      text('<bold> & "quoted"'),
      text('hi', { fontFamily: 'Inter, sans-serif', fontSize: 24, fontWeight: 'bold' }),
      text('styled', { fontFamily: '', fontWeight: '', fontStyle: 'italic', letterSpacing: 1.5, textAlign: 'right', textBaseline: 'top' }),
      text('esc', { fontFamily: '"Fira" <x>', fontWeight: '7&0', fontStyle: 'a"b', letterSpacing: NaN, textBaseline: 'bottom' }),
      text('weird', { textAlign: 'start', textBaseline: 'hanging', letterSpacing: -0.0001, fontSize: 9.87654 }),
      text('a\nb\n\nc', { fontSize: 10, lineHeight: 1.5 }, 3, 4),
      text('\n', { fontSize: 11, lineHeight: 1.3 }),
      text('trail\n', { fontSize: NaN }),
      text(''),
      text('stroked', { fill: null, stroke: 'rgba(255,0,0,0.5)', strokeWeight: 2, strokeCap: 'round' }),
      text('ünï©ødé 🌸 \t tab'),
    ]),
    // Paint splitting.
    doc('paints', [
      'rgba(244,71,61,0.5)', 'rgba( 1 , 2 ,3 , 0.250 )', 'rgba(1,2,3,0.5) ', 'rgba(1,2,3)',
      'rgb(1,2,3)', '#ff0000', 'red', 'rgba(-1,2,3,0.5)', 'rgba(NaN,0,0,0.500)', 'rgba(1,2,3,1e-3)',
      'rgba( 1,2,3,0.5﻿)', 'rgba(1,2,3,0.5\u0085)', 'hsl(0 50% "x" & <y>)', 'RGBA(1,2,3,0.5)',
    ].map((c, i) => rect({ x: i, style: st({ fill: c, stroke: c, strokeWeight: i }) }))),
    doc('styles', [
      rect({ style: st({ fill: null }) }),
      rect({ style: st({ fill: '' }) }),
      rect({ style: NEUTRAL_STYLE }),
      rect({ style: st({ stroke: '#000', strokeWeight: 1 }) }),
      rect({ style: st({ stroke: '#000', strokeCap: 'round', strokeJoin: 'bevel' }) }),
      rect({ style: st({ stroke: '#000', strokeCap: '', strokeJoin: '' }) }),
      rect({ style: st({ stroke: '', strokeCap: 'round' }) }),
    ]),
    // Groups.
    doc('groups', [
      { kind: 'group', children: [] },
      { kind: 'group', children: [rect()] },
      { kind: 'group', children: [rect()], transform: [2, 0, 0, 2, 10, 20] },
      { kind: 'group', children: [rect()], transform: [Math.cos(1), Math.sin(1), -Math.sin(1), Math.cos(1), NaN, 1e-9] },
      { kind: 'group', children: [rect()], blendMode: 'multiply' },
      { kind: 'group', children: [rect()], blendMode: 'destination-in' },
      { kind: 'group', children: [rect()], blendMode: '' },
      { kind: 'group', children: [rect()], transform: [1, 0, 0, 1, 0, 0], blendMode: 'screen', clip: { x: 10, y: 20, w: 50, h: 60 } },
      { kind: 'group', children: [shared, shared] },
    ] as SceneNode[]),
    doc('blend-modes', [
      'normal', 'multiply', 'screen', 'overlay', 'darken', 'lighten', 'color-dodge', 'color-burn',
      'hard-light', 'soft-light', 'difference', 'exclusion', 'hue', 'saturation', 'color', 'luminosity',
      'plus-lighter', 'source-over', 'lighter', 'xor', 'copy', 'MULTIPLY',
    ].map((blendMode) => ({ kind: 'group', children: [shared], blendMode }) as SceneNode)),
    doc('clips', [
      { kind: 'group', children: [rect()], clip: { x: 0, y: 0, w: 10, h: 10 } },
      { kind: 'group', clip: { x: -1.23456, y: NaN, w: 20, h: 20 }, children: [
        { kind: 'group', clip: { x: 1, y: 1, w: 2, h: 2 }, children: [rect()] },
        { kind: 'group', children: [{ kind: 'group', clip: { x: 3, y: 3, w: 4, h: 4 }, children: [] }] },
      ] },
      { kind: 'group', children: [rect()], clip: { x: 0, y: 0, w: 30, h: 30 } },
    ] as SceneNode[]),
    doc('nested', [
      { kind: 'group', transform: [1, 0, 0, 1, 5, 5], children: [
        { kind: 'group', blendMode: 'overlay', children: [tri(RED), text('t'),
          { kind: 'group', transform: [0.5, 0, 0, 0.5, 0, 0], clip: { x: 0, y: 0, w: 1, h: 1 }, children: [shared] }] },
      ] },
    ] as SceneNode[]),
  ];
}

// ─── Export ─────────────────────────────────────────────────────────────────

it('exports SVG goldens', () => {
  const summary: string[] = [];
  fs.rmSync(OUT_DIR, { recursive: true, force: true });
  writeCases(path.join(OUT_DIR, 'edge-cases.json.gz'), edgeCases());
  const ids = fs.readdirSync(TRACES_DIR).filter((f) => f.endsWith('.json.gz')).map((f) => f.replace(/\.json\.gz$/, '')).sort();
  for (const id of ids) {
    const cases = sampleCases(id, summary);
    if (cases.length) writeCases(path.join(OUT_DIR, 'samples', `${id}.json.gz`), cases);
  }
  // eslint-disable-next-line no-console
  console.log(summary.join('\n'));
});
