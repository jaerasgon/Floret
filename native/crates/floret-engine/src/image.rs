//! Image values: the `image` wire type.
//!
//! In TS an image wire carries a Canvas2D canvas (`HTMLCanvasElement` /
//! `OffscreenCanvas`) whose pixels mutate in place. Here it carries an
//! immutable [`Image`] behind an `Arc` (`Value::Image`):
//!
//! - **Pixels**: straight (non-premultiplied) RGBA8, row-major — the bytes
//!   Canvas2D's `getImageData` returns, so the pixel nodes port byte for
//!   byte. Everything lives on the CPU; the engine has no GPU.
//! - **Generation id**: every `Image` gets a process-unique [`Image::id`]
//!   when created. Images never change after creation, so the id names the
//!   content: floret-render caches one GPU texture per id. Image nodes
//!   memoise on their inputs' ids, so an unchanged chain keeps handing out
//!   the same `Arc` (and the renderer uploads nothing).
//! - **Placeholders**: like TS, a source that isn't ready (still loading,
//!   unwired) is a 1×1 transparent image, and consumers treat anything not
//!   larger than 1×1 as "not ready" ([`Image::is_ready`]).
//!
//! This module also decodes image files (PNG, JPEG, GIF, WebP, BMP via the
//! `image` crate; SVG rasterised with `resvg`, text and embedded images
//! included) and runs loads off the tick thread ([`Loader`]). Relative
//! paths resolve against the host's base folder (the open patch's).

use crate::host::Host;
use crate::value::Value;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Largest decoded / rasterised edge, in pixels.
pub const MAX_DIM: u32 = 8192;

pub struct Image {
    id: u64,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl Image {
    /// A `width × height` image from straight RGBA8 bytes (`4·w·h` long).
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Image {
        assert_eq!(
            rgba.len(),
            width as usize * height as usize * 4,
            "image buffer size"
        );
        Image {
            id: next_id(),
            width,
            height,
            rgba,
        }
    }

    /// Fully transparent black (what a fresh canvas holds).
    pub fn transparent(width: u32, height: u32) -> Image {
        Image::new(width, height, vec![0; width as usize * height as usize * 4])
    }

    /// From premultiplied RGBA8 (GPU readback, tiny-skia), un-premultiplied
    /// the way Canvas2D's `getImageData` does.
    pub fn from_premultiplied(width: u32, height: u32, mut rgba: Vec<u8>) -> Image {
        for px in rgba.chunks_exact_mut(4) {
            let a = px[3];
            if a != 0 && a != 255 {
                for c in &mut px[..3] {
                    *c = ((u32::from(*c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
                }
            }
        }
        Image::new(width, height, rgba)
    }

    /// The shared 1×1 "not ready yet" image.
    pub fn placeholder() -> Arc<Image> {
        static P: OnceLock<Arc<Image>> = OnceLock::new();
        P.get_or_init(|| Arc::new(Image::transparent(1, 1))).clone()
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Straight RGBA8, row-major.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }

    /// `isReadyImageCanvas`: both dimensions > 1 (placeholders are 1×1).
    pub fn is_ready(&self) -> bool {
        self.width > 1 && self.height > 1
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let o = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.rgba[o],
            self.rgba[o + 1],
            self.rgba[o + 2],
            self.rgba[o + 3],
        ]
    }

    /// Premultiplied RGBA8 (for GPU upload).
    pub fn premultiplied(&self) -> Vec<u8> {
        let mut out = self.rgba.clone();
        for px in out.chunks_exact_mut(4) {
            let a = u32::from(px[3]);
            if a != 255 {
                for c in &mut px[..3] {
                    *c = ((u32::from(*c) * a + 127) / 255) as u8;
                }
            }
        }
        out
    }
}

/// Equality is identity: same id, same pixels.
impl PartialEq for Image {
    fn eq(&self, other: &Image) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image#{}({}×{})", self.id, self.width, self.height)
    }
}

/// The image on a wire if it's ready to use (`isReadyImageCanvas`).
pub fn ready(v: &Value) -> Option<&Arc<Image>> {
    match v {
        Value::Image(img) if img.is_ready() => Some(img),
        _ => None,
    }
}

// ─── Decoding ────────────────────────────────────────────────────────────────

/// Is this (the start of) an SVG document?
pub fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    let text = String::from_utf8_lossy(head);
    let t = text.trim_start_matches('\u{feff}').trim_start();
    t.starts_with('<') && text.contains("<svg")
}

/// Decode an image file (PNG / JPEG / GIF / WebP / BMP, or SVG rasterised
/// at its natural size).
pub fn decode(bytes: &[u8]) -> Result<Image, String> {
    decode_in(bytes, None)
}

/// [`decode`], with an SVG's relative image links resolved in `dir`.
pub fn decode_in(bytes: &[u8], dir: Option<&Path>) -> Result<Image, String> {
    if looks_like_svg(bytes) {
        let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        return rasterize_svg(text, None, 1.0, dir).map(|s| s.image);
    }
    let img = ::image::load_from_memory(bytes).map_err(|e| {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]).to_ascii_lowercase();
        if head.contains("<html") || head.contains("<!doctype") {
            "got a web page, not an image".to_owned()
        } else {
            format!("not an image ({e})")
        }
    })?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
        return Err(format!("image size {w}×{h} out of range"));
    }
    Ok(Image::new(w, h, rgba.into_raw()))
}

