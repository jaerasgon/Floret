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
/// Text on a [`GOLD`] fill (the primary button): near-black in both
/// themes, ≥ 4.5:1 on either gold.
pub const ON_GOLD: Token = Token::new(0x191919, 0x14120d);
/// The primary (Export) button's fill: the brand gold; on the light
/// theme a lighter, cleaner gold than [`GOLD`]'s deep hairline tone.
pub const GOLD_FILL: Token = Token::new(0xf4c542, 0xf3c94f);
/// [`GOLD_FILL`] under the pointer: a touch lighter on dark, a touch
/// deeper on light.
pub const GOLD_FILL_HOVER: Token = Token::new(0xf8d470, 0xe9b934);
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

/// The curve editor's channel curves (red, green, blue) on its graph.
pub const CURVE_RED: Token = Token::new(0xff7a7a, 0xc93c3c);
pub const CURVE_GREEN: Token = Token::new(0x6fd88a, 0x2e8a4a);
pub const CURVE_BLUE: Token = Token::new(0x7ca8ff, 0x2f6db8);

/// The probe card's readout accent (its count and scrollbar). Spread pins
/// take their type's colour; their name carries a spread icon.
pub const SPREAD_PIN: Token = Token::new(0xf0d800, 0xa08600);

/// Wire / pin colour per port type (light: darker, ≥ 3:1 on the canvas).
pub fn port_color(ty: &str) -> Color32 {
    *port_token(ty)
}

/// [`port_color`] in both themes.
fn port_token(ty: &str) -> Token {
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
    Token::new(dark, light)
}

/// Accent for a node browser group (or, for catalogues without groups, a
/// category): a few muted families of related groups rather than a
/// rainbow (low saturation; darker on the light theme so it reads on
/// white).
pub fn category_color(group: &str) -> Color32 {
    // (hue, saturation)
    let (hue, sat) = match group.to_ascii_lowercase().as_str() {
        // Drawing and how it looks: warm sand.
        "draw" | "text" | "color & style" | "shape ops" => (38.0, 0.30),
        // Numbers, lists and the patterns built from them: slate blue.
        "math" | "lists" | "patterns" => (215.0, 0.22),
        // Time and variation: sage.
        "animate" | "noise & random" => (140.0, 0.20),
        // Media in, out and through: dusty rose.
        "source" | "image" | "effects" | "input & sound" => (345.0, 0.24),
        // Simulation: muted teal.
        "particles & physics" => (180.0, 0.22),
        // Categories (catalogues without groups).
        "style" | "color" | "transform" | "render" | "geometry" => (38.0, 0.30),
        "vec2" | "logic" | "convert" | "data" | "spread" | "string" | "random" => (215.0, 0.22),
        "time" | "motion" | "state" | "noise" => (140.0, 0.20),
        "pixel" | "video" | "sound" | "input" | "io" | "gpu" => (345.0, 0.24),
        "particles" | "force" | "collider" | "physics" => (180.0, 0.22),
        // Structure (regions, state, comments, probes): neutral.
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

/// The interface fonts. Proportional text is Inter (bundled, OFL), with
/// the Phosphor icon font right behind it so icons mix into labels, then
/// egui's defaults (emoji) and DejaVu Sans Mono (bundled with the engine)
/// as a last fallback, so arrows and other symbols in comments and labels
/// ("→", "×", "≤" …) draw instead of a box. Monospace stays egui's Hack.
/// [`MEDIUM`] is Inter Medium, for titles and emphasis. Call once at
/// startup.
pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
}

/// The font family for titles and emphasis: Inter Medium, with the same
/// fallbacks as proportional text.
pub const MEDIUM: &str = "medium";

/// Inter Medium at `size`.
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

/// The Phosphor icon font's family (icons only, no text).
const ICONS: &str = "icons";

/// A Phosphor icon (`egui_phosphor::regular::*`) at `size`, in `color`.
/// Put text beside it as a separate atom or label: this family has no
/// letters.
pub fn icon(glyph: &str, size: f32, color: impl Into<Color32>) -> egui::RichText {
    egui::RichText::new(glyph)
        .family(FontFamily::Name(ICONS.into()))
        .size(size)
        .color(color)
}

/// The Phosphor icon font at `size`, for painting an icon directly.
pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS.into()))
}

