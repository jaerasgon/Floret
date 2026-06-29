import { registerNode } from '../registry';
import type { PortSpec, NodeParamContext } from '../types';
import type { PortValue } from '../../runtime/types';
import type { SceneNode, ShaderShape, ShaderUniformValue } from '../../runtime/scene';
import { parseUniforms, uniformPortType } from '../../runtime/gl/glsl-shader';

/**
 * gpu/shader — a ShaderToy-style fragment shader as a layer.
 *
 * Wire a layer into `input` for post-FX (it becomes iChannel0), or leave it
 * unwired for a generative shader. Every `uniform` you declare in the source
 * grows a matching input port (a slider → `uniform float`, a Color → `vec3/4`,
 * a Vec2 → `vec2`, another layer → an `sampler2D` channel). ShaderToy built-ins
 * (iTime / iResolution / iMouse / iFrame / iChannelN) are supplied automatically.
 * The GL renderer executes it each frame; `feedback` exposes the previous output
 * as iChannel1 for reaction-diffusion / trails.
 */

const DEFAULT_SOURCE = `// iTime, iResolution, iMouse, iChannel0 are provided.
// Declare a 'uniform float foo;' to grow a wired input port.
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
  vec2 uv = fragCoord / iResolution.xy;
  vec3 col = 0.5 + 0.5 * cos(iTime + uv.xyx + vec3(0.0, 2.0, 4.0));
  fragColor = vec4(col, 1.0);
}`;

/** Port list: the primary input layer, then one port per user uniform. */
function shaderInputs(params: Record<string, unknown>): ReadonlyArray<PortSpec> {
  const source = typeof params.source === 'string' ? params.source : DEFAULT_SOURCE;
  const ports: PortSpec[] = [{ name: 'input', label: 'input', type: 'layer', kind: 'spread' }];
  for (const u of parseUniforms(source)) {
    ports.push({ name: u.name, label: u.name, type: uniformPortType(u.glslType) });
  }
  return ports;
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
  description: 'Fragment shader (ShaderToy-style). Wire a layer for post-FX, or leave it for generative. Edit the code in the side panel.',
  inputs: shaderInputs({}),
  outputs: [{ name: 'out', type: 'layer' }],
  defaultParams: { source: DEFAULT_SOURCE, feedback: false },
  dynamicInputs: shaderInputs,
  computeRaw: (inputs: PortValue[], params: NodeParamContext): PortValue[] => {
    const source = params.param<string>('source', DEFAULT_SOURCE);
    const feedback = Boolean(params.param('feedback', false));
    const decls = parseUniforms(source);
    const uniforms: Record<string, ShaderUniformValue> = {};
    const channels: { name: string; layer: SceneNode | null }[] = [];
    // inputs[0] = primary layer; inputs[i+1] = the i-th declared uniform.
    decls.forEach((u, i) => {
      const raw = inputs[i + 1];
      const pt = uniformPortType(u.glslType);
      if (pt === 'layer') channels.push({ name: u.name, layer: asLayer(raw) });
      else if (pt === 'vec2') uniforms[u.name] = { t: 'vec2', v: toVec2(raw?.[0]) };
      else if (pt === 'color') uniforms[u.name] = { t: 'color', v: toColor(raw?.[0]) };
      else uniforms[u.name] = { t: 'number', v: Number(raw?.[0] ?? 0) };
    });
    const shape: ShaderShape = {
      kind: 'shader',
      nodeId: params.nodeId,
      source,
      uniformDecls: decls,
      uniforms,
      channels,
      input: asLayer(inputs[0]),
      feedback,
    };
    return [[shape]];
  },
});
