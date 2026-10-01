//! Scene → GPU batches. A port of `src/runtime/gl/scene-to-batches.ts` plus
//! the `FrameBuilder` half of `gl-vector-renderer.ts`.
//!
//! Walks the `SceneNode` tree and routes every shape to a pipeline:
//!
//! - **SDF instances**: rect / ellipse (fill + stroke with align), capsules
//!   for lines, polygon / arc / triangle / quad outlines, points.
//! - **Stencil fill**: triangle / quad / polygon / arc fills — a fan into the
//!   stencil buffer (nonzero winding) then a cover quad.
//! - **Textured quads**: text runs (rasterised CPU-side, see `text.rs`).
//!
//! Group transforms are baked into world coordinates (incl. shear), blend
//! groups and blurred shapes become layer push/pop commands, clips become
//! scissor rects (axis-aligned) or a masked layer composite (rotated).
//!
//! World coordinates are centre-origin, y down — what the web app's canvas
//! driver feeds its renderer (`centreOrigin: true`).
//!
//! This module is pure CPU code: no wgpu types, so it can be unit-tested
//! without a GPU.

use crate::text::{TextKey, TextRasterizer};
use floret_engine::scene::{
    catmull_rom_beziers, mul, resolve_box, Affine, Paint, ResolvedStyle, SceneNode,
};
use floret_engine::Vec2;
use std::sync::Arc;

pub type Rgba = [f32; 4];
const TRANSPARENT: Rgba = [0.0; 4];

/// Spline flattening: line segments per Catmull–Rom span (TS: `K = 14`).
pub const SPLINE_STEPS: usize = 14;
/// Arc flattening: segments per arc (TS: `N = 28`).
pub const ARC_STEPS: usize = 28;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SdfType {
    Rect = 0,
    Ellipse = 1,
    Capsule = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SdfAlign {
    Center = 0,
    Inside = 1,
    Outside = 2,
}

/// Composite modes for blend groups. Unknown CSS modes fall back to
/// source-over, like TS (`darken` / `lighten` have codes there but the
/// compositor only implements these four).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Blend {
    SourceOver = 0,
    Add = 1,
    Multiply = 2,
    Screen = 3,
}

impl Blend {
    pub fn from_css(mode: &str) -> Blend {
        match mode {
            "lighter" => Blend::Add,
            "multiply" => Blend::Multiply,
            "screen" => Blend::Screen,
            _ => Blend::SourceOver,
        }
    }
}

/// One SDF instance: 5 × vec4, same packing as TS `SDF_FLOATS`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SdfInstance {
    /// type, cx, cy, m00
    pub tcr: [f32; 4],
    /// hx, hy, corner, stroke weight
    pub hcs: [f32; 4],
    /// premultiplied in the shader
    pub fill: [f32; 4],
    pub stroke: [f32; 4],
    /// align, m01, m10, m11
    pub misc: [f32; 4],
}

/// A stencil-fill vertex: position + premultiplied colour.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FillVertex {
    pub pos: [f32; 2],
    pub col: [f32; 4],
}

/// A textured-quad / composite vertex.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TexVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub tint: [f32; 4],
}

/// World-space AABB `[minx, miny, maxx, maxy]` for scissoring.
pub type Clip = Option<[f32; 4]>;

/// An empty AABB (unions to whatever it meets).
const EMPTY: [f32; 4] = [
    f32::INFINITY,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::NEG_INFINITY,
];

