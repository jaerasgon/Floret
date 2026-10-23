import { registerNode } from '../registry';
import type { PortSpec, NodeParamContext } from '../types';
import type { PortValue } from '../../runtime/types';
import type { SceneNode, ShaderShape, ShaderUniformValue, ShaderPass, ShaderBufferId, ShaderChannel } from '../../runtime/scene';
import { parseUniforms, uniformPortType } from '../../runtime/gl/glsl-shader';

/**
 * gpu/shader — a ShaderToy-style fragment shader as a layer.
 *
 * Wire a layer into `input` for post-FX (it becomes the 'input' channel), or
 * leave it unwired for a generative shader. Every non-sampler `uniform` you
 * declare (in any pass) grows a matching input port. ShaderToy built-ins
 * (iTime / iResolution / iMouse / iFrame / iChannelN) are supplied automatically.
 *
 * Multi-buffer (Phase 2): optional Buffer A–D passes (params `bufferA`..`bufferD`)
 * run before the Image pass; each pass routes its four iChannels (params
 * `channels`: { [pass]: [c0,c1,c2,c3] } with codes 'input' | 'self' | 'A'..'D' |
 * 'image' | '') so a buffer can read the input, sibling buffers, or its own
 * previous frame (feedback). The GL renderer executes every pass each frame.
 */

const DEFAULT_SOURCE = `// iTime, iResolution, iMouse, iChannel0 are provided.
// Declare a 'uniform float foo;' to grow a wired input port.
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
  vec2 uv = fragCoord / iResolution.xy;
  vec3 col = 0.5 + 0.5 * cos(iTime + uv.xyx + vec3(0.0, 2.0, 4.0));
  fragColor = vec4(col, 1.0);
}`;

const BUFS: ShaderBufferId[] = ['A', 'B', 'C', 'D'];

/** Sources of every present pass (Buffer A–D that have source, then Image). */
function passSources(params: Record<string, unknown>): { buf: ShaderBufferId; source: string }[] {
  const out: { buf: ShaderBufferId; source: string }[] = [];
  for (const b of BUFS) {
    const src = params['buffer' + b];
    if (typeof src === 'string' && src.trim()) out.push({ buf: b, source: src });
  }
  out.push({ buf: 'image', source: typeof params.source === 'string' ? params.source : DEFAULT_SOURCE });
  return out;
}

/** Union of user uniforms across all passes, in first-seen order. */
function allUniforms(params: Record<string, unknown>) {
  const seen = new Set<string>();
  const out: { name: string; glslType: string }[] = [];
  for (const { source } of passSources(params)) {
    for (const u of parseUniforms(source)) {
      if (u.glslType === 'sampler2D' || seen.has(u.name)) continue;   // samplers are iChannels, not ports
      seen.add(u.name);
      out.push(u);
    }
  }
  return out;
}

/** Port list: the primary input layer, then one port per (non-sampler) uniform. */
function shaderInputs(params: Record<string, unknown>): ReadonlyArray<PortSpec> {
  const ports: PortSpec[] = [{ name: 'input', label: 'input', type: 'layer', kind: 'spread' }];
  for (const u of allUniforms(params)) ports.push({ name: u.name, label: u.name, type: uniformPortType(u.glslType) });
  return ports;
}

function resolveChannel(code: unknown, selfBuf: ShaderBufferId): ShaderChannel {
  if (code === 'input') return { kind: 'input' };
  if (code === 'self') return { kind: 'buffer', buf: selfBuf };
  if (code === 'A' || code === 'B' || code === 'C' || code === 'D' || code === 'image') return { kind: 'buffer', buf: code };
  return null;
}

/** Resolve a pass's 4 channels: explicit config wins, else sensible defaults. */
function channelsFor(buf: ShaderBufferId, params: Record<string, unknown>, feedback: boolean): ShaderPass['channels'] {
  const cfg = (params.channels && typeof params.channels === 'object' ? params.channels : {}) as Record<string, unknown>;
  const conf = cfg[buf];
  if (Array.isArray(conf)) {
    const c = [0, 1, 2, 3].map((i) => resolveChannel(conf[i], buf));
    return [c[0]!, c[1]!, c[2]!, c[3]!];
  }
  // Defaults: Image reads input (+ own previous frame when `feedback`); a Buffer
  // reads its own previous frame on 0 and the input on 1.
  if (buf === 'image') return [{ kind: 'input' }, feedback ? { kind: 'buffer', buf: 'image' } : null, null, null];
  return [{ kind: 'buffer', buf }, { kind: 'input' }, null, null];
}

/** Collapse a spread of layers into a single SceneNode (null / passthrough / group). */
function asLayer(v: PortValue | undefined): SceneNode | null {
  if (!v || v.length === 0) return null;
  const nodes = v.filter(Boolean) as SceneNode[];
  if (nodes.length === 0) return null;
  if (nodes.length === 1) return nodes[0]!;
  return { kind: 'group', children: nodes };
}

function toVec2(v: unknown): [number, number] {
  const o = v as { x?: number; y?: number } | undefined;
  return [Number(o?.x ?? 0), Number(o?.y ?? 0)];
}
function toColor(v: unknown): [number, number, number, number] {
  const c = v as { r?: number; g?: number; b?: number; a?: number } | undefined;
  return [Number(c?.r ?? 0) / 255, Number(c?.g ?? 0) / 255, Number(c?.b ?? 0) / 255, Number(c?.a ?? 255) / 255];
}

registerNode({
  type: 'gpu/shader',
  category: 'Render',
  label: 'Shader',
  description: 'Fragment shader (ShaderToy-style) with optional Buffer A–D passes. Wire a layer for post-FX, or leave it for generative. Edit in the side panel.',
  inputs: shaderInputs({}),
  outputs: [{ name: 'out', type: 'layer' }],
  defaultParams: { source: DEFAULT_SOURCE, feedback: false },
  dynamicInputs: shaderInputs,
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const raw = (key: string, fallback?: unknown) => params.param(key, fallback);
    const pp: Record<string, unknown> = {
      source: raw('source', DEFAULT_SOURCE),
      bufferA: raw('bufferA'), bufferB: raw('bufferB'), bufferC: raw('bufferC'), bufferD: raw('bufferD'),
      channels: raw('channels'),
    };
    const feedback = Boolean(raw('feedback', false));
    const decls = allUniforms(pp);
    const uniforms: Record<string, ShaderUniformValue> = {};
    decls.forEach((u, i) => {
      const r = inputs[i + 1];
      const pt = uniformPortType(u.glslType);
      if (pt === 'vec2') uniforms[u.name] = { t: 'vec2', v: toVec2(r?.[0]) };
      else if (pt === 'color') uniforms[u.name] = { t: 'color', v: toColor(r?.[0]) };
      else uniforms[u.name] = { t: 'number', v: Number(r?.[0] ?? 0) };
    });
    const passes: ShaderPass[] = passSources(pp).map(({ buf, source }) => ({
      buf, source, channels: channelsFor(buf, pp, feedback),
    }));
    const shape: ShaderShape = {
      kind: 'shader',
      nodeId: params.nodeId,
      passes,
      uniformDecls: decls,
      uniforms,
      input: asLayer(inputs[0]),
    };
    return [[shape]];
  },
});
