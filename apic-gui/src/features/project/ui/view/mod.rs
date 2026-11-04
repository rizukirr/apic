//! Project feature rendering: the dialog shown when a picked folder cannot be
//! opened.

use eframe::egui;
use egui::RichText;

use crate::features::project::ui::state::{ProjectAction, ProjectState};
use crate::ui::components::text_button;
use crate::ui::theme::*;

/// Modal shown when a picked non-project folder has invalid contracts: the
/// user must fix them before it can be opened/initialized.
///
/// # Returns
///
/// [`ProjectAction::CloseBlocked`] when Close is clicked.
pub(crate) fn blocked_dialog(ctx: &egui::Context, state: &ProjectState) -> Option<ProjectAction> {
    let p = palette(ctx);
    let failures = state.open_blocked.as_ref()?;
    let mut close = false;
    egui::Window::new("Fix these contracts first")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(
                RichText::new("This folder is not an apic project and has invalid contracts. Fix them, then open it again.")
                    .color(p.on_surface),
            );
            ui.add_space(SPACE_SMALL);
            for (path, err) in failures {
                ui.label(RichText::new(path.to_string_lossy()).color(p.error).strong());
                ui.label(RichText::new(err).color(p.on_surface_variant).size(11.0));
                ui.add_space(SPACE_EXTRA_SMALL);
            }
            ui.add_space(SPACE_EXTRA_SMALL);
            if text_button(ui, "Close", p.primary) {
                close = true;
            }
        });
    close.then_some(ProjectAction::CloseBlocked)
}
