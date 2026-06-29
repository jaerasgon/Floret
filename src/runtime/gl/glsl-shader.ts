/**
 * GLSL shader plumbing for the gpu/shader node.
 *
 * Users write ShaderToy-style fragment shaders:
 *     void mainImage(out vec4 fragColor, in vec2 fragCoord) { ... }
 *
 * We compile them as WebGL2 (GLSL ES 3.00). This module is the pure,
 * GL-free half: parsing `uniform` declarations into typed node ports and
 * wrapping the user source with a built-in preamble + a `main()` entry.
 * Kept free of any `WebGLRenderingContext` so it's unit-testable headless.
 */

import type { PortType } from '../../nodes/types';

/** ShaderToy built-ins the renderer supplies — never exposed as node ports. */
export const BUILTIN_UNIFORMS = new Set<string>([
  'iTime', 'iTimeDelta', 'iFrame', 'iResolution', 'iMouse', 'iDate',
  'iChannel0', 'iChannel1', 'iChannel2', 'iChannel3', 'iChannelResolution',
]);

export interface ShaderUniform { name: string; glslType: string }

/** GLSL uniform type → Floret port type (and whether it's a texture channel). */
export function uniformPortType(glslType: string): PortType {
  switch (glslType) {
    case 'float': case 'int': return 'number';
    case 'vec2': return 'vec2';
    case 'vec3': case 'vec4': return 'color';   // colours are the common vec3/4 use
    case 'sampler2D': return 'layer';           // an extra iChannel, fed by a wired layer
    default: return 'number';
  }
}

// Match `uniform <type> <name>;` — one declaration per statement. Tolerates
// precision qualifiers (`uniform highp float x;`) and trailing comments are
// stripped by splitting on `;` first. Array uniforms (`uniform float k[4];`)
// and multi-declarations (`uniform float a, b;`) are intentionally not parsed
// in v1 — they'd need real GLSL parsing; declare them one-per-line.
const UNIFORM_RE = /\buniform\s+(?:(?:lowp|mediump|highp)\s+)?([A-Za-z_]\w*)\s+([A-Za-z_]\w*)\s*;/;

/**
 * Extract the user-declared (non-built-in) uniforms from shader source, in
 * declaration order, de-duplicated by name. Comments are ignored.
 */
export function parseUniforms(source: string): ShaderUniform[] {
  const stripped = stripComments(source);
  const seen = new Set<string>();
  const out: ShaderUniform[] = [];
  for (const stmt of stripped.split(';')) {
    const m = UNIFORM_RE.exec(stmt + ';');
    if (!m) continue;
    const glslType = m[1]!, name = m[2]!;
    if (BUILTIN_UNIFORMS.has(name) || seen.has(name)) continue;
    seen.add(name);
    out.push({ name, glslType });
  }
  return out;
}

/** Remove line and block comments so they don't yield phantom uniforms. */
function stripComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/[^\n]*/g, ' ');
}

// The fixed preamble. iChannelN are always declared (cheap; unused samplers are
// optimised out). User uniforms are injected right after so a re-declaration in
// the user source would be a clear duplicate-symbol compile error, not silent.
const PREAMBLE = `#version 300 es
precision highp float;
precision highp int;
uniform float iTime;
uniform float iTimeDelta;
uniform int iFrame;
uniform vec3 iResolution;
uniform vec4 iMouse;
uniform sampler2D iChannel0;
uniform sampler2D iChannel1;
uniform sampler2D iChannel2;
uniform sampler2D iChannel3;
out vec4 _fragColor;
`;

const EPILOGUE = `
void main(){ mainImage(_fragColor, gl_FragCoord.xy); }
`;

/** Number of preamble lines BEFORE the user's first line — used to map GL
 *  compile-error line numbers back to the user's editor. */
export function preambleLineCount(): number {
  return countLines(PREAMBLE);
}

/** Assemble the full WebGL2 fragment shader: built-in preamble + the user's
 *  source (which carries its own `uniform` declarations) + the main() entry. */
export function wrapFragmentShader(source: string): string {
  return PREAMBLE + source + EPILOGUE;
}

function countLines(s: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) if (s.charCodeAt(i) === 10) n++;
  return n;
}

/** A fallback shader used when the user source fails to compile: passes the
 *  input channel through, or shows a flat error colour when nothing is wired. */
export const FALLBACK_FRAGMENT = wrapFragmentShader(
  `void mainImage(out vec4 c, in vec2 fc){
     vec2 uv = fc / iResolution.xy;
     vec4 src = texture(iChannel0, uv);
     c = src.a > 0.0 ? src : vec4(0.85, 0.1, 0.45, 1.0); // magenta = compile error
   }`,
);

/** The vertex shader — a single fullscreen triangle, no attributes. */
export const FULLSCREEN_VERTEX = `#version 300 es
void main(){
  vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;
