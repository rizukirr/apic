//! Framing helpers shared across features: the URL bar and the empty state.

use eframe::egui;
use egui::RichText;

use crate::ui::theme::{SPACE_SMALL, palette};

/// A rounded bar with an `outline_variant` border and a `surface_container`
/// fill, laid out left to right, for a method and URL.
pub(crate) fn url_bar<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = palette(ui.ctx());
    egui::Frame::NONE
        .fill(p.surface_container)
        .stroke(egui::Stroke::new(1.0, p.outline_variant))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 8))
        .show(ui, |ui| {
            ui.set_min_height(20.0);
            ui.horizontal(add).inner
        })
        .inner
}

/// A centred muted title with an optional hint below, for a pane with nothing to show.
pub(crate) fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str) {
    let p = palette(ui.ctx());
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).color(p.on_surface_variant).size(16.0));
        if !hint.is_empty() {
            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new(hint).color(p.on_surface_variant));
        }
    });
}
