//! Small, reusable egui widgets shared by every section.
//!
//! These are the lowest-level building blocks (inputs, buttons, labels, a
//! framed panel). Keeping them here, rather than re-inlining the same
//! `RichText`/`Frame` recipe at each call site, is what keeps the UI uniform:
//! one delete button, one add button, one sub-heading, used everywhere.

use eframe::egui::{self, TextBuffer};
use egui::{Color32, RichText, Stroke};

use super::theme::{
    BORDER, CHIP_BG, CHIP_REQUIRED_BG, DIM, GREEN, RED, SPACE_EXTRA_SMALL, SPACE_SMALL, TEXT,
};

/// A single-line bordered text input. A non-finite `width` (`f32::INFINITY`)
/// fills the available space; otherwise the box is exactly `width` wide.
pub(crate) fn bordered_input(
    ui: &mut egui::Ui,
    buf: &mut String,
    width: f32,
    hint: &str,
) -> egui::Response {
    bordered_input_colored(ui, buf, width, hint, false)
}

/// `bordered_input` with an explicit error state: when `error` is set the
/// border and text turn red to flag an invalid value.
pub(crate) fn bordered_input_colored(
    ui: &mut egui::Ui,
    buf: &mut String,
    width: f32,
    hint: &str,
    error: bool,
) -> egui::Response {
    egui::Frame::new()
        .stroke(Stroke::new(1.0, if error { RED } else { BORDER }))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            let fill = !width.is_finite();
            if fill {
                ui.set_min_width(ui.available_width());
            }
            ui.add(
                egui::TextEdit::singleline(buf)
                    .frame(false)
                    .hint_text(hint)
                    .text_color(if error { RED } else { TEXT })
                    .desired_width(if fill { f32::INFINITY } else { width }),
            )
        })
        .inner
}

/// A two-column split with an explicit width ratio. `ui.columns(2, …)` only
/// ever splits 50/50; this gives the left column `left_frac` of the available
/// width and the right column the rest (minus the inter-column spacing), so a
/// section can render a wide schema next to a narrow example. Mirrors egui's
/// own `columns_dyn` layout, just with unequal widths.
#[allow(unused)]
pub(crate) fn weighted_columns<R>(
    ui: &mut egui::Ui,
    left_frac: f32,
    add: impl FnOnce(&mut [egui::Ui; 2]) -> R,
) -> R {
    let spacing = ui.spacing().item_spacing.x;
    let usable = (ui.available_width() - spacing).max(0.0);
    let widths = [usable * left_frac, usable * (1.0 - left_frac)];
    let top_left = ui.cursor().min;
    let bottom = ui.max_rect().bottom();

    let mut x = top_left.x;
    let mut columns: [egui::Ui; 2] = std::array::from_fn(|i| {
        let rect =
            egui::Rect::from_min_max(egui::pos2(x, top_left.y), egui::pos2(x + widths[i], bottom));
        x += widths[i] + spacing;
        let mut col = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        col.set_width(widths[i]);
        col
    });

    let result = add(&mut columns);

    let max_height = columns[0].min_size().y.max(columns[1].min_size().y);
    ui.advance_cursor_after_rect(egui::Rect::from_min_size(
        top_left,
        egui::vec2(ui.available_width(), max_height),
    ));
    result
}

/// A dim 11px sub-heading with the standard trailing gap. Used for every
/// labelled block (`QUERY PARAMS`, `SCHEMA DEFINITION`, ...) so they are all
/// spaced identically.
pub(crate) fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(DIM).size(11.0));
    ui.add_space(SPACE_EXTRA_SMALL);
}

/// A frameless `x` row-removal button: muted by default, brightening to red
/// over a subtle rounded backdrop on hover so it clearly reads as clickable.
/// Returns `true` on click.
#[must_use]
pub(crate) fn delete_button(ui: &mut egui::Ui) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    let resp = resp.on_hover_text("Remove");
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        if hovered {
            ui.painter()
                .rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "x",
            egui::FontId::proportional(14.0),
            if hovered { RED } else { DIM },
        );
    }
    resp.clicked()
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
    text_button(ui, label, GREEN)
}

