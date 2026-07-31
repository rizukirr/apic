//! The pill/chip primitive.

use eframe::egui;
use egui::RichText;

use crate::ui::theme::{CHIP_BG, CHIP_REQUIRED_BG, DIM, SIZE_SECONDARY, TEXT};

/// The `Required` / `Optional` pill for the REQUIREMENT column. In edit mode it
/// is a button that returns `Some(new_value)` when clicked; in view mode it is a
/// static label returning `None`.
pub(crate) fn required_chip(ui: &mut egui::Ui, required: bool, editing: bool) -> Option<bool> {
    let (label, fg, bg) = if required {
        ("Required", TEXT, CHIP_REQUIRED_BG)
    } else {
        ("Optional", DIM, CHIP_BG)
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
