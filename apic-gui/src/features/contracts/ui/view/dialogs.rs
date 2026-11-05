//! The contracts modals: new template, new request, new example, delete
//! confirmation, unsaved changes and the body-type switch.

use eframe::egui;
use egui::RichText;

use crate::features::contracts::domain::entry::DeleteTarget;
use crate::features::contracts::ui::state::{ContractsAction, ContractsState};
use crate::ui::components::{ButtonKind, DialogButton, bordered_input, detail_box, dialog};
use crate::ui::focus::take_pending_focus;
use crate::ui::theme::*;

// egui temp-data keys for the "focus the input when the dialog opens" markers,
// claimed once via `take_pending_focus` the first frame each modal renders.
pub(crate) const FOCUS_NEW_REQUEST: &str = "apic.focus.new_request";
pub(crate) const FOCUS_NEW_TEMPLATE: &str = "apic.focus.new_template";

// egui temp-data key set once the new-example popup has focused its code
// field, so the focus lands only on the frame the popup opens.
const NEW_EXAMPLE_FOCUSED: &str = "apic.new_example.focused";

/// Renders whichever of the new-template, new-request, new-example, delete,
/// unsaved-changes and body-switch dialogs is open.
///
/// # Returns
///
/// The create, confirm or cancel action the user chose this frame.
pub(crate) fn dialogs(ctx: &egui::Context, state: &mut ContractsState) -> Option<ContractsAction> {
    let template = new_template_dialog(ctx, state);
    let request = new_request_dialog(ctx, state);
    let example = new_example_dialog(ctx, state);
    let delete = delete_dialog(ctx, state);
    let unsaved = unsaved_dialog(ctx, state);
    let body_switch = body_switch_dialog(ctx, state);
    template
        .or(request)
        .or(example)
        .or(delete)
        .or(unsaved)
        .or(body_switch)
}

/// Renders the "new template" dialog when open, and returns what was chosen.
fn new_template_dialog(ctx: &egui::Context, state: &mut ContractsState) -> Option<ContractsAction> {
    let p = palette(ctx);
    let buf = state.new_template.as_mut()?;
    let mut enter = false;
    let outcome = dialog(
        ctx,
        "new_template_modal",
        "New template",
        |ui| {
            ui.label(RichText::new("template name").color(p.on_surface_variant));
            ui.add_space(SPACE_SMALL);
            let resp = bordered_input(ui, buf, f32::INFINITY, "");
            // Drop the caret into the input the frame the dialog opens.
            take_pending_focus(ui, FOCUS_NEW_TEMPLATE, "open", &resp);
            // Submit on Enter, same as clicking Create.
            enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Create", ButtonKind::Primary),
        ],
    );
    if enter || outcome.clicked == Some(1) {
        Some(ContractsAction::CreateTemplate)
    } else if outcome.clicked == Some(0) || outcome.closed {
        Some(ContractsAction::CancelDialog)
    } else {
        None
    }
}

/// Renders the "new request" dialog when open, and returns what was chosen.
fn new_request_dialog(ctx: &egui::Context, state: &mut ContractsState) -> Option<ContractsAction> {
    let p = palette(ctx);
    state.new_request.as_ref()?;
    let mut enter = false;
    let outcome = dialog(
        ctx,
        "new_request_modal",
        "New request",
        |ui| {
            ui.label(
                RichText::new("path under the contracts directory").color(p.on_surface_variant),
            );
            ui.add_space(SPACE_SMALL);
            let buf = state.new_request.as_mut().expect("dialog open");
            let resp = bordered_input(ui, buf, f32::INFINITY, "");
            // Drop the caret into the input the frame the dialog opens.
            take_pending_focus(ui, FOCUS_NEW_REQUEST, "open", &resp);
            // Submit on Enter, same as clicking Create.
            enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(SPACE_EXTRA_SMALL);
            ui.label(
                RichText::new(
                    "end with .json for a contract (auth/logout.json), a bare name makes a folder",
                )
                .color(p.on_surface_variant)
                .size(10.0),
            );

            // With more than one template, let the user pick which one seeds
            // the contract. With one it is used automatically; with none the
            // built-in default is used.
            if state.templates.len() > 1 {
                let names: Vec<String> = state.templates.iter().map(|(n, _)| n.clone()).collect();
                let current = names
                    .get(state.new_request_seed)
                    .cloned()
                    .unwrap_or_default();
                ui.add_space(SPACE_MEDIUM);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("template").color(p.on_surface_variant));
                    egui::ComboBox::from_id_salt("new_request_template")
                        .selected_text(RichText::new(current).color(p.primary))
                        .show_ui(ui, |ui| {
                            for (i, name) in names.iter().enumerate() {
                                ui.selectable_value(&mut state.new_request_seed, i, name);
                            }
                        });
                });
            }
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Create", ButtonKind::Primary),
        ],
    );
    if enter || outcome.clicked == Some(1) {
        Some(ContractsAction::CreateRequest)
    } else if outcome.clicked == Some(0) || outcome.closed {
        Some(ContractsAction::CancelDialog)
    } else {
        None
    }
}

