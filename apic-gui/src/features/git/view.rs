//! The git panel's rendering: the sidebar body (status list, refresh) and the
//! central body (diff view). Both bodies fill a frame the app shell owns, the
//! same shape as `features::contracts::view`.

use std::path::Path;

use eframe::egui;
use egui::RichText;

use crate::app::actions::GitAction;
use crate::features::git::state::GitState;
use crate::ui::theme::*;

/// Left sidebar body for the Git tab: a heading, a refresh button, the error
/// line when set, and the file list once later tasks add it.
///
/// The panel frame is owned by the app shell, not by this feature, so that a
/// second sidebar tab can render into the same frame; this only fills it.
pub(crate) fn sidebar_body(
    ui: &mut egui::Ui,
    state: &mut GitState,
    repo_root: Option<&Path>,
) -> Option<GitAction> {
    let mut action = None;
    ui.add_space(SPACE_MEDIUM);
    ui.horizontal(|ui| {
        ui.label(RichText::new("GIT").color(GREEN).strong().size(16.0));
        if ui
            .small_button(RichText::new("⟳").color(GREEN))
            .on_hover_text("Refresh status")
            .clicked()
        {
            action = Some(GitAction::Refresh);
        }
    });
    ui.separator();

    if !state.error.is_empty() {
        ui.label(RichText::new(&state.error).color(RED));
        ui.add_space(SPACE_SMALL);
    }

    if repo_root.is_none() {
        ui.label(RichText::new("Not inside a git repository.").color(DIM));
    }

    action
}

/// The central body for the Git tab: the diff view once selected, an empty
/// state until then.
///
/// The panel frame is owned by the app shell. This only sees the git slice.
pub(crate) fn central_body(ui: &mut egui::Ui, _state: &GitState) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("No file selected").color(DIM).size(16.0));
        ui.add_space(SPACE_SMALL);
        ui.label(RichText::new("Select a changed file on the left.").color(DIM));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_and_central_render_without_panicking() {
        eframe::egui::__run_test_ui(|ui| {
            let mut state = GitState::default();
            sidebar_body(ui, &mut state, None);
            central_body(ui, &state);
        });
    }
}
