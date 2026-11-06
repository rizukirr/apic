//! The git central pane's diff rendering: the field-level semantic diff of a contract and the inline diff of any file.

use eframe::egui;
use egui::RichText;

use crate::features::git::domain::diff::{self, DiffLine, DiffRow, FieldChange, LineKind};
use crate::ui::theme::*;

/// Renders `diff::diff_models` as one block per change. A single-line change
/// keeps the field name in the muted colour followed by `from` in the error colour, an arrow, and
/// `to` in the success colour. A change whose `from` or `to` spans multiple lines (a
/// body, mainly) instead renders as a line-level diff under the field name,
/// so a one-line edit inside a many-line body shows as one changed line
/// rather than the whole body twice.
pub(super) fn semantic_diff_view(ui: &mut egui::Ui, changes: &[FieldChange]) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for change in changes {
                if change.from.contains('\n') || change.to.contains('\n') {
                    multiline_change_view(ui, change);
                } else {
                    single_line_change_view(ui, change);
                }
                ui.add_space(SPACE_SMALL);
            }
        });
}

/// One row for a change whose old and new values are each a single line.
fn single_line_change_view(ui: &mut egui::Ui, change: &FieldChange) {
    let p = palette(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&change.what).color(p.on_surface_variant));
        if !change.from.is_empty() {
            ui.label(RichText::new(&change.from).color(p.error));
        }
        if !change.from.is_empty() && !change.to.is_empty() {
            ui.label(RichText::new("->").color(p.on_surface_variant));
        }
        if !change.to.is_empty() {
            ui.label(RichText::new(&change.to).color(p.success));
        }
    });
}

/// The field name as a heading, followed by a line-level diff: lines present
/// only in `from` in the error colour, lines present only in `to` in the success colour, unchanged
/// lines in the muted colour.
fn multiline_change_view(ui: &mut egui::Ui, change: &FieldChange) {
    let p = palette(ui.ctx());
    ui.label(RichText::new(&change.what).color(p.on_surface_variant));
    for row in diff::line_diff(&change.from, &change.to) {
        let color = match row.kind {
            diff::LineDiffKind::Removed => p.error,
            diff::LineDiffKind::Added => p.success,
            diff::LineDiffKind::Unchanged => p.on_surface_variant,
        };
        ui.label(RichText::new(&row.text).color(color).monospace());
    }
}

/// Renders `rows` as an inline diff: old and new line numbers, a `-`, `+`
/// or space marker, removed rows on a faint error tint, added rows on a
/// faint success tint, and each emphasis span in a stronger tint. Scrolls
/// both ways, never wraps, and lays out only the rows on screen.
pub(super) fn inline_diff_view(ui: &mut egui::Ui, rows: &[DiffRow]) {
    let p = palette(ui.ctx());
    let digits = rows
        .iter()
        .filter_map(|row| match row {
            DiffRow::Line(line) => line.old_no.max(line.new_no),
            DiffRow::Hunk(_) => None,
        })
        .max()
        .unwrap_or(0)
        .to_string()
        .len();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let row_h = ui.text_style_height(&egui::TextStyle::Monospace).max(1.0);
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show_rows(ui, row_h, rows.len(), |ui, range| {
                for row in &rows[range] {
                    diff_row_view(ui, row, digits, p);
                }
            });
    });
}

/// One inline diff row with its full-width background.
fn diff_row_view(ui: &mut egui::Ui, row: &DiffRow, digits: usize, p: &Palette) {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let format = |color: egui::Color32, background: egui::Color32| egui::TextFormat {
        font_id: font.clone(),
        color,
        background,
        ..Default::default()
    };
    let plain = egui::Color32::TRANSPARENT;
    let mut job = egui::text::LayoutJob::default();
    let background = match row {
        DiffRow::Hunk(text) => {
            job.append(text, 0.0, format(p.info, plain));
            Some(p.surface_container)
        }
        DiffRow::Line(line) => {
            let number = |n: Option<u32>| match n {
                Some(n) => format!("{n:>digits$}"),
                None => " ".repeat(digits),
            };
            let gutter = format!("{} {} ", number(line.old_no), number(line.new_no));
            job.append(&gutter, 0.0, format(p.on_surface_variant, plain));
            let (marker, tint) = match line.kind {
                LineKind::Removed => ("- ", Some(p.error)),
                LineKind::Added => ("+ ", Some(p.success)),
                LineKind::Context => ("  ", None),
            };
            let text_color = if line.old_no.is_none() && line.new_no.is_none() {
                p.on_surface_variant
            } else {
                p.on_surface
            };
            job.append(marker, 0.0, format(text_color, plain));
            append_text(&mut job, line, tint, &format(text_color, plain));
            tint.map(|c| c.gamma_multiply(0.15))
        }
    };
    let bg = ui.painter().add(egui::Shape::Noop);
    let response = ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
    if let Some(color) = background {
        let rect = egui::Rect::from_min_max(
            response.rect.min,
            egui::pos2(
                response.rect.right().max(ui.clip_rect().right()),
                response.rect.bottom(),
            ),
        );
        ui.painter()
            .set(bg, egui::Shape::rect_filled(rect, 0.0, color));
    }
}

/// Appends `line.text`, its emphasis span on `tint` at the stronger
/// strength when both are present and the span is valid.
fn append_text(
    job: &mut egui::text::LayoutJob,
    line: &DiffLine,
    tint: Option<egui::Color32>,
    base: &egui::TextFormat,
) {
    let text = &line.text;
    match (line.emphasis.clone(), tint) {
        (Some(range), Some(tint)) if text.get(range.clone()).is_some() => {
            let strong = egui::TextFormat {
                background: tint.gamma_multiply(0.35),
                ..base.clone()
            };
            job.append(&text[..range.start], 0.0, base.clone());
            job.append(&text[range.clone()], 0.0, strong);
            job.append(&text[range.end..], 0.0, base.clone());
        }
        _ => job.append(text, 0.0, base.clone()),
    }
}
