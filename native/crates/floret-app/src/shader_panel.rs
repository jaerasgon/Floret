//! Shader panel: a GLSL editor for the selected `gpu/shader` node. A port
//! of `ShaderPanel.tsx` / `GlslEditor.tsx`, kept simple: tabs for the Image
//! pass and Buffer A–D, a monospace editor with line numbers and light
//! highlighting, iChannel routing, the detected uniforms and compile
//! errors with user line numbers.
//!
//! Edits are committed to the node's params 300 ms after the last
//! keystroke (as in TS), which recompiles the patch so uniform ports
//! follow the source. Errors come from `floret_render::shader::check`
//! (naga, no GPU) plus whatever the renderer reported for the node.

use crate::theme;
use eframe::egui::{self, text::LayoutJob, Color32, FontId, RichText, TextFormat};
use floret_engine::patch::PatchFile;
use floret_engine::shader::{parse_uniforms, uniform_port_type, ShaderBuffer};
use floret_render::ShaderError;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const SHADER: &str = "gpu/shader";
const DEBOUNCE: Duration = Duration::from_millis(300);
const DEFAULT_BUFFER: &str = "// Buffer pass. iChannel0 is this buffer's previous frame (self).
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
  vec2 uv = fragCoord / iResolution.xy;
  fragColor = texture(iChannel0, uv);
}";

/// Renderer-side errors: (node id, pass, errors).
pub type RuntimeErrors = [(Arc<str>, ShaderBuffer, Vec<ShaderError>)];

#[derive(Default)]
pub struct ShaderPanel {
    /// Node + pass the editor buffer belongs to.
    editing: Option<(String, ShaderBuffer)>,
    text: String,
    /// Last keystroke not yet committed.
    pending: Option<Instant>,
    checks: HashMap<String, Vec<ShaderError>>,
}

fn param_str(patch: &PatchFile, id: &str, key: &str) -> Option<String> {
    let n = patch.nodes.iter().find(|n| n.id == id)?;
    n.params.get(key)?.as_str().map(str::to_owned)
}

fn set_param(patch: &mut PatchFile, id: &str, key: &str, v: serde_json::Value) {
    if let Some(n) = patch.nodes.iter_mut().find(|n| n.id == id) {
        if let Some(pv) = n.port_values.as_mut().filter(|pv| pv.contains_key(key)) {
            pv.insert(key.into(), v.clone());
        }
        n.params.insert(key.into(), v);
    }
}

/// Buffers A–D with (non-blank) source.
fn present_buffers(patch: &PatchFile, id: &str) -> Vec<ShaderBuffer> {
    ShaderBuffer::BUFFERS
        .into_iter()
        .filter(|b| param_str(patch, id, b.param_key()).is_some_and(|s| !s.trim().is_empty()))
        .collect()
}

fn source_of(patch: &PatchFile, id: &str, buf: ShaderBuffer) -> String {
    param_str(patch, id, buf.param_key()).unwrap_or_else(|| match buf {
        ShaderBuffer::Image => floret_engine::nodes::gpu::DEFAULT_SOURCE.to_owned(),
        _ => String::new(),
    })
}

fn pass_name(b: ShaderBuffer) -> &'static str {
    match b {
        ShaderBuffer::Image => "Image",
        ShaderBuffer::A => "Buf A",
        ShaderBuffer::B => "Buf B",
        ShaderBuffer::C => "Buf C",
        ShaderBuffer::D => "Buf D",
    }
}

impl ShaderPanel {
    /// Compile errors for `source` (cached by source text).
    fn check(&mut self, source: &str) -> &[ShaderError] {
        if !self.checks.contains_key(source) {
            if self.checks.len() > 64 {
                self.checks.clear();
            }
            let errs = floret_render::shader::check(source)
                .err()
                .unwrap_or_default();
            self.checks.insert(source.to_owned(), errs);
        }
        &self.checks[source]
    }

