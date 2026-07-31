//! Text input primitives, including the line-numbered JSON editor.

use eframe::egui::{self, TextBuffer};
use egui::{RichText, Stroke};

use crate::ui::theme::{BORDER, DIM, EXTREME_BG, RED, SPACE_SMALL, TEXT};

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
                    .frame(egui::Frame::NONE)
                    .hint_text(hint)
                    .text_color(if error { RED } else { TEXT })
                    .desired_width(if fill { f32::INFINITY } else { width }),
            )
        })
        .inner
}

/// An editable table cell that fills its column. With `egui_extras` the column
/// owns the width, so the cell just stretches to it (`desired_width` infinite).
pub(crate) fn tcell_edit(ui: &mut egui::Ui, buf: &mut String, hint: &str) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(buf)
            .frame(egui::Frame::NONE)
            .hint_text(hint)
            .text_color(TEXT)
            .desired_width(f32::INFINITY),
    )
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
        crate::ui::syntax_highlighting::highlight_json_cached(ui.ctx(), buf.as_str(), &font_id);
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
        .fill(EXTREME_BG)
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
                        .frame(egui::Frame::NONE)
                        .interactive(editing)
                        .lock_focus(editing)
                        .layouter(&mut layouter)
                        .desired_width(f32::INFINITY),
                );
            });
        });
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
}
