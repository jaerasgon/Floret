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

// ─── GPU Shader — Reaction-Diffusion (multi-buffer) ─────────────────────────
// Gray-Scott reaction-diffusion across a Buffer A pass that reads its own
// previous frame (the chemical field), with the Image pass colour-mapping it.
// This is the canonical multi-buffer case: A holds simulation STATE that Image
// only visualises. Buffer A → iChannel0 = self; Image → iChannel0 = Buffer A.
export function buildShaderReactionDiffusion() {
  const BUFFER_A = `// Gray-Scott. r = chemical A, g = chemical B. iChannel0 = this buffer (self).
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
  vec2 uv = fragCoord / iResolution.xy;
  vec2 px = 1.0 / iResolution.xy;
  if (iFrame < 1) {                       // seed: A=1, a few B blobs
    float b = step(0.985, fract(sin(dot(floor(uv * 24.0), vec2(12.9898, 78.233))) * 43758.5453));
    fragColor = vec4(1.0, b, 0.0, 1.0);
    return;
  }
  vec2 c = texture(iChannel0, uv).xy;
  vec2 lap = texture(iChannel0, uv + vec2(px.x, 0.0)).xy
           + texture(iChannel0, uv - vec2(px.x, 0.0)).xy
           + texture(iChannel0, uv + vec2(0.0, px.y)).xy
           + texture(iChannel0, uv - vec2(0.0, px.y)).xy
           - 4.0 * c;
  float A = c.x, B = c.y;
  float reaction = A * B * B;
  float feed = 0.055, kill = 0.062;
  float A2 = A + (1.0 * lap.x - reaction + feed * (1.0 - A));
  float B2 = B + (0.5 * lap.y + reaction - (kill + feed) * B);
  fragColor = vec4(clamp(A2, 0.0, 1.0), clamp(B2, 0.0, 1.0), 0.0, 1.0);
}`;
  const IMAGE = `// iChannel0 = Buffer A (the simulation field).
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
  vec2 uv = fragCoord / iResolution.xy;
  vec2 c = texture(iChannel0, uv).xy;
  float v = clamp(c.x - c.y, 0.0, 1.0);
  vec3 col = mix(vec3(0.02, 0.03, 0.08), vec3(0.95, 0.85, 0.55), smoothstep(0.2, 0.7, v));
  fragColor = vec4(col, 1.0);
}`;
  const shader = node('rd_shader', 'gpu/shader', { x: -60, y: 0 }, {
    source: IMAGE,
    bufferA: BUFFER_A,
    channels: { A: ['self', '', '', ''], image: ['A', '', '', ''] },
  });
  const renderer = node('rd_renderer', 'render/renderer', { x: 360, y: 0 }, { width: 500, height: 500 });
  return {
    nodes: [shader, renderer],
    edges: [edge('rd_shader', 'out', 'rd_renderer', 'layers')],
  };
}