    /// One line per failing pass of every shader node in the patch, for
    /// the status bar's warnings.
    pub fn warnings(&mut self, patch: &PatchFile, runtime: &RuntimeErrors) -> Vec<String> {
        let mut out = Vec::new();
        for n in patch.nodes.iter().filter(|n| n.def_type == SHADER) {
            let mut passes = present_buffers(patch, &n.id);
            passes.push(ShaderBuffer::Image);
            for b in passes {
                let src = source_of(patch, &n.id, b);
                let mut errs = self.check(&src).to_vec();
                if errs.is_empty() {
                    errs = runtime
                        .iter()
                        .find(|(id, buf, _)| **id == *n.id && *buf == b)
                        .map(|(_, _, e)| e.clone())
                        .unwrap_or_default();
                }
                if let Some(e) = errs.first() {
                    out.push(format!("Shader {} ({}): {e}", n.id, pass_name(b)));
                }
            }
        }
        out
    }

    /// The shader node `selected` names, if it is one.
    fn shader_id(patch: &PatchFile, selected: Option<&str>) -> Option<String> {
        selected
            .filter(|id| {
                patch
                    .nodes
                    .iter()
                    .any(|n| n.id == *id && n.def_type == SHADER)
            })
            .map(str::to_owned)
    }

    /// Whether `selected` is a shader node (the dock brings the Shader
    /// tab forward for it).
    pub fn is_shader(patch: &PatchFile, selected: Option<&str>) -> bool {
        Self::shader_id(patch, selected).is_some()
    }

    /// Every frame, shown or not: commit the pending edit once the
    /// debounce elapses or the selection moves off its node. Returns
    /// `true` when the patch was edited (the caller commits).
    pub fn tick(
        &mut self,
        ctx: &egui::Context,
        patch: &mut PatchFile,
        selected: Option<&str>,
    ) -> bool {
        let id = Self::shader_id(patch, selected);
        let mut edited = false;
        if self
            .editing
            .as_ref()
            .is_some_and(|(n, _)| Some(n) != id.as_ref())
        {
            edited |= self.flush(patch);
            self.editing = None;
        }
        if self.pending.is_some_and(|t| t.elapsed() >= DEBOUNCE) {
            edited |= self.flush(patch);
        } else if self.pending.is_some() {
            ctx.request_repaint_after(DEBOUNCE);
        }
        edited
    }

    /// The editor for `selected` inside a dock tab (a hint when it isn't a
    /// shader node). Returns `true` when the patch was edited.
    pub fn show_in(
        &mut self,
        ui: &mut egui::Ui,
        patch: &mut PatchFile,
        selected: Option<&str>,
        runtime: &RuntimeErrors,
    ) -> bool {
        let Some(id) = Self::shader_id(patch, selected) else {
            theme::empty_state(
                ui,
                "No shader selected",
                "Select a GPU Shader node to edit its GLSL",
            );
            return false;
        };
        let mut edited = false;
        if self.editing.as_ref().is_none_or(|(n, _)| *n != id) {
            edited |= self.flush(patch);
            self.load(patch, &id, ShaderBuffer::Image);
        }
        edited |= self.contents(ui, patch, &id, runtime);
        edited
    }

    fn load(&mut self, patch: &PatchFile, id: &str, tab: ShaderBuffer) {
        self.text = source_of(patch, id, tab);
        self.editing = Some((id.to_owned(), tab));
        self.pending = None;
    }