fn union(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

fn intersect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

fn aabb(pts: impl IntoIterator<Item = [f64; 2]>) -> [f32; 4] {
    pts.into_iter().fold(EMPTY, |b, p| {
        union(b, [p[0] as f32, p[1] as f32, p[0] as f32, p[1] as f32])
    })
}

/// Pixel radius of the blur kernel for `sigma` (matches the shader).
pub fn blur_radius(sigma: f32) -> f32 {
    (sigma.max(0.5) * 3.0).ceil().min(64.0)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// Instanced SDF draw of `sdf[start..start + count]`.
    Sdf { start: u32, count: u32, clip: Clip },
    /// Stencil winding pass over `fill[fan_start..fan_start + fan_count]`
    /// (a triangle list), then the 6-vertex cover at `cover_start`.
    PolyFill {
        fan_start: u32,
        fan_count: u32,
        cover_start: u32,
        clip: Clip,
    },
    /// Textured quad `tex_verts[quad * 6..quad * 6 + 6]` with `texture`.
    Tex {
        quad: u32,
        texture: TextKey,
        clip: Clip,
    },
    /// Start drawing into a fresh transparent layer.
    PushLayer,
    /// Composite the top layer onto its parent. `mask` is the composite
    /// quad (6 vertices at `composite[mask * 6..]`) for rotated clips;
    /// `None` = the whole viewport. `bounds`: world AABB of the layer's
    /// content — nothing outside it needs compositing.
    PopLayer {
        blend: Blend,
        mask: Option<u32>,
        bounds: [f32; 4],
    },
    /// Gaussian-blur the top layer (`sigma` in pixels), then composite it
    /// source-over. `bounds` covers the content plus the kernel radius.
    PopLayerBlur { sigma: f32, bounds: [f32; 4] },
}

/// One frame's worth of batched geometry + the ordered command list.
#[derive(Default)]
pub struct FrameBuilder {
    pub sdf: Vec<SdfInstance>,
    pub fill: Vec<FillVertex>,
    pub tex_verts: Vec<TexVertex>,
    /// Masked-composite quads (positions only matter).
    pub composite: Vec<TexVertex>,
    pub cmds: Vec<Cmd>,
    /// Shapes the renderer can't draw (kept for diagnostics).
    pub skipped: usize,
    clip_stack: Vec<Clip>,
    /// Content bounds of each open layer.
    bounds_stack: Vec<[f32; 4]>,
}

impl FrameBuilder {
    pub fn new() -> FrameBuilder {
        FrameBuilder::default()
    }

    pub fn reset(&mut self) {
        self.sdf.clear();
        self.fill.clear();
        self.tex_verts.clear();
        self.composite.clear();
        self.cmds.clear();
        self.skipped = 0;
        self.clip_stack.clear();
        self.bounds_stack.clear();
    }

    /// Note drawn geometry (world AABB) in the open layer's bounds.
    fn grow(&mut self, b: [f32; 4]) {
        let b = match self.cur_clip() {
            Some(c) => intersect(b, c),
            None => b,
        };
        if let Some(top) = self.bounds_stack.last_mut() {
            *top = union(*top, b);
        }
    }

    /// Start drawing into a fresh transparent layer.
    pub fn push_layer(&mut self) {
        self.cmds.push(Cmd::PushLayer);
        self.bounds_stack.push(EMPTY);
    }

    /// Composite the open layer onto its parent (see [`Cmd::PopLayer`]).
    pub fn pop_layer(&mut self, blend: Blend, mask: Option<[[f64; 2]; 4]>) {
        let mut bounds = self.bounds_stack.pop().unwrap_or(EMPTY);
        let mask = mask.map(|c| {
            bounds = intersect(bounds, aabb(c));
            self.push_mask_quad(c)
        });
        self.cmds.push(Cmd::PopLayer {
            blend,
            mask,
            bounds,
        });
        self.grow(bounds);
    }

    /// Blur the open layer and composite it (see [`Cmd::PopLayerBlur`]).
    pub fn pop_layer_blur(&mut self, sigma: f32) {
        let b = self.bounds_stack.pop().unwrap_or(EMPTY);
        let r = blur_radius(sigma) + 1.0;
        let bounds = [b[0] - r, b[1] - r, b[2] + r, b[3] + r];
        self.cmds.push(Cmd::PopLayerBlur { sigma, bounds });
        self.grow(bounds);
    }

    pub fn cur_clip(&self) -> Clip {
        self.clip_stack.last().copied().flatten()
    }

    fn push_clip(&mut self, c: Clip) {
        self.clip_stack.push(c);
    }

    fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    /// Number of layers the command list needs at its deepest point
    /// (blur needs one extra scratch target, accounted for by the renderer).
    pub fn max_layer_depth(&self) -> usize {
        let (mut d, mut max) = (0usize, 0usize);
        for c in &self.cmds {
            match c {
                Cmd::PushLayer => {
                    d += 1;
                    max = max.max(d);
                }
                Cmd::PopLayer { .. } | Cmd::PopLayerBlur { .. } => d = d.saturating_sub(1),
                _ => {}
            }
        }
        max
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_sdf(
        &mut self,
        ty: SdfType,
        cx: f64,
        cy: f64,
        m: [f64; 4],
        hx: f64,
        hy: f64,
        sw: f64,
        fill: Rgba,
        stroke: Rgba,
        align: SdfAlign,
    ) {
        // The instance quad: local ±(half extent + pad), through `m`.
        let (ex, ey) = (hx + sw * 0.5 + 2.0, hy + sw * 0.5 + 2.0);
        let (rx, ry) = (
            m[0].abs() * ex + m[1].abs() * ey,
            m[2].abs() * ex + m[3].abs() * ey,
        );
        self.grow(aabb([[cx - rx, cy - ry], [cx + rx, cy + ry]]));
        self.sdf.push(SdfInstance {
            tcr: [ty as u32 as f32, cx as f32, cy as f32, m[0] as f32],
            hcs: [hx as f32, hy as f32, 0.0, sw as f32],
            fill,
            stroke,
            misc: [align as u32 as f32, m[1] as f32, m[2] as f32, m[3] as f32],
        });
        let clip = self.cur_clip();
        if let Some(Cmd::Sdf { count, clip: c, .. }) = self.cmds.last_mut() {
            if *c == clip {
                *count += 1;
                return;
            }
        }
        self.cmds.push(Cmd::Sdf {
            start: self.sdf.len() as u32 - 1,
            count: 1,
            clip,
        });
    }

    /// A polygon fill: a fan (as a triangle list) for the stencil winding
    /// pass + an AABB cover quad. Nonzero winding makes self-intersecting,
    /// concave and holed outlines all correct.
    pub fn push_poly_fill(&mut self, pts: &[[f64; 2]], c: Rgba) {
        let n = pts.len();
        if n < 3 {
            return;
        }
        let col = premul(c);
        let v = |p: [f64; 2]| FillVertex {
            pos: [p[0] as f32, p[1] as f32],
            col,
        };
        let fan_start = self.fill.len() as u32;
        let (mut minx, mut miny, mut maxx, mut maxy) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for p in pts {
            minx = minx.min(p[0]);
            miny = miny.min(p[1]);
            maxx = maxx.max(p[0]);
            maxy = maxy.max(p[1]);
        }
        if !(minx.is_finite() && miny.is_finite() && maxx.is_finite() && maxy.is_finite()) {
            self.skipped += 1;
            return;
        }
        for i in 1..n - 1 {
            self.fill.push(v(pts[0]));
            self.fill.push(v(pts[i]));
            self.fill.push(v(pts[i + 1]));
        }
        self.grow(aabb([[minx, miny], [maxx, maxy]]));
        let fan_count = self.fill.len() as u32 - fan_start;
        let cover_start = self.fill.len() as u32;
        for p in quad_tris([[minx, miny], [maxx, miny], [minx, maxy], [maxx, maxy]]) {
            self.fill.push(v(p));
        }
        self.cmds.push(Cmd::PolyFill {
            fan_start,
            fan_count,
            cover_start,
            clip: self.cur_clip(),
        });
    }

    /// A textured quad from its four world corners (TL, TR, BL, BR).
    pub fn push_tex_quad(&mut self, c: [[f64; 2]; 4], uv: [f32; 4], tint: Rgba, texture: TextKey) {
        self.grow(aabb(c));
        let quad = (self.tex_verts.len() / 6) as u32;
        let [u0, v0, u1, v1] = uv;
        let uvs = [[u0, v0], [u1, v0], [u0, v1], [u1, v1]];
        for i in QUAD_TRI_ORDER {
            self.tex_verts.push(TexVertex {
                pos: [c[i][0] as f32, c[i][1] as f32],
                uv: uvs[i],
                tint,
            });
        }
        self.cmds.push(Cmd::Tex {
            quad,
            texture,
            clip: self.cur_clip(),
        });
    }

    fn push_mask_quad(&mut self, c: [[f64; 2]; 4]) -> u32 {
        let idx = (self.composite.len() / 6) as u32;
        for p in quad_tris(c) {
            self.composite.push(TexVertex {
                pos: [p[0] as f32, p[1] as f32],
                uv: [0.0; 2],
                tint: [1.0; 4],
            });
        }
        idx
    }
}

/// TL, TR, BL, BR → two triangles.
const QUAD_TRI_ORDER: [usize; 6] = [0, 1, 2, 2, 1, 3];

fn quad_tris(c: [[f64; 2]; 4]) -> [[f64; 2]; 6] {
    QUAD_TRI_ORDER.map(|i| c[i])
}

fn premul(c: Rgba) -> Rgba {
    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
}

/// A paint's RGBA, or transparent for "none". Non-finite channels become 0.
pub fn paint_rgba(p: Option<&Paint>) -> Rgba {
    match p {
        None => TRANSPARENT,
        Some(p) => p.rgba.map(|v| if v.is_finite() { v as f32 } else { 0.0 }),
    }
}

fn align_code(a: &str) -> SdfAlign {
    match a {
        "inside" => SdfAlign::Inside,
        "outside" => SdfAlign::Outside,
        _ => SdfAlign::Center,
    }
}

pub fn apply(m: &Affine, x: f64, y: f64) -> [f64; 2] {
    [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]
}

/// Mean axis scale of an affine (TS `scaleOf`).
pub fn scale_of(m: &Affine) -> f64 {
    (m[0].hypot(m[1]) + m[2].hypot(m[3])) / 2.0
}

fn emit_capsule(fb: &mut FrameBuilder, a: [f64; 2], b: [f64; 2], radius: f64, col: Rgba) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    if len < 1e-4 && radius < 1e-3 {
        return;
    }
    if !len.is_finite() || !radius.is_finite() {
        fb.skipped += 1;
        return;
    }
    let ang = dy.atan2(dx);
    let (sn, cs) = ang.sin_cos();
    fb.push_sdf(
        SdfType::Capsule,
        (a[0] + b[0]) / 2.0,
        (a[1] + b[1]) / 2.0,
        [cs, -sn, sn, cs],
        len / 2.0,
        radius.max(0.4),
        0.0,
        col,
        TRANSPARENT,
        SdfAlign::Center,
    );
}

/// Polygon points in world space; Catmull–Rom splines (tension > 0, ≥ 3
/// points) are flattened at [`SPLINE_STEPS`] per span.
fn poly_world(points: &[Vec2], closed: bool, tension: f64, m: &Affine) -> Vec<[f64; 2]> {
    let n = points.len();
    if tension <= 0.0 || n < 3 || tension.is_nan() {
        return points.iter().map(|p| apply(m, p.x, p.y)).collect();
    }
    let spans = catmull_rom_beziers(points, closed, tension);
    let mut out = Vec::with_capacity(1 + spans.len() * SPLINE_STEPS);
    let mut prev = points[0];
    out.push(apply(m, prev.x, prev.y));
    for s in spans {
        let p0 = prev;
        for i in 1..=SPLINE_STEPS {
            let t = i as f64 / SPLINE_STEPS as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            let x = a * p0.x + b * s.c1.x + c * s.c2.x + d * s.end.x;
            let y = a * p0.y + b * s.c1.y + c * s.c2.y + d * s.end.y;
            out.push(apply(m, x, y));
        }
        prev = s.end;
    }
    out
}

fn fill_polygon(fb: &mut FrameBuilder, pts: &[[f64; 2]], col: Rgba) {
    if col[3] <= 0.0 || pts.len() < 3 {
        return;
    }
    fb.push_poly_fill(pts, col);
}

fn stroke_polygon(fb: &mut FrameBuilder, pts: &[[f64; 2]], closed: bool, radius: f64, col: Rgba) {
    if col[3] <= 0.0 {
        return;
    }
    let n = pts.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    for i in 0..segs {
        emit_capsule(fb, pts[i], pts[(i + 1) % n], radius, col);
    }
}

/// Is `m` axis-aligned (maps axis-aligned rects to axis-aligned rects)?
fn axis_aligned(m: &Affine) -> bool {
    const E: f64 = 1e-9;
    (m[1].abs() < E && m[2].abs() < E) || (m[0].abs() < E && m[3].abs() < E)
}

/// Batching context: the frame builder plus the text rasteriser (text runs
/// become cached textures).
pub struct Emitter<'a> {
    pub fb: &'a mut FrameBuilder,
    pub text: &'a mut TextRasterizer,
}

