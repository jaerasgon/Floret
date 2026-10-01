//! Floret's design tokens (from the web app's `styles.css`, dark theme) and
//! the egui style built from them.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

pub const BG_0: Color32 = Color32::from_rgb(0x19, 0x19, 0x19); // canvas
pub const BG_1: Color32 = Color32::from_rgb(0x20, 0x20, 0x20); // panels, toolbar
pub const BG_2: Color32 = Color32::from_rgb(0x26, 0x26, 0x26); // cards, node faceplates
pub const BG_3: Color32 = Color32::from_rgb(0x2f, 0x2f, 0x2f); // hover / pressed
pub const BORDER: Color32 = Color32::from_rgb(0x34, 0x34, 0x34);
pub const FG_0: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xe4); // primary text
pub const FG_1: Color32 = Color32::from_rgb(0x9b, 0x9b, 0x98); // secondary
pub const FG_2: Color32 = Color32::from_rgb(0x6f, 0x6f, 0x6c); // tertiary
pub const FG_3: Color32 = Color32::from_rgb(0x4a, 0x4a, 0x48); // faintest
pub const GOLD: Color32 = Color32::from_rgb(0xf4, 0xc5, 0x42); // brand accent
pub const DANGER: Color32 = Color32::from_rgb(0xff, 0x7a, 0x7a);

/// Wire / pin colour per port type.
pub fn port_color(ty: &str) -> Color32 {
    let hex = match ty {
        "number" => 0xd8d4cc,
        "vec2" => 0xffd47c,
        "color" => 0xff7cc8,
        "layer" => 0x7cb6ff,
        "boolean" => 0xc9c97c,
        "style" => 0xd4a4ff,
        "audio" => 0xff9cd9,
        "image" => 0xf0b46e,
        "world" => 0x5fd3a8,
        "force" => 0xff9966,
        "collider" => 0xe0e0e0,
        "particle" => 0xb890ff,
        _ => 0x888c95,
    };
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Category accent: hues step 14° around the wheel (hsl(h, 58%, 63%)).
pub fn category_color(category: &str) -> Color32 {
    let hue = match category.to_ascii_lowercase().as_str() {
        "collider" => 20.0,
        "color" => 34.0,
        "convert" => 48.0,
        "data" => 62.0,
        "draw" => 76.0,
        "force" => 90.0,
        "image" => 104.0,
        "input" => 118.0,
        "io" => 132.0,
        "logic" => 146.0,
        "math" => 160.0,
        "meta" => 174.0,
        "motion" => 188.0,
        "particles" => 202.0,
        "pixel" => 216.0,
        "random" => 230.0,
        "region" => 244.0,
        "render" => 258.0,
        "sound" => 272.0,
        "spread" => 286.0,
        "state" => 300.0,
        "style" => 314.0,
        "time" => 328.0,
        "transform" => 342.0,
        _ => 349.0,
    };
    hsl(hue, 0.58, 0.63)
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

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
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
        v.dark_mode = true;
        v.panel_fill = BG_1;
        v.window_fill = BG_1;
        v.extreme_bg_color = BG_0;
        v.faint_bg_color = BG_2;
        v.code_bg_color = BG_2;
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.window_corner_radius = CornerRadius::same(9);
        v.menu_corner_radius = CornerRadius::same(6);
        v.hyperlink_color = GOLD;
        v.selection.bg_fill = GOLD.gamma_multiply(0.28);
        v.selection.stroke = Stroke::new(1.0, GOLD);
        v.override_text_color = Some(FG_0);

        let w = &mut v.widgets;
        let radius = CornerRadius::same(6);
        for (state, fill, fg) in [
            (&mut w.noninteractive, BG_1, FG_1),
            (&mut w.inactive, BG_2, FG_0),
            (&mut w.hovered, BG_3, FG_0),
            (&mut w.active, BG_3, GOLD),
            (&mut w.open, BG_3, FG_0),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.corner_radius = radius;
            state.fg_stroke = Stroke::new(1.0, fg);
            state.bg_stroke = Stroke::new(1.0, BORDER);
        }
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        w.hovered.bg_stroke = Stroke::new(1.0, FG_3);
    });
}
