//! Text input primitives, including the line-numbered JSON editor.

use eframe::egui::{self, TextBuffer};
use egui::Stroke;

use crate::ui::base::{Text, Tone, url_layout_job};

use crate::ui::theme::SPACE_SMALL;

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
    let p = crate::ui::theme::palette(ui.ctx());
    egui::Frame::new()
        .stroke(Stroke::new(
            1.0,
            if error { p.error } else { p.outline_variant },
        ))
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
                    .text_color(if error { p.error } else { p.on_surface })
                    .desired_width(if fill { f32::INFINITY } else { width }),
            )
        })
        .inner
}

/// An editable table cell that fills its column. With `egui_extras` the column
/// owns the width, so the cell just stretches to it.
pub(crate) fn tcell_edit(ui: &mut egui::Ui, buf: &mut String, hint: &str) -> egui::Response {
    InlineInput::new(buf).hint(hint).show(ui)
}

/// `tcell_edit` whose variable references are coloured like the URL bar, by
/// `defined`. `monospace` picks the monospace font (header values) over the
/// body font (query values).
pub(crate) fn tcell_edit_vars(
    ui: &mut egui::Ui,
    buf: &mut String,
    hint: &str,
    monospace: bool,
    defined: &dyn Fn(&str) -> bool,
) -> egui::Response {
    let input = InlineInput::new(buf).hint(hint).vars(defined);
    if monospace {
        input.mono().show(ui)
    } else {
        input.show(ui)
    }
}

/// A frameless single-line field that reads as part of the text around it,
/// such as an endpoint name or a response code edited in place. Built with
/// [`InlineInput::new`], drawn with [`InlineInput::show`].
#[must_use]
pub(crate) struct InlineInput<'a> {
    buf: &'a mut String,
    hint: &'a str,
    width: f32,
    tone: Tone,
    mono: bool,
    id: Option<egui::Id>,
    vars: Option<&'a dyn Fn(&str) -> bool>,
    grow: bool,
}

impl<'a> InlineInput<'a> {
    /// A field editing `buf`, as wide as the space it sits in, in the
    /// default tone.
    pub(crate) fn new(buf: &'a mut String) -> Self {
        Self {
            buf,
            hint: "",
            width: f32::INFINITY,
            tone: Tone::Default,
            mono: false,
            id: None,
            vars: None,
            grow: false,
        }
    }

    /// Greyed text shown while the field is empty.
    pub(crate) fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    /// Makes the field exactly `width` wide.
    pub(crate) fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Draws the text in `tone`.
    pub(crate) fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Uses the monospace font.
    pub(crate) fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// Lets the field grow past its width as the text gets longer, instead
    /// of scrolling the text inside it.
    pub(crate) fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    /// Gives the field a fixed id, so focus can be moved to it.
    pub(crate) fn id(mut self, id: egui::Id) -> Self {
        self.id = Some(id);
        self
    }

    /// Colours the variable references in the text, by whether `defined`
    /// accepts their name, like [`url_layout_job`]. The tone is then unused.
    pub(crate) fn vars(mut self, defined: &'a dyn Fn(&str) -> bool) -> Self {
        self.vars = Some(defined);
        self
    }

    /// Draws the field.
    pub(crate) fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let p = crate::ui::theme::palette(ui.ctx());
        let style = if self.mono {
            egui::TextStyle::Monospace
        } else {
            egui::TextStyle::Body
        };
        let font = style.resolve(ui.style());
        let mut edit = egui::TextEdit::singleline(self.buf)
            .frame(egui::Frame::NONE)
            .font(style)
            .hint_text(self.hint)
            .text_color(self.tone.color(p))
            .desired_width(self.width)
            .clip_text(!self.grow);
        if let Some(id) = self.id {
            edit = edit.id(id);
        }
        match self.vars {
            Some(defined) => {
                let mut layouter = |ui: &egui::Ui, buf: &dyn TextBuffer, wrap_width: f32| {
                    let mut job = url_layout_job(buf.as_str(), p, font.clone(), defined);
                    job.wrap.max_width = wrap_width;
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                ui.add(edit.layouter(&mut layouter))
            }
            None => ui.add(edit),
        }
    }
}

/// An editable multiline code box with a frame, `rows` lines tall at least,
/// as wide as the space it sits in. For text that is not JSON, or JSON that
/// does not parse yet; JSON bodies use [`json_editor`].
pub(crate) fn code_editor(ui: &mut egui::Ui, buf: &mut String, rows: usize) -> egui::Response {
    ui.add(
        egui::TextEdit::multiline(buf)
            .code_editor()
            .desired_width(f32::INFINITY)
            .desired_rows(rows),
    )
}

/// Read-only monospace text with no frame, as wide as the space it sits in,
/// selectable so it can be copied.
pub(crate) fn code_view(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let mut shown = text;
    ui.add(
        egui::TextEdit::multiline(&mut shown)
            .code_editor()
            .frame(egui::Frame::NONE)
            .desired_width(f32::INFINITY),
    )
}

/// A framed multiline text box filling the space it is given, such as a
/// commit message.
pub(crate) fn text_area(ui: &mut egui::Ui, buf: &mut String, hint: &str) -> egui::Response {
    ui.add_sized(
        ui.available_size(),
        egui::TextEdit::multiline(buf).hint_text(hint),
    )
}

/// A checkbox with `label` beside it, or none when `label` is empty.
pub(crate) fn checkbox(ui: &mut egui::Ui, checked: &mut bool, label: &str) -> egui::Response {
    ui.checkbox(checked, label)
}

/// A checkbox for a group: ticked when every item is, a dash when only some
/// are (`mixed`), and empty when none are.
pub(crate) fn checkbox_mixed(ui: &mut egui::Ui, checked: &mut bool, mixed: bool) -> egui::Response {
    ui.add(egui::Checkbox::new(checked, "").indeterminate(mixed))
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
    let p = crate::ui::theme::palette(ui.ctx());
    let line_count = text.lines().count().max(1);
    let mut layouter = json_layouter;
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(0, 4))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let gutter: String = (1..=line_count).map(|n| format!("{n}\n")).collect();
                ui.add(egui::Label::new(
                    Text::new(gutter).muted().mono().rich_in(p),
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
        for preference in crate::ui::theme::BOTH_THEMES {
            crate::ui::theme::run_in_theme(preference, |ui| {
                let mut good = "{\n  \"a\": 1\n}".to_string();
                let mut empty = String::new();
                json_editor(ui, &mut good, true);
                json_editor(ui, &mut good, false);
                json_editor(ui, &mut empty, false);
            });
        }
    }
}
