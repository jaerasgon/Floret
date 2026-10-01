//! Custom node bodies (`src/editor/nodes/*.tsx`): which node types get
//! one, and the value maths behind their controls — slider, toggle, XY
//! pad, bang / button / stepper / select, slot `− n +` steppers, palette
//! stops, probe readouts, file / SVG pickers and comments. Layout and
//! painting live in `canvas/custom.rs`.

use floret_engine::patch::PatchNode;
use floret_engine::{Spread, Value};
use serde_json::{json, Value as Json};

/// A node type with a custom body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Custom {
    Slider,
    Toggle,
    XyPad,
    Bang,
    Button,
    Stepper,
    Select,
    /// `− n +` arity controls in the header (group, merge, variadic math).
    Slots(Slots),
    /// Palette stops: a swatch per stop plus `− n +`.
    Palette,
    Probe,
    File,
    Svg,
    /// `video/*`: a picked video, referenced by path.
    Video,
}

/// An arity stepper: which param it drives and its bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slots {
    pub param: &'static str,
    pub min: i64,
    pub max: i64,
    pub default: i64,
    pub noun: &'static str,
}

pub const GROUP_SLOTS: Slots = Slots {
    param: "slotCount",
    min: 1,
    max: 32,
    default: 2,
    noun: "slot",
};

pub const PALETTE_STOPS: Slots = Slots {
    param: "stopCount",
    min: 1,
    max: 64,
    default: 4,
    noun: "stop",
};

pub fn custom_of(def_type: &str) -> Option<Custom> {
    Some(match def_type {
        "io/slider" => Custom::Slider,
        "io/toggle" => Custom::Toggle,
        "io/xy-pad" => Custom::XyPad,
        "io/bang" => Custom::Bang,
        "io/button" => Custom::Button,
        "io/stepper" => Custom::Stepper,
        "io/select" => Custom::Select,
        "color/palette" => Custom::Palette,
        "meta/probe" => Custom::Probe,
        "io/file" => Custom::File,
        "io/svg" => Custom::Svg,
        t if is_video(t) => Custom::Video,
        "math/add" | "math/sub" | "math/mul" | "math/div" | "math/min" | "math/max"
        | "spread/merge" | "transform/group" => Custom::Slots(GROUP_SLOTS),
        _ => return None,
    })
}

fn param_f64(n: &PatchNode, key: &str, default: f64) -> f64 {
    n.params
        .get(key)
        .and_then(Json::as_f64)
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

fn param_str<'a>(n: &'a PatchNode, key: &str, default: &'a str) -> &'a str {
    n.params.get(key).and_then(Json::as_str).unwrap_or(default)
}

// ─── Slots ──────────────────────────────────────────────────────────────────

/// The clamped slot count (`Math.floor(Number(v) || default)`).
pub fn slot_count(n: &PatchNode, s: Slots) -> i64 {
    let v = param_f64(n, s.param, 0.0).floor() as i64;
    let v = if v == 0 { s.default } else { v };
    v.clamp(s.min, s.max)
}

/// The count after pressing `−` (`delta < 0`) or `+`, if allowed.
pub fn step_slots(n: &PatchNode, s: Slots, delta: i64) -> Option<i64> {
    let cur = slot_count(n, s);
    let next = cur + delta;
    (s.min..=s.max).contains(&next).then_some(next)
}

// ─── Slider ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderState {
    pub value: f64,
    pub min: f64,
    /// `max`, or `min + 1` when the range is empty / inverted.
    pub max: f64,
}

pub fn slider(n: &PatchNode) -> SliderState {
    let min = param_f64(n, "min", 0.0);
    let max = param_f64(n, "max", 1.0);
    SliderState {
        value: param_f64(n, "value", 0.0),
        min,
        max: if max > min { max } else { min + 1.0 },
    }
}

impl SliderState {
    /// The `<input type=range>` step: coarser for wide ranges.
    pub fn step(&self) -> f64 {
        let range = self.max - self.min;
        if range > 100.0 {
            1.0
        } else if range > 10.0 {
            0.1
        } else {
            0.01
        }
    }

