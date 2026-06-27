// ─── Spike v2: WebGL2 vector renderer (full shape vocabulary) ───────────────
// One GL output, three pipelines, drawn in submission order so painter's
// layering is preserved:
//
//   A. SDF instances   — rect (rounded), ellipse (exact, IQ), capsule.
//                        Also: line, point, polygon strokes (edge capsules),
//                        particle batches (one instance per particle).
//      Per-instance 2×2 matrix → rotation / non-uniform scale / shear, exact.
//   C. Tessellated fill — triangle / quad / polygon / path fills via ear
//                        clipping; flat per-vertex colour; MSAA for edges.
//   B. Textured quad    — text (rasterised glyph canvas) and images.
//
// The harness (shader-spike.ts) walks SceneNodes and routes each shape to a
// pipeline via FrameBuilder; the renderer replays the command list.

// ── instance / vertex layouts ──
export const SDF_FLOATS = 20;          // 5 vec4 per SDF instance
export const FILL_FLOATS = 6;          // x,y, r,g,b,a per fill vertex
export const enum SdfType { Rect = 0, Ellipse = 1, Capsule = 2 }
export const enum SdfAlign { Center = 0, Inside = 1, Outside = 2 }

// world-space AABB [minx, miny, maxx, maxy] for scissor clipping, or null
export type Clip = readonly [number, number, number, number] | null;
type Cmd =
  | { kind: 'sdf'; start: number; count: number; clip: Clip }
  | { kind: 'polyFill'; fanStart: number; fanCount: number; coverStart: number; clip: Clip }
  | { kind: 'tex'; quad: number; clip: Clip }
  | { kind: 'pushLayer' }
  | { kind: 'popLayer'; blend: number }
  | { kind: 'popLayerBlur'; sigma: number };

export interface TexSource { tex: WebGLTexture; }

// ── FrameBuilder: the harness fills this each frame ──
export class FrameBuilder {
  sdf = new Float32Array(4096 * SDF_FLOATS);
  sdfCount = 0;
  fill = new Float32Array(4096 * FILL_FLOATS);
  fillVerts = 0;
  // four world corners (TL, TR, BL, BR) so rotation/scale survive
  texQuads: { c: [number, number, number, number, number, number, number, number]; u0: number; v0: number; u1: number; v1: number; tint: [number, number, number, number]; tex: WebGLTexture }[] = [];
  cmds: Cmd[] = [];
  skipped = 0;
  // active clip state (groups push/pop); commands snapshot the top.
  // blend is expressed structurally as pushLayer/popLayer commands (FBO compositing).
  private clipStack: Clip[] = [];
  curClip(): Clip { return this.clipStack.length ? this.clipStack[this.clipStack.length - 1]! : null; }
  pushClip(c: Clip) { this.clipStack.push(c); }
  popClip() { this.clipStack.pop(); }
  pushLayer() { this.cmds.push({ kind: 'pushLayer' }); }
  popLayer(blend: number) { this.cmds.push({ kind: 'popLayer', blend }); }
  popLayerBlur(sigma: number) { this.cmds.push({ kind: 'popLayerBlur', sigma }); }

  reset() { this.sdfCount = 0; this.fillVerts = 0; this.texQuads.length = 0; this.cmds.length = 0; this.skipped = 0; this.clipStack.length = 0; }

  private growSdf() { const n = new Float32Array(this.sdf.length * 2); n.set(this.sdf); this.sdf = n; }
  private growFill() { const n = new Float32Array(this.fill.length * 2); n.set(this.fill); this.fill = n; }