    /// Commit a pending edit to the node's params.
    fn flush(&mut self, patch: &mut PatchFile) -> bool {
        let (Some(_), Some((id, tab))) = (self.pending.take(), &self.editing) else {
            return false;
        };
        if param_str(patch, id, tab.param_key()).as_deref() == Some(&self.text) {
            return false;
        }
        set_param(patch, id, tab.param_key(), self.text.clone().into());
        true
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        patch: &mut PatchFile,
        id: &str,
        runtime: &RuntimeErrors,
    ) -> bool {
        let mut edited = false;
        let present = present_buffers(patch, id);
        let mut tabs = vec![ShaderBuffer::Image];
        tabs.extend(&present);
        let tab = self
            .editing
            .as_ref()
            .map_or(ShaderBuffer::Image, |(_, t)| *t);

        // Tabs: Image, present buffers, add / remove.
        ui.horizontal(|ui| {
            for t in &tabs {
                if ui.selectable_label(*t == tab, pass_name(*t)).clicked() && *t != tab {
                    edited |= self.flush(patch);
                    self.load(patch, id, *t);
                }
            }
            if let Some(free) = ShaderBuffer::BUFFERS
                .into_iter()
                .find(|b| !present.contains(b))
            {
                if ui
                    .small_button("+")
                    .on_hover_text("Add a buffer pass")
                    .clicked()
                {
                    self.flush(patch);
                    set_param(patch, id, free.param_key(), DEFAULT_BUFFER.into());
                    self.load(patch, id, free);
                    edited = true;
                }
            }
            if tab != ShaderBuffer::Image
                && ui
                    .small_button("🗑")
                    .on_hover_text(format!("Remove {}", pass_name(tab)))
                    .clicked()
            {
                self.pending = None;
                set_param(patch, id, tab.param_key(), "".into());
                if let Some(n) = patch.nodes.iter_mut().find(|n| n.id == id) {
                    if let Some(serde_json::Value::Object(ch)) = n.params.get_mut("channels") {
                        ch.remove(tab.as_str());
                    }
                }
                self.load(patch, id, ShaderBuffer::Image);
                edited = true;
            }
        });
        let tab = self
            .editing
            .as_ref()
            .map_or(ShaderBuffer::Image, |(_, t)| *t);

        // Errors for the editor text (as typed), else the renderer's.
        let text = self.text.clone();
        let mut errors = self.check(&text).to_vec();
        if errors.is_empty() && self.pending.is_none() {
            errors = runtime
                .iter()
                .find(|(n, b, _)| &**n == id && *b == tab)
                .map(|(_, _, e)| e.clone())
                .unwrap_or_default();
        }

        egui::ScrollArea::both()
            .id_salt("shader-src")
            .max_height(340.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.editor(ui, &errors) {
                    self.pending = Some(Instant::now());
                }
            });

