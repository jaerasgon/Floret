//! Floret's design tokens (the web app's `styles.css`, dark and light
//! themes) and the egui styles built from them.
//!
//! Each token is a [`Token`] holding both themes' colours. It derefs to the
//! active theme's [`Color32`], so drawing code uses tokens like colours
//! (`painter.rect_filled(r, 0.0, BG_0)`, `FG_3.gamma_multiply(0.5)`) and
//! re-skins when the theme changes. Where a plain `Color32` value is needed,
//! write `*BG_0` or `BG_0.into()`.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};
use std::sync::atomic::{AtomicBool, Ordering};

static LIGHT: AtomicBool = AtomicBool::new(false);

/// Whether the light theme is active (set by [`follow`]).
pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

/// A colour token: its dark- and light-theme values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub dark: Color32,
    pub light: Color32,
}

impl Token {
    const fn new(dark: u32, light: u32) -> Token {
        Token {
            dark: hex(dark),
            light: hex(light),
        }
    }

    /// The active theme's colour.
    pub fn get(self) -> Color32 {
        *self
    }
}

impl std::ops::Deref for Token {
    type Target = Color32;
    fn deref(&self) -> &Color32 {
        if is_light() {
            &self.light
        } else {
            &self.dark
        }
    }
}

impl From<Token> for Color32 {
    fn from(t: Token) -> Color32 {
        *t
    }
}

