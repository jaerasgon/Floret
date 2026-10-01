//! Text runs → bitmaps, like TS `rasterizeText` (glyph canvas → texture).
//!
//! Each distinct (text, font, line height[, colours]) is shaped with
//! cosmic-text, rasterised with swash at 2× into one premultiplied RGBA
//! bitmap, and cached. The GPU side uploads each bitmap once and draws it as
//! a textured quad through the full affine, so rotated / scaled text works.
//!
//! Fill-only text is rasterised in neutral white and keyed without colour:
//! its colour and opacity are applied as the quad tint, so animating opacity
//! or many same-letter cells reuse one bitmap. Stroked text bakes both
//! colours (fill then stroke on top), like TS.
//!
//! Metrics follow the TS glyph canvas: the bitmap is `ceil(maxLineWidth) + 6`
//! by `ceil(lines × fontSize × lineHeight) + 6` units, glyphs start 1 unit in,
//! and the alphabetic baseline sits `0.8 × fontSize` below each line's top.
//! `letterSpacing` is ignored (TS's GL path ignores it too).
//!
//! Fonts come from the engine's library (`floret_engine::text`): the
//! cosmic-text font system is built from a snapshot of its database and
//! rebuilt when a font file is loaded. Runs with variable-font axes, and
//! the `text/*` nodes' glyph runs, are laid out by the engine (cosmic-text
//! handles no axis but `wght`) and only rasterised here, at the engine's
//! axis coordinates, so drawn glyphs match the nodes' outputs.

use cosmic_text::{
    Attrs, Buffer, CacheKeyFlags, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight,
};
use floret_engine::scene::{compose_font_css, font_variations_css, GlyphRun, ResolvedStyle};
use floret_engine::text::{self as typo, FaceData, FaceId};
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::Arc;

/// Rasterisation scale (TS draws its glyph canvas at 2×).
const SCALE: f64 = 2.0;
/// Largest bitmap edge in pixels; bigger runs are rasterised at a lower scale.
const MAX_PX: f64 = 8192.0;
/// Bounded cache: ever-changing content (a frame counter) mints a new key
/// every frame; evict oldest past the cap (TS `GLYPH_CACHE_CAP`).
const CACHE_CAP: usize = 1024;

/// One rasterised text run.
pub struct TextRun {
    /// Unique per run; the GPU side keys its textures by it.
    pub id: u64,
    /// Quad size in scene units.
    pub w: f64,
    pub h: f64,
    /// Baseline offset from the quad's top (alphabetic anchor).
    pub asc: f64,
    /// White glyphs tinted at draw time (no stroke).
    pub neutral: bool,
    /// For a single glyph: the glyph origin's offset from the quad's left
    /// edge (its top is `asc` above the origin). 0 for runs.
    pub left: f64,
    pub px_w: u32,
    pub px_h: u32,
    /// Premultiplied RGBA8, `px_w × px_h`.
    pub rgba: Vec<u8>,
}

impl fmt::Debug for TextRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextRun")
            .field("id", &self.id)
            .field("w", &self.w)
            .field("h", &self.h)
            .field("px", &(self.px_w, self.px_h))
            .finish()
    }
}

/// A shared handle to a cached run; compares by id.
#[derive(Clone, Debug)]
pub struct TextKey(pub Arc<TextRun>);

impl PartialEq for TextKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}

impl std::ops::Deref for TextKey {
    type Target = TextRun;
    fn deref(&self) -> &TextRun {
        &self.0
    }
}

pub struct TextRasterizer {
    fonts: Option<FontSystem>,
    swash: SwashCache,
    stroke_ctx: swash::scale::ScaleContext,
    cache: HashMap<String, TextKey>,
    order: VecDeque<String>,
    /// CSS `font-family` → resolved family.
    families: HashMap<String, String>,
    /// The engine font library generation `fonts` was built from.
    generation: u64,
    next_id: u64,
}

impl Default for TextRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRasterizer {
    pub fn new() -> TextRasterizer {
        TextRasterizer {
            fonts: None,
            swash: SwashCache::new(),
            stroke_ctx: swash::scale::ScaleContext::new(),
            cache: HashMap::new(),
            order: VecDeque::new(),
            families: HashMap::new(),
            generation: 0,
            next_id: 1,
        }
    }

