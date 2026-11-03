//! The pill/chip primitive.

use eframe::egui;
use egui::RichText;

use crate::ui::theme::SIZE_SECONDARY;

/// The `Required` / `Optional` pill for the REQUIREMENT column. In edit mode it
/// is a button that returns `Some(new_value)` when clicked; in view mode it is a
/// static label returning `None`.
pub(crate) fn required_chip(ui: &mut egui::Ui, required: bool, editing: bool) -> Option<bool> {
    let p = crate::ui::theme::palette(ui.ctx());
    let (label, fg, bg) = if required {
        ("Required", p.on_error_container, p.error_container)
    } else {
        ("Optional", p.on_surface_variant, p.secondary_container)
    };
    let text = RichText::new(format!(" {label} "))
        .color(fg)
        .size(SIZE_SECONDARY)
        .background_color(bg);
    if editing {
        if ui.add(egui::Button::new(text).frame(false)).clicked() {
            return Some(!required);
        }
        None
    } else {
        ui.label(text);
        None
    }
}