  pushSdf(
    type: number, cx: number, cy: number, m00: number, m01: number, m10: number, m11: number,
    hx: number, hy: number, corner: number, sw: number,
    fill: ArrayLike<number>, stroke: ArrayLike<number>, align: number,
  ) {
    if ((this.sdfCount + 1) * SDF_FLOATS > this.sdf.length) this.growSdf();
    const o = this.sdfCount * SDF_FLOATS, b = this.sdf;
    b[o] = type; b[o + 1] = cx; b[o + 2] = cy; b[o + 3] = m00;
    b[o + 4] = hx; b[o + 5] = hy; b[o + 6] = corner; b[o + 7] = sw;
    b[o + 8] = fill[0]!; b[o + 9] = fill[1]!; b[o + 10] = fill[2]!; b[o + 11] = fill[3]!;
    b[o + 12] = stroke[0]!; b[o + 13] = stroke[1]!; b[o + 14] = stroke[2]!; b[o + 15] = stroke[3]!;
    b[o + 16] = align; b[o + 17] = m01; b[o + 18] = m10; b[o + 19] = m11;
    this.sdfCount++;
    const clip = this.curClip();
    const last = this.cmds[this.cmds.length - 1];
    if (last && last.kind === 'sdf' && last.clip === clip) last.count++;
    else this.cmds.push({ kind: 'sdf', start: this.sdfCount - 1, count: 1, clip });
  }

  private fv(x: number, y: number, c: ArrayLike<number>) {
    if ((this.fillVerts + 1) * FILL_FLOATS > this.fill.length) this.growFill();
    const o = this.fillVerts * FILL_FLOATS, b = this.fill;
    b[o] = x; b[o + 1] = y; b[o + 2] = c[0]! * c[3]!; b[o + 3] = c[1]! * c[3]!; b[o + 4] = c[2]! * c[3]!; b[o + 5] = c[3]!;
    this.fillVerts++;
  }
  // A polygon fill: store the polygon's vertices (a fan for the stencil winding
  // pass) + a 4-vertex AABB cover strip. The renderer fills via nonzero stencil,
  // so self-intersecting / holed / concave polygons are all correct.
  pushPolyFill(pts: ArrayLike<{ x: number; y: number }>, c: ArrayLike<number>) {
    const n = pts.length; if (n < 3) return;
    const fanStart = this.fillVerts;
    let minx = Infinity, miny = Infinity, maxx = -Infinity, maxy = -Infinity;
    for (let i = 0; i < n; i++) {
      const p = pts[i]!; this.fv(p.x, p.y, c);
      if (p.x < minx) minx = p.x; if (p.x > maxx) maxx = p.x;
      if (p.y < miny) miny = p.y; if (p.y > maxy) maxy = p.y;
    }
    const coverStart = this.fillVerts;
    this.fv(minx, miny, c); this.fv(maxx, miny, c); this.fv(minx, maxy, c); this.fv(maxx, maxy, c);
    this.cmds.push({ kind: 'polyFill', fanStart, fanCount: n, coverStart, clip: this.curClip() });
  }
  pushTexQuad(q: FrameBuilder['texQuads'][number]) {
    this.texQuads.push(q);
    this.cmds.push({ kind: 'tex', quad: this.texQuads.length - 1, clip: this.curClip() });
  }
}

// ── shaders ──
const SDF_VERT = `#version 300 es
layout(location=0) in vec2 a_quad;
layout(location=1) in vec4 a_tcr;    // type, cx, cy, m00
layout(location=2) in vec4 a_hcs;    // hx, hy, corner, strokeWeight
layout(location=3) in vec4 a_fill;
layout(location=4) in vec4 a_stroke;
layout(location=5) in vec4 a_misc;   // align, m01, m10, m11
uniform vec2 u_view;
out vec2 v_local;
flat out vec4 v_fill; flat out vec4 v_stroke; flat out vec4 v_hcs;
flat out float v_type; flat out float v_align;
void main() {
  vec2 centre = a_tcr.yz;
  mat2 m = mat2(a_tcr.w, a_misc.z, a_misc.y, a_misc.w);  // [m00 m01; m10 m11]
  vec2 hf = a_hcs.xy; float sw = a_hcs.w;
  float pad = sw * 0.5 + 2.0;
  vec2 local = a_quad * (hf + pad);
  vec2 world = centre + m * local;
  gl_Position = vec4(world.x / (u_view.x * 0.5), -world.y / (u_view.y * 0.5), 0.0, 1.0);
  v_local = local; v_fill = a_fill; v_stroke = a_stroke; v_hcs = a_hcs;
  v_type = a_tcr.x; v_align = a_misc.x;
}`;