/// A rasterised SVG plus its viewBox size (the TS node's `width` /
/// `height` outputs).
pub struct Svg {
    pub image: Image,
    pub width: f64,
    pub height: f64,
}

/// The root element's viewBox size, else its `width` / `height`, else 100
/// (`svg-registry.ts` `viewBoxOf`).
fn svg_view_size(tree: &resvg::usvg::Tree, markup: &str) -> (f64, f64) {
    let root = markup.find("<svg").map(|i| &markup[i..]).unwrap_or("");
    let tag = &root[..root.find('>').unwrap_or(root.len())];
    let attr = |name: &str| {
        let i = tag.find(&format!("{name}="))? + name.len() + 1;
        let q = tag[i..].chars().next()?;
        let rest = &tag[i + 1..];
        Some(rest[..rest.find(q)?].to_owned())
    };
    if let Some(vb) = attr("viewBox") {
        let n: Vec<f64> = vb
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        if let [_, _, w, h] = n[..] {
            if w > 0.0 && h > 0.0 {
                return (w, h);
            }
        }
    }
    let lead = |s: Option<String>| -> f64 {
        let s = s.unwrap_or_default();
        let end = s
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
            .unwrap_or(s.len());
        match s[..end].parse::<f64>() {
            Ok(v) if v != 0.0 && v.is_finite() => v,
            _ => 0.0,
        }
    };
    let (w, h) = (lead(attr("width")), lead(attr("height")));
    let size = tree.size();
    (
        if w != 0.0 {
            w
        } else {
            f64::from(size.width()).max(100.0)
        },
        if h != 0.0 {
            h
        } else {
            f64::from(size.height()).max(100.0)
        },
    )
}

