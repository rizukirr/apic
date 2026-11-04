//! The contracts modals: new template, new request and delete confirmation.

use eframe::egui;
use egui::RichText;

use crate::features::contracts::domain::entry::DeleteTarget;
use crate::features::contracts::ui::state::{ContractsAction, ContractsState};
use crate::ui::components::{bordered_input, text_button};
use crate::ui::focus::take_pending_focus;
use crate::ui::theme::*;

// egui temp-data keys for the "focus the input when the dialog opens" markers,
// claimed once via `take_pending_focus` the first frame each modal renders.
pub(crate) const FOCUS_NEW_REQUEST: &str = "apic.focus.new_request";
pub(crate) const FOCUS_NEW_TEMPLATE: &str = "apic.focus.new_template";

/// Renders whichever of the new-template, new-request and delete dialogs is
/// open.
///
/// # Returns
///
/// The create, confirm or cancel action the user chose this frame.
pub(crate) fn dialogs(ctx: &egui::Context, state: &mut ContractsState) -> Option<ContractsAction> {
    let template = new_template_dialog(ctx, state);
    let request = new_request_dialog(ctx, state);
    let delete = delete_dialog(ctx, state);
    template.or(request).or(delete)
}

/// Renders the "new template" dialog when open, and returns what was chosen.
fn new_template_dialog(ctx: &egui::Context, state: &mut ContractsState) -> Option<ContractsAction> {
    let p = palette(ctx);
    state.new_template.as_ref()?;
    let mut create = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::new("new_template_modal"))
        .frame(egui::Frame::window(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("NEW TEMPLATE")
                        .color(p.primary)
                        .strong()
                        .size(16.0),
                );
            });
            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new("template name").color(p.on_surface_variant));
            ui.add_space(SPACE_MEDIUM);
            let buf = state.new_template.as_mut().expect("dialog open");
            let resp = bordered_input(ui, buf, f32::INFINITY, "");
            // Drop the caret into the input the frame the dialog opens.
            take_pending_focus(ui, FOCUS_NEW_TEMPLATE, "open", &resp);
            // Submit on Enter, same as clicking Create.
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                create = true;
            }
            ui.add_space(SPACE_LARGE);
            ui.columns(2, |cols| {
                cols[0].vertical_centered(|ui| {
                    if text_button(ui, "Create", p.primary) {
                        create = true;
                    }
                });
                cols[1].vertical_centered(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });
    if create {
        Some(ContractsAction::CreateTemplate)
    } else if cancel || modal.should_close() {
        Some(ContractsAction::CancelDialog)
    } else {
        None
    }
}

/// Renders the "new request" dialog when open, and returns what was chosen.
fn new_request_dialog(ctx: &egui::Context, state: &mut ContractsState) -> Option<ContractsAction> {
    let p = palette(ctx);
    state.new_request.as_ref()?;
    let mut create = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::new("new_request_modal"))
        .frame(egui::Frame::window(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("New request")
                        .color(p.primary)
                        .strong()
                        .size(16.0),
                );
            });
            ui.add_space(SPACE_SMALL);
            ui.label(
                RichText::new("path under the contracts directory").color(p.on_surface_variant),
            );
            ui.add_space(SPACE_MEDIUM);
            let buf = state.new_request.as_mut().expect("dialog open");
            let resp = bordered_input(ui, buf, f32::INFINITY, "");
            // Drop the caret into the input the frame the dialog opens.
            take_pending_focus(ui, FOCUS_NEW_REQUEST, "open", &resp);
            // Submit on Enter, same as clicking Create.
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                create = true;
            }
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

            ui.add_space(SPACE_LARGE);
            ui.columns(2, |cols| {
                cols[0].vertical_centered(|ui| {
                    if text_button(ui, "Create", p.primary) {
                        create = true;
                    }
                });
                cols[1].vertical_centered(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });
    if create {
        Some(ContractsAction::CreateRequest)
    } else if cancel || modal.should_close() {
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
    let mut confirm = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::new("delete_modal"))
        .frame(egui::Frame::window(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("DELETE").color(p.error).strong().size(16.0));
            });
            ui.add_space(SPACE_MEDIUM);
            ui.label(RichText::new(format!("Delete {what}")).color(p.on_surface_variant));
            ui.label(RichText::new(&name).color(p.on_surface).strong());
            if folder_warn {
                ui.add_space(SPACE_EXTRA_SMALL);
                ui.label(
                    RichText::new("this also deletes every contract inside it")
                        .color(p.error)
                        .size(10.0),
                );
            }
            ui.add_space(SPACE_LARGE);
            ui.columns(2, |cols| {
                cols[0].vertical_centered(|ui| {
                    if text_button(ui, "Delete", p.error) {
                        confirm = true;
                    }
                });
                cols[1].vertical_centered(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });
    if confirm {
        Some(ContractsAction::ConfirmDelete)
    } else if cancel || modal.should_close() {
        Some(ContractsAction::CancelDialog)
    } else {
        None
    }
}
