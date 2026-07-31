//! Label primitives: sub-headings and table column headers.

use eframe::egui;
use egui::RichText;

use crate::ui::theme::{DIM, SIZE_HEADER_LABEL, SIZE_SECONDARY, SPACE_EXTRA_SMALL};

/// A dim 11px sub-heading with the standard trailing gap. Used for every
/// labelled block (`QUERY PARAMS`, `SCHEMA DEFINITION`, ...) so they are all
/// spaced identically.
pub(crate) fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(DIM).size(SIZE_SECONDARY));
    ui.add_space(SPACE_EXTRA_SMALL);
}

/// A dim, small, upper-case column-header cell label.
pub(crate) fn header_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .color(DIM)
            .size(SIZE_HEADER_LABEL)
            .strong(),
    );
}
