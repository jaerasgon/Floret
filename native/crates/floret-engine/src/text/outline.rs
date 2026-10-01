//! Glyph outlines as flattened contours, and point sampling along /
//! inside them.

use super::layout::AxisSetting;
use super::library::FaceData;
use crate::value::Vec2;

/// A glyph's contours at `size`, flattened to polylines (closed, last
/// point not repeated), y down, origin on the baseline at the pen.
pub fn glyph_contours(
    face: &FaceData,
    id: u16,
    size: f64,
    settings: &[AxisSetting],
) -> Vec<Vec<Vec2>> {
    use skrifa::instance::Size;
    use skrifa::outline::DrawSettings;
    use skrifa::MetadataProvider;
    let Some(font) = face.font() else {
        return Vec::new();
    };
    let Some(glyph) = font
        .outline_glyphs()
        .get(skrifa::GlyphId::new(u32::from(id)))
    else {
        return Vec::new();
    };
    let loc = font
        .axes()
        .location(settings.iter().map(|(t, v)| (skrifa::Tag::new(t), *v)));
    let mut pen = Flatten {
        // Curve flatness in scene units.
        tol: 0.15,
        contours: Vec::new(),
        cur: Vec::new(),
        last: Vec2::default(),
    };
    let ds = DrawSettings::unhinted(Size::new(size as f32), &loc);
    if glyph.draw(ds, &mut pen).is_err() {
        return Vec::new();
    }
    pen.finish();
    pen.contours
}

struct Flatten {
    tol: f64,
    contours: Vec<Vec<Vec2>>,
    cur: Vec<Vec2>,
    last: Vec2,
}

fn v(x: f32, y: f32) -> Vec2 {
    Vec2 {
        x: f64::from(x),
        y: -f64::from(y),
    }
}

impl Flatten {
    fn finish(&mut self) {
        let mut c = std::mem::take(&mut self.cur);
        if c.len() > 1 && dist(c[0], c[c.len() - 1]) < 1e-9 {
            c.pop();
        }
        if c.len() >= 2 {
            self.contours.push(c);
        }
    }

    fn push(&mut self, p: Vec2) {
        self.cur.push(p);
        self.last = p;
    }

    fn steps(&self, len: f64) -> usize {
        ((len / self.tol).sqrt().ceil() as usize).clamp(1, 64)
    }
}

impl skrifa::outline::OutlinePen for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        self.push(v(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push(v(x, y));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (p0, p1, p2) = (self.last, v(cx0, cy0), v(x, y));
        let n = self.steps(dist(p0, p1) + dist(p1, p2));
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            self.push(Vec2 {
                x: u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x,
                y: u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y,
            });
        }
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (p0, p1, p2, p3) = (self.last, v(cx0, cy0), v(cx1, cy1), v(x, y));
        let n = self.steps(dist(p0, p1) + dist(p1, p2) + dist(p2, p3));
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.push(Vec2 {
                x: a * p0.x + b * p1.x + c * p2.x + d * p3.x,
                y: a * p0.y + b * p1.y + c * p2.y + d * p3.y,
            });
        }
    }

    fn close(&mut self) {
        self.finish();
    }
}

