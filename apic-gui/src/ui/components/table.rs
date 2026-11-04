//! Table primitives for the metadata/schema tables.

use eframe::egui;
use egui::Stroke;

/// Horizontal padding inside each grid cell, either side of its content.
const GRID_CELL_PAD_X: f32 = 8.0;

/// A `TableBuilder` preconfigured for the metadata/schema tables: left-aligned
/// cells and no scroll area of its own (the pane already scrolls). Columns are
/// added by the caller; because the table lays them out, the header and every
/// row stay aligned. Item spacing is zeroed on `ui` so the cells tile and the
/// grid lines drawn by [`grid_cell`] meet.
pub(crate) fn metadata_table(ui: &mut egui::Ui) -> egui_extras::TableBuilder<'_> {
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    egui_extras::TableBuilder::new(ui)
        .vscroll(false)
        .striped(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
}

/// Draws `add` inside a table cell with [`GRID_CELL_PAD_X`] of horizontal
/// padding, over a 1px `outline_variant` line on the cell's top and right
/// edges. Every cell draws both lines: [`table_frame`] clips the top pixel row
/// and the rightmost pixels of its content, so the header row's top line and
/// the last column's right line never double the frame's own border. Lines on
/// the top edge rather than the bottom keep the last row clear of the bottom
/// border, whose position is not known until the table is laid out.
///
/// # Returns
///
/// Whatever `add` returns.
pub(crate) fn grid_cell<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let rect = ui.max_rect().expand2(0.5 * ui.spacing().item_spacing);
    let stroke = Stroke::new(1.0, crate::ui::theme::palette(ui.ctx()).outline_variant);
    let painter = ui.painter();
    painter.vline(rect.right() - 0.5, rect.y_range(), stroke);
    painter.hline(rect.x_range(), rect.top() + 0.5, stroke);
    let inner = ui.max_rect().shrink2(egui::vec2(GRID_CELL_PAD_X, 0.0));
    let layout = *ui.layout();
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(layout), add)
        .inner
}

/// The final stretch column, sized to fill the row's remaining width after the
/// fixed columns (pass every other column's width in `fixed`). The table has no
/// item spacing (see [`metadata_table`]), so the columns add up to exactly the
/// available width and the last column's grid line lands on the frame's
/// clipped edge.
///
/// We compute an *exact* width each frame rather than use `Column::remainder()`,
/// because egui_extras ratchets a remainder column to the widest content it has
/// ever held (`at_least(max_used)`) and never shrinks it back — so on a window
/// resize the table would stretch out but never contract. An exact column is
/// resolved to its literal width every frame, so it tracks the window both ways.
pub(crate) fn fill_column(ui: &egui::Ui, fixed: &[f32]) -> egui_extras::Column {
    let used: f32 = fixed.iter().sum();
    let width = (ui.available_width() - used).max(80.0);
    egui_extras::Column::exact(width)
}

/// The rounded bordered container that wraps a metadata table's header and
/// rows. It has no inner margin, so the grid touches the border, and it clips
/// the top pixel row and the rightmost two pixels of its content (two, so a
/// rounded column edge cannot leak half a line) so the grid's outer lines hide
/// instead of doubling the border.
pub(crate) fn table_frame<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .stroke(Stroke::new(
            1.0,
            crate::ui::theme::palette(ui.ctx()).outline_variant,
        ))
        .inner_margin(egui::Margin::ZERO)
        .show(ui, |ui| {
            // Pin the content to exactly the available width so the frame can
            // never grow past its (e.g. 50%) column and bleed into a sibling pane.
            let w = ui.available_width();
            ui.set_min_width(w);
            ui.set_max_width(w);
            let content = ui.max_rect();
            ui.shrink_clip_rect(egui::Rect::from_min_max(
                egui::pos2(content.left(), content.top() + 1.0),
                egui::pos2(content.right() - 2.0, f32::INFINITY),
            ));
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
        for preference in crate::ui::theme::BOTH_THEMES {
            crate::ui::theme::run_in_theme(preference, |ui| {
                let mut buf = "x".to_string();
                table_frame(ui, |ui| {
                    metadata_table(ui)
                        .column(egui_extras::Column::exact(120.0))
                        .column(egui_extras::Column::remainder())
                        .header(TABLE_HEADER_H, |mut h| {
                            h.col(|ui| grid_cell(ui, |ui| header_label(ui, "Header")));
                            h.col(|ui| grid_cell(ui, |ui| header_label(ui, "Value")));
                        })
                        .body(|mut body| {
                            body.row(TABLE_ROW_H, |mut row| {
                                row.col(|ui| {
                                    grid_cell(ui, |ui| tcell_edit(ui, &mut buf, "Value"));
                                });
                                row.col(|ui| {
                                    grid_cell(ui, |ui| required_chip(ui, true, true));
                                });
                            });
                        });
                });
            });
        }
    }
}