/// Fonts for SVG `<text>`: the system's, loaded once on first use. Kept
/// apart from the renderer's text fonts.
fn svg_fonts() -> Arc<resvg::usvg::fontdb::Database> {
    use resvg::usvg::fontdb::Database;
    static FONTS: OnceLock<Arc<Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = Database::new();
            db.load_system_fonts();
            // Point the generic families at fonts this machine has, so text
            // never silently disappears.
            let pick = |db: &Database, names: &[&str]| {
                names
                    .iter()
                    .find(|n| {
                        db.faces()
                            .any(|f| f.families.iter().any(|(m, _)| m.eq_ignore_ascii_case(n)))
                    })
                    .map(|n| (*n).to_owned())
            };
            let any = db
                .faces()
                .next()
                .and_then(|f| f.families.first())
                .map(|(n, _)| n.clone());
            let sans = pick(
                &db,
                &[
                    "Helvetica",
                    "Arial",
                    "Segoe UI",
                    "DejaVu Sans",
                    "Liberation Sans",
                    "Noto Sans",
                ],
            )
            .or(any);
            let serif = pick(
                &db,
                &[
                    "Times New Roman",
                    "Times",
                    "DejaVu Serif",
                    "Liberation Serif",
                    "Noto Serif",
                ],
            )
            .or_else(|| sans.clone());
            let mono = pick(
                &db,
                &[
                    "Menlo",
                    "Courier New",
                    "Consolas",
                    "DejaVu Sans Mono",
                    "Liberation Mono",
                ],
            )
            .or_else(|| sans.clone());
            if let Some(f) = sans {
                db.set_sans_serif_family(f.clone());
                db.set_cursive_family(f.clone());
                db.set_fantasy_family(f);
            }
            if let Some(f) = serif {
                db.set_serif_family(f);
            }
            if let Some(f) = mono {
                db.set_monospace_family(f);
            }
            Arc::new(db)
        })
        .clone()
}

/// Rasterise SVG markup at `scale` × its viewBox size. `style_sheet` is
/// extra CSS applied on top of the document's own (used by `io/svg` to
/// replace paint with a wired style); relative `<image>` links resolve in
/// `dir`.
pub fn rasterize_svg(
    markup: &str,
    style_sheet: Option<String>,
    scale: f64,
    dir: Option<&Path>,
) -> Result<Svg, String> {
    use resvg::{tiny_skia, usvg};
    let mut opt = usvg::Options {
        style_sheet,
        resources_dir: dir.map(Path::to_owned),
        ..Default::default()
    };
    // Scanning the system's fonts takes a moment; only text needs them.
    if markup.contains("<text") {
        opt.fontdb = svg_fonts();
    }
    let tree = usvg::Tree::from_str(markup, &opt).map_err(|e| e.to_string())?;
    let (vw, vh) = svg_view_size(&tree, markup);
    let px = |v: f64| (v * scale).ceil().clamp(1.0, f64::from(MAX_DIM)) as u32;
    let (pw, ph) = (px(vw), px(vh));
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph).ok_or("svg: bad size")?;
    let size = tree.size();
    let transform = tiny_skia::Transform::from_scale(
        pw as f32 / size.width().max(1e-6),
        ph as f32 / size.height().max(1e-6),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(Svg {
        image: Image::from_premultiplied(pw, ph, pixmap.take()),
        width: vw,
        height: vh,
    })
}

// ─── Sources ─────────────────────────────────────────────────────────────────

pub(crate) fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        let v = val(c).ok_or("invalid base64")?;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// The payload of a `data:` URL (base64 or percent-encoded).
pub fn decode_data_url(url: &str) -> Result<Vec<u8>, String> {
    let rest = url.strip_prefix("data:").ok_or("not a data: URL")?;
    let comma = rest.find(',').ok_or("malformed data: URL")?;
    let (meta, payload) = (&rest[..comma], &rest[comma + 1..]);
    if meta.to_ascii_lowercase().contains(";base64") {
        base64_decode(payload)
    } else {
        Ok(percent_decode(payload))
    }
}

fn is_remote(src: &str) -> bool {
    let lower = src.get(..8).unwrap_or(src).to_ascii_lowercase();
    lower.starts_with("data:") || lower.starts_with("http://") || lower.starts_with("https://")
}

/// The local file a source names (`file://` URL or path), made absolute
/// against the host's base folder when relative. `None` for `data:` and
/// `http(s)` sources.
pub fn local_path(src: &str, host: Option<&dyn Host>) -> Option<PathBuf> {
    if is_remote(src) {
        return None;
    }
    let path = src.strip_prefix("file://").unwrap_or(src);
    let path = PathBuf::from(String::from_utf8_lossy(&percent_decode(path)).into_owned());
    Some(match host.and_then(|h| h.base_dir()) {
        Some(base) if path.is_relative() => base.join(path),
        _ => path,
    })
}