fn dist(a: Vec2, b: Vec2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Points every `spacing` units along a closed contour, starting at its
/// first point.
pub fn sample_contour(c: &[Vec2], spacing: f64) -> Vec<Vec2> {
    let n = c.len();
    if n == 0 {
        return Vec::new();
    }
    let total: f64 = (0..n).map(|i| dist(c[i], c[(i + 1) % n])).sum();
    if !(spacing > 0.0 && total > 0.0) {
        return vec![c[0]];
    }
    // Even spacing that closes up: round the count, then stretch.
    let count = (total / spacing).round().max(1.0) as usize;
    let step = total / count as f64;
    let mut out = Vec::with_capacity(count);
    let (mut seg, mut seg_start) = (0usize, 0.0);
    for k in 0..count {
        let s = k as f64 * step;
        loop {
            let len = dist(c[seg], c[(seg + 1) % n]);
            if s <= seg_start + len || seg + 1 >= n {
                let t = if len > 0.0 {
                    ((s - seg_start) / len).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (a, b) = (c[seg], c[(seg + 1) % n]);
                out.push(Vec2 {
                    x: a.x + (b.x - a.x) * t,
                    y: a.y + (b.y - a.y) * t,
                });
                break;
            }
            seg_start += len;
            seg += 1;
        }
    }
    out
}

/// Non-zero winding number of `p` against every contour.
pub fn winding(contours: &[Vec<Vec2>], p: Vec2) -> i32 {
    let mut w = 0;
    for c in contours {
        let n = c.len();
        for i in 0..n {
            let (a, b) = (c[i], c[(i + 1) % n]);
            let cross = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
            if a.y <= p.y {
                if b.y > p.y && cross > 0.0 {
                    w += 1;
                }
            } else if b.y <= p.y && cross < 0.0 {
                w -= 1;
            }
        }
    }
    w
}

/// Grid points `spacing` apart that fall inside the filled contours.
pub fn fill_points(contours: &[Vec<Vec2>], spacing: f64) -> Vec<Vec2> {
    let mut out = Vec::new();
    let Some((min, max)) = bounds(contours) else {
        return out;
    };
    if spacing.is_nan()
        || spacing <= 0.0
        || (max.x - min.x) / spacing * (max.y - min.y) / spacing > 4e6
    {
        return out;
    }
    // Centre the grid on the bounds so thin strokes still get points.
    let ox = min.x + ((max.x - min.x) % spacing) / 2.0;
    let oy = min.y + ((max.y - min.y) % spacing) / 2.0;
    let mut y = oy;
    while y <= max.y {
        let mut x = ox;
        while x <= max.x {
            let p = Vec2 { x, y };
            if winding(contours, p) != 0 {
                out.push(p);
            }
            x += spacing;
        }
        y += spacing;
    }
    out
}

/// Every contour of a glyph run, in scene coordinates (each glyph turned
/// about its origin).
pub fn run_contours(run: &crate::scene::GlyphRun) -> Vec<Vec<Vec2>> {
    let font = super::FontSpec::from_style(&run.style);
    let mut out = Vec::new();
    for g in run.glyphs.iter() {
        let Some(face) = super::face(g.face) else {
            continue;
        };
        let settings = super::face_settings(&face, &font);
        let (s, c) = g.angle.sin_cos();
        for contour in glyph_contours(&face, g.id, run.size, &settings) {
            out.push(
                contour
                    .into_iter()
                    .map(|p| Vec2 {
                        x: g.x + p.x * c - p.y * s,
                        y: g.y + p.x * s + p.y * c,
                    })
                    .collect(),
            );
        }
    }
    out
}

/// About `count` points spread evenly along all contours (each gets its
/// share by length).
pub fn sample_contours(contours: &[Vec<Vec2>], count: usize) -> Vec<Vec2> {
    let len = |c: &Vec<Vec2>| {
        (0..c.len())
            .map(|i| dist(c[i], c[(i + 1) % c.len()]))
            .sum::<f64>()
    };
    let total: f64 = contours.iter().map(len).sum();
    if total.is_nan() || total <= 0.0 || count == 0 {
        return Vec::new();
    }
    let spacing = total / count as f64;
    contours
        .iter()
        .flat_map(|c| sample_contour(c, spacing))
        .collect()
}

pub fn bounds(contours: &[Vec<Vec2>]) -> Option<(Vec2, Vec2)> {
    let mut it = contours.iter().flatten();
    let first = *it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| {
        (
            Vec2 {
                x: lo.x.min(p.x),
                y: lo.y.min(p.y),
            },
            Vec2 {
                x: hi.x.max(p.x),
                y: hi.y.max(p.y),
            },
        )
    }))
}