impl Emitter<'_> {
    /// Emit one scene node under the world transform `m`. A shape with
    /// `style.blur > 0` renders into its own layer, gets a separable
    /// Gaussian, then composites — a CSS-style per-shape `filter: blur()`.
    pub fn emit(&mut self, node: &SceneNode, m: &Affine) {
        let blur = match node {
            SceneNode::Group { .. } => 0.0,
            other => style_of(other).map_or(0.0, |s| s.blur),
        };
        if blur > 0.0 && blur.is_finite() {
            self.fb.push_layer();
            self.emit_inner(node, m);
            self.fb.pop_layer_blur((blur * scale_of(m)) as f32);
            return;
        }
        self.emit_inner(node, m);
    }

    fn emit_inner(&mut self, node: &SceneNode, m: &Affine) {
        let fb = &mut *self.fb;
        match node {
            SceneNode::Group {
                children,
                transform,
                blend_mode,
                clip,
            } => {
                let gm = match transform {
                    Some(t) => mul(m, t),
                    None => *m,
                };
                let mut pushed_clip = false;
                let mut mask = None;
                if let Some(cl) = clip {
                    let corners = [
                        apply(&gm, cl.x, cl.y),
                        apply(&gm, cl.x + cl.w, cl.y),
                        apply(&gm, cl.x, cl.y + cl.h),
                        apply(&gm, cl.x + cl.w, cl.y + cl.h),
                    ];
                    let (mut minx, mut miny, mut maxx, mut maxy) = (
                        f64::INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::NEG_INFINITY,
                    );
                    for [x, y] in corners {
                        minx = minx.min(x);
                        miny = miny.min(y);
                        maxx = maxx.max(x);
                        maxy = maxy.max(y);
                    }
                    let mut r = [minx as f32, miny as f32, maxx as f32, maxy as f32];
                    if let Some(p) = fb.cur_clip() {
                        r = [
                            r[0].max(p[0]),
                            r[1].max(p[1]),
                            r[2].min(p[2]),
                            r[3].min(p[3]),
                        ];
                    }
                    fb.push_clip(Some(r));
                    pushed_clip = true;
                    // TS scissors to the clip's AABB, which over-draws for a
                    // rotated / sheared clip. Do it exactly: draw into a layer
                    // and composite it through the clip quad.
                    if !axis_aligned(&gm) {
                        mask = Some(corners);
                    }
                }
                let layered = blend_mode.is_some() || mask.is_some();
                if layered {
                    fb.push_layer();
                }
                for c in children.iter() {
                    self.emit(c, &gm);
                }
                let fb = &mut *self.fb;
                if layered {
                    let blend = blend_mode
                        .as_deref()
                        .map_or(Blend::SourceOver, Blend::from_css);
                    fb.pop_layer(blend, mask);
                }
                if pushed_clip {
                    fb.pop_clip();
                }
            }
            SceneNode::Rect {
                x,
                y,
                w,
                h,
                mode,
                style,
            }
            | SceneNode::Ellipse {
                x,
                y,
                w,
                h,
                mode,
                style,
            } => {
                let [left, top, w, h] = resolve_box(*x, *y, *w, *h, mode);
                let [cx, cy] = apply(m, left + w / 2.0, top + h / 2.0);
                if !(cx.is_finite() && cy.is_finite() && w.is_finite() && h.is_finite()) {
                    fb.skipped += 1;
                    return;
                }
                let ty = if matches!(node, SceneNode::Rect { .. }) {
                    SdfType::Rect
                } else {
                    SdfType::Ellipse
                };
                // |w|, |h|: negative dims have a valid centre but need a
                // positive half-extent or the SDF is never inside.
                fb.push_sdf(
                    ty,
                    cx,
                    cy,
                    [m[0], m[2], m[1], m[3]],
                    w.abs() / 2.0,
                    h.abs() / 2.0,
                    finite_or_zero(style.stroke_weight),
                    paint_rgba(style.fill.as_ref()),
                    paint_rgba(style.stroke.as_ref()),
                    align_code(&style.stroke_align),
                );
            }
            SceneNode::Line {
                x1,
                y1,
                x2,
                y2,
                style,
            } => {
                let Some(stroke) = style.stroke.as_ref() else {
                    return;
                };
                let a = apply(m, *x1, *y1);
                let b = apply(m, *x2, *y2);
                emit_capsule(
                    fb,
                    a,
                    b,
                    style.stroke_weight * scale_of(m) / 2.0,
                    paint_rgba(Some(stroke)),
                );
            }
            SceneNode::Point { x, y, style } => {
                let col = paint_rgba(style.stroke.as_ref().or(style.fill.as_ref()));
                if col[3] <= 0.0 {
                    return;
                }
                let [cx, cy] = apply(m, *x, *y);
                let r = style.stroke_weight.max(1.0) * scale_of(m) / 2.0;
                if !(cx.is_finite() && cy.is_finite() && r.is_finite()) {
                    fb.skipped += 1;
                    return;
                }
                fb.push_sdf(
                    SdfType::Rect,
                    cx,
                    cy,
                    [1.0, 0.0, 0.0, 1.0],
                    r,
                    r,
                    0.0,
                    col,
                    TRANSPARENT,
                    SdfAlign::Center,
                );
            }
            SceneNode::Triangle { pts, style } => {
                let p: Vec<[f64; 2]> = pts.chunks(2).map(|c| apply(m, c[0], c[1])).collect();
                closed_shape(fb, &p, style, m);
            }
            SceneNode::Quad { pts, style } => {
                let p: Vec<[f64; 2]> = pts.chunks(2).map(|c| apply(m, c[0], c[1])).collect();
                closed_shape(fb, &p, style, m);
            }
            SceneNode::Polygon {
                points,
                closed,
                style,
                tension,
            } => {
                let pts = poly_world(points, *closed, tension.unwrap_or(0.0), m);
                if *closed {
                    fill_polygon(fb, &pts, paint_rgba(style.fill.as_ref()));
                }
                stroke_polygon(
                    fb,
                    &pts,
                    *closed,
                    style.stroke_weight * scale_of(m) / 2.0,
                    paint_rgba(style.stroke.as_ref()),
                );
            }
            SceneNode::Arc {
                x,
                y,
                w,
                h,
                start,
                stop,
                mode,
                style,
            } => {
                let [left, top, w, h] = resolve_box(*x, *y, *w, *h, mode);
                let (cx, cy, rx, ry) = (left + w / 2.0, top + h / 2.0, w / 2.0, h / 2.0);
                let pts: Vec<[f64; 2]> = (0..=ARC_STEPS)
                    .map(|i| {
                        let a = start + (stop - start) * (i as f64 / ARC_STEPS as f64);
                        apply(m, cx + a.cos() * rx, cy + a.sin() * ry)
                    })
                    .collect();
                fill_polygon(fb, &pts, paint_rgba(style.fill.as_ref()));
                stroke_polygon(
                    fb,
                    &pts,
                    false,
                    style.stroke_weight * scale_of(m) / 2.0,
                    paint_rgba(style.stroke.as_ref()),
                );
            }
            SceneNode::Text { text, x, y, style } => {
                if text.is_empty() {
                    return;
                }
                let Some(run) = self.text.rasterize(text, style) else {
                    self.fb.skipped += 1;
                    return;
                };
                // Neutral (white) glyphs carry colour + opacity as the quad
                // tint; baked (stroked) glyphs draw at full white tint.
                let tint = if run.neutral {
                    paint_rgba(style.fill.as_ref())
                } else {
                    [1.0; 4]
                };
                if tint[3] <= 0.0 {
                    return;
                }
                let (w, h) = (run.w, run.h);
                let ax = match &*style.text_align {
                    "center" => -w / 2.0,
                    "right" => -w,
                    _ => 0.0,
                };
                let ay = match &*style.text_baseline {
                    "top" => 0.0,
                    "middle" => -h / 2.0,
                    "bottom" => -h,
                    _ => -run.asc,
                };
                let (lx, ly) = (x + ax, y + ay);
                let c = [
                    apply(m, lx, ly),
                    apply(m, lx + w, ly),
                    apply(m, lx, ly + h),
                    apply(m, lx + w, ly + h),
                ];
                if c.iter().flatten().any(|v| !v.is_finite()) {
                    self.fb.skipped += 1;
                    return;
                }
                self.fb
                    .push_tex_quad(c, [0.0, 0.0, 1.0, 1.0], tint, run.clone());
            }
            #[allow(unreachable_patterns)]
            _ => fb.skipped += 1,
        }
    }
}

