import { node, edge } from '../helpers';

// ─── GPU Shader — Plasma ───────────────────────────────────────────────────
// A generative fragment shader (gpu/shader). No input wired, so it draws from
// scratch — a classic animated plasma driven by iTime, with a `scale` uniform
// exposed as a wired port and steered by a slider. Select the node and edit the
// GLSL in the side panel; declare a `uniform` to grow a new input port.
export function buildShaderPlasma() {
  const SOURCE = `uniform float scale;
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
  vec2 uv = (fragCoord - 0.5 * iResolution.xy) / iResolution.y;
  uv *= scale;
  float t = iTime * 0.5;
  float v = sin(uv.x + t)
          + sin(uv.y + t)
          + sin(uv.x + uv.y + t)
          + sin(length(uv) * 2.0 - t);
  vec3 col = 0.5 + 0.5 * cos(vec3(0.0, 2.0, 4.0) + v + t);
  fragColor = vec4(col, 1.0);
}`;
  const scale = node('sh_scale', 'io/slider', { x: -320, y: 120 }, { value: 6, min: 1, max: 24 });
  const shader = node('sh_shader', 'gpu/shader', { x: -60, y: 0 }, { source: SOURCE, feedback: false });
  const renderer = node('sh_renderer', 'render/renderer', { x: 360, y: 0 }, { width: 500, height: 500 });
  return {
    nodes: [scale, shader, renderer],
    edges: [
      edge('sh_scale', 'number', 'sh_shader', 'scale'),
      edge('sh_shader', 'out', 'sh_renderer', 'layers'),
    ],
  };
}

// ─── GPU Shader — Feedback Ripples ──────────────────────────────────────────
// Self-feedback: each frame reads the previous output (iChannel1), advects and
// fades it, and injects a moving source — a reaction-diffusion-flavoured ripple
// field that builds over time. The `feedback` flag wires iChannel1 to the node's
// own previous frame (a persistent ping-pong buffer in the renderer).
export function buildShaderFeedback() {
  const SOURCE = `void mainImage(out vec4 fragColor, in vec2 fragCoord) {
  vec2 uv = fragCoord / iResolution.xy;
  vec2 px = 1.0 / iResolution.xy;
  // blur the previous frame slightly (cheap diffusion) and fade it
  vec4 p = texture(iChannel1, uv) * 0.25
         + texture(iChannel1, uv + vec2(px.x, 0.0)) * 0.1875
         + texture(iChannel1, uv - vec2(px.x, 0.0)) * 0.1875
         + texture(iChannel1, uv + vec2(0.0, px.y)) * 0.1875
         + texture(iChannel1, uv - vec2(0.0, px.y)) * 0.1875;
  p.rgb *= 0.985;
  // moving source
  vec2 c = 0.5 + 0.35 * vec2(cos(iTime * 0.7), sin(iTime * 1.1));
  float d = exp(-220.0 * dot(uv - c, uv - c));
  vec3 inject = d * (0.5 + 0.5 * cos(vec3(0.0, 2.0, 4.0) + iTime));
  fragColor = vec4(p.rgb + inject, 1.0);
}`;
  const shader = node('sf_shader', 'gpu/shader', { x: -60, y: 0 }, { source: SOURCE, feedback: true });
  const renderer = node('sf_renderer', 'render/renderer', { x: 360, y: 0 }, { width: 500, height: 500 });
  return {
    nodes: [shader, renderer],
    edges: [edge('sf_shader', 'out', 'sf_renderer', 'layers')],
  };
}
