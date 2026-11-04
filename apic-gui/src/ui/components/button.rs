//! Button primitives. Every ordinary action button in the app is built here so
//! they share one look.

use eframe::egui;
use egui::RichText;

use crate::ui::theme::SIZE_DELETE_GLYPH;

/// A frameless `x` row-removal button: muted by default, brightening to red
/// over a subtle rounded backdrop on hover so it clearly reads as clickable.
/// Returns `true` on click.
#[must_use]
pub(crate) fn delete_button(ui: &mut egui::Ui) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    let resp = resp.on_hover_text("Remove");
    if ui.is_rect_visible(rect) {
        let p = crate::ui::theme::palette(ui.ctx());
        let hovered = resp.hovered();
        if hovered {
            ui.painter()
                .rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "x",
            egui::FontId::proportional(SIZE_DELETE_GLYPH),
            if hovered {
                p.error
            } else {
                p.on_surface_variant
            },
        );
    }
    resp.clicked()
}

/// A frameless icon button: `icon` (an `egui_phosphor::regular` constant) in
/// the muted text colour, with egui's hover backdrop so it reads as
/// clickable. Returns the response so callers can chain `.clicked()`.
pub(crate) fn icon_button(ui: &mut egui::Ui, icon: &str, hover_text: &str) -> egui::Response {
    let p = crate::ui::theme::palette(ui.ctx());
    ui.add(
        egui::Button::new(RichText::new(icon).color(p.on_surface_variant))
            .frame_when_inactive(false),
    )
    .on_hover_text(hover_text)
}

/// The app's reusable flat text button: `label` in `color`, framed so egui
/// gives it the standard hover/press feedback. The single place ordinary action
/// buttons are built, so they share one look. Returns `true` on click.
#[must_use]
pub(crate) fn text_button(ui: &mut egui::Ui, label: &str, color: egui::Color32) -> bool {
    ui.button(RichText::new(label).color(color)).clicked()
}

/// A green `+ <noun>` add button. Returns `true` on click. Centralizes the
/// add affordance so every list shares one label convention and color.
#[must_use]
pub(crate) fn add_button(ui: &mut egui::Ui, label: &str) -> bool {
    text_button(ui, label, crate::ui::theme::palette(ui.ctx()).primary)
}
