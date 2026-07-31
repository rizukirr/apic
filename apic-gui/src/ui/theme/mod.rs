//! Palette, spacing scale, and the global egui theme.
//!
//! Tokens live in the child modules and are re-exported here so call sites can
//! keep using `theme::GREEN`. Nothing in this module may import from
//! `crate::features`.

pub(crate) mod colors;
pub(crate) mod spacing;

pub(crate) use colors::*;
pub(crate) use spacing::*;

use eframe::egui;
use egui::{Color32, RichText, Stroke};

/// Installs the dark, monospace, neon theme.
pub(crate) fn apply_theme(ctx: &egui::Context) {
    // `all_styles_mut` applies to every theme variant, so the neon palette
    // holds regardless of the OS light/dark preference (egui 0.35 dropped the
    // single global `set_style` in favour of per-theme styles).
    ctx.all_styles_mut(|style| {
        style.override_text_style = Some(egui::TextStyle::Monospace);
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.panel_fill = BG;
        v.window_fill = BG;
        v.extreme_bg_color = EXTREME_BG;
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
    });
}

/// Color for an HTTP method badge.
///
/// vibekit: contracts-domain code temporarily parked in the theme module.
/// Task 3 of the modular restructure moves this into
/// `features/contracts/view.rs`, after which `ui/` names no domain concept.
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
///
/// vibekit: see `method_color` above; moves in Task 3.
pub(crate) fn method_badge(ui: &mut egui::Ui, method: &str) {
    ui.label(
        RichText::new(format!(" {method} "))
            .color(BG)
            .background_color(method_color(method))
            .strong(),
    );
}
