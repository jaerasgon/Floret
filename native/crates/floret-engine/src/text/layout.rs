//! Shaping (harfrust) with bidi runs and per-character font fallback, and
//! the `draw/text` box layout.

use super::library::{self, FaceData, FaceId};
use crate::scene::ResolvedStyle;
use std::ops::Range;
use std::sync::Arc;

/// A variable-font axis setting: tag and user-space value.
pub type AxisSetting = ([u8; 4], f32);

/// What a run is set in.
#[derive(Clone, Debug, PartialEq)]
pub struct FontSpec {
    /// CSS `font-family` list.
    pub family: Arc<str>,
    pub weight: u16,
    pub italic: bool,
    pub variations: Arc<[AxisSetting]>,
    pub size: f64,
}

impl FontSpec {
    pub fn from_style(st: &ResolvedStyle) -> FontSpec {
        FontSpec {
            family: st.font_family.clone(),
            weight: library::css_weight(&st.font_weight),
            italic: matches!(&*st.font_style, "italic" | "oblique"),
            variations: st.font_variations.clone(),
            size: st.font_size,
        }
    }

    /// The face the family list selects (`None` only with no fonts at all).
    pub fn primary(&self) -> Option<Arc<FaceData>> {
        library::face(library::resolve(&self.family, self.weight, self.italic)?)
    }
}

/// The axis settings a face is drawn with: the explicit variations, plus
/// the CSS weight on a `wght` axis and italic on an `ital` axis when those
/// aren't set (what cosmic-text does for `wght`). Only the face's axes.
pub fn face_settings(face: &FaceData, font: &FontSpec) -> Vec<AxisSetting> {
    let mut out: Vec<AxisSetting> = Vec::new();
    for &(tag, v) in font.variations.iter() {
        if face.axis(tag).is_some() && v.is_finite() {
            match out.iter_mut().find(|(t, _)| *t == tag) {
                Some(s) => s.1 = v,
                None => out.push((tag, v)),
            }
        }
    }
    let has = |out: &[AxisSetting], tag: &[u8; 4]| out.iter().any(|(t, _)| t == tag);
    if let Some(a) = face.axis(*b"wght") {
        if !has(&out, b"wght") {
            out.push((*b"wght", f32::from(font.weight).clamp(a.min, a.max)));
        }
    }
    if font.italic && face.axis(*b"ital").is_some() && !has(&out, b"ital") {
        out.push((*b"ital", 1.0));
    }
    out
}

/// Normalised (F2Dot14) coordinates for `settings`, in axis order: what
/// the renderer's rasteriser takes, so it draws the location shaping used.
pub fn normalized_coords(face: &FaceData, settings: &[AxisSetting]) -> Vec<i16> {
    use skrifa::MetadataProvider;
    let Some(font) = face.font() else {
        return Vec::new();
    };
    let loc = font
        .axes()
        .location(settings.iter().map(|(t, v)| (skrifa::Tag::new(t), *v)));
    loc.coords().iter().map(|c| c.to_bits()).collect()
}

/// One shaped glyph. `x` / `y` are its origin relative to the line's pen
/// start and baseline (y down); `cluster` is the byte offset of the
/// characters it came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedGlyph {
    pub face: FaceId,
    pub id: u16,
    pub x: f64,
    pub y: f64,
    pub advance: f64,
    pub cluster: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapedLine {
    /// Visual (left-to-right) order.
    pub glyphs: Vec<ShapedGlyph>,
    pub width: f64,
}

impl ShapedLine {
    /// The source text of each glyph's cluster (a ligature's glyph gets
    /// every character it covers).
    pub fn cluster_ranges(&self, len: usize) -> Vec<Range<usize>> {
        let mut starts: Vec<usize> = self.glyphs.iter().map(|g| g.cluster).collect();
        starts.sort_unstable();
        starts.dedup();
        self.glyphs
            .iter()
            .map(|g| {
                let i = starts.partition_point(|s| *s <= g.cluster);
                g.cluster..starts.get(i).copied().unwrap_or(len)
            })
            .collect()
    }
}

