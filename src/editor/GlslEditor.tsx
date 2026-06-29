import { useRef, type KeyboardEvent } from 'react';

/**
 * Lightweight GLSL editor — a transparent <textarea> over a syntax-highlighted
 * <pre>, kept in sync on scroll. Dep-free (no CodeMirror); a regex tokenizer is
 * plenty for fragment-shader highlighting and matches the rest of the codebase's
 * zero-dependency editor surfaces.
 */

const KEYWORDS = new Set([
  'if', 'else', 'for', 'while', 'do', 'return', 'break', 'continue', 'discard', 'switch', 'case', 'default',
  'in', 'out', 'inout', 'uniform', 'const', 'varying', 'attribute', 'precision', 'highp', 'mediump', 'lowp',
  'layout', 'flat', 'smooth', 'struct', 'true', 'false',
]);
const TYPES = new Set([
  'void', 'float', 'int', 'uint', 'bool', 'vec2', 'vec3', 'vec4', 'ivec2', 'ivec3', 'ivec4',
  'uvec2', 'uvec3', 'uvec4', 'bvec2', 'bvec3', 'bvec4', 'mat2', 'mat3', 'mat4',
  'sampler2D', 'sampler3D', 'samplerCube',
]);
const BUILTINS = new Set([
  'iTime', 'iTimeDelta', 'iFrame', 'iResolution', 'iMouse', 'iDate',
  'iChannel0', 'iChannel1', 'iChannel2', 'iChannel3', 'iChannelResolution',
  'gl_FragCoord', 'gl_Position', 'gl_VertexID', 'mainImage', 'main', 'fragColor', 'fragCoord',
  'texture', 'textureLod', 'texelFetch', 'textureSize',
  'mix', 'clamp', 'smoothstep', 'step', 'length', 'distance', 'normalize', 'dot', 'cross',
  'reflect', 'refract', 'faceforward', 'pow', 'exp', 'exp2', 'log', 'log2', 'sqrt', 'inversesqrt',
  'abs', 'sign', 'floor', 'ceil', 'round', 'trunc', 'fract', 'mod', 'min', 'max',
  'sin', 'cos', 'tan', 'asin', 'acos', 'atan', 'sinh', 'cosh', 'tanh', 'radians', 'degrees',
]);

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

const TOKEN = /(\/\/[^\n]*|\/\*[\s\S]*?\*\/)|(\b\d+\.?\d*(?:[eE][+-]?\d+)?[fFuU]?\b|\.\d+[fF]?)|([A-Za-z_]\w*)|([\s\S])/g;

function highlight(code: string): string {
  let out = '';
  let m: RegExpExecArray | null;
  TOKEN.lastIndex = 0;
  while ((m = TOKEN.exec(code))) {
    if (m[1]) out += `<span class="tk-comment">${esc(m[1])}</span>`;
    else if (m[2]) out += `<span class="tk-num">${esc(m[2])}</span>`;
    else if (m[3]) {
      const w = m[3];
      const cls = KEYWORDS.has(w) ? 'tk-kw' : TYPES.has(w) ? 'tk-type' : BUILTINS.has(w) ? 'tk-builtin' : '';
      out += cls ? `<span class="${cls}">${esc(w)}</span>` : esc(w);
    } else out += esc(m[4]!);
  }
  return out;
}

export function GlslEditor({ value, onChange, className }: {
  value: string;
  onChange: (next: string) => void;
  className?: string;
}) {
  const taRef = useRef<HTMLTextAreaElement | null>(null);
  const preRef = useRef<HTMLPreElement | null>(null);
  const sync = () => {
    const ta = taRef.current, pre = preRef.current;
    if (ta && pre) { pre.scrollTop = ta.scrollTop; pre.scrollLeft = ta.scrollLeft; }
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Tab') {
      e.preventDefault();
      const t = e.currentTarget, a = t.selectionStart, b = t.selectionEnd;
      const next = value.slice(0, a) + '  ' + value.slice(b);
      onChange(next);
      requestAnimationFrame(() => { t.selectionStart = t.selectionEnd = a + 2; });
    }
  };
  return (
    <div className={`glsl-editor ${className ?? ''}`}>
      {/* +'\n' so a trailing newline in the textarea has matching height in the pre */}
      <pre ref={preRef} className="glsl-editor-pre" aria-hidden dangerouslySetInnerHTML={{ __html: highlight(value) + '\n' }} />
      <textarea
        ref={taRef}
        className="glsl-editor-ta nodrag"
        spellCheck={false}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onScroll={sync}
        onKeyDown={onKeyDown}
      />
    </div>
  );
}
