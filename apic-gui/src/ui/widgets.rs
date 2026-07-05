//! Small, reusable egui widgets shared by every section.
//!
//! These are the lowest-level building blocks (inputs, buttons, labels, a
//! framed panel). Keeping them here, rather than re-inlining the same
//! `RichText`/`Frame` recipe at each call site, is what keeps the UI uniform:
//! one delete button, one add button, one sub-heading, used everywhere.

use eframe::egui::{self, TextBuffer};
use egui::{Color32, RichText, Stroke};

use super::theme::{
    BORDER, CHIP_BG, CHIP_REQUIRED_BG, CYAN, DIM, GREEN, RED, SPACE_EXTRA_SMALL, SPACE_SMALL, TEXT,
};

/// Schema field types: scalars plus their array variants and `object`.
pub(crate) const SCHEMA_TYPES: &[&str] = &[
    "string",
    "int",
    "float",
    "boolean",
    "object",
    "string[]",
    "int[]",
    "float[]",
    "boolean[]",
    "object[]",
];

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

/// The red `x` row-removal button. Returns `true` on click.
#[must_use]
pub(crate) fn delete_button(ui: &mut egui::Ui) -> bool {
    ui.button(RichText::new("x").color(RED)).clicked()
}

/// A green `+ <noun>` add button. Returns `true` on click. Centralizes the
/// add affordance so every list shares one label convention and color.
#[must_use]
pub(crate) fn add_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.button(RichText::new(label).color(GREEN)).clicked()
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

/// Type-picker dropdown bound to `dtype`. `id_salt` disambiguates the combo
/// across rows; `types` is the option list (scalars only for params, scalars
/// plus array variants for schema fields).
pub(crate) fn type_dropdown(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    dtype: &mut String,
    types: &[&str],
) {
    // Clone the label so the read borrow ends before the closure takes `dtype`
    // mutably.
    let label = RichText::new(if dtype.is_empty() {
        "string".to_string()
    } else {
        dtype.clone()
    })
    .color(CYAN);
    egui::ComboBox::from_id_salt(id_salt)
        .width(90.0)
        .selected_text(label)
        .show_ui(ui, |ui| {
            for t in types {
                ui.selectable_value(dtype, t.to_string(), *t);
            }
        });
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
pub(crate) fn json_editor(ui: &mut egui::Ui, buf: &mut String, editing: bool, height: f32) {
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
            egui::ScrollArea::both()
                .id_salt("json_editor_scroll")
                .max_height(if height > 0.0 { height } else { 220.0 })
                .auto_shrink([false, false])
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

/// Uppercase dim column-header row for a metadata table.
/// Column-header row. Each `(label, width)` renders in a fixed-width cell so the
/// header sits directly above the same-width data cells below it. A non-finite
/// width (the last, fill column) left-aligns the label in the remaining space.
pub(crate) fn table_header(ui: &mut egui::Ui, cols: &[(&str, f32)]) {
    ui.horizontal(|ui| {
        for (label, w) in cols {
            let text = RichText::new(label.to_uppercase())
                .color(DIM)
                .size(10.0)
                .strong();
            if w.is_finite() {
                ui.add_sized([*w, 14.0], egui::Label::new(text));
            } else {
                ui.label(text);
            }
        }
    });
    ui.add_space(SPACE_EXTRA_SMALL);
}

/// The requirement chip in a fixed-width cell so it lines up under the
/// REQUIREMENT column header regardless of the `Required`/`Optional` text width.
/// Returns the chip's toggle result (`Some(new)` when clicked in edit mode).
pub(crate) fn chip_cell(
    ui: &mut egui::Ui,
    required: bool,
    editing: bool,
    width: f32,
) -> Option<bool> {
    let mut out = None;
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.text_style_height(&egui::TextStyle::Body) + 6.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            out = required_chip(ui, required, editing);
        },
    );
    out
}

/// A frameless inline-editable table cell of a given width.
pub(crate) fn cell_edit(
    ui: &mut egui::Ui,
    buf: &mut String,
    width: f32,
    hint: &str,
) -> egui::Response {
    ui.add_sized(
        [width, ui.text_style_height(&egui::TextStyle::Body) + 6.0],
        egui::TextEdit::singleline(buf)
            .frame(false)
            .hint_text(hint)
            .text_color(TEXT),
    )
}

/// A view-mode key cell (accent color).
pub(crate) fn cell_key(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.add_sized(
        [width, ui.text_style_height(&egui::TextStyle::Body) + 6.0],
        egui::Label::new(RichText::new(text).color(CYAN)),
    );
}

/// A view-mode plain-text cell.
pub(crate) fn cell_text(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.add_sized(
        [width, ui.text_style_height(&egui::TextStyle::Body) + 6.0],
        egui::Label::new(RichText::new(text).color(TEXT)),
    );
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
            json_editor(ui, &mut good, true, 200.0);
            json_editor(ui, &mut good, false, 200.0);
            json_editor(ui, &mut empty, false, 200.0);
        });
    }

    #[test]
    fn table_primitives_render_without_panicking() {
        egui::__run_test_ui(|ui| {
            let mut buf = "x".to_string();
            table_frame(ui, |ui| {
                table_header(
                    ui,
                    &[
                        ("header", 120.0),
                        ("requirement", 90.0),
                        ("value", f32::INFINITY),
                    ],
                );
                chip_cell(ui, true, true, 90.0);
                cell_key(ui, "Content-Type", 120.0);
                required_chip(ui, true, false);
                required_chip(ui, false, true);
                cell_edit(ui, &mut buf, 120.0, "value");
                cell_text(ui, "application/json", 160.0);
            });
        });
    }
}