        if !errors.is_empty() {
            ui.add_space(4.0);
            for e in errors.iter().take(6) {
                ui.label(
                    RichText::new(e.to_string())
                        .color(theme::DANGER)
                        .monospace()
                        .size(11.0),
                );
            }
        }
        ui.separator();
        edited |= self.channels(ui, patch, id, tab, &tabs);
        ui.add_space(4.0);
        ui.label(
            RichText::new("Uniforms -> ports")
                .color(theme::FG_1)
                .small(),
        );
        let uniforms: Vec<_> = parse_uniforms(&self.text)
            .into_iter()
            .filter(|u| &*u.glsl_type != "sampler2D")
            .collect();
        if uniforms.is_empty() {
            ui.label(
                RichText::new("none — declare e.g. uniform float k;")
                    .color(theme::FG_2)
                    .small(),
            );
        }
        for u in uniforms {
            ui.label(
                RichText::new(format!(
                    "{}  {} -> {:?}",
                    u.name,
                    u.glsl_type,
                    uniform_port_type(&u.glsl_type)
                ))
                .monospace()
                .size(11.0),
            );
        }
        ui.label(
            RichText::new("built-in: iTime · iTimeDelta · iResolution · iMouse · iFrame")
                .color(theme::FG_2)
                .small(),
        );
        edited
    }

    /// The code editor with a line-number gutter. Returns `true` on edit.
    fn editor(&mut self, ui: &mut egui::Ui, errors: &[ShaderError]) -> bool {
        let font = FontId::monospace(12.0);
        let lines = self.text.split('\n').count().max(1);
        let bad: Vec<u32> = errors.iter().map(|e| e.line).collect();
        let mut changed = false;
        ui.horizontal_top(|ui| {
            let mut gutter = LayoutJob::default();
            for i in 1..=lines {
                let color = if bad.contains(&(i as u32)) {
                    theme::DANGER
                } else {
                    theme::FG_2
                };
                let sep = if i < lines { "\n" } else { "" };
                gutter.append(
                    &format!("{i:>3}{sep}"),
                    0.0,
                    TextFormat::simple(font.clone(), *color),
                );
            }
            // Match the TextEdit's inner margin so lines align.
            ui.add_space(2.0);
            ui.vertical(|ui| {
                ui.add_space(2.0);
                ui.label(gutter);
            });
            // No wrapping: the gutter numbers source lines.
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| {
                let mut job = highlight(text.as_str(), &font);
                job.wrap.max_width = f32::INFINITY;
                ui.fonts_mut(|f| f.layout_job(job))
            };
            let r = ui.add(
                egui::TextEdit::multiline(&mut self.text)
                    .code_editor()
                    .font(FontId::monospace(12.0))
                    .desired_width(f32::INFINITY)
                    .desired_rows(22)
                    .lock_focus(true)
                    .layouter(&mut layouter),
            );
            changed = r.changed();
        });
        changed
    }

    /// iChannel routing for the active pass (`channels` param).
    fn channels(
        &mut self,
        ui: &mut egui::Ui,
        patch: &mut PatchFile,
        id: &str,
        tab: ShaderBuffer,
        tabs: &[ShaderBuffer],
    ) -> bool {
        let node = patch.nodes.iter().find(|n| n.id == id);
        let cfg = node.and_then(|n| n.params.get("channels")).cloned();
        let feedback = node
            .and_then(|n| n.params.get("feedback"))
            .is_some_and(|v| v.as_bool().unwrap_or(false));
        let current: Vec<String> = match cfg.as_ref().and_then(|c| c.get(tab.as_str())) {
            Some(serde_json::Value::Array(a)) => (0..4)
                .map(|i| a.get(i).and_then(|v| v.as_str()).unwrap_or("").to_owned())
                .collect(),
            _ if tab == ShaderBuffer::Image => {
                let fb = if feedback { "self" } else { "" };
                vec!["input".into(), fb.into(), String::new(), String::new()]
            }
            _ => vec!["self".into(), "input".into(), String::new(), String::new()],
        };
        let shown = |code: &str| {
            if code == tab.as_str() {
                "self".to_owned()
            } else {
                code.to_owned()
            }
        };
        let mut options: Vec<(String, String)> = vec![
            (String::new(), "none".into()),
            ("input".into(), "input".into()),
            ("self".into(), "self".into()),
        ];
        for t in tabs.iter().filter(|t| **t != tab) {
            let label = match t {
                ShaderBuffer::Image => "Image".to_owned(),
                b => format!("Buffer {}", b.as_str()),
            };
            options.push((t.as_str().to_owned(), label));
        }
        let mut next = None;
        ui.label(
            RichText::new("iChannels (this pass)")
                .color(theme::FG_1)
                .small(),
        );
        egui::Grid::new("shader-channels").show(ui, |ui| {
            for (i, code) in current.iter().enumerate() {
                ui.label(RichText::new(format!("iChannel{i}")).monospace().size(11.0));
                let value = shown(code);
                let label = options
                    .iter()
                    .find(|(v, _)| *v == value)
                    .map_or(value.clone(), |(_, l)| l.clone());
                egui::ComboBox::from_id_salt(("shader-ch", i))
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for (v, l) in &options {
                            if ui.selectable_label(*v == value, l).clicked() && *v != value {
                                next = Some((i, v.clone()));
                            }
                        }
                    });
                ui.end_row();
            }
        });
        let Some((i, code)) = next else {
            return false;
        };
        let mut arr = current;
        arr[i] = code;
        let mut obj = match cfg {
            Some(serde_json::Value::Object(m)) => m,
            _ => serde_json::Map::new(),
        };
        obj.insert(tab.as_str().into(), arr.into());
        set_param(patch, id, "channels", obj.into());
        true
    }
}

