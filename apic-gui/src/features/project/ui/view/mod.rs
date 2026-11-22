//! Project feature rendering: the welcome screen shown while no project is
//! open, and the popup shown when a picked folder cannot be opened.

use eframe::egui;

use crate::features::project::data::ImportSource;
use crate::features::project::ui::state::{ProjectAction, ProjectState};
use crate::ui::base::{ButtonKind, Size, Text, button, menu_item};
use crate::ui::components::{DialogButton, dialog};
use crate::ui::theme::*;

/// The welcome screen for when no project is open: a title, a hint and the
/// New, Open and Import buttons, centred in `ui`.
///
/// # Returns
///
/// The project action a button or the Import menu asks for this frame.
/// The file formats a project imports from and exports to, as menu items.
/// Closes the menu on a click.
///
/// # Returns
///
/// The format clicked, if any.
pub(crate) fn format_items(ui: &mut egui::Ui) -> Option<ImportSource> {
    let mut picked = None;
    for (label, source) in [
        ("Postman collection", ImportSource::Postman),
        ("OpenAPI spec", ImportSource::OpenApi),
    ] {
        if menu_item(ui, label) {
            picked = Some(source);
        }
    }
    picked
}

pub(crate) fn welcome(ui: &mut egui::Ui) -> Option<ProjectAction> {
    let mut action = None;
    let rect = ui.available_rect_before_wrap();
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            // About the height of the three rows below, so they sit centred.
            ui.add_space(((rect.height() - 110.0) / 2.0).max(0.0));
            Text::new("Welcome to apic").size(Size::Display).show(ui);
            ui.add_space(SPACE_SMALL);
            Text::new("Open a project folder, or create a new one.")
                .muted()
                .show(ui);
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
                    if button(ui, "New project", ButtonKind::Primary).clicked() {
                        action = Some(ProjectAction::New);
                    }
                    if button(ui, "Open project", ButtonKind::Standard).clicked() {
                        action = Some(ProjectAction::Open);
                    }
                    let import = format!("Import {}", egui_phosphor::regular::CARET_DOWN);
                    ui.menu_button(Text::new(import).rich(ui.ctx()), |ui| {
                        if let Some(source) = format_items(ui) {
                            action = Some(ProjectAction::Import(source));
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
    let refusal = state.refusal.as_ref()?;
    let outcome = dialog(
        ctx,
        "open_refused_modal",
        "Cannot open folder",
        |ui| {
            Text::new(refusal).show(ui);
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
