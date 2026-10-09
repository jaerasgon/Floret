//! Scene → GPU batches. A port of `src/runtime/gl/scene-to-batches.ts` plus
//! the `FrameBuilder` half of `gl-vector-renderer.ts`.
//!
//! Walks the `SceneNode` tree and routes every shape to a pipeline:
//!
//! - **SDF instances**: rect / ellipse (fill + stroke with align), capsules
//!   for lines, polygon / arc / triangle / quad outlines, points, particle
//!   batches (one instance per live particle) and particle trails.
//! - **Stencil fill**: triangle / quad / polygon / arc fills — a fan into the
//!   stencil buffer (nonzero winding) then a cover quad.
//! - **Textured quads**: text runs (rasterised CPU-side, see `text.rs`) and
//!   images (`SceneNode::Image`, one texture per image generation id).
//!
//! Group transforms are baked into world coordinates (incl. shear), blend
//! groups and blurred shapes become layer push/pop commands, clips become
//! scissor rects (axis-aligned) or a masked layer composite (rotated).
//! Gradient paints become references into a per-frame gradient table
//! ([`FrameBuilder::paint`]); a mask is a layer composited into the layer
//! below it (destination-in), and a shader's map layer pops into its
//! channel before the input is drawn.
//!
//! World coordinates are centre-origin, y down — what the web app's canvas
//! driver feeds its renderer (`centreOrigin: true`).
//!
//! This module is pure CPU code: no wgpu types, so it can be unit-tested
//! without a GPU.

