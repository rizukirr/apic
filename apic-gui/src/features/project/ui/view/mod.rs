//! Project feature rendering: the popup shown when a picked folder cannot be
//! opened.

use eframe::egui;
use egui::RichText;

use crate::features::project::ui::state::{ProjectAction, ProjectState};
use crate::ui::components::text_button;
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
    let mut ok = false;
    let modal = egui::Modal::new(egui::Id::new("open_refused_modal"))
        .frame(egui::Frame::window(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Cannot open folder")
                        .color(p.error)
                        .strong()
                        .size(16.0),
                );
            });
            ui.add_space(SPACE_MEDIUM);
            ui.label(RichText::new(refusal).color(p.on_surface));
            ui.add_space(SPACE_LARGE);
            ui.vertical_centered(|ui| {
                if text_button(ui, "OK", p.primary) {
                    ok = true;
                }
            });
        });
    (ok || modal.should_close()).then_some(ProjectAction::DismissRefusal)
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
