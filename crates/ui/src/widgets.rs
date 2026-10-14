//! The few shared controls every view is built from.

use crate::theme::{self as t};
use egui::{Color32, CornerRadius, CursorIcon, FontId, Response, Sense, Stroke, StrokeKind, Ui, vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
}

pub fn button(ui: &mut Ui, text: &str, kind: Kind, min_width: f32) -> Response {
    button_sized(ui, text, kind, min_width, t::CONTROL_H, 13.0)
}

pub fn button_sized(ui: &mut Ui, text: &str, kind: Kind, min_width: f32, height: f32, font: f32) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(font), Color32::WHITE);
    let size = vec2((galley.size().x + 28.0).max(min_width), height);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let (hovered, pressed) = (resp.hovered(), resp.is_pointer_button_down_on());
    let (fill, stroke, fg) = match kind {
        Kind::Primary => (if pressed { t::BLUE_PRESSED } else if hovered { t::BLUE_HOVER } else { t::BLUE }, Color32::TRANSPARENT, Color32::WHITE),
        Kind::Secondary => (if hovered { t::CARD_HOVER } else { t::CARD }, if hovered { t::TEXT_SECONDARY } else { t::BORDER }, t::TEXT),
        Kind::Ghost => (if hovered { t::CARD } else { Color32::TRANSPARENT }, Color32::TRANSPARENT, if hovered { t::TEXT } else { t::TEXT_SECONDARY }),
    };
    ui.painter().rect(rect, CornerRadius::same(t::RADIUS), fill, Stroke::new(1.0, stroke), StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn badge(ui: &mut Ui, text: &str, colors: (Color32, Color32)) {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(10.0), colors.1);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(12.0, 6.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(4), colors.0, Stroke::NONE, StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, colors.1);
}

pub fn pill(ui: &mut Ui, text: &str) {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(12.0), t::TEXT);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(24.0, 10.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(16), t::CARD, t::hairline(), StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, t::TEXT);
}

pub fn text(ui: &mut Ui, s: &str, size: f32, color: Color32) {
    ui.label(egui::RichText::new(s).size(size).color(color));
}

pub fn strong(ui: &mut Ui, s: &str, size: f32, color: Color32) {
    ui.label(egui::RichText::new(s).size(size).strong().color(color));
}

/// A monitor glyph standing in for the resource icon.
pub fn monitor_glyph(ui: &mut Ui, side: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(10), t::SURFACE, t::hairline(), StrokeKind::Inside);
    let screen = egui::Rect::from_center_size(rect.center() - vec2(0.0, side * 0.08), vec2(side * 0.56, side * 0.38));
    p.rect(screen, CornerRadius::same(3), t::BLUE, Stroke::new(1.5, t::INFO), StrokeKind::Inside);
    let stand_top = screen.center_bottom();
    p.line_segment([stand_top, stand_top + vec2(0.0, side * 0.1)], Stroke::new(2.0, t::TEXT_SECONDARY));
    p.line_segment([stand_top + vec2(-side * 0.14, side * 0.1), stand_top + vec2(side * 0.14, side * 0.1)], Stroke::new(2.0, t::TEXT_SECONDARY));
}
