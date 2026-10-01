// Floret scene renderer — WGSL port of the WebGL2 programs in
// src/runtime/gl/gl-vector-renderer.ts. All colour output is premultiplied
// and blended ONE / ONE_MINUS_SRC_ALPHA (gamma space, like WebGL).
//
// Scene coordinates are centre-origin, y down; `view.size` is the scene's
// extent in units (= the canvas size in pixels).

struct View {
    size: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> view: View;

fn to_clip(p: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(p.x / (view.size.x * 0.5), -p.y / (view.size.y * 0.5), 0.0, 1.0);
}

// ─── A. SDF instances: rect / ellipse / capsule ──────────────────────────────

struct SdfIn {
    @location(0) tcr: vec4<f32>,    // type, cx, cy, m00
    @location(1) hcs: vec4<f32>,    // hx, hy, corner, strokeWeight
    @location(2) fill: vec4<f32>,
    @location(3) stroke: vec4<f32>,
    @location(4) misc: vec4<f32>,   // align, m01, m10, m11
};

struct SdfOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) fill: vec4<f32>,
    @location(2) @interpolate(flat) stroke: vec4<f32>,
    @location(3) @interpolate(flat) hcs: vec4<f32>,
    @location(4) @interpolate(flat) type_align: vec2<f32>,
};

@vertex
fn vs_sdf(@builtin(vertex_index) vi: u32, inst: SdfIn) -> SdfOut {
    // Triangle strip: (-1,-1) (1,-1) (-1,1) (1,1).
    let q = vec2<f32>(f32(vi & 1u) * 2.0 - 1.0, f32((vi >> 1u) & 1u) * 2.0 - 1.0);
    let centre = inst.tcr.yz;
    let hf = inst.hcs.xy;
    let sw = inst.hcs.w;
    let pad = sw * 0.5 + 2.0;
    let local = q * (hf + pad);
    // [m00 m01; m10 m11] · local
    let world = centre + vec2<f32>(
        inst.tcr.w * local.x + inst.misc.y * local.y,
        inst.misc.z * local.x + inst.misc.w * local.y,
    );
    var o: SdfOut;
    o.pos = to_clip(world);
    o.local = local;
    o.fill = inst.fill;
    o.stroke = inst.stroke;
    o.hcs = inst.hcs;
    o.type_align = vec2<f32>(inst.tcr.x, inst.misc.x);
    return o;
}

fn sd_rect(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + r;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn sd_capsule(p: vec2<f32>, half_len: f32, r: f32) -> f32 {
    let dx = abs(p.x) - half_len;
    let q = vec2<f32>(max(dx, 0.0), p.y);
    return length(q) - r;
}

fn cbrt_signed(v: f32) -> f32 {
    return sign(v) * pow(abs(v), 1.0 / 3.0);
}

// Exact ellipse distance (Inigo Quilez), as in the TS shader.
fn sd_ellipse(p_in: vec2<f32>, ab_in: vec2<f32>) -> f32 {
    var ab = max(ab_in, vec2<f32>(1e-4));
    if (abs(ab.x - ab.y) < 1e-3) {
        return length(p_in) - ab.x;   // circle: l → 0 is singular
    }
    var p = abs(p_in);
    if (p.x > p.y) {
        p = p.yx;
        ab = ab.yx;
    }
    let l = ab.y * ab.y - ab.x * ab.x;
    let m = ab.x * p.x / l;
    let m2 = m * m;
    let n = ab.y * p.y / l;
    let n2 = n * n;
    let c = (m2 + n2 - 1.0) / 3.0;
    let c3 = c * c * c;
    let q = c3 + m2 * n2 * 2.0;
    let d = c3 + m2 * n2;
    let g = m + m * n2;
    var co: f32;
    if (d < 0.0) {
        let h = acos(clamp(q / c3, -1.0, 1.0)) / 3.0;
        let s = cos(h);
        let t = sin(h) * sqrt(3.0);
        let rx = sqrt(max(-c * (s + t + 2.0) + m2, 0.0));
        let ry = sqrt(max(-c * (s - t + 2.0) + m2, 0.0));
        co = (ry + sign(l) * rx + abs(g) / (rx * ry + 1e-6) - m) / 2.0;
    } else {
        let h = 2.0 * m * n * sqrt(max(d, 0.0));
        let s = cbrt_signed(q + h);
        let u = cbrt_signed(q - h);
        let rx = -s - u - c * 4.0 + 2.0 * m2;
        let ry = (s - u) * sqrt(3.0);
        let rm = sqrt(rx * rx + ry * ry);
        co = (ry / sqrt(max(rm - rx, 1e-6)) + 2.0 * g / rm - m) / 2.0;
    }
    co = clamp(co, -1.0, 1.0);
    let r = ab * vec2<f32>(co, sqrt(max(1.0 - co * co, 0.0)));
    return length(r - p) * sign(p.y - r.y);
}

fn over(dst: vec4<f32>, rgb: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(rgb * a + dst.rgb * (1.0 - a), a + dst.a * (1.0 - a));
}

@fragment
fn fs_sdf(in: SdfOut) -> @location(0) vec4<f32> {
    let hf = in.hcs.xy;
    let corner = in.hcs.z;
    let sw = in.hcs.w;
    let ty = in.type_align.x;
    let align = in.type_align.y;
    var d: f32;
    if (ty < 0.5) {
        d = sd_rect(in.local, hf, corner);
    } else if (ty < 1.5) {
        d = sd_ellipse(in.local, hf);
    } else {
        d = sd_capsule(in.local, hf.x, hf.y);
    }
    let aa = fwidth(d) + 1e-6;
    var col = vec4<f32>(0.0);
    if (in.fill.a > 0.0) {
        let c = 1.0 - smoothstep(-aa, aa, d);
        col = over(col, in.fill.rgb, in.fill.a * c);
    }
    if (ty < 1.5 && in.stroke.a > 0.0 && sw > 0.0) {
        var bc = 0.0;
        if (align > 1.5) {
            bc = sw * 0.5;          // outside
        } else if (align > 0.5) {
            bc = -sw * 0.5;         // inside
        }
        let edge = abs(d - bc) - sw * 0.5;
        let c = 1.0 - smoothstep(-aa, aa, edge);
        col = over(col, in.stroke.rgb, in.stroke.a * c);
    }
    if (col.a <= 0.0) {
        discard;
    }
    return col;
}

// ─── C. Stencil fill (winding pass + cover) ──────────────────────────────────

struct FillOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) col: vec4<f32>,
};

