//! PNG export (#54): the preview's current frame, re-rendered at 1× / 2× /
//! 4× (a true re-render at scale, not TS's nearest-neighbour upscale), read
//! back from the GPU and written through a native save dialog.
//!
//! Like TS the file is named after the preview (the renderer's label, else
//! the patch name), slugged; scales above 1× add an `@2x` / `@4x` suffix.
//! Unlike TS, export works while the patch runs: it takes the frame shown
//! when the export starts. Cmd/Ctrl+E exports again at the last scale.

use crate::recording::Format;
use crate::theme;
use eframe::egui::{self, RichText};
use eframe::egui_wgpu::{self, wgpu};
use floret_engine::scene::SceneNode;
use floret_render::Renderer;
use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

const SCALES: [u32; 3] = [1, 2, 4];
/// Above this many pixels the export renders without MSAA (a 4× export of
/// a tall canvas would otherwise need over half a gigabyte of samples).
const MSAA_MAX_PIXELS: u64 = 4096 * 4096;

/// What the preview is showing, at its native size.
pub struct Frame<'a> {
    pub layers: &'a [Arc<SceneNode>],
    pub background: [f64; 4],
    pub size: (u32, u32),
}

pub struct Export {
    scale: u32,
    /// Result of the last export, shown on the button for a few seconds.
    status: Option<(Result<String, String>, Instant)>,
    /// PNG encodes run off the UI thread and report back here.
    done: Option<mpsc::Receiver<Result<String, String>>>,
    /// A recording (video / GIF / PNG sequence) picked from the menu, for
    /// the app to open its export window with.
    record: Option<Format>,
    /// "Variations…" was picked from the menu.
    variations: bool,
    /// Where the button sat last frame (its hover tint).
    rect: Option<egui::Rect>,
}

impl Default for Export {
    fn default() -> Self {
        Export {
            scale: 1,
            status: None,
            done: None,
            record: None,
            variations: false,
            rect: None,
        }
    }
}

