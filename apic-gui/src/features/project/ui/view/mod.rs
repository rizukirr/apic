//! Project feature rendering: the popup shown when a picked folder cannot be
//! opened.

use eframe::egui;
use egui::RichText;

use crate::features::project::ui::state::{ProjectAction, ProjectState};
use crate::ui::components::{ButtonKind, DialogButton, dialog};
use crate::ui::theme::*;

/// Popup explaining why the folder picked with Open was refused, drawn while
/// `state.refusal` is set.
///
/// # Returns
///
/// [`ProjectAction::DismissRefusal`] when OK is clicked or the popup is
/// closed.
pub(crate) fn refusal_dialog(ctx: &egui::Context, state: &ProjectState) -> Option<ProjectAction> {
    let p = palette(ctx);
    let refusal = state.refusal.as_ref()?;
    let outcome = dialog(
        ctx,
        "open_refused_modal",
        "Cannot open folder",
        |ui| {
            ui.label(RichText::new(refusal).color(p.on_surface));
        },
        &[DialogButton::new("OK", ButtonKind::Primary)],
    );
    (outcome.clicked.is_some() || outcome.closed).then_some(ProjectAction::DismissRefusal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::{BOTH_THEMES, run_in_theme};

    #[test]
    fn refusal_dialog_renders_in_both_themes() {
        let state = ProjectState {
            refusal: Some("Not an apic project: ~/x. Use New project to create one.".into()),
            ..ProjectState::default()
        };
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                let _ = refusal_dialog(ui.ctx(), &state);
            });
        }
    }
}