const SDF_FRAG = `#version 300 es
precision highp float;
in vec2 v_local;
flat in vec4 v_fill; flat in vec4 v_stroke; flat in vec4 v_hcs;
flat in float v_type; flat in float v_align;
out vec4 frag;
float sdRect(vec2 p, vec2 b, float r){ vec2 q = abs(p) - b + r; return length(max(q,0.0)) + min(max(q.x,q.y),0.0) - r; }
float sdCapsule(vec2 p, float halfLen, float r){ float dx = abs(p.x) - halfLen; vec2 q = vec2(max(dx,0.0), p.y); return length(q) - r; }
float sdEllipse(vec2 p, vec2 ab){            // exact (Inigo Quilez)
  ab = max(ab, 1e-4);
  if (abs(ab.x - ab.y) < 1e-3) return length(p) - ab.x;   // circle: l→0 singular
  p = abs(p);
  if (p.x > p.y){ p = p.yx; ab = ab.yx; }
  float l = ab.y*ab.y - ab.x*ab.x;
  float m = ab.x*p.x/l; float m2 = m*m;
  float n = ab.y*p.y/l; float n2 = n*n;
  float c = (m2 + n2 - 1.0)/3.0; float c3 = c*c*c;
  float q = c3 + m2*n2*2.0;
  float d = c3 + m2*n2;
  float g = m + m*n2;
  float co;
  if (d < 0.0){
    float h = acos(clamp(q/c3,-1.0,1.0))/3.0;
    float s = cos(h); float t = sin(h)*sqrt(3.0);
    float rx = sqrt(max(-c*(s+t+2.0)+m2, 0.0));
    float ry = sqrt(max(-c*(s-t+2.0)+m2, 0.0));
    co = (ry + sign(l)*rx + abs(g)/(rx*ry+1e-6) - m)/2.0;
  } else {
    float h = 2.0*m*n*sqrt(max(d,0.0));
    float s = sign(q+h)*pow(abs(q+h), 1.0/3.0);
    float u = sign(q-h)*pow(abs(q-h), 1.0/3.0);
    float rx = -s - u - c*4.0 + 2.0*m2;
    float ry = (s - u)*sqrt(3.0);
    float rm = sqrt(rx*rx + ry*ry);
    co = (ry/sqrt(max(rm-rx,1e-6)) + 2.0*g/rm - m)/2.0;
  }
  co = clamp(co, -1.0, 1.0);
  vec2 r = ab*vec2(co, sqrt(max(1.0-co*co, 0.0)));
  return length(r - p)*sign(p.y - r.y);
}
vec4 over(vec4 dst, vec3 rgb, float a){ return vec4(rgb*a + dst.rgb*(1.0-a), a + dst.a*(1.0-a)); }
void main(){
  vec2 hf = v_hcs.xy; float corner = v_hcs.z; float sw = v_hcs.w;
  float d;
  if (v_type < 0.5)      d = sdRect(v_local, hf, corner);
  else if (v_type < 1.5) d = sdEllipse(v_local, hf);
  else                   d = sdCapsule(v_local, hf.x, hf.y);
  float aa = fwidth(d) + 1e-6;
  vec4 col = vec4(0.0);
  if (v_fill.a > 0.0){ float c = 1.0 - smoothstep(-aa, aa, d); col = over(col, v_fill.rgb, v_fill.a*c); }
  if (v_type < 1.5 && v_stroke.a > 0.0 && sw > 0.0){
    float bc = v_align < 0.5 ? 0.0 : (v_align < 1.5 ? -sw*0.5 : sw*0.5);
    float edge = abs(d - bc) - sw*0.5;
    float c = 1.0 - smoothstep(-aa, aa, edge);
    col = over(col, v_stroke.rgb, v_stroke.a*c);
  }
  if (col.a <= 0.0) discard;
  frag = col;
}`;

