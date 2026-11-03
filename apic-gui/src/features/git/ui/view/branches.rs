//! The git sidebar's branch controls: the branch dropdown row and the create,
//! delete and discard dialogs.

use eframe::egui;
use egui::RichText;

use crate::features::git::domain::status::{DiscardTarget, discard_target};
use crate::features::git::ui::state::{GitAction, GitState};
use crate::ui::components::{bordered_input, text_button};
use crate::ui::theme::*;

/// The row above `STAGED`: a dropdown over every local branch, showing the
/// current one as its selected text (or the detached marker when there is
/// none), plus glyph buttons to create or delete a branch. Picking a branch
/// in the dropdown switches to it immediately, there is no separate Switch
/// control and no highlight distinct from the current branch: a row with two
/// competing notions of "selected" is worse than either.
///
/// In detached HEAD `Branches::current` is `None`, a real git state rather
/// than an error. Every branch is then a legitimate way out, so switching and
/// deleting both stay enabled instead of guessing a current branch.
pub(super) fn branch_row(ui: &mut egui::Ui, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    let current = state.branches.current.clone();
    let eligible: Vec<&String> = state
        .branches
        .all
        .iter()
        .filter(|name| Some(name.as_str()) != current.as_deref())
        .collect();

    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let can_delete = !eligible.is_empty();
            ui.add_enabled_ui(can_delete, |ui| {
                if ui
                    .small_button(RichText::new("x").color(p.error))
                    .on_hover_text("Delete a branch")
                    .clicked()
                    && let Some(name) = eligible.first()
                {
                    action = Some(GitAction::RequestBranchDelete {
                        name: (*name).clone(),
                    });
                }
            });
            if ui
                .small_button(RichText::new("+").color(p.success))
                .on_hover_text("Create a new branch")
                .clicked()
            {
                state.new_branch = Some(String::new());
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("git_branch_combo")
                    .selected_text(branch_label(current.as_deref(), p))
                    .show_ui(ui, |ui| {
                        for name in &state.branches.all {
                            let is_current = Some(name.as_str()) == current.as_deref();
                            if ui.selectable_label(is_current, name).clicked()
                                && let Some(a) = branch_select_action(name, current.as_deref())
                            {
                                action = Some(a);
                            }
                        }
                    });
            });
        });
    });

    action
}

/// What picking a branch from the dropdown should do: switch to it, or
/// nothing when it is already the current branch. Reselecting the current
/// branch is the ordinary case once the dropdown always shows it, and must
/// not spawn a redundant checkout.
pub(super) fn branch_select_action(name: &str, current: Option<&str>) -> Option<GitAction> {
    if Some(name) == current {
        None
    } else {
        Some(GitAction::SwitchBranch {
            name: name.to_string(),
        })
    }
}

/// The dropdown's selected text: the current branch name, or the detached
/// marker when `Branches::current` is `None`, the real git state for
/// detached HEAD rather than an error.
pub(super) fn branch_label(current: Option<&str>, p: &Palette) -> RichText {
    match current {
        Some(name) => RichText::new(name),
        None => RichText::new("(detached)").color(p.warning),
    }
}

/// The create-branch dialog, shaped like `new_template_dialog` in
/// `features::contracts::view` so the codebase does not grow a second
/// input-dialog style.
pub(super) fn create_branch_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    state.new_branch.as_ref()?;
    let mut create = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::new("create_branch_modal"))
        .frame(egui::Frame::window(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("NEW BRANCH")
                        .color(p.primary)
                        .strong()
                        .size(16.0),
                );
            });
            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new("branch name").color(p.on_surface_variant));
            ui.add_space(SPACE_MEDIUM);
            let buf = state.new_branch.as_mut().expect("dialog open");
            let resp = bordered_input(ui, buf, f32::INFINITY, "");
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
        let name = state.new_branch.take().unwrap_or_default();
        let name = name.trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(GitAction::CreateBranch { name })
        }
    } else if cancel || modal.should_close() {
        state.new_branch = None;
        None
    } else {
        None
    }
}

/// Delete confirmation for a branch, following the shape of `discard_dialog`
/// rather than inventing a second confirmation style. The `x` button no
/// longer names a fixed target, since there is no highlight left to read one
/// from, so this single dialog carries its own picker: every branch except
/// the current one, defaulting to whichever branch it was opened for.
pub(super) fn delete_branch_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    state.pending_branch_delete.as_ref()?;
    let current = state.branches.current.clone();
    let eligible: Vec<String> = state
        .branches
        .all
        .iter()
        .filter(|name| Some(name.as_str()) != current.as_deref())
        .cloned()
        .collect();
    if eligible.is_empty() {
        // Defensively close rather than show an empty picker: the branch
        // list can change under the dialog, e.g. after a refresh.
        state.pending_branch_delete = None;
        return None;
    }
    let picked = state.pending_branch_delete.as_mut().expect("checked above");
    if !eligible.contains(picked) {
        *picked = eligible[0].clone();
    }

    let mut confirm = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::new("delete_branch_modal"))
        .frame(egui::Frame::window(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("DELETE BRANCH")
                        .color(p.error)
                        .strong()
                        .size(16.0),
                );
            });
            ui.add_space(SPACE_MEDIUM);
            ui.label(RichText::new("Delete branch").color(p.on_surface_variant));
            let picked = state.pending_branch_delete.as_mut().expect("dialog open");
            egui::ComboBox::from_id_salt("delete_branch_combo")
                .selected_text(picked.clone())
                .show_ui(ui, |ui| {
                    for name in &eligible {
                        ui.selectable_value(picked, name.clone(), name);
                    }
                });
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
        Some(GitAction::ConfirmBranchDelete)
    } else if cancel || modal.should_close() {
        state.pending_branch_delete = None;
        None
    } else {
        None
    }
}

/// Discard confirmation modal, shown when a discard is pending. Follows the
/// shape of `delete_dialog` in `features::contracts::view`, so the codebase
/// does not grow a second confirmation style.
pub(super) fn discard_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    let path = state.pending_discard.clone()?;
    let mut confirm = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::new("discard_modal"))
        .frame(egui::Frame::window(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("DISCARD").color(p.error).strong().size(16.0));
            });
            ui.add_space(SPACE_MEDIUM);
            match discard_target(&state.status, &path) {
                DiscardTarget::File => {
                    ui.label(RichText::new("Discard changes to").color(p.on_surface_variant));
                    ui.label(RichText::new(&path).color(p.on_surface).strong());
                }
                DiscardTarget::Folder(count) => {
                    ui.label(RichText::new("Discard changes in").color(p.on_surface_variant));
                    ui.label(RichText::new(&path).color(p.on_surface).strong());
                    ui.label(
                        RichText::new(format!(
                            "{count} tracked file{} will be reverted",
                            if count == 1 { "" } else { "s" }
                        ))
                        .color(p.on_surface_variant),
                    );
                }
            }
            ui.add_space(SPACE_LARGE);
            ui.columns(2, |cols| {
                cols[0].vertical_centered(|ui| {
                    if text_button(ui, "Discard", p.error) {
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
        Some(GitAction::ConfirmDiscard)
    } else if cancel || modal.should_close() {
        state.pending_discard = None;
        None
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_the_current_branch_returns_no_action() {
        assert!(branch_select_action("main", Some("main")).is_none());
    }

    #[test]
    fn selecting_a_different_branch_returns_a_switch() {
        match branch_select_action("feature", Some("main")) {
            Some(GitAction::SwitchBranch { name }) => assert_eq!(name, "feature"),
            _ => panic!("expected a switch action"),
        }
    }
}