impl PartialEq<Color32> for Token {
    fn eq(&self, other: &Color32) -> bool {
        **self == *other
    }
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

// Text tokens are tuned for WCAG contrast: FG_0 / FG_1 ≥ 4.5:1 on every
// background, FG_2 ≥ 4.5:1 on panels and cards (≥ 4:1 on the canvas).
// FG_3 is decoration only (grid dots, hairlines), never text.
pub const BG_0: Token = Token::new(0x191919, 0xeae8e1); // canvas
pub const BG_1: Token = Token::new(0x202020, 0xf6f5f2); // panels, toolbar
pub const BG_2: Token = Token::new(0x262626, 0xffffff); // cards, node faceplates
pub const BG_3: Token = Token::new(0x2f2f2f, 0xecebe6); // hover / pressed
pub const BORDER: Token = Token::new(0x343434, 0xdcdad3);
pub const FG_0: Token = Token::new(0xe6e6e4, 0x37352f); // primary text
pub const FG_1: Token = Token::new(0xa3a3a0, 0x5c5a54); // secondary
pub const FG_2: Token = Token::new(0x8f8f8b, 0x716f68); // tertiary: hints, units
pub const FG_3: Token = Token::new(0x555553, 0xb5b3ab); // faintest (not for text)
/// Brand accent for fills, rings and large marks. The light theme uses a
/// deeper gold so it still reads as a hairline on white.
pub const GOLD: Token = Token::new(0xf4c542, 0xa87c10);
/// The accent as small text (values, readouts, headings): the web app's
/// `--accent-dim` on the light theme.
pub const GOLD_TEXT: Token = Token::new(0xf4c542, 0x836006);
pub const DANGER: Token = Token::new(0xff7a7a, 0xc93c3c);

pub const AMBER: Token = Token::new(0xffc864, 0x8c5a00);
pub const BLUE: Token = Token::new(0x7cb6ff, 0x2f6db8);
/// Region frame accents (ForEach uses [`GOLD`] / [`GOLD_TEXT`]).
pub const REGION_IF: Token = AMBER;
pub const REGION_CACHE: Token = BLUE;
pub const REGION_SUBPATCH: Token = Token::new(0xbea0ff, 0x7a4fc4);
/// Comment notes: paper (tinted fill, dashed edge) and ink.
pub const COMMENT: Token = Token::new(0xffdc78, 0xb8901e);
pub const COMMENT_INK: Token = Token::new(0xf0d896, 0x5e4a12);
/// Probe cards: a dark glass panel on the dark theme, paper on the light.
pub const PROBE_EDGE: Token = Token::new(0x565c66, 0xa9adb4);

/// GLSL highlighting in the shader panel (comments use [`FG_2`],
/// built-ins [`GOLD_TEXT`]).
pub const SYNTAX_KEYWORD: Token = Token::new(0xc68af0, 0x8a3fc0);
pub const SYNTAX_TYPE: Token = Token::new(0x6cc4f0, 0x1f6fa0);
pub const SYNTAX_NUMBER: Token = Token::new(0xf0a86c, 0xa8541a);

/// Pins of spread ports.
pub const SPREAD_PIN: Token = Token::new(0xf0d800, 0xa08600);

/// Wire / pin colour per port type (light: darker, ≥ 3:1 on the canvas).
pub fn port_color(ty: &str) -> Color32 {
    let (dark, light) = match ty {
        "number" => (0xd8d4cc, 0x7a7568),
        "vec2" => (0xffd47c, 0x9a7316),
        "color" => (0xff7cc8, 0xc23f8e),
        "layer" => (0x7cb6ff, 0x3373c0),
        "boolean" => (0xc9c97c, 0x7a7a22),
        "style" => (0xd4a4ff, 0x9a5fd6),
        "audio" => (0xff9cd9, 0xc03f9e),
        "image" => (0xf0b46e, 0xa8661e),
        "world" => (0x5fd3a8, 0x1f8a62),
        "force" => (0xff9966, 0xc4532a),
        "collider" => (0xe0e0e0, 0x75757a),
        "particle" => (0xb890ff, 0x8a5fd6),
        _ => (0x888c95, 0x6b6f78),
    };
    *Token::new(dark, light)
}

/// Category accent: a few muted families of related categories rather
/// than a rainbow (low saturation; darker on the light theme so it reads
/// on white).
pub fn category_color(category: &str) -> Color32 {
    // (hue, saturation)
    let (hue, sat) = match category.to_ascii_lowercase().as_str() {
        // Drawing and output: warm sand.
        "draw" | "style" | "text" | "color" | "transform" | "render" => (38.0, 0.30),
        // Shape generators and operations sit with drawing.
        "geometry" => (38.0, 0.30),
        // Numbers and lists: slate blue.
        "math" | "vec2" | "logic" | "convert" | "data" | "spread" | "string" | "random" => {
            (215.0, 0.22)
        }
        // Time and change: sage.
        "time" | "motion" | "state" | "noise" => (140.0, 0.20),
        // Media and devices: dusty rose.
        "image" | "pixel" | "video" | "sound" | "input" | "io" | "gpu" => (345.0, 0.24),
        // Simulation: muted teal.
        "particles" | "force" | "collider" | "physics" => (180.0, 0.22),
        // Structure (regions, comments, probes): neutral.
        _ => (0.0, 0.0),
    };
    if is_light() {
        hsl(hue, sat, 0.38)
    } else {
        hsl(hue, sat, 0.66)
    }
}

fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f32| ((v + m) * 255.0).round() as u8;
    Color32::from_rgb(to(r), to(g), to(b))
}

/// Install both themes' styles and follow `pref` (dark / light / system).
/// egui's default fonts plus DejaVu Sans Mono (bundled with the engine)
/// as a last fallback, so arrows and other symbols in comments and labels
/// ("→", "×", "≤" …) draw instead of a box. Call once at startup.
pub fn install_fonts(ctx: &egui::Context) {
    const FALLBACK: &str = "dejavu-fallback";
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        FALLBACK.to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../../floret-engine/fonts/DejaVuSansMono.ttf"
        ))),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(FALLBACK.to_owned());
    }
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, pref: egui::ThemePreference) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |style| style_for(style, theme == egui::Theme::Light));
    }
    ctx.set_theme(pref);
    follow(ctx);
}

/// Point the tokens at the theme egui resolved (the OS theme for
/// "system"). Call once per frame.
pub fn follow(ctx: &egui::Context) {
    LIGHT.store(ctx.theme() == egui::Theme::Light, Ordering::Relaxed);
}