const FILL_VERT = `#version 300 es
layout(location=0) in vec2 a_pos;
layout(location=1) in vec4 a_col;    // premultiplied
uniform vec2 u_view;
out vec4 v_col;
void main(){ gl_Position = vec4(a_pos.x/(u_view.x*0.5), -a_pos.y/(u_view.y*0.5), 0.0, 1.0); v_col = a_col; }`;
const FILL_FRAG = `#version 300 es
precision highp float; in vec4 v_col; out vec4 frag;
void main(){ frag = v_col; }`;       // premultiplied; blend ONE,1-SA

const TEX_VERT = `#version 300 es
layout(location=0) in vec2 a_pos; layout(location=1) in vec2 a_uv;
uniform vec2 u_view; out vec2 v_uv;
void main(){ gl_Position = vec4(a_pos.x/(u_view.x*0.5), -a_pos.y/(u_view.y*0.5), 0.0, 1.0); v_uv = a_uv; }`;
const TEX_FRAG = `#version 300 es
precision highp float; in vec2 v_uv; out vec4 frag;
uniform sampler2D u_tex; uniform vec4 u_tint;
void main(){ vec4 t = texture(u_tex, v_uv); frag = vec4(t.rgb*u_tint.rgb, t.a)*u_tint.a; }`;

// Fullscreen blit — samples a layer FBO texture (premultiplied) to composite a
// blend group onto its parent. The blend mode is set via blendFunc externally.
const BLIT_VERT = `#version 300 es
layout(location=0) in vec2 a_pos;
out vec2 v_uv;
void main(){ v_uv = a_pos * 0.5 + 0.5; gl_Position = vec4(a_pos, 0.0, 1.0); }`;
const BLIT_FRAG = `#version 300 es
precision highp float; in vec2 v_uv; out vec4 frag;
uniform sampler2D u_tex;
void main(){ frag = texture(u_tex, v_uv); }`;

// Separable Gaussian (one axis per pass) for style.blur.
const GAUSS_FRAG = `#version 300 es
precision highp float; in vec2 v_uv; out vec4 frag;
uniform sampler2D u_tex; uniform vec2 u_dir; uniform float u_sigma;
void main(){
  float s = max(u_sigma, 0.5);
  int R = int(min(ceil(s * 3.0), 64.0));
  vec4 acc = vec4(0.0); float wsum = 0.0;
  for (int i = -64; i <= 64; i++) {
    if (i < -R || i > R) continue;
    float w = exp(-0.5 * float(i * i) / (s * s));
    acc += texture(u_tex, v_uv + u_dir * float(i)) * w; wsum += w;
  }
  frag = acc / wsum;
}`;

function sh(gl: WebGL2RenderingContext, t: number, src: string) {
  const s = gl.createShader(t)!; gl.shaderSource(s, src); gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error('shader: ' + gl.getShaderInfoLog(s));
  return s;
}
function prog(gl: WebGL2RenderingContext, v: string, f: string) {
  const p = gl.createProgram()!; gl.attachShader(p, sh(gl, gl.VERTEX_SHADER, v)); gl.attachShader(p, sh(gl, gl.FRAGMENT_SHADER, f));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error('link: ' + gl.getProgramInfoLog(p));
  return p;
}

export class VectorRenderer {
  readonly gl: WebGL2RenderingContext;
  private pSdf: WebGLProgram; private pFill: WebGLProgram; private pTex: WebGLProgram;
  private uViewSdf: WebGLUniformLocation; private uViewFill: WebGLUniformLocation; private uViewTex: WebGLUniformLocation;
  private uTint: WebGLUniformLocation;
  private vaoSdf: WebGLVertexArrayObject; private instBuf: WebGLBuffer; private cap = 0;
  private vaoFill: WebGLVertexArrayObject; private fillBuf: WebGLBuffer; private fillCap = 0;
  private vaoTex: WebGLVertexArrayObject; private texVBuf: WebGLBuffer;
  private texCache = new WeakMap<object, WebGLTexture>();
  // blend-group FBO pool (full-viewport layer textures), used as a LIFO stack
  private pBlit: WebGLProgram; private pGauss: WebGLProgram; private vaoBlit: WebGLVertexArrayObject;
  private layerPool: { fbo: WebGLFramebuffer; tex: WebGLTexture; rb: WebGLRenderbuffer }[] = [];
  private layerInUse = 0; private poolW = 0; private poolH = 0;