/// Renders the "new example" popup when open, and returns what was chosen.
fn new_example_dialog(ctx: &egui::Context, state: &mut ContractsState) -> Option<ContractsAction> {
    let focused_id = egui::Id::new(NEW_EXAMPLE_FOCUSED);
    let Some((code, name)) = state.new_example.as_mut() else {
        ctx.data_mut(|d| d.remove::<bool>(focused_id));
        return None;
    };
    let p = palette(ctx);
    // The footer is built before the body edits the code, so Add's enabled
    // state follows the code as it stood at the start of this frame.
    let add_enabled = code.len() == 3;
    let mut enter = false;
    let outcome = dialog(
        ctx,
        "new_example_modal",
        "New example",
        |ui| {
            let code_resp = ui
                .horizontal(|ui| {
                    ui.label(RichText::new("Code").color(p.on_surface_variant));
                    bordered_input(ui, code, f32::INFINITY, "200")
                })
                .inner;
            // A status code is exactly three digits.
            code.retain(|c| c.is_ascii_digit());
            code.truncate(3);
            let name_resp = ui
                .horizontal(|ui| {
                    ui.label(RichText::new("Name").color(p.on_surface_variant));
                    bordered_input(ui, name, f32::INFINITY, "")
                })
                .inner;
            // Drop the caret into the code field the frame the popup opens.
            if !ui.data(|d| d.get_temp::<bool>(focused_id).unwrap_or(false)) {
                code_resp.request_focus();
                ui.data_mut(|d| d.insert_temp(focused_id, true));
            }
            // Submit on Enter in either field, same as clicking Add.
            enter = ui.input(|i| i.key_pressed(egui::Key::Enter))
                && (code_resp.lost_focus() || name_resp.lost_focus());
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton {
                label: "Add",
                kind: ButtonKind::Primary,
                enabled: add_enabled,
            },
        ],
    );
    let valid = state
        .new_example
        .as_ref()
        .is_some_and(|(code, _)| code.len() == 3);
    if valid && (enter || outcome.clicked == Some(1)) {
        Some(ContractsAction::CreateExample)
    } else if outcome.clicked == Some(0) || outcome.closed {
        Some(ContractsAction::CancelDialog)
    } else {
        None
    }
}

/// Renders the delete-confirmation dialog when a delete is pending, and
/// returns what was chosen.
fn delete_dialog(ctx: &egui::Context, state: &mut ContractsState) -> Option<ContractsAction> {
    let p = palette(ctx);
    let target = state.pending_delete.clone()?;
    let (what, name, folder_warn) = match &target {
        DeleteTarget::Contract { rel, is_dir } => (
            if *is_dir { "folder" } else { "contract" },
            rel.clone(),
            *is_dir,
        ),
        DeleteTarget::Template { name, .. } => ("template", name.clone(), false),
    };
    let outcome = dialog(
        ctx,
        "delete_modal",
        &format!("Delete {what}"),
        |ui| {
            ui.label(
                RichText::new(format!("Are you sure you want to delete this {what}?"))
                    .color(p.on_surface),
            );
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                ui.label(RichText::new(&name).color(p.on_surface).strong());
            });
            if folder_warn {
                ui.add_space(SPACE_EXTRA_SMALL);
                ui.label(
                    RichText::new("this also deletes every contract inside it")
                        .color(p.error)
                        .size(10.0),
                );
            }
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Delete", ButtonKind::Danger),
        ],
    );
    if outcome.clicked == Some(1) {
        Some(ContractsAction::ConfirmDelete)
    } else if outcome.clicked == Some(0) || outcome.closed {
        Some(ContractsAction::CancelDialog)
    } else {
        None
    }
}

