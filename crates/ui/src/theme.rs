//! JW365's palette (its styles.css semantic tokens) applied to egui. Views never name a colour;
//! they name a token here.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

pub const CANVAS: Color32 = Color32::from_rgb(0x0f, 0x11, 0x1a);
pub const SURFACE: Color32 = Color32::from_rgb(0x18, 0x1a, 0x26);
pub const CARD: Color32 = Color32::from_rgb(0x20, 0x24, 0x36);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x2a, 0x2f, 0x47);
pub const BORDER: Color32 = Color32::from_rgb(0x31, 0x36, 0x4f);
pub const FOCUS: Color32 = Color32::from_rgb(0x00, 0x78, 0xd4);
pub const TEXT: Color32 = Color32::from_rgb(0xf8, 0xfa, 0xfc);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x94, 0xa3, 0xb8);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x64, 0x74, 0x8b);
pub const BLUE: Color32 = Color32::from_rgb(0x00, 0x78, 0xd4);
pub const BLUE_HOVER: Color32 = Color32::from_rgb(0x10, 0x6e, 0xbe);
pub const BLUE_PRESSED: Color32 = Color32::from_rgb(0x00, 0x5a, 0x9e);
pub const GREEN: Color32 = Color32::from_rgb(0x10, 0xb9, 0x81);
pub const YELLOW: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x0b);
pub const RED: Color32 = Color32::from_rgb(0xef, 0x44, 0x44);
pub const INFO: Color32 = Color32::from_rgb(0x60, 0xa5, 0xfa);
pub const ERROR_TEXT: Color32 = Color32::from_rgb(0xf8, 0x71, 0x71);
pub const BRAND_BADGE_TEXT: Color32 = Color32::from_rgb(0x38, 0xbd, 0xf8);

/// Badge colours: (background, text).
pub const BADGE_TYPE: (Color32, Color32) = (Color32::from_rgba_unmultiplied_const(0x3b, 0x82, 0xf6, 0x22), Color32::from_rgb(0x60, 0xa5, 0xfa));
pub const BADGE_IDLE: (Color32, Color32) = (Color32::from_rgba_unmultiplied_const(0x64, 0x74, 0x8b, 0x22), TEXT_SECONDARY);
pub const BADGE_BUSY: (Color32, Color32) = (Color32::from_rgba_unmultiplied_const(0xf5, 0x9e, 0x0b, 0x22), Color32::from_rgb(0xfb, 0xbf, 0x24));
pub const BADGE_CONNECTED: (Color32, Color32) = (Color32::from_rgba_unmultiplied_const(0x10, 0xb9, 0x81, 0x22), Color32::from_rgb(0x34, 0xd3, 0x99));
pub const BADGE_FAILED: (Color32, Color32) = (Color32::from_rgba_unmultiplied_const(0xef, 0x44, 0x44, 0x22), ERROR_TEXT);
pub const BADGE_BRAND: (Color32, Color32) = (Color32::from_rgba_unmultiplied_const(0x00, 0x78, 0xd4, 0x22), BRAND_BADGE_TEXT);

pub const RADIUS: u8 = 6;
pub const CARD_RADIUS: u8 = 12;
pub const CONTROL_H: f32 = 32.0;

pub fn hairline() -> Stroke {
    Stroke::new(1.0, BORDER)
}

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = CANVAS;
    v.window_fill = SURFACE;
    v.extreme_bg_color = CARD;
    v.text_edit_bg_color = Some(CARD);
    v.faint_bg_color = SURFACE;
    v.code_bg_color = CARD;
    v.window_stroke = hairline();
    v.window_corner_radius = CornerRadius::same(CARD_RADIUS);
    v.menu_corner_radius = CornerRadius::same(RADIUS);
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = INFO;
    v.selection.bg_fill = BLUE;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.warn_fg_color = YELLOW;
    v.error_fg_color = ERROR_TEXT;
    for (w, fill, stroke, fg) in [
        (&mut v.widgets.noninteractive, SURFACE, BORDER, TEXT_SECONDARY),
        (&mut v.widgets.inactive, CARD, BORDER, TEXT),
        (&mut v.widgets.hovered, CARD_HOVER, TEXT_SECONDARY, TEXT),
        (&mut v.widgets.active, CARD_HOVER, FOCUS, TEXT),
        (&mut v.widgets.open, CARD_HOVER, FOCUS, TEXT),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0, stroke);
        w.corner_radius = CornerRadius::same(RADIUS);
        w.fg_stroke = Stroke::new(1.0, fg);
        w.expansion = 0.0;
    }
    ctx.set_visuals(v);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.icon_spacing = 8.0;
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = CONTROL_H;
        style.spacing.window_margin = egui::Margin::same(20);
        style.spacing.menu_margin = egui::Margin::same(6);
        style.text_styles = [
            (TextStyle::Heading, FontId::new(24.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
    });
}