@vertex
fn vs_fill(@location(0) pos: vec2<f32>, @location(1) col: vec4<f32>) -> FillOut {
    var o: FillOut;
    o.pos = to_clip(pos);
    o.col = col;
    return o;
}

@fragment
fn fs_fill(in: FillOut) -> @location(0) vec4<f32> {
    return in.col;   // premultiplied
}

// ─── B. Textured quads (text) ────────────────────────────────────────────────

@group(1) @binding(0) var t_tex: texture_2d<f32>;
@group(1) @binding(1) var t_samp: sampler;

struct TexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
};

@vertex
fn vs_tex(
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tint: vec4<f32>,
) -> TexOut {
    var o: TexOut;
    o.pos = to_clip(pos);
    o.uv = uv;
    o.tint = tint;
    return o;
}

@fragment
fn fs_tex(in: TexOut) -> @location(0) vec4<f32> {
    let t = textureSample(t_tex, t_samp, in.uv);   // premultiplied
    return vec4<f32>(t.rgb * in.tint.rgb, t.a) * in.tint.a;
}

// ─── Layer composite ─────────────────────────────────────────────────────────
// Draws a quad (the whole view, or a rotated clip's corners) that fetches the
// layer texture at the same pixel. The blend mode is pipeline state.

@vertex
fn vs_composite(
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tint: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    return to_clip(pos);
}

@fragment
fn fs_composite(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(t_tex));
    let p = clamp(vec2<i32>(floor(pos.xy)), vec2<i32>(0), dims - vec2<i32>(1));
    return textureLoad(t_tex, p, 0);
}

// ─── Separable Gaussian (style.blur) ─────────────────────────────────────────

struct Blur {
    dir: vec2<f32>,
    sigma: f32,
    _pad: f32,
};
@group(2) @binding(0) var<uniform> blur: Blur;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_gauss(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(t_tex));
    let p = vec2<i32>(floor(pos.xy));
    let dir = vec2<i32>(blur.dir);
    let s = max(blur.sigma, 0.5);
    let r = i32(min(ceil(s * 3.0), 64.0));
    var acc = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var i = -r; i <= r; i++) {
        let w = exp(-0.5 * f32(i * i) / (s * s));
        let q = clamp(p + dir * i, vec2<i32>(0), dims - vec2<i32>(1));
        acc += textureLoad(t_tex, q, 0) * w;
        wsum += w;
    }
    return acc / wsum;
}