/// Shape one line (no line breaks): bidi runs in visual order, each split
/// into runs of the font that has its characters. `tracking` is added to
/// every advancing glyph.
pub fn shape_line(text: &str, font: &FontSpec, tracking: f64) -> ShapedLine {
    let mut line = ShapedLine::default();
    if text.is_empty() || !(font.size.is_finite() && font.size > 0.0) {
        return line;
    }
    let Some(primary) = font.primary() else {
        return line;
    };
    let tracking = if tracking.is_finite() { tracking } else { 0.0 };
    let mut pen = 0.0;
    for (range, rtl) in bidi_runs(text) {
        let mut segs = face_runs(text, range, &primary);
        if rtl {
            segs.reverse();
        }
        for (r, face) in segs {
            shape_segment(text, r, &face, font, rtl, tracking, &mut pen, &mut line);
        }
    }
    line.width = pen;
    line
}

/// Directional runs in visual order: `(byte range, right-to-left)`.
fn bidi_runs(text: &str) -> Vec<(Range<usize>, bool)> {
    use unicode_bidi::ParagraphBidiInfo;
    let info = ParagraphBidiInfo::new(text, None);
    if !info.has_rtl() {
        return vec![(0..text.len(), false)];
    }
    let (levels, runs) = info.visual_runs(0..text.len());
    runs.into_iter()
        .map(|r| {
            let rtl = levels[r.start].is_rtl();
            (r, rtl)
        })
        .collect()
}

/// Split a run (logical order) by the face that draws each character.
fn face_runs(
    text: &str,
    range: Range<usize>,
    primary: &Arc<FaceData>,
) -> Vec<(Range<usize>, Arc<FaceData>)> {
    let mut out: Vec<(Range<usize>, Arc<FaceData>)> = Vec::new();
    for (i, c) in text[range.clone()].char_indices() {
        let at = range.start + i;
        let prev = out.last().map(|(_, f)| f.clone());
        let face = pick_face(c, primary, prev.as_ref());
        match out.last_mut() {
            Some((r, f)) if f.id == face.id => r.end = at + c.len_utf8(),
            _ => out.push((at..at + c.len_utf8(), face)),
        }
    }
    out
}

fn pick_face(c: char, primary: &Arc<FaceData>, prev: Option<&Arc<FaceData>>) -> Arc<FaceData> {
    if c.is_control() {
        return prev.unwrap_or(primary).clone();
    }
    // Stay in a fallback face for marks, spaces and punctuation, so a
    // fallback script run (Arabic in a Latin font) shapes as one piece.
    if let Some(p) = prev {
        if p.id != primary.id && !c.is_ascii_alphanumeric() && p.has_char(c) {
            return p.clone();
        }
    }
    if primary.has_char(c) {
        return primary.clone();
    }
    if let Some(p) = prev {
        if p.has_char(c) {
            return p.clone();
        }
    }
    library::fallback(c, primary.id)
        .and_then(library::face)
        .unwrap_or_else(|| primary.clone())
}