/// Renders the unsaved-changes popup while an action is held, and returns
/// what was chosen.
fn unsaved_dialog(ctx: &egui::Context, state: &ContractsState) -> Option<ContractsAction> {
    state.pending_discard?;
    let p = palette(ctx);
    let name = state
        .model
        .as_ref()
        .map(|m| m.name.trim())
        .filter(|n| !n.is_empty())
        .unwrap_or("This contract");
    let outcome = dialog(
        ctx,
        "unsaved_modal",
        "Unsaved changes",
        |ui| {
            ui.label(RichText::new(format!("{name} has unsaved changes.")).color(p.on_surface));
        },
        &[
            DialogButton::new("Keep editing", ButtonKind::Secondary),
            DialogButton::new("Discard", ButtonKind::SecondaryDanger),
            DialogButton::new("Save", ButtonKind::Primary),
        ],
    );
    match outcome.clicked {
        Some(1) => Some(ContractsAction::DiscardAndContinue),
        Some(2) => Some(ContractsAction::SaveAndContinue),
        Some(_) => Some(ContractsAction::KeepEditing),
        None if outcome.closed => Some(ContractsAction::KeepEditing),
        None => None,
    }
}

/// Renders the body-type switch confirmation while a switch is pending, and
/// returns what was chosen.
fn body_switch_dialog(ctx: &egui::Context, state: &ContractsState) -> Option<ContractsAction> {
    let (_, multipart) = state.pending_body_switch.as_ref()?;
    let p = palette(ctx);
    let (title, line) = if *multipart {
        ("Switch to multipart?", "The JSON body will be removed.")
    } else {
        ("Switch to JSON?", "The multipart parts will be removed.")
    };
    let outcome = dialog(
        ctx,
        "body_switch_modal",
        title,
        |ui| {
            ui.label(RichText::new(line).color(p.on_surface));
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Switch", ButtonKind::Danger),
        ],
    );
    if outcome.clicked == Some(1) {
        Some(ContractsAction::ConfirmBodySwitch)
    } else if outcome.clicked == Some(0) || outcome.closed {
        Some(ContractsAction::CancelBodySwitch)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::contracts::ui::state::Guarded;
    use crate::ui::theme::{BOTH_THEMES, run_in_theme};
    use apic_core::edit::BodyLoc;

    #[test]
    fn body_switch_popup_renders_in_both_themes() {
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                let ctx = ui.ctx().clone();
                for pending in [(BodyLoc::Request, true), (BodyLoc::Response(0), false)] {
                    let mut state = ContractsState {
                        pending_body_switch: Some(pending.clone()),
                        ..Default::default()
                    };
                    assert!(dialogs(&ctx, &mut state).is_none());
                    assert_eq!(state.pending_body_switch, Some(pending));
                }
            });
        }
    }

    #[test]
    fn new_example_popup_renders_in_both_themes() {
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                let ctx = ui.ctx().clone();
                let mut state = ContractsState {
                    new_example: Some(("404".to_string(), "Not found".to_string())),
                    ..Default::default()
                };
                dialogs(&ctx, &mut state);
                state.new_example = Some(("4x".to_string(), String::new()));
                dialogs(&ctx, &mut state);
                assert_eq!(
                    state.new_example.as_ref().map(|(code, _)| code.as_str()),
                    Some("4")
                );
            });
        }
    }

    #[test]
    fn unsaved_popup_renders_in_both_themes() {
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                let ctx = ui.ctx().clone();
                let mut state = ContractsState {
                    pending_discard: Some(Guarded::CloseWindow),
                    ..Default::default()
                };
                assert!(dialogs(&ctx, &mut state).is_none());
                assert_eq!(state.pending_discard, Some(Guarded::CloseWindow));
            });
        }
    }
}