/// Records that the row identified by `ident` (under list `key`) should grab
/// keyboard focus once it renders. The new row does not exist on the click
/// frame, so we stash the target in egui's temp data and claim it next frame in
/// [`take_pending_focus`]. This is what lets `+ query` / `+ field` drop the
/// caret straight into the new name box without a second click.
pub(crate) fn request_new_row_focus(ui: &egui::Ui, key: &str, ident: impl ToString) {
    ui.data_mut(|d| d.insert_temp(egui::Id::new(key), ident.to_string()));
}

/// If `ident` matches the focus target stashed under `key`, focus `resp` (the
/// row's name input) and clear the marker so it fires exactly once. Call right
/// after rendering each row's name field.
pub(crate) fn take_pending_focus(
    ui: &egui::Ui,
    key: &str,
    ident: impl ToString,
    resp: &egui::Response,
) {
    let id = egui::Id::new(key);
    let pending = ui.data(|d| d.get_temp::<String>(id));
    if pending.as_deref() == Some(ident.to_string().as_str()) {
        resp.request_focus();
        ui.data_mut(|d| d.remove::<String>(id));
    }
}

/// `TextEdit` layouter used by `json_editor`, so its editable and read-only
/// variants color JSON identically. The highlight is memoized per
/// `(text, font)`, so re-rendering unchanged JSON every frame is cheap.
fn json_layouter(
    ui: &egui::Ui,
    buf: &dyn TextBuffer,
    wrap_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let font_id = egui::TextStyle::Monospace.resolve(ui.style());
    let mut job =
        super::syntax_highlighting::highlight_json_cached(ui.ctx(), buf.as_str(), &font_id);
    job.wrap.max_width = wrap_width;
    ui.fonts_mut(|f| f.layout_job(job))
}

/// A line-numbered JSON editor. `editing` picks editable vs read-only; `height`
/// is the stretched box height. A dim gutter of logical line numbers sits left
/// of the syntax-highlighted text; soft-wrap is off so numbers stay 1:1 and long
/// lines scroll horizontally. This is the single JSON block used everywhere.
/// Renders as a plain vertical block that grows to its content height; the
/// enclosing pane owns the scroll, so this widget has no scroll area of its own.
pub(crate) fn json_editor(ui: &mut egui::Ui, buf: &mut String, editing: bool) {
    // Read-only preview pretty-prints and shows a placeholder when empty.
    let mut owned;
    let text: &mut String = if editing {
        buf
    } else {
        owned = if buf.trim().is_empty() {
            "(no example)".to_string()
        } else {
            apic_core::json::pretty_json(buf)
        };
        &mut owned
    };
    let line_count = text.lines().count().max(1);
    let mut layouter = json_layouter;
    egui::Frame::new()
        .fill(Color32::from_rgb(11, 15, 13))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let gutter: String = (1..=line_count).map(|n| format!("{n}\n")).collect();
                ui.add(egui::Label::new(
                    RichText::new(gutter).color(DIM).monospace(),
                ));
                ui.add_space(SPACE_SMALL);
                ui.add(
                    egui::TextEdit::multiline(text)
                        .code_editor()
                        .frame(false)
                        .interactive(editing)
                        .lock_focus(editing)
                        .layouter(&mut layouter)
                        .desired_width(f32::INFINITY),
                );
            });
        });
}

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
        .size(11.0)
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

/// Header height and per-row height for the metadata/schema tables.
pub(crate) const TABLE_HEADER_H: f32 = 16.0;
pub(crate) const TABLE_ROW_H: f32 = 24.0;

/// A dim, small, upper-case column-header cell label.
pub(crate) fn header_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .color(DIM)
            .size(10.0)
            .strong(),
    );
}

/// An editable table cell that fills its column. With `egui_extras` the column
/// owns the width, so the cell just stretches to it (`desired_width` infinite).
pub(crate) fn tcell_edit(ui: &mut egui::Ui, buf: &mut String, hint: &str) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(buf)
            .frame(false)
            .hint_text(hint)
            .text_color(TEXT)
            .desired_width(f32::INFINITY),
    )
}

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

    #[test]
    fn json_editor_renders_edit_and_preview() {
        egui::__run_test_ui(|ui| {
            let mut good = "{\n  \"a\": 1\n}".to_string();
            let mut empty = String::new();
            json_editor(ui, &mut good, true);
            json_editor(ui, &mut good, false);
            json_editor(ui, &mut empty, false);
        });
    }

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