/// The fonts [`install_fonts`] installs (tests measure text with them).
pub fn font_definitions() -> egui::FontDefinitions {
    const INTER: &str = "inter";
    const FALLBACK: &str = "dejavu-fallback";
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        (
            INTER,
            &include_bytes!("../assets/fonts/Inter-Regular.ttf")[..],
        ),
        (
            MEDIUM,
            &include_bytes!("../assets/fonts/Inter-Medium.ttf")[..],
        ),
        (
            FALLBACK,
            &include_bytes!("../../floret-engine/fonts/DejaVuSansMono.ttf")[..],
        ),
    ] {
        fonts.font_data.insert(
            name.to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(bytes)),
        );
    }
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, INTER.to_owned());
    // Icons second: behind Inter, ahead of egui's emoji fonts.
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    // Inter maps glyphs of its own to hundreds of the private-use
    // codepoints Phosphor uses, so icons are drawn in a family with
    // Phosphor first ([`icon`]); only Phosphor, as its blank a–z would
    // hide any text.
    fonts
        .families
        .insert(FontFamily::Name(ICONS.into()), vec!["phosphor".to_owned()]);
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(FALLBACK.to_owned());
    }
    let mut medium = fonts.families[&FontFamily::Proportional].clone();
    medium[0] = MEDIUM.to_owned();
    fonts
        .families
        .insert(FontFamily::Name(MEDIUM.into()), medium);
    fonts
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
            FontId::new(10.5, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(12.5, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(12.5, FontFamily::Proportional),
        ),
        (TextStyle::Heading, medium(14.0)),
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

/// A panel's empty state, filling the rest of `ui`: a short title
/// ([`FG_1`], body) over a one-line hint ([`FG_2`], small), centred and
/// wrapped to fit narrow panels.
pub fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str) {
    let width = ui.available_width();
    let mut height = ui.available_height();
    if !height.is_finite() || height > 10_000.0 {
        // Inside a scroll area: what's left of the visible part.
        height = ui.clip_rect().bottom() - ui.cursor().top();
    }
    let (t, h) = empty_state_galleys(ui, title, hint, width);
    let need = t.size().y + EMPTY_GAP + h.size().y + 24.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, height.max(need)), egui::Sense::hover());
    paint_empty_state(ui, rect, title, hint);
}

const EMPTY_GAP: f32 = 3.0;

/// [`empty_state`] painted centred in `rect` (for views that draw their
/// own background).
pub fn paint_empty_state(ui: &egui::Ui, rect: egui::Rect, title: &str, hint: &str) {
    let (t, h) = empty_state_galleys(ui, title, hint, rect.width());
    let total = t.size().y + EMPTY_GAP + h.size().y;
    let top = rect.center().y - total / 2.0;
    let x = rect.center().x;
    let painter = ui.painter();
    let hint_top = top + t.size().y + EMPTY_GAP;
    painter.galley(egui::pos2(x, top), t, *FG_1);
    painter.galley(egui::pos2(x, hint_top), h, *FG_2);
}

/// What an empty canvas says (title, hint): the same words on the Canvas
/// and in the Preview.
pub const EMPTY_CANVAS: (&str, &str) = (
    "Empty canvas",
    "Double-click or press Tab to add a node, or drag one in from the Nodes panel. \
     Open the Library for samples.",
);

fn empty_state_galleys(
    ui: &egui::Ui,
    title: &str,
    hint: &str,
    width: f32,
) -> (std::sync::Arc<egui::Galley>, std::sync::Arc<egui::Galley>) {
    let wrap = (width - 24.0).clamp(60.0, 300.0);
    let layout = |text: &str, style: TextStyle, color: Color32| {
        let mut job =
            egui::text::LayoutJob::simple(text.to_owned(), style.resolve(ui.style()), color, wrap);
        job.halign = egui::Align::Center;
        ui.painter().layout_job(job)
    };
    (
        layout(title, TextStyle::Body, *FG_1),
        layout(hint, TextStyle::Small, *FG_2),
    )
}

