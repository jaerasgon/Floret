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

use cosmic_text::fontdb;
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight};
use floret_engine::scene::{compose_font_css, ResolvedStyle};
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
            next_id: 1,
        }
    }

    /// Rasterise (or fetch) a run. `None` for nothing drawable (empty text,
    /// bad font size).
    pub fn rasterize(&mut self, text: &str, st: &ResolvedStyle) -> Option<TextKey> {
        let neutral = st.stroke.is_none();
        let colour_key = if neutral {
            "N".to_owned()
        } else {
            let css = |p: &Option<floret_engine::scene::Paint>| {
                p.as_ref().map_or("null".to_owned(), |p| p.css.to_string())
            };
            format!("{}|{}|{}", css(&st.fill), css(&st.stroke), st.stroke_weight)
        };
        let key = format!(
            "{text}|{}|{}|{colour_key}",
            compose_font_css(st),
            st.line_height
        );
        if let Some(hit) = self.cache.get(&key) {
            return Some(hit.clone());
        }
        let run = self.render(text, st, neutral)?;
        let run = TextKey(Arc::new(run));
        if self.cache.len() >= CACHE_CAP {
            if let Some(old) = self.order.pop_front() {
                self.cache.remove(&old);
            }
        }
        self.order.push_back(key.clone());
        self.cache.insert(key, run.clone());
        Some(run)
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
            .or_insert_with(|| resolve_family(fonts.db(), &st.font_family))
            .clone();
        let family = match family_name.as_str() {
            "serif" => Family::Serif,
            "monospace" => Family::Monospace,
            "cursive" => Family::Cursive,
            "fantasy" => Family::Fantasy,
            "sans-serif" => Family::SansSerif,
            name => Family::Name(name),
        };
        let attrs = Attrs::new()
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

        let fill = if neutral {
            Some([1.0f32; 4])
        } else {
            st.fill.as_ref().map(|p| p.rgba.map(|v| v as f32))
        };
        let stroke = if neutral {
            None
        } else {
            st.stroke.as_ref().map(|p| p.rgba.map(|v| v as f32))
        };
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

        let id = self.next_id;
        self.next_id += 1;
        Some(TextRun {
            id,
            w,
            h,
            asc,
            neutral,
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
                out.push((c.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        out
    }
}

/// Metric-compatible stand-ins (what fontconfig aliases them to).
const ALIASES: &[(&str, &[&str])] = &[
    ("arial", &["Liberation Sans", "Arimo"]),
    ("helvetica", &["Liberation Sans", "Arimo"]),
    ("times new roman", &["Liberation Serif", "Tinos"]),
    ("times", &["Liberation Serif", "Tinos"]),
    ("courier new", &["Liberation Mono", "Cousine"]),
    ("courier", &["Liberation Mono", "Cousine"]),
];

/// CSS `font-family` fallback: the first family in the list that is
/// installed (or has an installed metric-compatible alias), else the first
/// generic keyword, else `sans-serif`. Generic names come back lower-case.
fn resolve_family(db: &fontdb::Database, css: &str) -> String {
    let installed = |name: &str| {
        db.faces()
            .flat_map(|f| f.families.iter())
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(n, _)| n.clone())
    };
    for raw in css.split(',') {
        let name = raw.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "" => continue,
            "serif" | "sans-serif" | "monospace" | "cursive" | "fantasy" => return lower,
            "system-ui" | "ui-sans-serif" | "-apple-system" => return "sans-serif".into(),
            "ui-serif" => return "serif".into(),
            "ui-monospace" => return "monospace".into(),
            _ => {}
        }
        if let Some(n) = installed(name) {
            return n;
        }
        if let Some((_, alts)) = ALIASES.iter().find(|(k, _)| *k == lower) {
            if let Some(n) = alts.iter().find_map(|a| installed(a)) {
                return n;
            }
        }
    }
    "sans-serif".into()
}

fn parse_weight(w: &str) -> Weight {
    match w.trim() {
        "bold" | "bolder" => Weight::BOLD,
        "lighter" => Weight::LIGHT,
        "normal" | "" => Weight::NORMAL,
        n => n
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map_or(Weight::NORMAL, |v| Weight(v.clamp(1.0, 1000.0) as u16)),
    }
}

/// The faces behind the generic families, bundled so `sans-serif`, `serif`
/// and `monospace` draw the same on every OS. They are what Chrome on Linux
/// picks (the reference images), metric-compatible with Arial / Times.
const BUNDLED: &[&[u8]] = &[
    include_bytes!("../fonts/LiberationSans-Regular.ttf"),
    include_bytes!("../fonts/LiberationSans-Bold.ttf"),
    include_bytes!("../fonts/LiberationSans-Italic.ttf"),
    include_bytes!("../fonts/LiberationSans-BoldItalic.ttf"),
    include_bytes!("../fonts/LiberationSerif-Regular.ttf"),
    include_bytes!("../fonts/LiberationSerif-Bold.ttf"),
    include_bytes!("../fonts/LiberationSerif-Italic.ttf"),
    include_bytes!("../fonts/LiberationSerif-BoldItalic.ttf"),
    include_bytes!("../fonts/DejaVuSansMono.ttf"),
    include_bytes!("../fonts/DejaVuSansMono-Bold.ttf"),
    include_bytes!("../fonts/DejaVuSansMono-Oblique.ttf"),
    include_bytes!("../fonts/DejaVuSansMono-BoldOblique.ttf"),
];

/// System fonts (for named families) plus the bundled faces, which back
/// every generic family.
fn font_system() -> FontSystem {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    for face in BUNDLED {
        db.load_font_data(face.to_vec());
    }
    db.set_sans_serif_family("Liberation Sans");
    db.set_cursive_family("Liberation Sans");
    db.set_fantasy_family("Liberation Sans");
    db.set_serif_family("Liberation Serif");
    db.set_monospace_family("DejaVu Sans Mono");
    FontSystem::new_with_locale_and_db("en-US".into(), db)
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
        let db = font_system().into_locale_and_db().1;
        assert_eq!(resolve_family(&db, "'NoSuchFont', Serif"), "serif");
        assert_eq!(resolve_family(&db, "NoSuchFont"), "sans-serif");
        assert_eq!(
            resolve_family(&db, "dejavu sans mono, serif"),
            "DejaVu Sans Mono"
        );
        assert_eq!(parse_weight("bold"), Weight::BOLD);
        assert_eq!(parse_weight("300"), Weight(300));
    }
}
