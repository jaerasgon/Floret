/**
 * Main-thread shader compile check for the editor panel.
 *
 * The render worker silently falls back on a bad shader (it can't block the
 * frame), so the editor compiles the source itself — on a throwaway 1×1 GL2
 * context — purely to surface line-mapped errors as the user types. No drawing.
 */

import { wrapFragmentShader, preambleLineCount } from './glsl-shader';

export interface ShaderError { line: number; message: string }
export interface CompileResult { ok: boolean; errors: ShaderError[] }

let cachedGl: WebGL2RenderingContext | null | undefined;
function ctx(): WebGL2RenderingContext | null {
  if (cachedGl !== undefined) return cachedGl;
  try {
    const c = typeof document !== 'undefined' ? document.createElement('canvas') : null;
    cachedGl = c ? c.getContext('webgl2') : null;
  } catch { cachedGl = null; }
  return cachedGl ?? null;
}

/**
 * Compile the wrapped fragment shader and return any errors, with line numbers
 * mapped back to the user's source (the preamble offset removed). When no GL2 is
 * available (headless), returns ok so editing is never blocked.
 */
export function checkShaderCompile(source: string): CompileResult {
  const gl = ctx();
  if (!gl) return { ok: true, errors: [] };
  const sh = gl.createShader(gl.FRAGMENT_SHADER);
  if (!sh) return { ok: true, errors: [] };
  gl.shaderSource(sh, wrapFragmentShader(source));
  gl.compileShader(sh);
  const ok = gl.getShaderParameter(sh, gl.COMPILE_STATUS) as boolean;
  const log = ok ? '' : (gl.getShaderInfoLog(sh) ?? '');
  gl.deleteShader(sh);
  if (ok) return { ok: true, errors: [] };

  const pre = preambleLineCount();
  const errors: ShaderError[] = [];
  for (const raw of log.split('\n')) {
    const ln = raw.replace(/\0/g, '').trim();   // GL logs can carry a trailing NUL
    if (!ln) continue;
    // Driver format: "ERROR: 0:<line>: <message>"
    const m = /ERROR:\s*\d+:(\d+):\s*(.*)/.exec(ln);
    if (m) errors.push({ line: Math.max(1, Number(m[1]) - pre), message: m[2]!.trim() });
    else errors.push({ line: 0, message: ln });
  }
  if (errors.length === 0) errors.push({ line: 0, message: 'Shader failed to compile.' });
  return { ok: false, errors };
}