#[allow(clippy::too_many_arguments)]
fn shape_segment(
    text: &str,
    range: Range<usize>,
    face: &FaceData,
    font: &FontSpec,
    rtl: bool,
    tracking: f64,
    pen: &mut f64,
    line: &mut ShapedLine,
) {
    let Some(fref) = face.font() else {
        return;
    };
    let vars: Vec<harfrust::Variation> = face_settings(face, font)
        .into_iter()
        .map(|(t, v)| harfrust::Variation {
            tag: harfrust::Tag::new(&t),
            value: v,
        })
        .collect();
    let data = harfrust::ShaperData::new(&fref);
    let instance = harfrust::ShaperInstance::from_variations(&fref, vars);
    let shaper = data.shaper(&fref).instance(Some(&instance)).build();
    let mut buf = harfrust::UnicodeBuffer::new();
    let seg = &text[range.clone()];
    if seg.contains('\t') {
        buf.push_str(&seg.replace('\t', " "));
    } else {
        buf.push_str(seg);
    }
    buf.set_direction(if rtl {
        harfrust::Direction::RightToLeft
    } else {
        harfrust::Direction::LeftToRight
    });
    buf.guess_segment_properties();
    let shaped = shaper.shape(buf, &[]);
    let scale = font.size / f64::from(face.units_per_em);
    for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        let mut advance = f64::from(pos.x_advance) * scale;
        if advance != 0.0 {
            advance += tracking;
        }
        line.glyphs.push(ShapedGlyph {
            face: face.id,
            id: u16::try_from(info.glyph_id).unwrap_or(0),
            x: *pen + f64::from(pos.x_offset) * scale,
            y: -f64::from(pos.y_offset) * scale,
            advance,
            cluster: range.start + info.cluster as usize,
        });
        *pen += advance;
    }
}

/// Glyphs laid out the way `draw/text` draws a run (the TS glyph canvas):
/// a `ceil(widest line) + 6` by `ceil(lines × size × lineHeight) + 6` box,
/// glyphs 1 unit in, each baseline `0.8 × size` below its line's top.
/// Glyph `x` / `y` are relative to the box's top-left.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBlock {
    pub glyphs: Vec<BlockGlyph>,
    pub w: f64,
    pub h: f64,
    /// Baseline of the first line below the box top.
    pub asc: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockGlyph {
    pub glyph: ShapedGlyph,
    pub line: usize,
    /// Byte range of the glyph's cluster in the whole text.
    pub source: Range<usize>,
}

impl TextBlock {
    /// Where the box's top-left goes for a run drawn at `(x, y)`: the
    /// renderer's alignment (`textAlign`) and baseline (`textBaseline`)
    /// rules for `draw/text`.
    pub fn origin(&self, st: &ResolvedStyle, x: f64, y: f64) -> (f64, f64) {
        let ax = match &*st.text_align {
            "center" => -self.w / 2.0,
            "right" => -self.w,
            _ => 0.0,
        };
        let ay = match &*st.text_baseline {
            "top" => 0.0,
            "middle" => -self.h / 2.0,
            "bottom" => -self.h,
            _ => -self.asc,
        };
        (x + ax, y + ay)
    }
}

pub fn block(text: &str, st: &ResolvedStyle, tracking: f64) -> Option<TextBlock> {
    let size = st.font_size;
    if !(size.is_finite() && size > 0.0) {
        return None;
    }
    let lh = if st.line_height.is_finite() {
        st.line_height
    } else {
        1.2
    };
    let dy = size * lh;
    let font = FontSpec::from_style(st);
    let asc = size * 0.8;
    let mut glyphs = Vec::new();
    let mut max_w: f64 = 1.0;
    let mut start = 0;
    let mut lines = 0;
    for (i, text_line) in text.split('\n').enumerate() {
        lines += 1;
        let shaped = shape_line(text_line, &font, tracking);
        max_w = max_w.max(shaped.width);
        let ranges = shaped.cluster_ranges(text_line.len());
        let base = 1.0 + i as f64 * dy + asc;
        for (g, r) in shaped.glyphs.iter().zip(ranges) {
            glyphs.push(BlockGlyph {
                glyph: ShapedGlyph {
                    x: 1.0 + g.x,
                    y: base + g.y,
                    cluster: start + g.cluster,
                    ..*g
                },
                line: i,
                source: start + r.start..start + r.end,
            });
        }
        start += text_line.len() + 1;
    }
    let w = max_w.ceil() + 6.0;
    let h = (lines as f64 * dy).ceil() + 6.0;
    (w.is_finite() && h.is_finite() && h > 0.0).then_some(TextBlock { glyphs, w, h, asc })
}