impl Export {
    /// The header's Export menu, plus the Cmd/Ctrl+E shortcut.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        name: &str,
        frame: Frame<'_>,
    ) {
        if let Some(rx) = &self.done {
            if let Ok(r) = rx.try_recv() {
                self.status = Some((r, Instant::now()));
                self.done = None;
            }
        }
        let max = rs.map_or(0, |rs| rs.device.limits().max_texture_dimension_2d);
        let fits = |s: u32| frame.size.0 * s <= max && frame.size.1 * s <= max;
        let mut pick = None;
        let shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::E);
        if ui.ctx().input_mut(|i| i.consume_shortcut(&shortcut)) && rs.is_some() {
            pick = Some(if fits(self.scale) { self.scale } else { 1 });
        }
        // Like TS, the button itself says how the last export went.
        let recent = self
            .status
            .as_ref()
            .filter(|(_, at)| at.elapsed() < Duration::from_secs(3));
        // The preview's one primary button: dark text on gold, except
        // after a failed export (red text, no fill).
        let (label, saved, failed, hover) = match (&self.done, recent) {
            (Some(_), _) => ("Exporting…", false, false, "Writing the PNG…".to_owned()),
            (None, Some((Ok(m), _))) => ("Saved", true, false, m.clone()),
            (None, Some((Err(e), _))) => ("Export failed", false, true, e.clone()),
            (None, None) => ("Export", false, false, tooltip(&shortcut, self.scale)),
        };
        if recent.is_some() || self.done.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
        let button = if failed {
            egui::Button::new(RichText::new(label).small().color(theme::DANGER))
        } else {
            let text = RichText::new(label).small().color(theme::ON_GOLD);
            let b = if saved {
                let check = theme::icon(egui_phosphor::regular::CHECK, 12.0, theme::ON_GOLD);
                egui::Button::new((check, text))
            } else {
                egui::Button::new(text)
            };
            let hovered = self.rect.is_some_and(|r| ui.rect_contains_pointer(r));
            let fill = if hovered {
                theme::GOLD_FILL_HOVER
            } else {
                theme::GOLD_FILL
            };
            b.fill(fill).stroke(egui::Stroke::NONE)
        };
        ui.add_enabled_ui(rs.is_some(), |ui| {
            let resp = egui::containers::menu::MenuButton::from_button(button)
                .ui(ui, |ui| {
                    for s in SCALES {
                        let (w, h) = (frame.size.0 * s, frame.size.1 * s);
                        let mut b = egui::Button::new(format!("PNG · {s}×   {w} × {h}"));
                        if s == self.scale {
                            b = b.shortcut_text(ui.ctx().format_shortcut(&shortcut));
                        }
                        let r = ui.add_enabled(fits(s), b);
                        let r = if fits(s) {
                            r
                        } else {
                            r.on_disabled_hover_text(format!(
                                "Larger than this GPU's {max} px texture limit"
                            ))
                        };
                        if r.clicked() {
                            pick = Some(s);
                            ui.close();
                        }
                    }
                    // Recordings: the export window takes it from here.
                    ui.separator();
                    for f in Format::ALL {
                        if ui.button(f.menu_label()).clicked() {
                            self.record = Some(f);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui
                        .button("Variations…")
                        .on_hover_text("Render many seeds or presets at once, with a contact sheet")
                        .clicked()
                    {
                        self.variations = true;
                        ui.close();
                    }
                })
                .0
                .on_hover_text(hover);
            self.rect = Some(resp.rect);
        });
        if let (Some(scale), Some(rs)) = (pick, rs) {
            self.scale = scale;
            self.export(rs, name, frame, scale);
        }
    }

    /// A recording picked from the menu since the last call.
    pub fn take_record(&mut self) -> Option<Format> {
        self.record.take()
    }

    /// "Variations…" picked from the menu since the last call.
    pub fn take_variations(&mut self) -> bool {
        std::mem::take(&mut self.variations)
    }

    fn export(&mut self, rs: &egui_wgpu::RenderState, name: &str, frame: Frame<'_>, scale: u32) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Export PNG")
            .add_filter("PNG image", &["png"])
            .set_file_name(file_name(name, scale))
            .save_file()
        else {
            return;
        };
        let (w, h) = (frame.size.0 * scale, frame.size.1 * scale);
        let pixels = match render(rs, &frame, scale) {
            Ok(p) => p,
            Err(e) => {
                self.status = Some((Err(format!("Export failed: {e}")), Instant::now()));
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        self.done = Some(rx);
        std::thread::spawn(move || {
            let r = write_png(&path, w, h, &pixels)
                .map(|()| format!("Saved {} ({w} × {h})", display_name(&path)))
                .map_err(|e| format!("Export failed: {e}"));
            let _ = tx.send(r);
        });
    }
}

/// The Export button's tooltip: what it makes, and the shortcut.
fn tooltip(shortcut: &egui::KeyboardShortcut, scale: u32) -> String {
    format!(
        "Export PNG: save the current frame as an image at 1×, 2× or 4× the canvas size \
         (re-rendered, not upscaled). {} exports again at the last scale ({scale}×).",
        crate::menu::shortcut_text(shortcut)
    )
}

/// `<slug>.png`, `<slug>@2x.png`, … (TS slug rule; "floret" if empty).
pub fn file_name(name: &str, scale: u32) -> String {
    let slug = slug(name);
    if scale == 1 {
        format!("{slug}.png")
    } else {
        format!("{slug}@{scale}x.png")
    }
}

/// A file-name stem from a patch or preview name (TS slug rule; "floret"
/// if empty).
pub fn slug(name: &str) -> String {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "floret".to_owned()
    } else {
        slug.to_owned()
    }
}

fn display_name(p: &Path) -> String {
    p.file_name().map_or_else(
        || p.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Render the frame at `scale` on the app's device with a renderer of its
/// own (dropped afterwards: its targets are as big as the export).
fn render(rs: &egui_wgpu::RenderState, frame: &Frame<'_>, scale: u32) -> Result<Vec<u8>, String> {
    let (w, h) = (frame.size.0 * scale, frame.size.1 * scale);
    let device = &rs.device;
    let scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = Renderer::new(
        device,
        &rs.queue,
        floret_render::OFFSCREEN_FORMAT,
        samples(&rs.adapter, (w, h)),
    );
    let s = f64::from(scale);
    let layers = scaled(frame.layers, s, s);
    let (pixels, _) = floret_render::render_and_read(
        device,
        &rs.queue,
        &mut renderer,
        &layers,
        (w, h),
        frame.background,
    );
    if let Some(e) = pollster::block_on(validation.pop()) {
        return Err(e.to_string());
    }
    if let Some(e) = pollster::block_on(scope.pop()) {
        return Err(e.to_string());
    }
    Ok(pixels)
}

/// MSAA samples for an export renderer of `size` on this adapter: 4 when
/// supported and the frame isn't huge, else 1.
pub(crate) fn samples(adapter: &wgpu::Adapter, (w, h): (u32, u32)) -> u32 {
    let msaa = u64::from(w) * u64::from(h) <= MSAA_MAX_PIXELS
        && [
            floret_render::OFFSCREEN_FORMAT,
            floret_render::STENCIL_FORMAT,
        ]
        .iter()
        .all(|f| {
            adapter
                .get_texture_format_features(*f)
                .flags
                .sample_count_supported(4)
        });
    if msaa {
        4
    } else {
        1
    }
}

/// The scene scaled by `sx`, `sy`. Scene units are canvas pixels around a
/// centre origin, so scaling the whole scene about the origin is a
/// re-render at the bigger size.
pub(crate) fn scaled(layers: &[Arc<SceneNode>], sx: f64, sy: f64) -> Vec<Arc<SceneNode>> {
    if sx == 1.0 && sy == 1.0 {
        return layers.to_vec();
    }
    vec![Arc::new(SceneNode::Group {
        children: Arc::from(layers.to_vec()),
        transform: Some([sx, 0.0, 0.0, sy, 0.0, 0.0]),
        blend_mode: None,
        clip: None,
    })]
}

/// RGBA8 → PNG. The frame is opaque (the background is cleared opaque), so
/// premultiplied and straight alpha agree.
pub(crate) fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(rgba))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_follow_the_ts_slug() {
        assert_eq!(file_name("Preview", 1), "preview.png");
        assert_eq!(file_name("  Fountain — v2! ", 2), "fountain-v2@2x.png");
        assert_eq!(file_name("***", 4), "floret@4x.png");
    }

    #[test]
    fn tooltip_names_the_scales_and_shortcut() {
        let sc = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::E);
        let tip = tooltip(&sc, 2);
        assert!(tip.contains("1×, 2× or 4×") && tip.contains("+E"), "{tip}");
    }
}
