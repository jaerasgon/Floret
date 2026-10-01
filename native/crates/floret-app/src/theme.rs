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

pub const BG_0: Token = Token::new(0x191919, 0xeae8e1); // canvas
pub const BG_1: Token = Token::new(0x202020, 0xf6f5f2); // panels, toolbar
pub const BG_2: Token = Token::new(0x262626, 0xffffff); // cards, node faceplates
pub const BG_3: Token = Token::new(0x2f2f2f, 0xecebe6); // hover / pressed
pub const BORDER: Token = Token::new(0x343434, 0xdcdad3);
pub const FG_0: Token = Token::new(0xe6e6e4, 0x37352f); // primary text
pub const FG_1: Token = Token::new(0x9b9b98, 0x73716b); // secondary
pub const FG_2: Token = Token::new(0x6f6f6c, 0x9b9a93); // tertiary
pub const FG_3: Token = Token::new(0x4a4a48, 0xc4c2ba); // faintest
/// Brand accent. The light theme keeps the gold for fills and uses the
/// deeper `--accent-dim` where gold is text or a hairline on white.
pub const GOLD: Token = Token::new(0xf4c542, 0xc99a1e);
pub const DANGER: Token = Token::new(0xff7a7a, 0xd64545);

/// Wire / pin colour per port type (light: the darker re-tune).
pub fn port_color(ty: &str) -> Color32 {
    let (dark, light) = match ty {
        "number" => (0xd8d4cc, 0x7a7568),
        "vec2" => (0xffd47c, 0xc1962c),
        "color" => (0xff7cc8, 0xd24f9e),
        "layer" => (0x7cb6ff, 0x3d82d6),
        "boolean" => (0xc9c97c, 0x949430),
        "style" => (0xd4a4ff, 0x9a5fd6),
        "audio" => (0xff9cd9, 0xd24fae),
        "image" => (0xf0b46e, 0xcf8a3a),
        "world" => (0x5fd3a8, 0x2ea87a),
        "force" => (0xff9966, 0xd9663a),
        "collider" => (0xe0e0e0, 0x8a8a8a),
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
        // Numbers and lists: slate blue.
        "math" | "vec2" | "logic" | "convert" | "data" | "spread" | "string" | "random" => {
            (215.0, 0.22)
        }
        // Time and change: sage.
        "time" | "motion" | "state" | "noise" => (140.0, 0.20),
        // Media and devices: dusty rose.
        "image" | "pixel" | "sound" | "input" | "io" | "gpu" => (345.0, 0.24),
        // Simulation: muted teal.
        "particles" | "force" | "collider" | "physics" => (180.0, 0.22),
        // Structure (regions, comments, probes): neutral.
        _ => (0.0, 0.0),
    };
    if is_light() {
        hsl(hue, sat, 0.42)
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
    let gold = pick(GOLD);
    v.panel_fill = pick(BG_1);
    v.window_fill = pick(BG_1);
    v.extreme_bg_color = pick(if light { BG_2 } else { BG_0 });
    v.faint_bg_color = pick(BG_2);
    v.code_bg_color = pick(BG_2);
    v.window_stroke = Stroke::new(1.0, pick(BORDER));
    v.window_corner_radius = CornerRadius::same(9);
    v.menu_corner_radius = CornerRadius::same(6);
    v.hyperlink_color = gold;
    v.selection.bg_fill = GOLD.dark.gamma_multiply(if light { 0.45 } else { 0.28 });
    v.selection.stroke = Stroke::new(1.0, gold);
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
        (&mut w.active, BG_3, GOLD),
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
}
