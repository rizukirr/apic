//! Table primitives for the metadata/schema tables.

use eframe::egui;
use egui::Stroke;

use crate::ui::theme::BORDER;

/// A `TableBuilder` preconfigured for the metadata/schema tables: left-aligned
/// cells and no scroll area of its own (the pane already scrolls). Columns are
/// added by the caller; because the table lays them out, the header and every
/// row stay aligned.
pub(crate) fn metadata_table(ui: &mut egui::Ui) -> egui_extras::TableBuilder<'_> {
    egui_extras::TableBuilder::new(ui)
        .vscroll(false)
        .striped(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
}

/// The final stretch column, sized to fill the row's remaining width after the
/// fixed columns (pass every other column's width in `fixed`).
///
/// We compute an *exact* width each frame rather than use `Column::remainder()`,
/// because egui_extras ratchets a remainder column to the widest content it has
/// ever held (`at_least(max_used)`) and never shrinks it back — so on a window
/// resize the table would stretch out but never contract. An exact column is
/// resolved to its literal width every frame, so it tracks the window both ways.
pub(crate) fn fill_column(ui: &egui::Ui, fixed: &[f32]) -> egui_extras::Column {
    let cols = fixed.len() + 1; // + the stretch column itself
    let spacing = ui.spacing().item_spacing.x * (cols as f32 - 1.0);
    let used: f32 = fixed.iter().sum::<f32>() + spacing;
    // Small safety margin so rounding never pushes the row past the frame.
    let width = (ui.available_width() - used - 2.0).max(80.0);
    egui_extras::Column::exact(width)
}

/// The bordered container that wraps a metadata table's header + rows.
pub(crate) fn table_frame<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            // Pin the content to exactly the available width so the frame can
            // never grow past its (e.g. 50%) column and bleed into a sibling pane.
            let w = ui.available_width();
            ui.set_min_width(w);
            ui.set_max_width(w);
            add(ui)
        })
        .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::components::{chip::required_chip, input::tcell_edit, text::header_label};
    use crate::ui::theme::{TABLE_HEADER_H, TABLE_ROW_H};

    #[test]
    fn table_primitives_render_without_panicking() {
        egui::__run_test_ui(|ui| {
            let mut buf = "x".to_string();
            table_frame(ui, |ui| {
                metadata_table(ui)
                    .column(egui_extras::Column::exact(120.0))
                    .column(egui_extras::Column::remainder())
                    .header(TABLE_HEADER_H, |mut h| {
                        h.col(|ui| header_label(ui, "header"));
                        h.col(|ui| header_label(ui, "value"));
                    })
                    .body(|mut body| {
                        body.row(TABLE_ROW_H, |mut row| {
                            row.col(|ui| {
                                tcell_edit(ui, &mut buf, "value");
                            });
                            row.col(|ui| {
                                required_chip(ui, true, true);
                            });
                        });
                    });
            });
        });
    }
}