  constructor(canvas: HTMLCanvasElement | OffscreenCanvas) {
    const gl = canvas.getContext('webgl2', { alpha: false, antialias: true, premultipliedAlpha: false, stencil: true }) as WebGL2RenderingContext | null;
    if (!gl) throw new Error('WebGL2 unavailable');
    this.gl = gl;
    gl.clearStencil(0);
    this.pSdf = prog(gl, SDF_VERT, SDF_FRAG);
    this.pFill = prog(gl, FILL_VERT, FILL_FRAG);
    this.pTex = prog(gl, TEX_VERT, TEX_FRAG);
    this.pBlit = prog(gl, BLIT_VERT, BLIT_FRAG);
    this.pGauss = prog(gl, BLIT_VERT, GAUSS_FRAG);
    this.uViewSdf = gl.getUniformLocation(this.pSdf, 'u_view')!;
    this.uViewFill = gl.getUniformLocation(this.pFill, 'u_view')!;
    this.uViewTex = gl.getUniformLocation(this.pTex, 'u_view')!;
    this.uTint = gl.getUniformLocation(this.pTex, 'u_tint')!;

    // SDF VAO: static quad (loc 0) + instanced attrs (loc 1..5)
    this.vaoSdf = gl.createVertexArray()!; gl.bindVertexArray(this.vaoSdf);
    const quad = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    this.instBuf = gl.createBuffer()!; gl.bindBuffer(gl.ARRAY_BUFFER, this.instBuf);
    for (let i = 0; i < 5; i++) { const l = 1 + i; gl.enableVertexAttribArray(l); gl.vertexAttribPointer(l, 4, gl.FLOAT, false, SDF_FLOATS * 4, i * 16); gl.vertexAttribDivisor(l, 1); }

    // Fill VAO: interleaved pos(2)+col(4)
    this.vaoFill = gl.createVertexArray()!; gl.bindVertexArray(this.vaoFill);
    this.fillBuf = gl.createBuffer()!; gl.bindBuffer(gl.ARRAY_BUFFER, this.fillBuf);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 2, gl.FLOAT, false, FILL_FLOATS * 4, 0);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 4, gl.FLOAT, false, FILL_FLOATS * 4, 8);

    // Tex VAO: dynamic pos(2)+uv(2) per quad (4 verts)
    this.vaoTex = gl.createVertexArray()!; gl.bindVertexArray(this.vaoTex);
    this.texVBuf = gl.createBuffer()!; gl.bindBuffer(gl.ARRAY_BUFFER, this.texVBuf);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 16, 0);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 2, gl.FLOAT, false, 16, 8);

    // Blit VAO: static fullscreen NDC quad
    this.vaoBlit = gl.createVertexArray()!; gl.bindVertexArray(this.vaoBlit);
    const bq = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, bq);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);

    gl.disable(gl.DEPTH_TEST);
  }

  // Acquire a full-viewport FBO from the pool (LIFO). Pool rebuilds on resize.
  private acquireLayer(): { fbo: WebGLFramebuffer; tex: WebGLTexture } {
    const gl = this.gl, w = gl.drawingBufferWidth, h = gl.drawingBufferHeight;
    if (w !== this.poolW || h !== this.poolH) {
      for (const p of this.layerPool) { gl.deleteFramebuffer(p.fbo); gl.deleteTexture(p.tex); gl.deleteRenderbuffer(p.rb); }
      this.layerPool = []; this.layerInUse = 0; this.poolW = w; this.poolH = h;
    }
    let layer = this.layerPool[this.layerInUse];
    if (!layer) {
      const tex = gl.createTexture()!; gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      const fbo = gl.createFramebuffer()!; gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
      // depth-stencil so polygon fills can use the stencil winding technique inside blend groups
      const rb = gl.createRenderbuffer()!; gl.bindRenderbuffer(gl.RENDERBUFFER, rb);
      gl.renderbufferStorage(gl.RENDERBUFFER, gl.DEPTH24_STENCIL8, w, h);
      gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.DEPTH_STENCIL_ATTACHMENT, gl.RENDERBUFFER, rb);
      layer = { fbo, tex, rb }; this.layerPool[this.layerInUse] = layer;
    }
    this.layerInUse++;
    return layer;
  }

  // Composite a group's premultiplied layer texture onto the parent target
  // with a separable blend mode. Non-separable modes fall back to source-over.
  private composite(tex: WebGLTexture, blend: number) {
    const gl = this.gl;
    gl.disable(gl.SCISSOR_TEST);
    gl.blendEquation(gl.FUNC_ADD);
    switch (blend) {
      case 1: gl.blendFunc(gl.ONE, gl.ONE); break;                        // lighter / add
      case 2: gl.blendFunc(gl.DST_COLOR, gl.ONE_MINUS_SRC_ALPHA); break;  // multiply
      case 3: gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_COLOR); break;        // screen
      default: gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA); break;       // source-over (premult)
    }
    gl.useProgram(this.pBlit);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.uniform1i(gl.getUniformLocation(this.pBlit, 'u_tex'), 0);
    gl.bindVertexArray(this.vaoBlit);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }

  // One separable-Gaussian pass (replace mode) sampling `tex` along (dx,dy).
  private gauss(tex: WebGLTexture, dx: number, dy: number, sigma: number) {
    const gl = this.gl;
    gl.useProgram(this.pGauss);
    gl.uniform2f(gl.getUniformLocation(this.pGauss, 'u_dir'), dx, dy);
    gl.uniform1f(gl.getUniformLocation(this.pGauss, 'u_sigma'), sigma);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.uniform1i(gl.getUniformLocation(this.pGauss, 'u_tex'), 0);
    gl.bindVertexArray(this.vaoBlit);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }

  /** Upload (or reuse) a texture for a CanvasImageSource keyed by object identity. */
  texture(key: object, source: TexImageSource): WebGLTexture {
    const gl = this.gl;
    let t = this.texCache.get(key);
    if (!t) { t = gl.createTexture()!; this.texCache.set(key, t); }
    gl.bindTexture(gl.TEXTURE_2D, t);
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, source);
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    return t;
  }

  private applyClip(clip: Clip, viewW: number, viewH: number) {
    const gl = this.gl;
    if (!clip) { gl.disable(gl.SCISSOR_TEST); return; }
    const bw = gl.drawingBufferWidth, bh = gl.drawingBufferHeight;
    const sx = (x: number) => (x / viewW + 0.5) * bw;
    const sy = (y: number) => (0.5 - y / viewH) * bh;   // GL scissor origin is bottom-left
    const x0 = sx(clip[0]), x1 = sx(clip[2]), ya = sy(clip[1]), yb = sy(clip[3]);
    const left = Math.floor(Math.min(x0, x1)), right = Math.ceil(Math.max(x0, x1));
    const bot = Math.floor(Math.min(ya, yb)), top = Math.ceil(Math.max(ya, yb));
    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(left, bot, Math.max(0, right - left), Math.max(0, top - bot));
  }

  render(fb: FrameBuilder, viewW: number, viewH: number, bg: [number, number, number]) {
    const gl = this.gl;
    gl.viewport(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight);
    gl.disable(gl.SCISSOR_TEST);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.clearColor(bg[0], bg[1], bg[2], 1); gl.clear(gl.COLOR_BUFFER_BIT | gl.STENCIL_BUFFER_BIT);
    gl.enable(gl.BLEND);
    // FBO target stack: base = default framebuffer (null). Blend groups push a
    // layer FBO; popLayer composites it back. layerInUse resets each frame.
    this.layerInUse = 0;
    const targets: ({ fbo: WebGLFramebuffer; tex: WebGLTexture } | null)[] = [null];

    // upload SDF + fill buffers once
    if (fb.sdfCount > 0) {
      gl.bindBuffer(gl.ARRAY_BUFFER, this.instBuf);
      if (fb.sdfCount > this.cap) { this.cap = Math.ceil(fb.sdfCount * 1.5); gl.bufferData(gl.ARRAY_BUFFER, this.cap * SDF_FLOATS * 4, gl.DYNAMIC_DRAW); }
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, fb.sdf.subarray(0, fb.sdfCount * SDF_FLOATS));
    }
    if (fb.fillVerts > 0) {
      gl.bindBuffer(gl.ARRAY_BUFFER, this.fillBuf);
      if (fb.fillVerts > this.fillCap) { this.fillCap = Math.ceil(fb.fillVerts * 1.5); gl.bufferData(gl.ARRAY_BUFFER, this.fillCap * FILL_FLOATS * 4, gl.DYNAMIC_DRAW); }
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, fb.fill.subarray(0, fb.fillVerts * FILL_FLOATS));
    }

    for (const cmd of fb.cmds) {
      if (cmd.kind === 'pushLayer') {
        const layer = this.acquireLayer();
        gl.bindFramebuffer(gl.FRAMEBUFFER, layer.fbo);
        gl.disable(gl.SCISSOR_TEST);
        gl.clearColor(0, 0, 0, 0); gl.clear(gl.COLOR_BUFFER_BIT | gl.STENCIL_BUFFER_BIT);
        targets.push(layer);
        continue;
      }
      if (cmd.kind === 'popLayer') {
        const child = targets.pop()!;
        const parent = targets[targets.length - 1] ?? null;
        gl.bindFramebuffer(gl.FRAMEBUFFER, parent ? parent.fbo : null);
        if (child) this.composite(child.tex, cmd.blend);
        this.layerInUse--;
        continue;
      }
      if (cmd.kind === 'popLayerBlur') {
        const child = targets.pop()!;
        const parent = targets[targets.length - 1] ?? null;
        const w = gl.drawingBufferWidth, h = gl.drawingBufferHeight;
        const temp = this.acquireLayer();
        gl.disable(gl.SCISSOR_TEST);
        gl.blendFunc(gl.ONE, gl.ZERO);                 // replace, not blend
        gl.bindFramebuffer(gl.FRAMEBUFFER, temp.fbo); this.gauss(child.tex, 1 / w, 0, cmd.sigma);
        gl.bindFramebuffer(gl.FRAMEBUFFER, child.fbo); this.gauss(temp.tex, 0, 1 / h, cmd.sigma);
        gl.bindFramebuffer(gl.FRAMEBUFFER, parent ? parent.fbo : null);
        this.composite(child.tex, 0);                  // source-over
        this.layerInUse -= 2;                          // release temp + child
        continue;
      }
      this.applyClip(cmd.clip, viewW, viewH);
      if (cmd.kind === 'sdf') {
        gl.useProgram(this.pSdf); gl.uniform2f(this.uViewSdf, viewW, viewH);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
        gl.bindVertexArray(this.vaoSdf);
        // draw a contiguous instance slice via base-instance emulation: re-point attrs
        gl.bindBuffer(gl.ARRAY_BUFFER, this.instBuf);
        for (let i = 0; i < 5; i++) { const l = 1 + i; gl.vertexAttribPointer(l, 4, gl.FLOAT, false, SDF_FLOATS * 4, cmd.start * SDF_FLOATS * 4 + i * 16); }
        gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, cmd.count);
      } else if (cmd.kind === 'polyFill') {
        gl.useProgram(this.pFill); gl.uniform2f(this.uViewFill, viewW, viewH);
        gl.bindVertexArray(this.vaoFill);
        gl.enable(gl.STENCIL_TEST);
        // winding pass: accumulate nonzero winding in stencil, no colour
        gl.colorMask(false, false, false, false);
        gl.stencilFunc(gl.ALWAYS, 0, 0xFF);
        gl.stencilOpSeparate(gl.FRONT, gl.KEEP, gl.KEEP, gl.INCR_WRAP);
        gl.stencilOpSeparate(gl.BACK, gl.KEEP, gl.KEEP, gl.DECR_WRAP);
        gl.drawArrays(gl.TRIANGLE_FAN, cmd.fanStart, cmd.fanCount);
        // cover pass: fill where winding != 0, reset stencil to 0
        gl.colorMask(true, true, true, true);
        gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);   // premultiplied
        gl.stencilFunc(gl.NOTEQUAL, 0, 0xFF);
        gl.stencilOp(gl.KEEP, gl.KEEP, gl.ZERO);
        gl.drawArrays(gl.TRIANGLE_STRIP, cmd.coverStart, 4);
        gl.disable(gl.STENCIL_TEST);
      } else {
        const q = fb.texQuads[cmd.quad]!;
        gl.useProgram(this.pTex); gl.uniform2f(this.uViewTex, viewW, viewH);
        gl.uniform4f(this.uTint, q.tint[0], q.tint[1], q.tint[2], q.tint[3]);
        gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);   // texture is premultiplied
        gl.bindVertexArray(this.vaoTex);
        gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, q.tex);
        gl.uniform1i(gl.getUniformLocation(this.pTex, 'u_tex'), 0);
        const c = q.c;   // TL, TR, BL, BR
        gl.bindBuffer(gl.ARRAY_BUFFER, this.texVBuf);
        gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([
          c[0], c[1], q.u0, q.v0, c[2], c[3], q.u1, q.v0, c[4], c[5], q.u0, q.v1, c[6], c[7], q.u1, q.v1,
        ]), gl.DYNAMIC_DRAW);
        gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
      }
    }
    gl.bindVertexArray(null);
  }
}