fn style_for(style: &mut egui::Style, light: bool) {
    let pick = |t: Token| if light { t.light } else { t.dark };
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(15.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.interact_size.y = 22.0;

    let v = &mut style.visuals;
    *v = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    v.panel_fill = pick(BG_1);
    v.window_fill = pick(BG_1);
    v.extreme_bg_color = pick(if light { BG_2 } else { BG_0 });
    v.faint_bg_color = pick(BG_2);
    v.code_bg_color = pick(BG_2);
    v.window_stroke = Stroke::new(1.0, pick(BORDER));
    v.window_corner_radius = CornerRadius::same(9);
    v.menu_corner_radius = CornerRadius::same(6);
    v.hyperlink_color = pick(GOLD_TEXT);
    v.selection.bg_fill = GOLD.dark.gamma_multiply(if light { 0.45 } else { 0.28 });
    v.selection.stroke = Stroke::new(1.0, pick(GOLD_TEXT));
    v.override_text_color = Some(pick(FG_0));
    if light {
        v.window_shadow.color = Color32::from_black_alpha(28);
        v.popup_shadow.color = Color32::from_black_alpha(22);
    }

    let w = &mut v.widgets;
    let radius = CornerRadius::same(6);
    for (state, fill, fg) in [
        (&mut w.noninteractive, BG_1, FG_1),
        (&mut w.inactive, BG_2, FG_0),
        (&mut w.hovered, BG_3, FG_0),
        (&mut w.active, BG_3, GOLD_TEXT),
        (&mut w.open, BG_3, FG_0),
    ] {
        state.bg_fill = pick(fill);
        state.weak_bg_fill = pick(fill);
        state.corner_radius = radius;
        state.fg_stroke = Stroke::new(1.0, pick(fg));
        state.bg_stroke = Stroke::new(1.0, pick(BORDER));
    }
    w.hovered.bg_stroke = Stroke::new(1.0, pick(FG_3));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_follow_the_active_theme() {
        // Other tests may draw; restore the default afterwards.
        LIGHT.store(true, Ordering::Relaxed);
        let light = (BG_0.get(), port_color("layer"));
        LIGHT.store(false, Ordering::Relaxed);
        let dark = (BG_0.get(), port_color("layer"));
        assert_eq!(light.0, Color32::from_rgb(0xea, 0xe8, 0xe1));
        assert_eq!(dark.0, Color32::from_rgb(0x19, 0x19, 0x19));
        assert_ne!(light.1, dark.1);
        assert_eq!(Color32::from(FG_0), FG_0.dark);
    }

    /// WCAG relative-luminance contrast ratio.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let lum = |c: Color32| {
            let f = |v: u8| {
                let v = f32::from(v) / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn text_tokens_are_legible_in_both_themes() {
        let themes: [fn(Token) -> Color32; 2] = [|t| t.dark, |t| t.light];
        for t in themes {
            for bg in [BG_0, BG_1, BG_2, BG_3] {
                for fg in [FG_0, FG_1] {
                    assert!(contrast(t(fg), t(bg)) >= 4.5, "{fg:?} on {bg:?}");
                }
                assert!(contrast(t(FG_2), t(bg)) >= 3.8, "FG_2 on {bg:?}");
            }
            for bg in [BG_1, BG_2] {
                assert!(contrast(t(FG_2), t(bg)) >= 4.5, "FG_2 on {bg:?}");
            }
            for fg in [GOLD_TEXT, REGION_IF, REGION_CACHE, REGION_SUBPATCH] {
                assert!(contrast(t(fg), t(BG_0)) >= 4.2, "{fg:?} on the canvas");
            }
            for fg in [SYNTAX_KEYWORD, SYNTAX_TYPE, SYNTAX_NUMBER] {
                assert!(contrast(t(fg), t(BG_2)) >= 4.5, "{fg:?}");
            }
        }
    }

    #[test]
    fn wires_stand_off_the_canvas() {
        let types = [
            "number", "vec2", "color", "layer", "boolean", "style", "audio", "image", "world",
            "force", "collider", "particle", "other",
        ];
        for light in [false, true] {
            LIGHT.store(light, Ordering::Relaxed);
            for ty in types {
                let c = contrast(port_color(ty), BG_0.get());
                assert!(c >= 3.0, "{ty} wire: {c:.2} (light: {light})");
            }
        }
        LIGHT.store(false, Ordering::Relaxed);
    }
}