    /// Rebuild the font system when the engine has loaded new fonts.
    fn sync_fonts(&mut self) {
        let g = typo::generation();
        if g != self.generation {
            self.generation = g;
            self.fonts = None;
            self.families.clear();
            self.cache.clear();
            self.order.clear();
        }
    }

    fn insert(&mut self, key: String, run: TextRun) -> TextKey {
        let run = TextKey(Arc::new(run));
        if self.cache.len() >= CACHE_CAP {
            if let Some(old) = self.order.pop_front() {
                self.cache.remove(&old);
            }
        }
        self.order.push_back(key.clone());
        self.cache.insert(key, run.clone());
        run
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Rasterise (or fetch) a run. `None` for nothing drawable (empty text,
    /// bad font size).
    pub fn rasterize(&mut self, text: &str, st: &ResolvedStyle) -> Option<TextKey> {
        self.sync_fonts();
        let neutral = st.stroke.is_none();
        let mut key = format!(
            "{text}|{}|{}|{}",
            compose_font_css(st),
            st.line_height,
            colour_key(st)
        );
        if !st.font_variations.is_empty() {
            key.push('|');
            key.push_str(&font_variations_css(&st.font_variations));
        }
        if let Some(hit) = self.cache.get(&key) {
            return Some(hit.clone());
        }
        let run = if st.font_variations.is_empty() {
            self.render(text, st, neutral)?
        } else {
            self.render_shaped(text, st, neutral)?
        };
        Some(self.insert(key, run))
    }

    /// One glyph of a `text/*` glyph run, as its own bitmap.
    pub fn glyph(&mut self, face: FaceId, id: u16, run: &GlyphRun) -> Option<TextKey> {
        self.sync_fonts();
        let st = &*run.style;
        let size = run.size;
        if !(size.is_finite() && size > 0.0) {
            return None;
        }
        let data = typo::face(face)?;
        let coords = typo::normalized_coords(
            &data,
            &typo::face_settings(&data, &typo::FontSpec::from_style(st)),
        );
        let key = format!("\u{1}g|{face:?}|{id}|{size}|{coords:?}|{}", colour_key(st));
        if let Some(hit) = self.cache.get(&key) {
            return Some(hit.clone());
        }
        let neutral = st.stroke.is_none();
        let s = SCALE.min(MAX_PX / size);
        let (fill, stroke) = paints(st, neutral);
        let stroke_w = (st.stroke_weight * s) as f32;
        let mut imgs = Vec::new();
        if let Some(col) = fill {
            if let Some(img) = self.glyph_image(&data, id, size * s, &coords, (0.0, 0.0), None) {
                imgs.push((img, col));
            }
        }
        if let (Some(col), true) = (stroke, stroke_w > 0.0) {
            let img = self.glyph_image(&data, id, size * s, &coords, (0.0, 0.0), Some(stroke_w));
            if let Some(img) = img {
                imgs.push((img, col));
            }
        }
        // Pixel bounds (y up from the origin) plus a transparent 1px border,
        // which keeps sampling clean at the quad's edges.
        let p = |i: &swash::scale::image::Image| i.placement;
        let left = imgs.iter().map(|(i, _)| p(i).left).min().unwrap_or(0) - 1;
        let top = imgs.iter().map(|(i, _)| p(i).top).max().unwrap_or(0) + 1;
        let right = imgs
            .iter()
            .map(|(i, _)| p(i).left + p(i).width as i32)
            .max()
            .unwrap_or(0)
            + 1;
        let bottom = imgs
            .iter()
            .map(|(i, _)| p(i).top - p(i).height as i32)
            .min()
            .unwrap_or(0)
            - 1;
        let (px_w, px_h) = ((right - left).max(1) as u32, (top - bottom).max(1) as u32);
        let mut canvas = Canvas::new(px_w, px_h);
        for (img, col) in &imgs {
            canvas.draw_image(img, p(img).left - left, top - p(img).top, *col);
        }
        let run = TextRun {
            id: self.next_id(),
            w: f64::from(px_w) / s,
            h: f64::from(px_h) / s,
            asc: f64::from(top) / s,
            neutral,
            left: f64::from(-left) / s,
            px_w,
            px_h,
            rgba: canvas.into_rgba8(),
        };
        Some(self.insert(key, run))
    }

    /// A glyph rasterised at `px` pixels per em at axis `coords`, offset by
    /// a subpixel `(dx, dy)`; outlined `stroke` px wide when given.
    fn glyph_image(
        &mut self,
        face: &FaceData,
        id: u16,
        px: f64,
        coords: &[i16],
        (dx, dy): (f32, f32),
        stroke: Option<f32>,
    ) -> Option<swash::scale::image::Image> {
        use swash::scale::{Render, Source, StrikeWith};
        let font = swash::FontRef::from_index(face.bytes(), face.index() as usize)?;
        let mut scaler = self
            .stroke_ctx
            .builder(font)
            .size(px as f32)
            .hint(false)
            .normalized_coords(coords)
            .build();
        let sources: &[Source] = if stroke.is_some() {
            &[Source::Outline]
        } else {
            &[
                Source::ColorOutline(0),
                Source::ColorBitmap(StrikeWith::BestFit),
                Source::Outline,
            ]
        };
        let mut render = Render::new(sources);
        render
            .format(swash::zeno::Format::Alpha)
            .offset(swash::zeno::Vector::new(dx, dy));
        if let Some(w) = stroke {
            render.style(swash::zeno::Stroke::new(w));
        }
        render.render(&mut scaler, id)
    }

    /// `draw/text` with variation axes: the engine lays the run out (the
    /// same box metrics as `render`) and each glyph is rasterised at its
    /// face's axis coordinates.
    fn render_shaped(&mut self, text: &str, st: &ResolvedStyle, neutral: bool) -> Option<TextRun> {
        let block = typo::block(text, st, 0.0)?;
        let (w, h) = (block.w, block.h);
        let s = SCALE.min(MAX_PX / w).min(MAX_PX / h);
        if s < 0.05 {
            return None;
        }
        let px_w = (w * s).ceil() as u32;
        let px_h = (h * s).ceil() as u32;
        let mut canvas = Canvas::new(px_w, px_h);
        let (fill, stroke) = paints(st, neutral);
        let stroke_w = (st.stroke_weight * s) as f32;
        let font = typo::FontSpec::from_style(st);
        let mut faces: HashMap<FaceId, (Arc<FaceData>, Vec<i16>)> = HashMap::new();
        // Fill first, then the stroke on top (like the cosmic-text path).
        let passes = [
            (fill, None),
            (stroke.filter(|_| stroke_w > 0.0), Some(stroke_w)),
        ];
        for (col, outline) in passes {
            let Some(col) = col else {
                continue;
            };
            for g in &block.glyphs {
                let (face, coords) = match faces.get(&g.glyph.face) {
                    Some(f) => f.clone(),
                    None => {
                        let Some(data) = typo::face(g.glyph.face) else {
                            continue;
                        };
                        let coords =
                            typo::normalized_coords(&data, &typo::face_settings(&data, &font));
                        faces.insert(g.glyph.face, (data.clone(), coords.clone()));
                        (data, coords)
                    }
                };
                let (gx, gy) = (g.glyph.x * s, g.glyph.y * s);
                let (fx, fy) = (gx.floor(), gy.floor());
                let off = ((gx - fx) as f32, (gy - fy) as f32);
                let px = st.font_size * s;
                let Some(img) = self.glyph_image(&face, g.glyph.id, px, &coords, off, outline)
                else {
                    continue;
                };
                let x0 = fx as i32 + img.placement.left;
                let y0 = fy as i32 - img.placement.top;
                canvas.draw_image(&img, x0, y0, col);
            }
        }
        Some(TextRun {
            id: self.next_id(),
            w,
            h,
            asc: block.asc,
            neutral,
            left: 0.0,
            px_w,
            px_h,
            rgba: canvas.into_rgba8(),
        })
    }

    fn render(&mut self, text: &str, st: &ResolvedStyle, neutral: bool) -> Option<TextRun> {
        let fs_px = st.font_size;
        if !(fs_px.is_finite() && fs_px > 0.0) {
            return None;
        }
        let lh = if st.line_height.is_finite() {
            st.line_height
        } else {
            1.2
        };
        let dy = fs_px * lh;
        let fonts = self.fonts.get_or_insert_with(font_system);
        let family_name = self
            .families
            .entry(st.font_family.to_string())
            .or_insert_with(|| typo::resolve_family(&st.font_family))
            .clone();
        let family = match family_name.as_str() {
            "serif" => Family::Serif,
            "monospace" => Family::Monospace,
            "cursive" => Family::Cursive,
            "fantasy" => Family::Fantasy,
            "sans-serif" => Family::SansSerif,
            name => Family::Name(name),
        };
        // The text is drawn at 2× and scaled; hinting buys nothing there
        // and re-running a font's hinting program for every new size is
        // most of the cost of animated text.
        let attrs = Attrs::new()
            .cache_key_flags(CacheKeyFlags::DISABLE_HINTING)
            .family(family)
            .weight(parse_weight(&st.font_weight))
            .style(match &*st.font_style {
                "italic" => Style::Italic,
                "oblique" => Style::Oblique,
                _ => Style::Normal,
            });

        // Shape each line on its own (no wrapping), like `fillText` per line.
        let lines: Vec<&str> = text.split('\n').collect();
        let metrics = Metrics::new(fs_px as f32, (dy as f32).max(1.0));
        let mut shaped: Vec<Vec<cosmic_text::LayoutGlyph>> = Vec::with_capacity(lines.len());
        let mut max_w: f64 = 1.0;
        for line in &lines {
            let mut buf = Buffer::new(fonts, metrics);
            buf.set_size(None, None);
            buf.set_text(line, &attrs, Shaping::Advanced, None);
            buf.shape_until_scroll(fonts, false);
            let mut glyphs = Vec::new();
            for run in buf.layout_runs() {
                max_w = max_w.max(f64::from(run.line_w));
                glyphs.extend(run.glyphs.iter().cloned());
            }
            shaped.push(glyphs);
        }

        let w = max_w.ceil() + 6.0;
        let h = (lines.len() as f64 * dy).ceil() + 6.0;
        if !(w.is_finite() && h.is_finite()) || h <= 0.0 {
            return None;
        }
        let s = SCALE.min(MAX_PX / w).min(MAX_PX / h);
        if s < 0.05 {
            return None;
        }
        let px_w = (w * s).ceil() as u32;
        let px_h = (h * s).ceil() as u32;
        let mut canvas = Canvas::new(px_w, px_h);
        let asc = fs_px * 0.8;

        let (fill, stroke) = paints(st, neutral);
        let stroke_w = (st.stroke_weight * s) as f32;

        for (i, glyphs) in shaped.iter().enumerate() {
            let base_x = (1.0 * s) as f32;
            let base_y = ((1.0 + i as f64 * dy + asc) * s) as f32;
            if let Some(col) = fill {
                for g in glyphs {
                    let pg = g.physical((base_x, base_y), s as f32);
                    let Some(img) = self.swash.get_image(fonts, pg.cache_key) else {
                        continue;
                    };
                    let x0 = pg.x + img.placement.left;
                    let y0 = pg.y - img.placement.top;
                    canvas.draw_image(img, x0, y0, col);
                }
            }
            if let (Some(col), true) = (stroke, stroke_w > 0.0) {
                for g in glyphs {
                    let pg = g.physical((base_x, base_y), s as f32);
                    let key = pg.cache_key;
                    let Some(font) = fonts.get_font(key.font_id, key.font_weight) else {
                        continue;
                    };
                    let mut scaler = self
                        .stroke_ctx
                        .builder(font.as_swash())
                        .size(f32::from_bits(key.font_size_bits))
                        .hint(false)
                        .build();
                    let img = swash::scale::Render::new(&[swash::scale::Source::Outline])
                        .format(swash::zeno::Format::Alpha)
                        .offset(swash::zeno::Vector::new(
                            key.x_bin.as_float(),
                            key.y_bin.as_float(),
                        ))
                        .style(swash::zeno::Stroke::new(stroke_w))
                        .render(&mut scaler, key.glyph_id);
                    if let Some(img) = img {
                        let x0 = pg.x + img.placement.left;
                        let y0 = pg.y - img.placement.top;
                        canvas.draw_image(&img, x0, y0, col);
                    }
                }
            }
        }

        Some(TextRun {
            id: self.next_id(),
            w,
            h,
            asc,
            neutral,
            left: 0.0,
            px_w,
            px_h,
            rgba: canvas.into_rgba8(),
        })
    }
}

/// A premultiplied float canvas glyphs are composited onto (source-over).
struct Canvas {
    w: u32,
    h: u32,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Canvas {
        Canvas {
            w,
            h,
            px: vec![[0.0; 4]; (w as usize) * (h as usize)],
        }
    }

    /// Composite a swash image at (x0, y0); masks are tinted with straight
    /// `col`, colour bitmaps (emoji) keep their own colours.
    fn draw_image(&mut self, img: &swash::scale::image::Image, x0: i32, y0: i32, col: [f32; 4]) {
        use swash::scale::image::Content;
        let (iw, ih) = (img.placement.width as i32, img.placement.height as i32);
        for yy in 0..ih {
            let y = y0 + yy;
            if y < 0 || y >= self.h as i32 {
                continue;
            }
            for xx in 0..iw {
                let x = x0 + xx;
                if x < 0 || x >= self.w as i32 {
                    continue;
                }
                let i = (yy * iw + xx) as usize;
                let src = match img.content {
                    Content::Mask => {
                        let a = f32::from(img.data[i]) / 255.0 * col[3];
                        [col[0] * a, col[1] * a, col[2] * a, a]
                    }
                    Content::Color => {
                        let p = &img.data[i * 4..i * 4 + 4];
                        let a = f32::from(p[3]) / 255.0 * col[3];
                        [
                            f32::from(p[0]) / 255.0 * a,
                            f32::from(p[1]) / 255.0 * a,
                            f32::from(p[2]) / 255.0 * a,
                            a,
                        ]
                    }
                    Content::SubpixelMask => {
                        let p = &img.data[i * 4..i * 4 + 4];
                        let a = (f32::from(p[0]) + f32::from(p[1]) + f32::from(p[2]))
                            / (3.0 * 255.0)
                            * col[3];
                        [col[0] * a, col[1] * a, col[2] * a, a]
                    }
                };
                if src[3] <= 0.0 {
                    continue;
                }
                let d = &mut self.px[(y as usize) * (self.w as usize) + x as usize];
                let k = 1.0 - src[3];
                for c in 0..4 {
                    d[c] = src[c] + d[c] * k;
                }
            }
        }
    }

    fn into_rgba8(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for p in self.px {
            for c in p {
                // `+ 0.5` then truncate: rounds like `round()` on 0..=255
                // without a libm call per channel.
                out.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
        out
    }
}

/// The colour part of a cache key: neutral runs are tinted at draw time.
fn colour_key(st: &ResolvedStyle) -> String {
    if st.stroke.is_none() {
        return "N".to_owned();
    }
    let css = |p: &Option<floret_engine::scene::Paint>| {
        p.as_ref().map_or("null".to_owned(), |p| p.css.to_string())
    };
    format!("{}|{}|{}", css(&st.fill), css(&st.stroke), st.stroke_weight)
}

/// The fill and stroke colours a run bakes (just white fill when neutral).
fn paints(st: &ResolvedStyle, neutral: bool) -> (Option<[f32; 4]>, Option<[f32; 4]>) {
    if neutral {
        return (Some([1.0; 4]), None);
    }
    let rgba =
        |p: &Option<floret_engine::scene::Paint>| p.as_ref().map(|p| p.rgba.map(|v| v as f32));
    (rgba(&st.fill), rgba(&st.stroke))
}

fn parse_weight(w: &str) -> Weight {
    Weight(typo::css_weight(w))
}

/// The engine's font library (system + bundled + loaded files) as a
/// cosmic-text font system.
fn font_system() -> FontSystem {
    FontSystem::new_with_locale_and_db("en-US".into(), typo::database())
}

#[cfg(test)]
mod tests {
    use super::*;
    use floret_engine::scene::{default_style, Paint};

    #[test]
    fn rasterises_and_caches_runs() {
        let mut r = TextRasterizer::new();
        let st = (**default_style()).clone();
        let a = r.rasterize("Hello", &st).expect("run");
        assert!(a.neutral);
        assert!(a.w > 20.0 && a.h > 14.0, "{a:?}");
        assert_eq!(a.rgba.len(), (a.px_w * a.px_h * 4) as usize);
        // Some ink, and white (neutral) ink.
        let ink: u32 = a.rgba.chunks(4).map(|p| u32::from(p[3])).sum();
        assert!(ink > 1000);
        assert!(a.rgba.chunks(4).all(|p| p[0] == p[3]));
        // Same text + font → cached; fill colour doesn't matter when neutral.
        let mut red = st.clone();
        red.fill = Some(Paint::css("#ff0000"));
        assert_eq!(r.rasterize("Hello", &red).unwrap(), a);
        // Two lines are taller.
        let b = r.rasterize("Hello\nWorld", &st).unwrap();
        assert!(b.h > a.h * 1.5);
        // Stroked text bakes its colours.
        let mut stroked = st.clone();
        stroked.stroke = Some(Paint::css("#00ff00"));
        stroked.stroke_weight = 2.0;
        let c = r.rasterize("Hello", &stroked).unwrap();
        assert!(!c.neutral);
        assert!(c.rgba.chunks(4).any(|p| p[1] > 200 && p[0] < 50));
    }

    #[test]
    fn parses_font_fields() {
        assert_eq!(typo::resolve_family("'NoSuchFont', Serif"), "serif");
        assert_eq!(typo::resolve_family("NoSuchFont"), "sans-serif");
        assert_eq!(
            typo::resolve_family("dejavu sans mono, serif"),
            "DejaVu Sans Mono"
        );
        assert_eq!(parse_weight("bold"), Weight::BOLD);
        assert_eq!(parse_weight("300"), Weight(300));
    }

    fn ink(run: &TextRun) -> u32 {
        run.rgba.chunks(4).map(|p| u32::from(p[3])).sum()
    }

    fn variable(wght: f32) -> ResolvedStyle {
        ResolvedStyle {
            font_family: Arc::from(typo::VARIABLE_FAMILY),
            font_size: 40.0,
            font_variations: Arc::from([(*b"wght", wght)]),
            ..(**default_style()).clone()
        }
    }

    #[test]
    fn variation_axes_reach_the_raster() {
        let mut r = TextRasterizer::new();
        let thin = r.rasterize("Weight", &variable(100.0)).unwrap();
        let black = r.rasterize("Weight", &variable(1000.0)).unwrap();
        assert_ne!(thin, black);
        assert!(
            ink(&black) > ink(&thin) * 2,
            "{} vs {}",
            ink(&black),
            ink(&thin)
        );
        // The box follows the engine's layout: wider at the heavier weight.
        assert!(black.w > thin.w);
        assert!((thin.asc - 32.0).abs() < 1e-9);
    }

    #[test]
    fn glyph_runs_rasterise_per_glyph() {
        let mut r = TextRasterizer::new();
        let st = variable(700.0);
        let line = typo::shape_line("H", &typo::FontSpec::from_style(&st), 0.0);
        let g = line.glyphs[0];
        let run = GlyphRun {
            glyphs: Arc::from([]),
            size: 40.0,
            style: Arc::new(st),
        };
        let b = r.glyph(g.face, g.id, &run).unwrap();
        assert!(b.neutral && ink(&b) > 1000);
        // "H" has a left side bearing: the origin is left of the bitmap.
        // The bitmap's top is about the cap height (≈ 0.7 em) above it.
        assert!(b.left < 0.0 && b.left > -10.0, "{b:?} left {}", b.left);
        assert!(b.asc > 25.0 && b.asc < 35.0, "asc {}", b.asc);
        assert_eq!(r.glyph(g.face, g.id, &run).unwrap(), b);
        // A different weight is a different bitmap.
        let light = GlyphRun {
            style: Arc::new(variable(200.0)),
            ..run.clone()
        };
        let l = r.glyph(g.face, g.id, &light).unwrap();
        assert!(ink(&l) < ink(&b));
    }
}