    /// Thumb position, 0..1.
    pub fn t(&self) -> f64 {
        ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
    }

    /// The value at track fraction `t`, snapped to the step like a range
    /// input.
    pub fn at(&self, t: f64) -> f64 {
        let raw = self.min + t.clamp(0.0, 1.0) * (self.max - self.min);
        self.snap(raw)
    }

    fn snap(&self, v: f64) -> f64 {
        let step = self.step();
        let s = self.min + ((v - self.min) / step).round() * step;
        tidy(s.clamp(self.min, self.max))
    }

    /// Fine drag (Shift): a tenth of the track's speed, from `start`.
    pub fn fine(&self, start: f64, dt: f64) -> f64 {
        let v = start + dt * (self.max - self.min) * 0.1;
        tidy(v.clamp(self.min, self.max))
    }

    /// The value pulled back into range after min / max changed.
    pub fn clamped(&self) -> Option<f64> {
        let c = self.value.clamp(self.min, self.max);
        (c != self.value).then_some(c)
    }

    /// Scrub step for the min / max fields (`ScrubInput step`).
    pub fn scrub_step(&self) -> f64 {
        ((self.max - self.min) / 100.0).max(0.01)
    }
}

/// Drop float noise from stepped values (0.1 + 0.2 …).
fn tidy(v: f64) -> f64 {
    (v * 1e9).round() / 1e9
}

// ─── XY pad ─────────────────────────────────────────────────────────────────

/// The pad's value and its range `[x0, x1, y0, y1]` (TS: fixed 0..1; here
/// the `xMin` / `xMax` / `yMin` / `yMax` params widen it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pad {
    pub x: f64,
    pub y: f64,
    pub range: [f64; 4],
}

pub const PAD_RANGE_KEYS: [&str; 4] = ["xMin", "xMax", "yMin", "yMax"];

pub fn pad(n: &PatchNode) -> Pad {
    let defaults = [0.0, 1.0, 0.0, 1.0];
    let mut range = [0.0; 4];
    for (i, k) in PAD_RANGE_KEYS.iter().enumerate() {
        range[i] = param_f64(n, k, defaults[i]);
    }
    Pad {
        x: param_f64(n, "x", 0.5),
        y: param_f64(n, "y", 0.5),
        range,
    }
}

impl Pad {
    /// The dot's position in the pad, 0..1 each (y down, as in TS).
    pub fn uv(&self) -> (f64, f64) {
        let f = |v: f64, a: f64, b: f64| {
            if b == a {
                0.5
            } else {
                ((v - a) / (b - a)).clamp(0.0, 1.0)
            }
        };
        (
            f(self.x, self.range[0], self.range[1]),
            f(self.y, self.range[2], self.range[3]),
        )
    }

    /// The value under pad position `(u, v)` (clamped to the pad).
    pub fn at(&self, u: f64, v: f64) -> (f64, f64) {
        let l = |t: f64, a: f64, b: f64| tidy(a + t.clamp(0.0, 1.0) * (b - a));
        (
            l(u, self.range[0], self.range[1]),
            l(v, self.range[2], self.range[3]),
        )
    }
}

// ─── Stepper / select ───────────────────────────────────────────────────────

/// `nudge`: `value ± step`, clamped to min / max.
pub fn nudge(n: &PatchNode, dir: f64) -> f64 {
    let value = param_f64(n, "value", 0.0);
    let min = param_f64(n, "min", f64::NEG_INFINITY);
    let max = param_f64(n, "max", f64::INFINITY);
    let step = param_f64(n, "step", 1.0);
    let step = if step == 0.0 { 1.0 } else { step };
    tidy((value + dir * step).clamp(min, max.max(min)))
}

/// The dropdown's choices: the comma-separated `options`, with the
/// current value first when it isn't one of them.
pub fn select_options(n: &PatchNode) -> Vec<String> {
    let list: Vec<String> = param_str(n, "options", "a,b,c")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let value = param_str(n, "value", "");
    if list.iter().any(|o| o == value) {
        list
    } else {
        let mut out = vec![if value.is_empty() { "—" } else { value }.to_owned()];
        out.extend(list);
        out
    }
}

