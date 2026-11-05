//! Project feature rendering: the welcome screen shown while no project is
//! open, and the popup shown when a picked folder cannot be opened.

use eframe::egui;
use egui::RichText;

use crate::features::project::ui::state::{ProjectAction, ProjectState};
use crate::ui::components::{ButtonKind, DialogButton, dialog};
use crate::ui::theme::*;

/// The welcome screen for when no project is open: a title, a hint and the
/// New, Open and Import buttons, centred in `ui`.
///
/// # Returns
///
/// The project action a button or the Import menu asks for this frame.
pub(crate) fn welcome(ui: &mut egui::Ui) -> Option<ProjectAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    let rect = ui.available_rect_before_wrap();
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            // About the height of the three rows below, so they sit centred.
            ui.add_space(((rect.height() - 110.0) / 2.0).max(0.0));
            ui.label(
                RichText::new("Welcome to apic")
                    .color(p.on_surface)
                    .size(22.0),
            );
            ui.add_space(SPACE_SMALL);
            ui.label(
                RichText::new("Open a project folder, or create a new one.")
                    .color(p.on_surface_variant),
            );
            ui.add_space(SPACE_MEDIUM);
            // A horizontal row starts at its left edge, so it goes in a child
            // sized to the width the row took last frame, which the centred
            // layout then centres.
            let width_id = ui.id().with("welcome_button_row");
            let remembered = ui.data(|d| d.get_temp::<f32>(width_id));
            let row_width = remembered.unwrap_or(300.0).min(rect.width());
            ui.allocate_ui_with_layout(
                egui::vec2(row_width, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let new = egui::Button::new(RichText::new("New project").color(p.on_primary))
                        .fill(p.primary)
                        .corner_radius(4)
                        .min_size(egui::vec2(0.0, 28.0));
                    if ui.add(new).clicked() {
                        action = Some(ProjectAction::New);
                    }
                    let open = egui::Button::new(RichText::new("Open project").color(p.on_surface))
                        .min_size(egui::vec2(0.0, 28.0));
                    if ui.add(open).clicked() {
                        action = Some(ProjectAction::Open);
                    }
                    let import = format!("Import {}", egui_phosphor::regular::CARET_DOWN);
                    ui.menu_button(RichText::new(import).color(p.on_surface), |ui| {
                        if ui.button("Postman collection").clicked() {
                            action = Some(ProjectAction::ImportPostman);
                            ui.close();
                        }
                    });
                    let used = ui.min_rect().width();
                    if remembered.is_none_or(|w| (used - w).abs() > 0.5) {
                        ui.data_mut(|d| d.insert_temp(width_id, used));
                        ui.ctx().request_repaint();
                    }
                },
            );
        },
    );
    action
}

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
    fn welcome_screen_renders_in_both_themes() {
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                assert_eq!(welcome(ui), None, "nothing is clicked");
            });
        }
    }

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
