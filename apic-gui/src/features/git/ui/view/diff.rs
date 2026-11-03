//! The git central pane's diff rendering: the field-level semantic diff of a
//! contract and the raw git-style diff of any other file.

use eframe::egui;
use egui::RichText;

use crate::features::git::domain::diff::{self, FieldChange, strip_diff_header};
use crate::ui::theme::*;

/// Renders `diff::diff_models` as one block per change. A single-line change
/// keeps the field name in `DIM` followed by `from` in `RED`, an arrow, and
/// `to` in `GREEN`. A change whose `from` or `to` spans multiple lines (a
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
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&change.what).color(DIM));
        if !change.from.is_empty() {
            ui.label(RichText::new(&change.from).color(RED));
        }
        if !change.from.is_empty() && !change.to.is_empty() {
            ui.label(RichText::new("->").color(DIM));
        }
        if !change.to.is_empty() {
            ui.label(RichText::new(&change.to).color(GREEN));
        }
    });
}

/// The field name as a heading, followed by a line-level diff: lines present
/// only in `from` in `RED`, lines present only in `to` in `GREEN`, unchanged
/// lines in `DIM`.
fn multiline_change_view(ui: &mut egui::Ui, change: &FieldChange) {
    ui.label(RichText::new(&change.what).color(DIM));
    for row in diff::line_diff(&change.from, &change.to) {
        let color = match row.kind {
            diff::LineDiffKind::Removed => RED,
            diff::LineDiffKind::Added => GREEN,
            diff::LineDiffKind::Unchanged => DIM,
        };
        ui.label(RichText::new(&row.text).color(color).monospace());
    }
}

/// Renders a diff body the way git prints it: markers kept, `+` green, `-`
/// red, and each `@@` hunk header in cyan, which is the colour git itself
/// uses for them. Keeping the marker is what makes a multi-hunk diff
/// readable, and it is why the header lines have to survive the stripper
/// rather than being eaten a character at a time. Scrolls inside its own
/// area.
pub(super) fn raw_diff_view(ui: &mut egui::Ui, raw: &str) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for line in strip_diff_header(raw).lines() {
                let color = if line.starts_with("@@") {
                    CYAN
                } else if line.starts_with('+') {
                    GREEN
                } else if line.starts_with('-') {
                    RED
                } else {
                    TEXT
                };
                ui.label(RichText::new(line).color(color).monospace());
            }
        });
}