fn closed_shape(fb: &mut FrameBuilder, pts: &[[f64; 2]], style: &ResolvedStyle, m: &Affine) {
    fill_polygon(fb, pts, paint_rgba(style.fill.as_ref()));
    stroke_polygon(
        fb,
        pts,
        true,
        style.stroke_weight * scale_of(m) / 2.0,
        paint_rgba(style.stroke.as_ref()),
    );
}

fn finite_or_zero(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

fn style_of(n: &SceneNode) -> Option<&Arc<ResolvedStyle>> {
    match n {
        SceneNode::Rect { style, .. }
        | SceneNode::Ellipse { style, .. }
        | SceneNode::Line { style, .. }
        | SceneNode::Arc { style, .. }
        | SceneNode::Triangle { style, .. }
        | SceneNode::Quad { style, .. }
        | SceneNode::Point { style, .. }
        | SceneNode::Polygon { style, .. }
        | SceneNode::Text { style, .. } => Some(style),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floret_engine::scene::{default_style, line_default_style, RectF, IDENTITY};

    fn build(nodes: &[SceneNode]) -> FrameBuilder {
        let mut fb = FrameBuilder::new();
        let mut text = TextRasterizer::new();
        let mut e = Emitter {
            fb: &mut fb,
            text: &mut text,
        };
        for n in nodes {
            e.emit(n, &IDENTITY);
        }
        fb
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> SceneNode {
        SceneNode::Rect {
            x,
            y,
            w,
            h,
            mode: Arc::from("corner"),
            style: default_style().clone(),
        }
    }

    #[test]
    fn consecutive_sdf_shapes_share_one_draw() {
        let fb = build(&[rect(0.0, 0.0, 10.0, 10.0), rect(5.0, 5.0, 4.0, 4.0)]);
        assert_eq!(fb.sdf.len(), 2);
        assert_eq!(
            fb.cmds,
            vec![Cmd::Sdf {
                start: 0,
                count: 2,
                clip: None
            }]
        );
        // Centre + half extents.
        assert_eq!(fb.sdf[0].tcr[1..3], [5.0, 5.0]);
        assert_eq!(fb.sdf[0].hcs[0..2], [5.0, 5.0]);
    }

    #[test]
    fn polygon_fill_is_fan_plus_cover() {
        let style = Arc::new(ResolvedStyle {
            stroke: None,
            ..(**default_style()).clone()
        });
        let pts: Vec<Vec2> = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]
            .iter()
            .map(|&(x, y)| Vec2 { x, y })
            .collect();
        let fb = build(&[SceneNode::Polygon {
            points: Arc::from(pts),
            closed: true,
            style,
            tension: None,
        }]);
        assert_eq!(
            fb.cmds,
            vec![Cmd::PolyFill {
                fan_start: 0,
                fan_count: 6,
                cover_start: 6,
                clip: None
            }]
        );
        assert_eq!(fb.fill.len(), 12);
    }

    #[test]
    fn open_polyline_strokes_with_capsules() {
        let pts: Vec<Vec2> = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]
            .iter()
            .map(|&(x, y)| Vec2 { x, y })
            .collect();
        let fb = build(&[SceneNode::Polygon {
            points: Arc::from(pts.clone()),
            closed: false,
            style: line_default_style().clone(),
            tension: None,
        }]);
        assert_eq!(fb.sdf.len(), 2);
        assert!(fb.fill.is_empty());
        // With tension the spline is flattened: 2 spans × 14 steps.
        let fb = build(&[SceneNode::Polygon {
            points: Arc::from(pts),
            closed: false,
            style: line_default_style().clone(),
            tension: Some(1.0),
        }]);
        assert_eq!(fb.sdf.len(), 2 * SPLINE_STEPS);
    }

    #[test]
    fn groups_bake_transforms_blend_and_clip() {
        let child = Arc::new(rect(0.0, 0.0, 10.0, 10.0));
        let g = SceneNode::Group {
            children: Arc::from(vec![child.clone()]),
            transform: Some([2.0, 0.0, 0.0, 2.0, 100.0, 0.0]),
            blend_mode: Some(Arc::from("multiply")),
            clip: Some(RectF {
                x: 0.0,
                y: 0.0,
                w: 5.0,
                h: 5.0,
            }),
        };
        let fb = build(&[g]);
        assert_eq!(fb.sdf[0].tcr[1..3], [110.0, 10.0]);
        assert_eq!(
            fb.cmds,
            vec![
                Cmd::PushLayer,
                Cmd::Sdf {
                    start: 0,
                    count: 1,
                    clip: Some([100.0, 0.0, 110.0, 10.0])
                },
                Cmd::PopLayer {
                    blend: Blend::Multiply,
                    mask: None,
                    bounds: [100.0, 0.0, 110.0, 10.0],
                },
            ]
        );
        // A rotated clip composites through its quad instead.
        let g = SceneNode::Group {
            children: Arc::from(vec![child]),
            transform: Some(floret_engine::scene::rotate_mat(0.5)),
            blend_mode: None,
            clip: Some(RectF {
                x: 0.0,
                y: 0.0,
                w: 5.0,
                h: 5.0,
            }),
        };
        let fb = build(&[g]);
        assert!(matches!(
            fb.cmds.last(),
            Some(Cmd::PopLayer {
                blend: Blend::SourceOver,
                mask: Some(0),
                ..
            })
        ));
        assert_eq!(fb.composite.len(), 6);
    }

    #[test]
    fn blurred_shapes_get_their_own_layer() {
        let style = Arc::new(ResolvedStyle {
            blur: 4.0,
            ..(**default_style()).clone()
        });
        let fb = build(&[SceneNode::Ellipse {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            mode: Arc::from("center"),
            style,
        }]);
        assert_eq!(fb.cmds.first(), Some(&Cmd::PushLayer));
        // Bounds: the ellipse quad (±5 + 2.5 pad) grown by the 12 px kernel + 1.
        assert_eq!(
            fb.cmds.last(),
            Some(&Cmd::PopLayerBlur {
                sigma: 4.0,
                bounds: [-20.5, -20.5, 20.5, 20.5]
            })
        );
        assert_eq!(fb.max_layer_depth(), 1);
    }

    #[test]
    fn unknown_blend_modes_fall_back_to_source_over() {
        assert_eq!(Blend::from_css("darken"), Blend::SourceOver);
        assert_eq!(Blend::from_css("lighter"), Blend::Add);
        assert_eq!(Blend::from_css("screen"), Blend::Screen);
    }
}
