//! Palette, spacing scale, and the global egui theme.
//!
//! The whole GUI draws from these constants so the look stays uniform; nothing
//! here knows about contracts or editing.

use eframe::egui;
use egui::{Color32, RichText, Stroke};

// Terminal-green identity, toned down for a calmer, cleaner surface.
pub(crate) const BG: Color32 = Color32::from_rgb(15, 20, 17);
pub(crate) const PANEL_BG: Color32 = Color32::from_rgb(20, 27, 23);
pub(crate) const BORDER: Color32 = Color32::from_rgb(45, 66, 54);
pub(crate) const GREEN: Color32 = Color32::from_rgb(78, 201, 138);
pub(crate) const CYAN: Color32 = Color32::from_rgb(120, 190, 230);
pub(crate) const DIM: Color32 = Color32::from_rgb(128, 150, 138);
pub(crate) const TEXT: Color32 = Color32::from_rgb(205, 225, 213);
pub(crate) const RED: Color32 = Color32::from_rgb(230, 110, 110);
pub(crate) const AMBER: Color32 = Color32::from_rgb(224, 176, 60);

/// Chip backgrounds for the REQUIREMENT column.
pub(crate) const CHIP_BG: Color32 = Color32::from_rgb(40, 50, 45);
pub(crate) const CHIP_REQUIRED_BG: Color32 = Color32::from_rgb(120, 60, 30);

// Spacing scale (wider for calmer padding).
pub(crate) const SPACE_EXTRA_SMALL: f32 = 5.0;
pub(crate) const SPACE_SMALL: f32 = 8.0;
pub(crate) const SPACE_MEDIUM: f32 = 12.0;
pub(crate) const SPACE_LARGE: f32 = 20.0;

/// Installs the dark, monospace, neon theme.
pub(crate) fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.override_text_style = Some(egui::TextStyle::Monospace);
    let v = &mut style.visuals;
    v.dark_mode = true;
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = Color32::from_rgb(11, 15, 13);
    v.faint_bg_color = PANEL_BG;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = CYAN;
    v.selection.bg_fill = Color32::from_rgb(0, 80, 45);
    v.selection.stroke = Stroke::new(1.0, GREEN);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.bg_fill = PANEL_BG;
    v.widgets.inactive.weak_bg_fill = PANEL_BG;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, GREEN);
    v.widgets.active.bg_stroke = Stroke::new(1.0, GREEN);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    ctx.set_style(style);
}

/// Color for an HTTP method badge.
pub(crate) fn method_color(method: &str) -> Color32 {
    match method {
        "GET" | "HEAD" => GREEN,
        "POST" => CYAN,
        "PUT" | "PATCH" => AMBER,
        "DELETE" => RED,
        _ => DIM,
    }
}

/// A filled, method-colored badge (read mode); the edit view uses a plain
/// button instead so it can cycle the method on click.
pub(crate) fn method_badge(ui: &mut egui::Ui, method: &str) {
    ui.label(
        RichText::new(format!(" {method} "))
            .color(BG)
            .background_color(method_color(method))
            .strong(),
    );
}