use crate::text::{TextKey, TextRasterizer};
use floret_engine::image::Image;
use floret_engine::particles::{ParticleBuffer, ParticleHandle, ParticleTemplate};
use floret_engine::scene::{
    catmull_rom_beziers, mul, resolve_box, rotate_mat, translate_mat, Affine, GlyphRun, Gradient,
    GradientExtend, GradientShape, GradientUnits, Paint, ResolvedStyle, SceneNode,
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
///
/// The `Mask*` modes composite a mask layer INTO the layer below it
/// (destination-in): that layer keeps its pixels × the mask's coverage —
/// alpha, or luminance × alpha — or × (1 − coverage) when inverted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Blend {
    SourceOver = 0,
    Add = 1,
    Multiply = 2,
    Screen = 3,
    MaskAlpha = 4,
    MaskLuminance = 5,
    MaskAlphaInverted = 6,
    MaskLuminanceInverted = 7,
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

    /// The mask composite for a [`SceneNode::Mask`].
    pub fn mask(luminance: bool, invert: bool) -> Blend {
        match (luminance, invert) {
            (false, false) => Blend::MaskAlpha,
            (true, false) => Blend::MaskLuminance,
            (false, true) => Blend::MaskAlphaInverted,
            (true, true) => Blend::MaskLuminanceInverted,
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
    /// Textured quad `tex_verts[quad * 6..quad * 6 + 6]` showing `image`.
    Image {
        quad: u32,
        image: Arc<Image>,
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
    /// Run `shaders[index]` and composite its Image pass over the whole
    /// view. With `input`, the top layer holds the rendered input subtree
    /// and is consumed (popped) as the `input` channel.
    Shader { index: u32, input: bool },
    /// The top layer holds `shaders[index]`'s map subtree: pop it into the
    /// draw's map channel (before its input is drawn).
    ShaderMap { index: u32 },
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
    /// `gpu/shader` draws, referenced by [`Cmd::Shader`].
    pub shaders: Vec<Arc<floret_engine::shader::ShaderShape>>,
    /// Gradient paints as RGBA32F texels (see [`FrameBuilder::paint`]).
    pub gradients: Vec<[f32; 4]>,
    /// Stop blocks already in `gradients`, by gradient (pointer).
    gradient_stops: Vec<(usize, u32)>,
    clip_stack: Vec<Clip>,
    /// Content bounds of each open layer.
    bounds_stack: Vec<[f32; 4]>,
}

/// A paint's alpha at or above this marks a gradient: `r` is the texel
/// index of its header in [`FrameBuilder::gradients`] and `a − GRADIENT`
/// its opacity (see `shaders.wgsl`).
pub const GRADIENT: f32 = 2.0;
/// Stops per gradient the shaders read (more are dropped).
pub const MAX_STOPS: usize = 64;

/// Is this paint colour a gradient reference?
pub fn is_gradient(c: Rgba) -> bool {
    c[3] >= GRADIENT - 0.5
}

/// Scale a paint's opacity (a gradient reference keeps its tag).
pub fn fade(mut c: Rgba, k: f32) -> Rgba {
    if is_gradient(c) {
        c[3] = GRADIENT + (c[3] - GRADIENT) * k;
    } else {
        c[3] *= k;
    }
    c
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
        self.shaders.clear();
        self.gradients.clear();
        self.gradient_stops.clear();
        self.clip_stack.clear();
        self.bounds_stack.clear();
    }

    /// A shape's paint as an instance / vertex colour. Flat colours are
    /// their RGBA; a gradient becomes a reference to texels in
    /// [`FrameBuilder::gradients`]: a 3-texel header (kind, extend, stop
    /// offset, stop count; then the world → gradient-space affine as rows
    /// `[a, c, e]`, `[b, d, f]` in texels 1–2) and a stop block shared by
    /// every use of the same gradient (2 texels a stop: position, then
    /// premultiplied colour). `m` is the shape's world transform and `bx`
    /// its local bounds `[x, y, w, h]` (for shape units).
    pub fn paint(&mut self, p: Option<&Paint>, m: &Affine, bx: [f64; 4]) -> Rgba {
        match p.and_then(|p| p.gradient.as_ref()) {
            Some(g) => self.gradient(g, m, bx),
            None => paint_rgba(p),
        }
    }

    fn gradient(&mut self, g: &Arc<Gradient>, m: &Affine, bx: [f64; 4]) -> Rgba {
        let key = Arc::as_ptr(g) as usize;
        let n = g.stops.len().min(MAX_STOPS);
        let stops = match self.gradient_stops.iter().find(|(k, _)| *k == key) {
            Some(&(_, off)) => off,
            None => {
                let off = self.gradients.len() as u32;
                for s in g.stops.iter().take(n) {
                    let c = s.rgba.map(|v| if v.is_finite() { v as f32 } else { 0.0 });
                    self.gradients.push([s.pos as f32, 0.0, 0.0, 0.0]);
                    self.gradients.push(premul(c));
                }
                self.gradient_stops.push((key, off));
                off
            }
        };
        // World → local → (shape box) → the canonical space of the kind:
        // linear t = x, radial t = |p|, conic t = angle(p) / 2π.
        let mut a = invert(m);
        if g.units == GradientUnits::Box {
            let hw = (bx[2].abs() / 2.0).max(1e-6);
            let hh = (bx[3].abs() / 2.0).max(1e-6);
            let (cx, cy) = (bx[0] + bx[2] / 2.0, bx[1] + bx[3] / 2.0);
            a = mul(&[1.0 / hw, 0.0, 0.0, 1.0 / hh, -cx / hw, -cy / hh], &a);
        }
        let (kind, k) = match g.shape {
            GradientShape::Linear { start, end } => {
                let (dx, dy) = (end.x - start.x, end.y - start.y);
                let l2 = dx * dx + dy * dy;
                let k = if l2 > 1e-12 && l2.is_finite() {
                    let c = -(start.x * dx + start.y * dy) / l2;
                    [dx / l2, 0.0, dy / l2, 0.0, c, 0.0]
                } else {
                    [0.0; 6]
                };
                (0.0, k)
            }
            GradientShape::Radial { center, radius } => {
                let r = radius.abs().max(1e-9);
                (
                    1.0,
                    [1.0 / r, 0.0, 0.0, 1.0 / r, -center.x / r, -center.y / r],
                )
            }
            GradientShape::Conic { center, angle } => (
                2.0,
                mul(&rotate_mat(-angle), &translate_mat(-center.x, -center.y)),
            ),
        };
        let a = mul(&k, &a).map(|v| if v.is_finite() { v as f32 } else { 0.0 });
        let extend = match g.extend {
            GradientExtend::Pad => 0.0,
            GradientExtend::Repeat => 1.0,
            GradientExtend::Mirror => 2.0,
        };
        let header = self.gradients.len() as u32;
        self.gradients.push([kind, extend, stops as f32, n as f32]);
        self.gradients.push([a[0], a[2], a[4], 0.0]);
        self.gradients.push([a[1], a[3], a[5], 0.0]);
        [header as f32, 0.0, 0.0, GRADIENT + 1.0]
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

    /// Composite the open (mask) layer into the layer below it with a
    /// `Blend::Mask*` mode. The mask covers the whole layer below (where
    /// the mask is empty the layer is cleared, or kept when inverted).
    pub fn pop_mask(&mut self, luminance: bool, invert: bool) {
        let mask = self.bounds_stack.pop().unwrap_or(EMPTY);
        let Some(content) = self.bounds_stack.last_mut() else {
            return;
        };
        self.cmds.push(Cmd::PopLayer {
            blend: Blend::mask(luminance, invert),
            mask: None,
            bounds: *content,
        });
        if !invert {
            *content = intersect(*content, mask);
        }
    }

    /// The map layer of a shader draw is open: pop it into the draw's map
    /// channel (see [`Cmd::ShaderMap`]). Returns the draw's index for
    /// [`FrameBuilder::push_shader`].
    pub fn push_shader_map(&mut self, shape: Arc<floret_engine::shader::ShaderShape>) -> u32 {
        self.bounds_stack.pop();
        self.shaders.push(shape);
        let index = self.shaders.len() as u32 - 1;
        self.cmds.push(Cmd::ShaderMap { index });
        index
    }

    /// A shader draw (see [`Cmd::Shader`]). It covers the whole view and
    /// ignores clips, like TS. `mapped`: the draw's index from
    /// [`FrameBuilder::push_shader_map`].
    pub fn push_shader(
        &mut self,
        shape: Arc<floret_engine::shader::ShaderShape>,
        input: bool,
        mapped: Option<u32>,
    ) {
        if input {
            self.bounds_stack.pop();
        }
        let index = mapped.unwrap_or_else(|| {
            self.shaders.push(shape);
            self.shaders.len() as u32 - 1
        });
        self.cmds.push(Cmd::Shader { index, input });
        if let Some(top) = self.bounds_stack.last_mut() {
            *top = [
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::INFINITY,
            ];
        }
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
                Cmd::PopLayer { .. }
                | Cmd::PopLayerBlur { .. }
                | Cmd::ShaderMap { .. }
                | Cmd::Shader { input: true, .. } => d = d.saturating_sub(1),
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
        // Gradient references go through as they are.
        let col = if is_gradient(c) { c } else { premul(c) };
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
        let quad = self.tex_quad_verts(c, uv, tint);
        self.cmds.push(Cmd::Tex {
            quad,
            texture,
            clip: self.cur_clip(),
        });
    }

    /// An image quad from its four world corners (TL, TR, BL, BR).
    pub fn push_image_quad(
        &mut self,
        c: [[f64; 2]; 4],
        uv: [f32; 4],
        tint: Rgba,
        image: Arc<Image>,
    ) {
        let quad = self.tex_quad_verts(c, uv, tint);
        self.cmds.push(Cmd::Image {
            quad,
            image,
            clip: self.cur_clip(),
        });
    }

    fn tex_quad_verts(&mut self, c: [[f64; 2]; 4], uv: [f32; 4], tint: Rgba) -> u32 {
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
        quad
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

/// The inverse affine; all zeros when `m` is singular.
pub fn invert(m: &Affine) -> Affine {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-12 || !det.is_finite() {
        return [0.0; 6];
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    [a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])]
}

/// Local bounds `[x, y, w, h]` of points.
fn bounds_of(pts: impl IntoIterator<Item = [f64; 2]>) -> [f64; 4] {
    let b = pts.into_iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |b, p| {
            [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ]
        },
    );
    if !(b[0] <= b[2] && b[1] <= b[3]) {
        return [0.0; 4];
    }
    [b[0], b[1], b[2] - b[0], b[3] - b[1]]
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
                let bx = bounds_of([[left, top], [left + w, top + h]]);
                let fill = fb.paint(style.fill.as_ref(), m, bx);
                let stroke = fb.paint(style.stroke.as_ref(), m, bx);
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
                    fill,
                    stroke,
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
                let col = fb.paint(Some(stroke), m, bounds_of([[*x1, *y1], [*x2, *y2]]));
                emit_capsule(fb, a, b, style.stroke_weight * scale_of(m) / 2.0, col);
            }
            SceneNode::Point { x, y, style } => {
                let lr = style.stroke_weight.max(1.0) / 2.0;
                let bx = [x - lr, y - lr, lr * 2.0, lr * 2.0];
                let col = fb.paint(style.stroke.as_ref().or(style.fill.as_ref()), m, bx);
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
            SceneNode::Triangle { pts, style } => closed_shape(fb, pts, style, m),
            SceneNode::Quad { pts, style } => closed_shape(fb, pts, style, m),
            SceneNode::Polygon {
                points,
                closed,
                style,
                tension,
            } => {
                let pts = poly_world(points, *closed, tension.unwrap_or(0.0), m);
                let bx = bounds_of(points.iter().map(|p| [p.x, p.y]));
                if *closed {
                    let fill = fb.paint(style.fill.as_ref(), m, bx);
                    fill_polygon(fb, &pts, fill);
                }
                let stroke = fb.paint(style.stroke.as_ref(), m, bx);
                stroke_polygon(
                    fb,
                    &pts,
                    *closed,
                    style.stroke_weight * scale_of(m) / 2.0,
                    stroke,
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
                let bx = bounds_of([[left, top], [left + w, top + h]]);
                let fill = fb.paint(style.fill.as_ref(), m, bx);
                fill_polygon(fb, &pts, fill);
                let stroke = fb.paint(style.stroke.as_ref(), m, bx);
                stroke_polygon(
                    fb,
                    &pts,
                    false,
                    style.stroke_weight * scale_of(m) / 2.0,
                    stroke,
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
                // Neutral (white) glyphs carry colour + opacity as the quad
                // tint; baked (stroked) glyphs draw at full white tint.
                let tint = if run.neutral {
                    self.fb.paint(style.fill.as_ref(), m, [lx, ly, w, h])
                } else {
                    [1.0; 4]
                };
                if tint[3] <= 0.0 {
                    return;
                }
                self.fb
                    .push_tex_quad(c, [0.0, 0.0, 1.0, 1.0], tint, run.clone());
            }
            SceneNode::ParticleBatch {
                buffer,
                template,
                style,
                use_buffer_color,
                ..
            } => self.particles(buffer, template, style, *use_buffer_color, m),
            SceneNode::ParticleTrailBatch {
                buffer,
                history,
                segments,
                head_idx,
                style,
                fade_out,
                use_buffer_color,
                tapered,
                tail_width,
            } => {
                let trail = Trail {
                    history: history.as_slice(),
                    segments: *segments,
                    head: *head_idx,
                    fade_out: *fade_out,
                    tapered: *tapered,
                    tail_width: *tail_width,
                };
                particle_trails(fb, &buffer.read(), &trail, style, *use_buffer_color, m);
            }
            SceneNode::Image {
                image,
                src,
                dst,
                tint,
            } => {
                // `scene-to-batches.ts`: the source rect becomes UVs (the
                // sampler clamps outside the image), the tint multiplies.
                let (iw, ih) = (f64::from(image.width()), f64::from(image.height()));
                let uv = match src {
                    Some(s) => [s.x / iw, s.y / ih, (s.x + s.w) / iw, (s.y + s.h) / ih],
                    None => [0.0, 0.0, 1.0, 1.0],
                };
                let c = [
                    apply(m, dst.x, dst.y),
                    apply(m, dst.x + dst.w, dst.y),
                    apply(m, dst.x, dst.y + dst.h),
                    apply(m, dst.x + dst.w, dst.y + dst.h),
                ];
                if c.iter().flatten().chain(&uv).any(|v| !v.is_finite()) {
                    fb.skipped += 1;
                    return;
                }
                let tint = tint.as_ref().map_or([1.0; 4], |p| paint_rgba(Some(p)));
                fb.push_image_quad(c, uv.map(|v| v as f32), tint, image.clone());
            }
            SceneNode::Shader(sh) => {
                // A map layer (`gpu/effect`) goes to its channel first.
                let mapped = sh.map.as_ref().map(|map| {
                    self.fb.push_layer();
                    self.emit(map, m);
                    self.fb.push_shader_map(sh.clone())
                });
                if let Some(input) = &sh.input {
                    self.fb.push_layer();
                    self.emit(input, m);
                }
                self.fb.push_shader(sh.clone(), sh.input.is_some(), mapped);
            }
            SceneNode::Glyphs(run) => self.glyphs(run, m),
            SceneNode::Mask {
                content,
                mask,
                luminance,
                invert,
            } => {
                // The content in a layer, the mask composited into it
                // (destination-in), then the result onto the parent.
                fb.push_layer();
                self.emit(content, m);
                self.fb.push_layer();
                self.emit(mask, m);
                self.fb.pop_mask(*luminance, *invert);
                self.fb.pop_layer(Blend::SourceOver, None);
            }
            #[allow(unreachable_patterns)]
            _ => fb.skipped += 1,
        }
    }

    /// A `text/*` glyph run: each glyph's cached bitmap as a quad turned
    /// about the glyph's origin.
    fn glyphs(&mut self, run: &GlyphRun, m: &Affine) {
        // Each glyph's quad in run space; their bounds place a gradient.
        let mut quads = Vec::with_capacity(run.glyphs.len());
        for g in run.glyphs.iter() {
            let Some(bmp) = self.text.glyph(g.face, g.id, run) else {
                self.fb.skipped += 1;
                continue;
            };
            let (s, c) = g.angle.sin_cos();
            let at = |lx: f64, ly: f64| {
                let (dx, dy) = (lx - bmp.left, ly - bmp.asc);
                [g.x + dx * c - dy * s, g.y + dx * s + dy * c]
            };
            let q = [
                at(0.0, 0.0),
                at(bmp.w, 0.0),
                at(0.0, bmp.h),
                at(bmp.w, bmp.h),
            ];
            quads.push((bmp, q));
        }
        let bx = bounds_of(quads.iter().flat_map(|(_, q)| *q));
        let mut fill = None;
        for (bmp, q) in quads {
            let tint = if bmp.neutral {
                *fill.get_or_insert_with(|| self.fb.paint(run.style.fill.as_ref(), m, bx))
            } else {
                [1.0; 4]
            };
            if tint[3] <= 0.0 {
                continue;
            }
            let q = q.map(|p| apply(m, p[0], p[1]));
            if q.iter().flatten().any(|v| !v.is_finite()) {
                self.fb.skipped += 1;
                continue;
            }
            self.fb.push_tex_quad(q, [0.0, 0.0, 1.0, 1.0], tint, bmp);
        }
    }

    /// Every live particle of a buffer as one run of SDF instances (one
    /// instanced draw), like TS `scene-to-batches`. As there, `size[i]` is
    /// the half extent: a circle's radius, a square's half side, at least
    /// 0.4. A `Layer` template (drawn as a circle by TS's GL path) is the
    /// layer translated to each particle.
    fn particles(
        &mut self,
        buffer: &ParticleHandle,
        template: &ParticleTemplate,
        style: &ResolvedStyle,
        use_buffer_color: bool,
        m: &Affine,
    ) {
        let buf = buffer.read();
        let n = live_len(&buf);
        if let ParticleTemplate::Layer(node) = template {
            for i in (0..n).filter(|&i| buf.alive[i] != 0) {
                let (x, y) = (f64::from(buf.pos_x[i]), f64::from(buf.pos_y[i]));
                if !(x.is_finite() && y.is_finite()) {
                    self.fb.skipped += 1;
                    continue;
                }
                self.emit(node, &mul(m, &translate_mat(x, y)));
            }
            return;
        }
        let fb = &mut *self.fb;
        // A gradient in shape units is placed on every particle.
        let per_particle = !use_buffer_color && is_shape_gradient(style.fill.as_ref());
        let fill = if per_particle {
            TRANSPARENT
        } else {
            fb.paint(style.fill.as_ref(), m, [0.0; 4])
        };
        let sc = scale_of(m);
        // Circles stay round under any transform (TS: identity basis,
        // radius × mean scale); squares follow the full transform.
        let (ty, basis, unit) = match template {
            ParticleTemplate::Rect => (SdfType::Rect, [m[0], m[2], m[1], m[3]], 1.0),
            _ => (SdfType::Ellipse, [1.0, 0.0, 0.0, 1.0], sc),
        };
        fb.sdf.reserve(n);
        for i in (0..n).filter(|&i| buf.alive[i] != 0) {
            let col = if use_buffer_color {
                buffer_rgba(&buf, i)
            } else {
                fill
            };
            if col[3] <= 0.0 && !per_particle {
                continue;
            }
            let (x, y) = (f64::from(buf.pos_x[i]), f64::from(buf.pos_y[i]));
            let [cx, cy] = apply(m, x, y);
            let s = f64::from(buf.size[i]).max(0.4);
            let r = s * unit;
            if !(cx.is_finite() && cy.is_finite() && r.is_finite()) {
                fb.skipped += 1;
                continue;
            }
            let col = if per_particle {
                fb.paint(style.fill.as_ref(), m, [x - s, y - s, s * 2.0, s * 2.0])
            } else {
                col
            };
            fb.push_sdf(
                ty,
                cx,
                cy,
                basis,
                r,
                r,
                0.0,
                col,
                TRANSPARENT,
                SdfAlign::Center,
            );
        }
    }
}

/// The history half of a `ParticleTrailBatch`.
struct Trail<'a> {
    history: &'a [f32],
    segments: usize,
    head: usize,
    fade_out: bool,
    tapered: bool,
    tail_width: f64,
}

/// A capsule per trail segment of every live particle, newest → oldest,
/// stopping at the first missing (NaN) sample. Segment k (1-based) fades
/// to alpha × (1 − (k − ½)/segments) and tapers to width ×
/// (1 − (k − ½)/segments × (1 − tail_width)), as the TS canvas / SVG
/// renderers did (TS's GL path draws trails flat).
fn particle_trails(
    fb: &mut FrameBuilder,
    buf: &ParticleBuffer,
    trail: &Trail<'_>,
    style: &ResolvedStyle,
    use_buffer_color: bool,
    m: &Affine,
) {
    let segs = trail.segments;
    if segs < 2 {
        return;
    }
    let n = live_len(buf).min(trail.history.len() / (segs * 2));
    // A gradient in shape units is placed on every trail.
    let per_trail = !use_buffer_color && is_shape_gradient(style.stroke.as_ref());
    let stroke = if per_trail {
        TRANSPARENT
    } else {
        fb.paint(style.stroke.as_ref(), m, [0.0; 4])
    };
    let sc = scale_of(m);
    let head = trail.head % segs;
    let mut pts: Vec<[f64; 2]> = Vec::with_capacity(segs);
    let mut local: Vec<[f64; 2]> = Vec::with_capacity(segs);
    for slot in (0..n).filter(|&i| buf.alive[i] != 0) {
        let col = if use_buffer_color {
            buffer_rgba(buf, slot)
        } else {
            stroke
        };
        if col[3] <= 0.0 && !per_trail {
            continue;
        }
        pts.clear();
        local.clear();
        for s in 0..segs {
            let base = (slot * segs + (head + segs - s) % segs) * 2;
            let (x, y) = (
                f64::from(trail.history[base]),
                f64::from(trail.history[base + 1]),
            );
            if !(x.is_finite() && y.is_finite()) {
                break;
            }
            pts.push(apply(m, x, y));
            local.push([x, y]);
        }
        let col = if per_trail && pts.len() > 1 {
            fb.paint(style.stroke.as_ref(), m, bounds_of(local.iter().copied()))
        } else {
            col
        };
        for k in 1..pts.len() {
            let age = (k as f64 - 0.5) / segs as f64;
            let mut w = style.stroke_weight;
            if trail.tapered {
                w *= 1.0 - age * (1.0 - trail.tail_width);
                if w <= 0.0 {
                    w = 0.001;
                }
            }
            let c = if trail.fade_out {
                fade(col, (1.0 - age) as f32)
            } else {
                col
            };
            emit_capsule(fb, pts[k - 1], pts[k], w * sc / 2.0, c);
        }
    }
}

/// Slots safe to index in every per-particle array.
fn live_len(b: &ParticleBuffer) -> usize {
    [
        b.alive.len(),
        b.pos_x.len(),
        b.pos_y.len(),
        b.size.len(),
        b.color_r.len(),
        b.color_g.len(),
        b.color_b.len(),
        b.color_a.len(),
    ]
    .into_iter()
    .fold(b.capacity, usize::min)
}

fn buffer_rgba(b: &ParticleBuffer, i: usize) -> Rgba {
    [b.color_r[i], b.color_g[i], b.color_b[i], b.color_a[i]].map(|v| {
        if v.is_finite() {
            v
        } else {
            0.0
        }
    })
}

/// A triangle / quad from its local corner coordinates (x, y pairs).
fn closed_shape(fb: &mut FrameBuilder, local: &[f64], style: &ResolvedStyle, m: &Affine) {
    let bx = bounds_of(local.chunks(2).map(|c| [c[0], c[1]]));
    let pts: Vec<[f64; 2]> = local.chunks(2).map(|c| apply(m, c[0], c[1])).collect();
    let fill = fb.paint(style.fill.as_ref(), m, bx);
    fill_polygon(fb, &pts, fill);
    let stroke = fb.paint(style.stroke.as_ref(), m, bx);
    stroke_polygon(
        fb,
        &pts,
        true,
        style.stroke_weight * scale_of(m) / 2.0,
        stroke,
    );
}

/// A gradient paint in shape units (placed per shape).
fn is_shape_gradient(p: Option<&Paint>) -> bool {
    p.and_then(|p| p.gradient.as_ref())
        .is_some_and(|g| g.units == GradientUnits::Box)
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
        | SceneNode::Text { style, .. }
        | SceneNode::ParticleBatch { style, .. }
        | SceneNode::ParticleTrailBatch { style, .. } => Some(style),
        SceneNode::Glyphs(run) => Some(&run.style),
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

    /// A buffer with particles at `pts` (size 3, colour = index / 10).
    fn particles(cap: usize, pts: &[(f32, f32)]) -> ParticleHandle {
        let mut b = ParticleBuffer::new(cap as f64);
        for (k, &(x, y)) in pts.iter().enumerate() {
            let i = b.spawn().unwrap();
            b.pos_x[i] = x;
            b.pos_y[i] = y;
            b.size[i] = 3.0;
            b.color_r[i] = k as f32 / 10.0;
            b.color_a[i] = 1.0;
        }
        ParticleHandle::new(b)
    }

    fn batch(buffer: &ParticleHandle, template: ParticleTemplate) -> SceneNode {
        SceneNode::ParticleBatch {
            buffer: buffer.clone(),
            count: 0.0,
            template,
            style: default_style().clone(),
            use_buffer_color: true,
        }
    }

    #[test]
    fn particle_batch_is_one_instanced_draw_of_live_slots() {
        let buf = particles(8, &[(1.0, 2.0), (3.0, 4.0), (5.0, 6.0)]);
        buf.write().kill(1);
        let g = SceneNode::Group {
            children: Arc::from(vec![Arc::new(batch(&buf, ParticleTemplate::Circle))]),
            transform: Some([2.0, 0.0, 0.0, 2.0, 10.0, 0.0]),
            blend_mode: None,
            clip: None,
        };
        let fb = build(&[g]);
        assert_eq!(
            fb.cmds,
            vec![Cmd::Sdf {
                start: 0,
                count: 2,
                clip: None
            }]
        );
        let s = fb.sdf[1];
        assert_eq!(s.tcr, [SdfType::Ellipse as u32 as f32, 20.0, 12.0, 1.0]);
        // size = radius, × the transform's scale.
        assert_eq!(s.hcs[0..2], [6.0, 6.0]);
        assert_eq!(s.fill, [0.2, 0.0, 0.0, 1.0]);
        // Squares follow the transform; a layer is translated per particle.
        let fb = build(&[batch(&buf, ParticleTemplate::Rect)]);
        assert_eq!(fb.sdf[0].tcr[0], SdfType::Rect as u32 as f32);
        let dot = Arc::new(rect(-1.0, -1.0, 2.0, 2.0));
        let fb = build(&[batch(&buf, ParticleTemplate::Layer(dot))]);
        assert_eq!(fb.sdf.len(), 2);
        assert_eq!(fb.sdf[1].tcr[1..3], [5.0, 6.0]);
    }

    #[test]
    fn diverged_particles_are_skipped() {
        let buf = particles(4, &[(f32::NAN, 0.0), (1.0, 1.0)]);
        let fb = build(&[batch(&buf, ParticleTemplate::Circle)]);
        assert_eq!((fb.sdf.len(), fb.skipped), (1, 1));
    }

    fn trail(buf: &ParticleHandle, history: Vec<f32>, segments: usize, head: usize) -> SceneNode {
        SceneNode::ParticleTrailBatch {
            buffer: buf.clone(),
            history: Arc::new(history),
            segments,
            head_idx: head,
            style: Arc::new(ResolvedStyle {
                stroke: Some(Paint::from_color(floret_engine::Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                })),
                stroke_weight: 4.0,
                ..(**default_style()).clone()
            }),
            fade_out: true,
            use_buffer_color: false,
            tapered: true,
            tail_width: 0.5,
        }
    }

    #[test]
    fn trails_walk_newest_to_oldest_with_fade_and_taper() {
        let buf = particles(2, &[(0.0, 0.0)]);
        // Slot 0, 4 segments, head = 1: samples at x = 10 (newest), 0,
        // 30, 20 (oldest). Slot 1 is dead.
        let nan = f32::NAN;
        let hist = vec![
            0.0, 0.0, 10.0, 0.0, 20.0, 0.0, 30.0, 0.0, nan, nan, nan, nan, nan, nan, nan, nan,
        ];
        let fb = build(&[trail(&buf, hist.clone(), 4, 1)]);
        assert_eq!(fb.sdf.len(), 3);
        let centres: Vec<f32> = fb.sdf.iter().map(|s| s.tcr[1]).collect();
        assert_eq!(centres, [5.0, 15.0, 25.0]);
        // k = 1: age 1/8 → alpha 7/8, width 4 × (1 − 1/16) → radius 1.875.
        assert_eq!(fb.sdf[0].fill[3], 0.875);
        assert_eq!(fb.sdf[0].hcs[1], 1.875);
        // k = 3: age 5/8.
        assert_eq!(fb.sdf[2].fill[3], 0.375);
        // A missing sample ends the line.
        let mut gap = hist;
        gap[6] = nan;
        let fb = build(&[trail(&buf, gap, 4, 1)]);
        assert_eq!(fb.sdf.len(), 1);
    }

    #[test]
    fn images_become_textured_quads() {
        let image = Arc::new(Image::transparent(10, 20));
        let node = SceneNode::Image {
            image: image.clone(),
            src: Some(RectF {
                x: 5.0,
                y: 0.0,
                w: 5.0,
                h: 10.0,
            }),
            dst: RectF {
                x: 1.0,
                y: 2.0,
                w: 30.0,
                h: 40.0,
            },
            tint: Some(Paint::css("rgba(255,0,0,0.5)")),
        };
        let g = SceneNode::Group {
            children: Arc::from(vec![Arc::new(node)]),
            transform: Some([2.0, 0.0, 0.0, 2.0, 100.0, 0.0]),
            blend_mode: None,
            clip: None,
        };
        let fb = build(&[g]);
        assert_eq!(
            fb.cmds,
            vec![Cmd::Image {
                quad: 0,
                image,
                clip: None
            }]
        );
        // TL and BR corners, through the group transform.
        assert_eq!(fb.tex_verts[0].pos, [102.0, 4.0]);
        assert_eq!(fb.tex_verts[5].pos, [162.0, 84.0]);
        assert_eq!(fb.tex_verts[0].uv, [0.5, 0.0]);
        assert_eq!(fb.tex_verts[5].uv, [1.0, 0.5]);
        assert_eq!(fb.tex_verts[0].tint, [1.0, 0.0, 0.0, 0.5]);
        // Non-finite geometry is skipped.
        let bad = SceneNode::Image {
            image: Arc::new(Image::transparent(2, 2)),
            src: None,
            dst: RectF {
                x: f64::NAN,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            tint: None,
        };
        let fb = build(&[bad]);
        assert!(fb.cmds.is_empty());
        assert_eq!(fb.skipped, 1);
    }

    #[test]
    fn unknown_blend_modes_fall_back_to_source_over() {
        assert_eq!(Blend::from_css("darken"), Blend::SourceOver);
        assert_eq!(Blend::from_css("lighter"), Blend::Add);
        assert_eq!(Blend::from_css("screen"), Blend::Screen);
    }

    fn gradient_style(units: GradientUnits) -> Arc<ResolvedStyle> {
        let stop = |pos, rgba| floret_engine::scene::GradientStop { pos, rgba };
        let g = Gradient {
            shape: GradientShape::Linear {
                start: Vec2 { x: -1.0, y: 0.0 },
                end: Vec2 { x: 1.0, y: 0.0 },
            },
            stops: Arc::from([
                stop(0.0, [1.0, 0.0, 0.0, 1.0]),
                stop(1.0, [0.0, 0.0, 1.0, 0.5]),
            ]),
            units,
            extend: GradientExtend::Mirror,
        };
        Arc::new(ResolvedStyle {
            fill: Some(Paint::gradient(g)),
            stroke: None,
            ..(**default_style()).clone()
        })
    }

    /// Evaluate a gradient header's affine at a world point.
    fn gradient_space(fb: &FrameBuilder, header: usize, x: f32, y: f32) -> [f32; 2] {
        let (r0, r1) = (fb.gradients[header + 1], fb.gradients[header + 2]);
        [r0[0] * x + r0[1] * y + r0[2], r1[0] * x + r1[1] * y + r1[2]]
    }

    #[test]
    fn gradients_become_table_references() {
        let style = gradient_style(GradientUnits::Box);
        let shape = |x| SceneNode::Rect {
            x,
            y: 10.0,
            w: 40.0,
            h: 20.0,
            mode: Arc::from("center"),
            style: style.clone(),
        };
        let fb = build(&[shape(0.0), shape(100.0)]);
        // The stop block once (2 stops × 2 texels), a header per shape.
        assert_eq!(fb.gradients.len(), 4 + 3 + 3);
        assert_eq!(fb.gradients[0], [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(fb.gradients[3], [0.0, 0.0, 0.5, 0.5]);
        let (a, b) = (fb.sdf[0].fill, fb.sdf[1].fill);
        assert!(is_gradient(a) && is_gradient(b));
        assert_eq!((a[0], b[0]), (4.0, 7.0));
        // Header: linear, mirror, stops at 0, 2 of them.
        assert_eq!(fb.gradients[4], [0.0, 2.0, 0.0, 2.0]);
        // Shape units: t runs 0 → 1 across each rect's own box.
        assert_eq!(gradient_space(&fb, 4, -20.0, 10.0)[0], 0.0);
        assert_eq!(gradient_space(&fb, 4, 20.0, 0.0)[0], 1.0);
        assert_eq!(gradient_space(&fb, 7, 80.0, 0.0)[0], 0.0);
        assert_eq!(gradient_space(&fb, 7, 100.0, 0.0)[0], 0.5);
        // Canvas units ignore the box; transforms still apply.
        let style = gradient_style(GradientUnits::Scene);
        let g = SceneNode::Group {
            children: Arc::from(vec![Arc::new(SceneNode::Rect {
                x: 0.0,
                y: 0.0,
                w: 40.0,
                h: 20.0,
                mode: Arc::from("center"),
                style,
            })]),
            transform: Some([2.0, 0.0, 0.0, 2.0, 10.0, 0.0]),
            blend_mode: None,
            clip: None,
        };
        let fb = build(&[g]);
        assert_eq!(gradient_space(&fb, 4, 10.0, 0.0)[0], 0.5);
        assert_eq!(gradient_space(&fb, 4, 12.0, 0.0)[0], 1.0);
        // A faded gradient keeps its tag.
        let f = fade(a, 0.5);
        assert!(is_gradient(f) && f[3] == GRADIENT + 0.5);
        assert_eq!(fade([1.0, 1.0, 1.0, 0.8], 0.5)[3], 0.4);
    }

    #[test]
    fn masks_composite_into_their_content_layer() {
        let content = Arc::new(rect(0.0, 0.0, 10.0, 10.0));
        let m = Arc::new(rect(5.0, 5.0, 10.0, 10.0));
        let fb = build(&[SceneNode::Mask {
            content,
            mask: m,
            luminance: true,
            invert: false,
        }]);
        let kinds: Vec<_> = fb
            .cmds
            .iter()
            .map(|c| match c {
                Cmd::PushLayer => "push",
                Cmd::PopLayer {
                    blend: Blend::MaskLuminance,
                    ..
                } => "mask",
                Cmd::PopLayer {
                    blend: Blend::SourceOver,
                    ..
                } => "pop",
                Cmd::Sdf { .. } => "sdf",
                _ => "?",
            })
            .collect();
        assert_eq!(kinds, ["push", "sdf", "push", "sdf", "mask", "pop"]);
        assert_eq!(fb.max_layer_depth(), 2);
        // The mask covers the whole content; the result only their overlap.
        let bounds: Vec<_> = fb
            .cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::PopLayer { bounds, .. } => Some(*bounds),
                _ => None,
            })
            .collect();
        assert_eq!(bounds, [[-2.5, -2.5, 12.5, 12.5], [2.5, 2.5, 12.5, 12.5]]);
        assert_eq!(Blend::mask(false, true), Blend::MaskAlphaInverted);
    }

    #[test]
    fn shader_maps_draw_before_their_input() {
        use floret_engine::shader::ShaderShape;
        let dot = Arc::new(rect(0.0, 0.0, 2.0, 2.0));
        let sh = Arc::new(ShaderShape {
            node_id: Arc::from("fx"),
            passes: Arc::from([]),
            uniform_decls: Arc::from([]),
            uniforms: Arc::from([]),
            input: Some(dot.clone()),
            map: Some(dot),
            history: None,
            lut: None,
        });
        let fb = build(&[SceneNode::Shader(sh)]);
        assert_eq!(fb.shaders.len(), 1);
        assert!(matches!(
            fb.cmds[..],
            [
                Cmd::PushLayer,
                Cmd::Sdf { .. },
                Cmd::ShaderMap { index: 0 },
                Cmd::PushLayer,
                Cmd::Sdf { .. },
                Cmd::Shader {
                    index: 0,
                    input: true
                },
            ]
        ));
        assert_eq!(fb.max_layer_depth(), 1);
    }

    #[test]
    fn inverse_affines() {
        let m = [2.0, 1.0, -1.0, 3.0, 5.0, -7.0];
        let r = mul(&m, &invert(&m));
        for (got, want) in r.iter().zip(IDENTITY) {
            assert!((got - want).abs() < 1e-12);
        }
        assert_eq!(invert(&[0.0; 6]), [0.0; 6]);
    }
}
