import { describe, it, expect } from 'vitest';
import {
  parseUniforms, uniformPortType, wrapFragmentShader, preambleLineCount, BUILTIN_UNIFORMS,
} from '../glsl-shader';

describe('parseUniforms', () => {
  it('extracts user uniforms in order, typed', () => {
    const src = `uniform float strength;
      uniform vec2 offset;
      uniform vec3 tint;
      uniform sampler2D extra;
      void mainImage(out vec4 c, in vec2 f){ c = vec4(strength); }`;
    expect(parseUniforms(src)).toEqual([
      { name: 'strength', glslType: 'float' },
      { name: 'offset', glslType: 'vec2' },
      { name: 'tint', glslType: 'vec3' },
      { name: 'extra', glslType: 'sampler2D' },
    ]);
  });

  it('excludes ShaderToy built-ins', () => {
    const src = `uniform float iTime; uniform vec3 iResolution; uniform sampler2D iChannel0; uniform float k;`;
    expect(parseUniforms(src)).toEqual([{ name: 'k', glslType: 'float' }]);
    for (const b of ['iTime', 'iResolution', 'iChannel0']) expect(BUILTIN_UNIFORMS.has(b)).toBe(true);
  });

  it('ignores uniforms inside comments', () => {
    const src = `// uniform float commented;
      /* uniform float blockComment; */
      uniform float real;`;
    expect(parseUniforms(src)).toEqual([{ name: 'real', glslType: 'float' }]);
  });

  it('de-duplicates and tolerates precision qualifiers', () => {
    const src = `uniform highp float a; uniform float a; uniform mediump vec2 b;`;
    expect(parseUniforms(src)).toEqual([
      { name: 'a', glslType: 'float' },
      { name: 'b', glslType: 'vec2' },
    ]);
  });
});

describe('uniformPortType', () => {
  it('maps glsl types to floret port types', () => {
    expect(uniformPortType('float')).toBe('number');
    expect(uniformPortType('int')).toBe('number');
    expect(uniformPortType('vec2')).toBe('vec2');
    expect(uniformPortType('vec3')).toBe('color');
    expect(uniformPortType('vec4')).toBe('color');
    expect(uniformPortType('sampler2D')).toBe('layer');
  });
});

describe('wrapFragmentShader', () => {
  it('wraps with #version, built-ins, and a main() that calls mainImage', () => {
    const wrapped = wrapFragmentShader('void mainImage(out vec4 c, in vec2 f){ c = vec4(1.0); }');
    expect(wrapped.startsWith('#version 300 es')).toBe(true);
    expect(wrapped).toContain('uniform float iTime;');
    expect(wrapped).toContain('uniform sampler2D iChannel0;');
    expect(wrapped).toContain('out vec4 _fragColor;');
    expect(wrapped).toContain('mainImage(_fragColor, gl_FragCoord.xy)');
  });

  it('does NOT inject user uniforms (they live in the user source)', () => {
    // The user already declares `uniform float k;` — re-injecting would duplicate-declare.
    const wrapped = wrapFragmentShader('uniform float k; void mainImage(out vec4 c, in vec2 f){ c = vec4(k); }');
    expect(wrapped.match(/uniform float k;/g)?.length).toBe(1);
  });

  it('preambleLineCount counts the lines before user source', () => {
    const wrapped = wrapFragmentShader('LINE0');
    const before = wrapped.slice(0, wrapped.indexOf('LINE0'));
    expect(before.split('\n').length - 1).toBe(preambleLineCount());
  });
});