/// `src` with a relative path made absolute (see [`local_path`]): the key
/// a load is cached under.
pub fn resolve_source(src: &str, host: Option<&dyn Host>) -> String {
    match local_path(src, host) {
        Some(p) if p.is_absolute() => p.to_string_lossy().into_owned(),
        _ => src.to_owned(),
    }
}

/// The folder relative links inside a source (an SVG's `<image>`s)
/// resolve in: the file's own folder, else the host's base folder.
pub fn resource_dir(src: &str, host: Option<&dyn Host>) -> Option<PathBuf> {
    match local_path(src, host) {
        Some(p) => p.parent().map(Path::to_owned),
        None => host.and_then(|h| h.base_dir()),
    }
}

/// Read a source's bytes: a `data:` URL, an `http(s)` URL (through the
/// host), a `file://` URL or a local path (relative to the host's base
/// folder).
pub fn read_source(src: &str, host: Option<&dyn Host>) -> Result<Vec<u8>, String> {
    let lower = src.get(..8).unwrap_or(src).to_ascii_lowercase();
    if lower.starts_with("data:") {
        decode_data_url(src)
    } else if lower.starts_with("http://") || lower.starts_with("https://") {
        match host {
            Some(h) => h.fetch(src),
            None => Err(format!("no network access to load {src}")),
        }
    } else {
        let path = local_path(src, host).unwrap_or_default();
        std::fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("{}: file not found", path.display()),
            _ => format!("{}: {e}", path.display()),
        })
    }
}

// ─── Async loading ───────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum LoadState<T> {
    Loading,
    Ready(T),
    Failed(String),
}

/// One node's load, run on its own thread so ticks never block on disk,
/// network or decoding. Dropping it abandons the result.
pub struct Loader<T> {
    key: String,
    state: Arc<Mutex<LoadState<T>>>,
    started: std::time::Instant,
}

/// A failed download is tried again after this long (the network may
/// have come back); local files only when their path changes.
const RETRY_REMOTE: std::time::Duration = std::time::Duration::from_secs(10);

/// Bytes (and the folder relative links resolve in) → the loaded value.
pub type Parse<T> = fn(Vec<u8>, Option<&Path>) -> Result<T, String>;

/// [`Parse`] for images: decode to pixels.
pub fn parse_image(bytes: Vec<u8>, dir: Option<&Path>) -> Result<Arc<Image>, String> {
    decode_in(&bytes, dir).map(Arc::new)
}

/// [`Parse`] for text (SVG markup).
pub fn parse_text(bytes: Vec<u8>, _: Option<&Path>) -> Result<Arc<str>, String> {
    String::from_utf8(bytes)
        .map(Arc::from)
        .map_err(|e| e.to_string())
}