// ── ear-clipping triangulation (simple polygon, no holes; concave OK) ──
export function earClip(pts: ReadonlyArray<{ x: number; y: number }>): number[] {
  const n = pts.length;
  if (n < 3) return [];
  // signed area → orientation
  let area = 0;
  for (let i = 0, j = n - 1; i < n; j = i++) area += (pts[j]!.x * pts[i]!.y - pts[i]!.x * pts[j]!.y);
  const ccw = area > 0;
  const idx = Array.from({ length: n }, (_, i) => i);
  const tris: number[] = [];
  const cross = (ax: number, ay: number, bx: number, by: number, cx: number, cy: number) => (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
  const inTri = (px: number, py: number, ax: number, ay: number, bx: number, by: number, cx: number, cy: number) => {
    const d1 = cross(px, py, ax, ay, bx, by), d2 = cross(px, py, bx, by, cx, cy), d3 = cross(px, py, cx, cy, ax, ay);
    const neg = d1 < 0 || d2 < 0 || d3 < 0, pos = d1 > 0 || d2 > 0 || d3 > 0;
    return !(neg && pos);
  };
  let guard = 0;
  while (idx.length > 3 && guard++ < n * n) {
    let clipped = false;
    for (let i = 0; i < idx.length; i++) {
      const a = idx[(i + idx.length - 1) % idx.length]!, b = idx[i]!, c = idx[(i + 1) % idx.length]!;
      const ax = pts[a]!.x, ay = pts[a]!.y, bx = pts[b]!.x, by = pts[b]!.y, cx = pts[c]!.x, cy = pts[c]!.y;
      const conv = ccw ? cross(ax, ay, bx, by, cx, cy) > 0 : cross(ax, ay, bx, by, cx, cy) < 0;
      if (!conv) continue;
      let ear = true;
      for (const k of idx) {
        if (k === a || k === b || k === c) continue;
        if (inTri(pts[k]!.x, pts[k]!.y, ax, ay, bx, by, cx, cy)) { ear = false; break; }
      }
      if (!ear) continue;
      tris.push(a, b, c); idx.splice(i, 1); clipped = true; break;
    }
    if (!clipped) break; // degenerate; bail
  }
  if (idx.length === 3) tris.push(idx[0]!, idx[1]!, idx[2]!);
  return tris;
}