/// Floret's sunflower mark (`macos/icon.svg`): 13 petals around a disc,
/// `size` points across.
pub fn paint_logo(painter: &egui::Painter, center: egui::Pos2, size: f32, color: Color32) {
    use std::f32::consts::TAU;
    // The mark's own units: a 24-unit box, petals 1.5 × 3.5 centred 7.7
    // out, a disc of radius 4.
    let s = size / 22.4;
    for i in 0..13 {
        let (sin, cos) = (i as f32 * TAU / 13.0).sin_cos();
        let petal = (0..20)
            .map(|k| {
                let (ps, pc) = (k as f32 * TAU / 20.0).sin_cos();
                let (x, y) = (1.5 * pc, -7.7 + 3.5 * ps);
                center + egui::vec2(x * cos - y * sin, x * sin + y * cos) * s
            })
            .collect();
        painter.add(egui::Shape::convex_polygon(petal, color, Stroke::NONE));
    }
    painter.circle_filled(center, 4.0 * s, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_group_but_structure_has_a_family_accent() {
        let neutral = category_color("no such group");
        for g in floret_engine::spec::GROUPS {
            let c = category_color(g.name);
            assert_eq!(c == neutral, g.name == "Structure", "{}", g.name);
        }
        // Groups share the families their old categories had.
        assert_eq!(category_color("Draw"), category_color("draw"));
        assert_eq!(category_color("Math"), category_color("spread"));
        assert_eq!(category_color("Animate"), category_color("time"));
        assert_eq!(category_color("Effects"), category_color("image"));
        assert_eq!(
            category_color("Particles & Physics"),
            category_color("force")
        );
    }

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

    /// `over` (premultiplied, possibly translucent) composited on `bg`.
    fn blend(over: Color32, bg: Color32) -> Color32 {
        let a = f32::from(over.a()) / 255.0;
        let mix = |o: u8, b: u8| (f32::from(o) + f32::from(b) * (1.0 - a)).round() as u8;
        Color32::from_rgb(
            mix(over.r(), bg.r()),
            mix(over.g(), bg.g()),
            mix(over.b(), bg.b()),
        )
    }

    #[test]
    fn chrome_text_is_legible_in_both_themes() {
        for light in [false, true] {
            let t = |tok: Token| if light { tok.light } else { tok.dark };
            let mut style = egui::Style::default();
            style_for(&mut style, light);
            // The primary (Export) button: dark text on gold.
            for fill in [GOLD, GOLD_FILL, GOLD_FILL_HOVER] {
                let c = contrast(t(ON_GOLD), t(fill));
                assert!(c >= 4.5, "text on the gold button: {c:.2} (light: {light})");
            }
            // The hover tint is visibly different from the resting fill,
            // and the light theme's button is lighter than its hairline
            // gold.
            assert_ne!(t(GOLD_FILL), t(GOLD_FILL_HOVER));
            if light {
                let lum = |c: Color32| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b());
                assert!(lum(t(GOLD_FILL)) > lum(t(GOLD)));
            }
            // Dock tab titles: active / focused on the tab fill, inactive
            // and hovered on the tab bar.
            for (fg, bg) in [(FG_1, BG_2), (FG_0, BG_2), (FG_2, BG_1), (FG_1, BG_1)] {
                let c = contrast(t(fg), t(bg));
                assert!(c >= 4.5, "tab title {fg:?} on {bg:?}: {c:.2}");
            }
            // Toolbar toggles: the gold icon on the selected fill (an
            // icon, so WCAG's 3:1 for graphics), and the selected text
            // colour egui uses for selected buttons.
            let selected = blend(style.visuals.selection.bg_fill, t(BG_1));
            let c = contrast(t(GOLD_TEXT), selected);
            assert!(c >= 3.0, "gold icon on the selected fill: {c:.2}");
            assert_eq!(style.visuals.selection.stroke.color, t(GOLD_TEXT));
            // Empty states and chips: FG_1 titles, FG_2 hints and chips on
            // panels; the preview's empty state on the canvas colour.
            for bg in [BG_1, BG_2] {
                assert!(contrast(t(FG_1), t(bg)) >= 4.5);
                assert!(contrast(t(FG_2), t(bg)) >= 4.5);
            }
            assert!(contrast(t(FG_1), t(BG_0)) >= 4.5);
            assert!(contrast(t(FG_2), t(BG_0)) >= 3.8);
        }
    }

    #[test]
    fn fonts_include_inter_icons_and_fallbacks() {
        let fonts = font_definitions();
        let prop = &fonts.families[&FontFamily::Proportional];
        assert_eq!(prop[0], "inter");
        assert!(prop.contains(&"phosphor".to_owned()));
        assert_eq!(prop.last().map(String::as_str), Some("dejavu-fallback"));
        assert_eq!(fonts.families[&FontFamily::Monospace][0], "Hack");
        assert_eq!(fonts.families[&FontFamily::Name(MEDIUM.into())][0], MEDIUM);
        assert_eq!(
            fonts.families[&FontFamily::Name(ICONS.into())],
            vec!["phosphor".to_owned()]
        );
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

    /// Node cards: the title and row labels on the flat card, and the
    /// pins (an unwired pin is a ring in its type colour on the card).
    #[test]
    fn node_cards_are_legible_in_both_themes() {
        let types = [
            "number", "vec2", "color", "layer", "boolean", "style", "audio", "image", "world",
            "force", "collider", "particle", "other",
        ];
        let themes: [fn(Token) -> Color32; 2] = [|t| t.dark, |t| t.light];
        for t in themes {
            let card = t(BG_2);
            for fg in [FG_0, FG_1, GOLD_TEXT] {
                let c = contrast(t(fg), card);
                assert!(c >= 4.5, "{fg:?} on a node: {c:.2}");
            }
            for ty in types {
                let c = contrast(t(port_token(ty)), card);
                assert!(c >= 3.0, "{ty} pin: {c:.2}");
            }
            let c = contrast(t(SPREAD_PIN), card);
            assert!(c >= 3.0, "probe accent: {c:.2}");
            // The lit bypass icon on a (dimmed) card, and the curve
            // editor's channel curves on its graph (the canvas colour).
            let dimmed = blend(t(BG_2).gamma_multiply(0.6), card);
            let c = contrast(t(AMBER), dimmed);
            assert!(c >= 3.0, "bypass icon: {c:.2}");
            for ch in [CURVE_RED, CURVE_GREEN, CURVE_BLUE] {
                let c = contrast(t(ch), t(BG_0));
                assert!(c >= 3.0, "{ch:?} curve: {c:.2}");
            }
        }
    }
}
