//! Collapsibles: a titled group with a count, and a tree node whose header
//! row the caller draws.

use std::fmt::Debug;
use std::hash::Hash;

use eframe::egui;
use egui::collapsing_header::{CollapsingState, HeaderResponse};

use crate::ui::base::Text;

/// A group that folds open and closed under a muted title, with an optional
/// count after it, such as `Changes  3`. Open by default.
#[must_use]
pub(crate) struct Collapsible {
    id: egui::Id,
    title: String,
    count: Option<usize>,
    open: Option<bool>,
}

impl Collapsible {
    /// A group identified by `id_salt` (unique per group on screen) titled
    /// `title`.
    pub(crate) fn new(id_salt: impl Hash + Debug, title: impl Into<String>) -> Self {
        Self {
            id: egui::Id::new(id_salt),
            title: title.into(),
            count: None,
            open: None,
        }
    }

    /// Shows `count` after the title.
    pub(crate) fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }

    /// Opens or closes the group from the caller's own state instead of
    /// egui's memory. The caller flips it when [`Collapsible::show`] reports
    /// a header click.
    pub(crate) fn open(mut self, open: bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Draws the group, with `body` filling it while open.
    ///
    /// # Returns
    ///
    /// Whether the title was clicked this frame.
    pub(crate) fn show(self, ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) -> bool {
        let title = match self.count {
            Some(count) => format!("{}  {count}", self.title),
            None => self.title,
        };
        let title = Text::new(title).muted().rich(ui.ctx());
        egui::CollapsingHeader::new(title)
            .id_salt(self.id)
            .default_open(true)
            .open(self.open)
            .show(ui, body)
            .header_response
            .clicked()
    }
}

/// A node of a folder tree: a fold arrow, then the header row `header`
/// draws, open by default and remembered under `id`. Chain `.body(...)` on
/// the result to draw the children while open.
pub(crate) fn tree_node(
    ui: &mut egui::Ui,
    id: egui::Id,
    header: impl FnOnce(&mut egui::Ui),
) -> HeaderResponse<'_, ()> {
    CollapsingState::load_with_default_open(ui.ctx(), id, true).show_header(ui, header)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::base::{
        InlineInput, Tone, checkbox, checkbox_mixed, code_editor, code_view, divider,
        divider_flush, loading, nowrap_label, row_label, text_area, truncated_label,
    };
    use crate::ui::theme::{BOTH_THEMES, run_in_theme};

    #[test]
    fn the_new_base_pieces_render_in_both_themes() {
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                let mut open = true;
                let clicked = Collapsible::new("group", "Changes")
                    .count(3)
                    .open(open)
                    .show(ui, |ui| {
                        tree_node(ui, egui::Id::new("node"), |ui| {
                            row_label(ui, Text::new("folder").muted());
                        })
                        .body(|ui| {
                            truncated_label(ui, Text::new("long name").rich(ui.ctx()));
                            nowrap_label(ui, Text::new("+ a code line").rich(ui.ctx()));
                        });
                    });
                if clicked {
                    open = !open;
                }
                let mut text = String::from("200");
                InlineInput::new(&mut text)
                    .hint("code")
                    .width(40.0)
                    .tone(Tone::Success)
                    .show(ui);
                InlineInput::new(&mut text)
                    .mono()
                    .vars(&|name: &str| name == "base")
                    .show(ui);
                let mut on = true;
                checkbox(ui, &mut on, "Amend");
                checkbox_mixed(ui, &mut on, true);
                divider(ui);
                divider_flush(ui);
                loading(ui, "Sending");
                code_view(ui, "plain text");
                code_editor(ui, &mut text, 2);
                text_area(ui, &mut text, "commit message");
                assert!(open);
            });
        }
    }
}