// ─── Palette ────────────────────────────────────────────────────────────────

/// `DEFAULT_STOPS` in `nodes/color/palette.ts`.
pub const DEFAULT_STOPS: [&str; 8] = [
    "#fed000", "#fc8405", "#ed361a", "#4464a1", "#cf3895", "#6d358a", "#06b4b0", "#4b8a5f",
];

/// Stop `i`'s hex (the `c<i>` param, else the default seed).
pub fn palette_stop(n: &PatchNode, i: usize) -> String {
    n.params
        .get(&format!("c{i}"))
        .and_then(Json::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| DEFAULT_STOPS.get(i).unwrap_or(&"#ffffff").to_string())
}

// ─── Probe ──────────────────────────────────────────────────────────────────

pub const PROBE_MAX_ROWS: usize = 400;

/// What a probe shows: a text list, or a picture.
#[derive(Clone, Debug, PartialEq)]
pub enum ProbeView {
    /// Not ticked yet.
    NoData,
    Text {
        count: usize,
        ty: String,
        rows: Vec<String>,
    },
    Image {
        width: u32,
        height: u32,
    },
    Layer,
}

impl ProbeView {
    /// The header's right-hand meta text.
    pub fn meta(&self) -> String {
        match self {
            ProbeView::NoData => "—".into(),
            ProbeView::Text { count: 0, .. } => "empty".into(),
            ProbeView::Text { count: 1, ty, .. } => ty.clone(),
            ProbeView::Text { count, ty, .. } => format!("{ty} · {count}"),
            ProbeView::Image { width, height } => format!("image {width}×{height}"),
            ProbeView::Layer => "layer".into(),
        }
    }
}

pub fn probe_view(port: Option<&Spread>) -> ProbeView {
    let Some(port) = port else {
        return ProbeView::NoData;
    };
    match port.first() {
        Some(Value::Image(i)) => ProbeView::Image {
            width: i.width(),
            height: i.height(),
        },
        Some(Value::Scene(_)) => ProbeView::Layer,
        first => ProbeView::Text {
            count: port.len(),
            ty: first.map_or_else(String::new, |v| probe_type(v).to_owned()),
            rows: port.iter().take(PROBE_MAX_ROWS).map(probe_fmt).collect(),
        },
    }
}

/// `inferType`.
fn probe_type(v: &Value) -> &'static str {
    match v {
        Value::Undefined | Value::Null => "null",
        Value::Num(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Str(_) => "string",
        Value::Style(_) => "fn",
        Value::List(_) => "array",
        Value::Vec2(_) => "vec2",
        Value::Color(_) => "color",
        Value::Object(_) | Value::Scene(_) | Value::Handle(_) | Value::Image(_) => "object",
    }
}

fn probe_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{v:.0}");
    }
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// `fmt`: one element on one line.
pub fn probe_fmt(v: &Value) -> String {
    match v {
        Value::Undefined => "—".into(),
        Value::Null => "null".into(),
        Value::Num(n) => probe_num(*n),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => format!("\"{s}\""),
        Value::Style(_) => "[fn]".into(),
        Value::List(items) => {
            let head: Vec<String> = items.iter().take(4).map(probe_fmt).collect();
            if items.len() > 4 {
                format!("[{}, …×{}]", head.join(", "), items.len())
            } else {
                format!("[{}]", head.join(", "))
            }
        }
        Value::Scene(n) => format!("‹{}›", n.kind()),
        Value::Vec2(p) => format!("({}, {})", probe_num(p.x), probe_num(p.y)),
        Value::Color(c) => {
            let h = |k: f64| (k.clamp(0.0, 1.0) * 255.0).round() as u8;
            format!("#{:02x}{:02x}{:02x}", h(c.r), h(c.g), h(c.b))
        }
        Value::Object(_) => "{…}".into(),
        Value::Handle(h) => format!("‹{}›", h.type_name()),
        Value::Image(i) => format!("‹image {}×{}›", i.width(), i.height()),
    }
}

// ─── Files ──────────────────────────────────────────────────────────────────

/// `data:<mime>;base64,<bytes>` — how `io/file` / `io/svg` keep a picked
/// file inside the patch.
pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64(bytes))
}

pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A MIME type from a file name (what the browser's `File.type` gives).
pub fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "json" => "application/json",
        "csv" => "text/csv",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// The picked file's size, estimated from the base64 length.
pub fn data_url_size(url: &str) -> usize {
    match url.find(',') {
        Some(i) => (url.len() - i - 1) * 3 / 4,
        None => 0,
    }
}

/// Above this the size readout turns amber (`SIZE_HINT_LIMIT`).
pub const SIZE_HINT_LIMIT: usize = 8 * 1024 * 1024;

pub fn format_bytes(n: usize) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

/// The params a picked file writes (`dataUrl`, `filename`, and for
/// `io/file` the `mimeType`). `None` for an SVG node given a non-SVG.
pub fn file_params(def_type: &str, name: &str, bytes: &[u8]) -> Option<Vec<(String, Json)>> {
    let mime = mime_for(name);
    let mut out = vec![
        ("dataUrl".to_owned(), json!(data_url(mime, bytes))),
        ("filename".to_owned(), json!(name)),
    ];
    match def_type {
        "io/svg" if mime != "image/svg+xml" => return None,
        "io/svg" => {}
        _ => out.push(("mimeType".to_owned(), json!(mime))),
    }
    Some(out)
}

/// Params that clear a picked file.
pub fn clear_file_params(def_type: &str) -> Vec<(String, Json)> {
    if is_video(def_type) {
        return vec![("url".to_owned(), json!(""))];
    }
    let mut keys = vec!["dataUrl", "filename"];
    if def_type != "io/svg" {
        keys.push("mimeType");
    }
    keys.into_iter()
        .map(|k| (k.to_owned(), json!("")))
        .collect()
}

// ─── Video ──────────────────────────────────────────────────────────────────

pub fn is_video(def_type: &str) -> bool {
    matches!(def_type, "video/file" | "video/frame-at")
}

/// What the video file picker offers.
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov"];

/// The params a picked / dropped video writes: its path (videos are too
/// big to copy into the patch), or a data URL when there's no path.
pub fn video_params(
    path: Option<&std::path::Path>,
    bytes: Option<&[u8]>,
    name: &str,
) -> Option<Vec<(String, Json)>> {
    let url = match (path, bytes) {
        (Some(p), _) if !p.as_os_str().is_empty() => p.to_string_lossy().into_owned(),
        (_, Some(b)) => data_url(mime_for(name), b),
        _ => return None,
    };
    Some(vec![("url".to_owned(), json!(url))])
}

/// The picked video's name for the node body: its file name, or
/// "embedded video" for a data URL.
pub fn video_name(n: &PatchNode) -> Option<String> {
    let url = param_str(n, "url", "").trim();
    if url.is_empty() {
        None
    } else if url.starts_with("data:") {
        Some("embedded video".into())
    } else {
        let path = url.strip_prefix("file://").unwrap_or(url);
        Some(
            std::path::Path::new(path)
                .file_name()
                .map_or_else(|| path.to_owned(), |f| f.to_string_lossy().into_owned()),
        )
    }
}

// ─── Comment ────────────────────────────────────────────────────────────────

pub const COMMENT_MIN: (f64, f64) = (120.0, 48.0);

/// A comment resized by `(dx, dy)` from `start`, kept above the minimum.
pub fn comment_size(start: (f64, f64), dx: f64, dy: f64) -> (f64, f64) {
    (
        (start.0 + dx).max(COMMENT_MIN.0).round(),
        (start.1 + dy).max(COMMENT_MIN.1).round(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::node;
    use floret_engine::value::one;
    use std::sync::Arc;

    fn with(ty: &str, params: Json) -> PatchNode {
        let mut n = node("n", ty);
        n.params = params.as_object().cloned().unwrap_or_default();
        n
    }

    #[test]
    fn custom_bodies_cover_the_ts_components() {
        for t in [
            "io/slider",
            "io/toggle",
            "io/xy-pad",
            "io/bang",
            "io/button",
            "io/stepper",
            "io/select",
            "color/palette",
            "meta/probe",
            "io/file",
            "io/svg",
            "transform/group",
            "spread/merge",
            "math/add",
            "math/max",
        ] {
            assert!(custom_of(t).is_some(), "{t}");
        }
        assert_eq!(custom_of("math/pow"), None);
        assert_eq!(custom_of("math/add"), Some(Custom::Slots(GROUP_SLOTS)));
    }

    #[test]
    fn slots_clamp_and_step() {
        let n = with("transform/group", json!({ "slotCount": 3 }));
        assert_eq!(slot_count(&n, GROUP_SLOTS), 3);
        assert_eq!(step_slots(&n, GROUP_SLOTS, 1), Some(4));
        assert_eq!(step_slots(&n, GROUP_SLOTS, -1), Some(2));
        let one_slot = with("transform/group", json!({ "slotCount": 1 }));
        assert_eq!(step_slots(&one_slot, GROUP_SLOTS, -1), None);
        let full = with("transform/group", json!({ "slotCount": 99 }));
        assert_eq!(slot_count(&full, GROUP_SLOTS), 32);
        assert_eq!(step_slots(&full, GROUP_SLOTS, 1), None);
        // Missing / zero falls back to the default, as `Number(v) || 2`.
        assert_eq!(slot_count(&with("math/add", json!({})), GROUP_SLOTS), 2);
        assert_eq!(
            slot_count(&with("color/palette", json!({})), PALETTE_STOPS),
            4
        );
    }

    #[test]
    fn slider_snaps_like_a_range_input() {
        let s = slider(&with(
            "io/slider",
            json!({ "value": 0.5, "min": 0, "max": 1 }),
        ));
        assert_eq!(s.step(), 0.01);
        assert_eq!(s.at(0.333), 0.33);
        assert_eq!(s.at(-1.0), 0.0);
        assert_eq!(s.at(2.0), 1.0);
        assert_eq!(s.t(), 0.5);
        let wide = slider(&with(
            "io/slider",
            json!({ "value": 3, "min": 0, "max": 500 }),
        ));
        assert_eq!(wide.step(), 1.0);
        assert_eq!(wide.at(0.4991), 250.0);
        assert_eq!(wide.scrub_step(), 5.0);
        // Inverted range keeps the slider usable.
        let inv = slider(&with(
            "io/slider",
            json!({ "value": 3, "min": 2, "max": 1 }),
        ));
        assert_eq!((inv.min, inv.max), (2.0, 3.0));
        // Shift-drag moves a tenth as far.
        assert_eq!(s.fine(0.5, 0.5), 0.55);
    }

    #[test]
    fn slider_pulls_the_value_into_range() {
        let s = slider(&with(
            "io/slider",
            json!({ "value": 50, "min": 0, "max": 1 }),
        ));
        assert_eq!(s.clamped(), Some(1.0));
        let ok = slider(&with("io/slider", json!({ "value": 0.2 })));
        assert_eq!(ok.clamped(), None);
    }

    #[test]
    fn pad_maps_its_range() {
        let p = pad(&with("io/xy-pad", json!({ "x": 0.25, "y": 1 })));
        assert_eq!(p.uv(), (0.25, 1.0));
        assert_eq!(p.at(0.5, 0.0), (0.5, 0.0));
        let wide = pad(&with(
            "io/xy-pad",
            json!({ "x": 0, "y": 50, "xMin": -1, "xMax": 1, "yMin": 0, "yMax": 100 }),
        ));
        assert_eq!(wide.uv(), (0.5, 0.5));
        assert_eq!(wide.at(1.0, 0.25), (1.0, 25.0));
        assert_eq!(wide.at(9.0, -3.0), (1.0, 0.0));
    }

    #[test]
    fn stepper_and_select() {
        let n = with(
            "io/stepper",
            json!({ "value": 9, "min": 0, "max": 10, "step": 2 }),
        );
        assert_eq!(nudge(&n, 1.0), 10.0);
        assert_eq!(nudge(&n, -1.0), 7.0);
        let s = with("io/select", json!({ "options": " a, b,,c", "value": "b" }));
        assert_eq!(select_options(&s), ["a", "b", "c"]);
        let odd = with("io/select", json!({ "options": "a,b", "value": "z" }));
        assert_eq!(select_options(&odd), ["z", "a", "b"]);
    }

    #[test]
    fn palette_stops_fall_back_to_the_seeds() {
        let n = with("color/palette", json!({ "c1": "#123456" }));
        assert_eq!(palette_stop(&n, 0), "#fed000");
        assert_eq!(palette_stop(&n, 1), "#123456");
        assert_eq!(palette_stop(&n, 12), "#ffffff");
    }

    #[test]
    fn probe_formats_like_the_web_app() {
        assert_eq!(probe_view(None), ProbeView::NoData);
        let empty: Spread = Arc::from(Vec::<Value>::new());
        assert_eq!(probe_view(Some(&empty)).meta(), "empty");
        let one_num = one(Value::Num(0.5));
        assert_eq!(
            probe_view(Some(&one_num)),
            ProbeView::Text {
                count: 1,
                ty: "number".into(),
                rows: vec!["0.5".into()]
            }
        );
        let many: Spread = Arc::from(vec![
            Value::Vec2(floret_engine::Vec2 { x: 1.0, y: 2.5 }),
            Value::Vec2(floret_engine::Vec2 { x: 0.1234, y: -3.0 }),
        ]);
        let v = probe_view(Some(&many));
        assert_eq!(v.meta(), "vec2 · 2");
        let ProbeView::Text { rows, .. } = v else {
            panic!()
        };
        assert_eq!(rows, ["(1, 2.5)", "(0.123, -3)"]);
        assert_eq!(
            probe_fmt(&Value::list(
                (0..6).map(|i| Value::Num(f64::from(i))).collect()
            )),
            "[0, 1, 2, 3, …×6]"
        );
        assert_eq!(
            probe_fmt(&Value::Color(floret_engine::Color {
                r: 1.0,
                g: 0.5,
                b: 0.0,
                a: 1.0
            })),
            "#ff8000"
        );
        assert_eq!(probe_fmt(&Value::str("hi")), "\"hi\"");
    }

    #[test]
    fn files_become_data_urls() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(data_url("text/plain", b"hi"), "data:text/plain;base64,aGk=");
        assert_eq!(data_url_size("data:text/plain;base64,Zm9vYmFy"), 6);
        assert_eq!(mime_for("Photo.JPG"), "image/jpeg");
        assert_eq!(mime_for("noext"), "application/octet-stream");
        assert_eq!(format_bytes(12), "12 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MB");
        let v = with("video/file", json!({ "url": "/clips/sea.mov" }));
        assert_eq!(custom_of("video/file"), Some(Custom::Video));
        assert_eq!(video_name(&v).as_deref(), Some("sea.mov"));
        let v = with("video/file", json!({ "url": "data:video/mp4;base64,AAAA" }));
        assert_eq!(video_name(&v).as_deref(), Some("embedded video"));
        assert_eq!(video_name(&with("video/file", json!({}))), None);
        let p = video_params(Some(std::path::Path::new("/a/b.mp4")), None, "").unwrap();
        assert_eq!(p[0].1, json!("/a/b.mp4"));
        let p = video_params(None, Some(b"x"), "b.mp4").unwrap();
        assert_eq!(p[0].1, json!("data:video/mp4;base64,eA=="));
        assert_eq!(
            clear_file_params("video/frame-at"),
            vec![("url".into(), json!(""))]
        );
        let p = file_params("io/file", "a.png", b"x").unwrap();
        assert_eq!(p[2], ("mimeType".to_owned(), json!("image/png")));
        assert!(file_params("io/svg", "a.png", b"x").is_none());
        assert_eq!(file_params("io/svg", "a.svg", b"x").unwrap().len(), 2);
        assert_eq!(clear_file_params("io/file").len(), 3);
    }

    #[test]
    fn comments_keep_a_minimum_size() {
        assert_eq!(comment_size((200.0, 80.0), 30.4, -100.0), (230.0, 48.0));
        assert_eq!(comment_size((200.0, 80.0), -500.0, 10.0), (120.0, 90.0));
    }
}