const KEYWORDS: &[&str] = &[
    "if",
    "else",
    "for",
    "while",
    "do",
    "return",
    "break",
    "continue",
    "discard",
    "in",
    "out",
    "inout",
    "uniform",
    "const",
    "struct",
    "precision",
    "highp",
    "mediump",
    "lowp",
    "true",
    "false",
];
const TYPES: &[&str] = &[
    "void",
    "bool",
    "int",
    "uint",
    "float",
    "vec2",
    "vec3",
    "vec4",
    "ivec2",
    "ivec3",
    "ivec4",
    "uvec2",
    "uvec3",
    "uvec4",
    "bvec2",
    "bvec3",
    "bvec4",
    "mat2",
    "mat3",
    "mat4",
    "sampler2D",
];

/// Light GLSL highlighting: comments, keywords, types, built-ins, numbers.
fn highlight(text: &str, font: &FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    let fmt = |c: Color32| TextFormat::simple(font.clone(), c);
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let color = if text[i..].starts_with("//") {
            i = text[i..].find('\n').map_or(b.len(), |n| i + n);
            *theme::FG_2
        } else if text[i..].starts_with("/*") {
            i = text[i + 2..].find("*/").map_or(b.len(), |n| i + n + 4);
            *theme::FG_2
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let w = &text[start..i];
            if KEYWORDS.contains(&w) {
                *theme::SYNTAX_KEYWORD
            } else if TYPES.contains(&w) {
                *theme::SYNTAX_TYPE
            } else if w.starts_with('i') && floret_engine::shader::BUILTIN_UNIFORMS.contains(&w)
                || w == "mainImage"
            {
                *theme::GOLD_TEXT
            } else {
                *theme::FG_0
            }
        } else if b[i].is_ascii_digit()
            || (b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.') {
                i += 1;
            }
            *theme::SYNTAX_NUMBER
        } else {
            // One char (UTF-8 aware).
            i += text[i..].chars().next().map_or(1, char::len_utf8);
            *theme::FG_1
        };
        job.append(&text[start..i], 0.0, fmt(color));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_covers_the_text() {
        let src = "uniform float k; // note é\n/* x */ void mainImage() { c = 1.5 * iTime; }";
        let job = highlight(src, &FontId::monospace(12.0));
        assert_eq!(job.text, src);
    }

    #[test]
    fn edits_commit_to_params() {
        let text = include_str!("../../../samples/shader-plasma.flora");
        let mut patch = floret_engine::load_patch(text).unwrap();
        let mut p = ShaderPanel::default();
        p.load(&patch, "sh_shader", ShaderBuffer::Image);
        assert!(p.text.starts_with("uniform float scale;"));
        p.text = "uniform float zoom;\n".to_owned() + &p.text;
        assert!(!p.flush(&mut patch), "nothing pending yet");
        p.pending = Some(Instant::now());
        assert!(p.flush(&mut patch));
        assert!(param_str(&patch, "sh_shader", "source")
            .unwrap()
            .starts_with("uniform float zoom;"));
        let w = p.warnings(&patch, &[]);
        assert!(w.is_empty(), "{w:?}");
        set_param(
            &mut patch,
            "sh_shader",
            "source",
            "void mainImage(out vec4 c, in vec2 f) { c = nope; }".into(),
        );
        let w = p.warnings(&patch, &[]);
        assert_eq!(w.len(), 1);
        assert!(w[0].starts_with("Shader sh_shader (Image): L1:"), "{w:?}");
    }
}