impl<T: Clone + Send + 'static> Loader<T> {
    /// Start reading `src` (see [`read_source`]) and parsing it.
    pub fn start(src: &str, host: Option<Arc<dyn Host>>, parse: Parse<T>) -> Loader<T> {
        let state = Arc::new(Mutex::new(LoadState::Loading));
        let shared = state.clone();
        let owned = src.to_owned();
        let spawned = std::thread::Builder::new()
            .name("floret load".into())
            .spawn(move || {
                let dir = resource_dir(&owned, host.as_deref());
                let read = read_source(&owned, host.as_deref());
                let next = match read.and_then(|b| parse(b, dir.as_deref())) {
                    Ok(v) => LoadState::Ready(v),
                    Err(e) => LoadState::Failed(e),
                };
                if let Ok(mut s) = shared.lock() {
                    *s = next;
                }
            });
        if let Err(e) = spawned {
            *state.lock().unwrap_or_else(|p| p.into_inner()) = LoadState::Failed(e.to_string());
        }
        Loader {
            key: src.to_owned(),
            state,
            started: std::time::Instant::now(),
        }
    }

    fn retry_due(&self) -> bool {
        let lower = self.key.get(..8).unwrap_or(&self.key).to_ascii_lowercase();
        (lower.starts_with("http://") || lower.starts_with("https://"))
            && self.started.elapsed() >= RETRY_REMOTE
            && matches!(self.state(), LoadState::Failed(_))
    }

    /// What this loader was started with.
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn state(&self) -> LoadState<T> {
        self.state
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|p| p.into_inner().clone())
    }

    /// Block until the load finishes (tests, export).
    pub fn wait(&self) -> LoadState<T> {
        loop {
            let s = self.state();
            if !matches!(s, LoadState::Loading) {
                return s;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

/// A per-node loader that restarts when its source changes (the TS image
/// registry's `ensure(nodeId, url)`).
pub struct LoadSlot<T>(Option<Loader<T>>);

impl<T> Default for LoadSlot<T> {
    fn default() -> Self {
        LoadSlot(None)
    }
}

impl<T: Clone + Send + 'static> LoadSlot<T> {
    /// The load for `src`, restarted when it names something new. Relative
    /// paths are resolved first, so saving the patch elsewhere reloads them.
    pub fn ensure(
        &mut self,
        src: &str,
        host: Option<Arc<dyn Host>>,
        parse: Parse<T>,
    ) -> &Loader<T> {
        let src = resolve_source(src, host.as_deref());
        if self
            .0
            .as_ref()
            .is_none_or(|l| l.key() != src || l.retry_due())
        {
            self.0 = Some(Loader::start(&src, host, parse));
        }
        self.0.as_ref().expect("loader")
    }

    pub fn clear(&mut self) {
        self.0 = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_placeholders_not_ready() {
        let a = Image::transparent(2, 2);
        let b = Image::transparent(2, 2);
        assert_ne!(a.id(), b.id());
        assert!(a.is_ready());
        assert!(!Image::placeholder().is_ready());
        assert!(!Image::transparent(1, 50).is_ready());
    }

    #[test]
    fn premultiply_round_trip() {
        let img = Image::from_premultiplied(1, 1, vec![64, 32, 0, 128]);
        assert_eq!(img.rgba(), &[128, 64, 0, 128]);
        assert_eq!(img.premultiplied(), vec![64, 32, 0, 128]);
    }

    #[test]
    fn data_urls() {
        assert_eq!(
            decode_data_url("data:text/plain;base64,aGk=").unwrap(),
            b"hi"
        );
        assert_eq!(decode_data_url("data:,a%20b").unwrap(), b"a b");
        assert_eq!(base64_decode("TWFu\nTWE=").unwrap(), b"ManMa");
    }

    #[test]
    fn decodes_png_and_svg() {
        let mut png = Vec::new();
        ::image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128])
            .unwrap()
            .write_to(
                &mut std::io::Cursor::new(&mut png),
                ::image::ImageFormat::Png,
            )
            .unwrap();
        let img = decode(&png).unwrap();
        assert_eq!((img.width(), img.height()), (2, 1));
        assert_eq!(img.pixel(1, 0), [0, 0, 255, 128]);

        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10" width="40" height="20"><rect width="10" height="10" fill="#ff0000"/></svg>"##;
        let s = rasterize_svg(svg, None, 1.0, None).unwrap();
        assert_eq!((s.width, s.height), (20.0, 10.0));
        assert_eq!((s.image.width(), s.image.height()), (20, 10));
        assert_eq!(s.image.pixel(2, 5), [255, 0, 0, 255]);
        assert_eq!(s.image.pixel(15, 5), [0, 0, 0, 0]);
        // An injected stylesheet overrides presentation attributes.
        let s = rasterize_svg(
            svg,
            Some("* { fill: #00ff00 !important; }".into()),
            2.0,
            None,
        )
        .unwrap();
        assert_eq!(s.image.width(), 40);
        assert_eq!(s.image.pixel(4, 10), [0, 255, 0, 255]);
        assert!(decode(svg.as_bytes()).is_ok());
    }

    fn png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        ::image::RgbaImage::from_pixel(w, h, ::image::Rgba(rgba))
            .write_to(
                &mut std::io::Cursor::new(&mut out),
                ::image::ImageFormat::Png,
            )
            .unwrap();
        out
    }

    #[test]
    fn svg_text_and_embedded_images_render() {
        let b64 = |bytes: &[u8]| {
            const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut s = String::new();
            for c in bytes.chunks(3) {
                let n = (u32::from(c[0]) << 16)
                    | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
                    | u32::from(*c.get(2).unwrap_or(&0));
                for k in 0..4 {
                    s.push(if k <= c.len() {
                        T[(n >> (18 - 6 * k) & 63) as usize] as char
                    } else {
                        '='
                    });
                }
            }
            s
        };
        let dir = std::env::temp_dir().join(format!("floret-svg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("blue.png"), png(2, 2, [0, 0, 255, 255])).unwrap();
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="40" height="20">
<image x="0" y="0" width="10" height="10" href="data:image/png;base64,{}"/>
<image x="10" y="0" width="10" height="10" xlink:href="blue.png"/>
<text x="20" y="18" font-family="sans-serif" font-size="18" fill="#00ff00">W</text></svg>"##,
            b64(&png(2, 2, [255, 0, 0, 255]))
        );
        let s = rasterize_svg(&svg, None, 1.0, Some(&dir)).unwrap();
        assert_eq!(s.image.pixel(5, 5), [255, 0, 0, 255], "data: image");
        assert_eq!(s.image.pixel(15, 5), [0, 0, 255, 255], "relative image");
        if !svg_fonts().is_empty() {
            let green = (20..40)
                .flat_map(|x| (0..20).map(move |y| (x, y)))
                .any(|(x, y)| s.image.pixel(x, y)[1] > 200);
            assert!(green, "text drawn");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn relative_paths_resolve_in_the_host_folder() {
        struct Folder(PathBuf);
        impl Host for Folder {
            fn base_dir(&self) -> Option<PathBuf> {
                Some(self.0.clone())
            }
        }
        let dir = std::env::temp_dir().join(format!("floret-rel-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("img")).unwrap();
        std::fs::write(dir.join("img/a b.png"), png(3, 2, [1, 2, 3, 255])).unwrap();
        let host: Arc<dyn Host> = Arc::new(Folder(dir.clone()));
        let src = "img/a%20b.png";
        assert_eq!(
            resolve_source(src, Some(&*host)),
            dir.join("img/a b.png").to_string_lossy()
        );
        assert_eq!(resolve_source("data:,x", Some(&*host)), "data:,x");
        assert_eq!(
            resource_dir("img/x.svg", Some(&*host)),
            Some(dir.join("img"))
        );
        let mut slot = LoadSlot::default();
        match slot.ensure(src, Some(host.clone()), parse_image).wait() {
            LoadState::Ready(img) => assert_eq!((img.width(), img.height()), (3, 2)),
            other => panic!("{other:?}"),
        }
        // Without a host it's relative to the working directory: not there.
        assert!(matches!(
            Loader::start(src, None, parse_image).wait(),
            LoadState::Failed(e) if e.contains("not found")
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loads_files_off_thread() {
        let dir = std::env::temp_dir().join(format!("floret-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.svg");
        std::fs::write(
            &path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="3"></svg>"#,
        )
        .unwrap();
        let l = Loader::start(path.to_str().unwrap(), None, parse_image);
        match l.wait() {
            LoadState::Ready(img) => assert_eq!((img.width(), img.height()), (4, 3)),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            Loader::start("https://example.com/x.png", None, parse_image).wait(),
            LoadState::Failed(_)
        ));
        assert!(matches!(
            Loader::start(dir.join("missing.png").to_str().unwrap(), None, parse_image).wait(),
            LoadState::Failed(_)
        ));
        std::fs::remove_dir_all(&dir).ok();
    }
}
