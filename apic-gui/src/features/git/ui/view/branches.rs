//! The git sidebar's branch controls: the branch dropdown row and the create,
//! delete and discard dialogs.

use eframe::egui;
use egui::RichText;

use crate::features::git::domain::status::{Branches, DiscardTarget, discard_target};
use crate::features::git::ui::state::{BranchOpKind, GitAction, GitState};
use crate::ui::components::{ButtonKind, DialogButton, bordered_input, detail_box, dialog};
use crate::ui::theme::*;

/// The menu entries for one branch: pull into it, merge into it, and delete
/// it. Delete is enabled unless it is the current branch, which git refuses
/// to delete.
fn branch_menu_items(name: &str, current: Option<&str>) -> Vec<(&'static str, bool, GitAction)> {
    vec![
        (
            "Pull",
            true,
            GitAction::RequestBranchOp {
                kind: BranchOpKind::Pull,
                target: name.to_string(),
            },
        ),
        (
            "Merge",
            true,
            GitAction::RequestBranchOp {
                kind: BranchOpKind::Merge,
                target: name.to_string(),
            },
        ),
        (
            "Delete",
            Some(name) != current,
            GitAction::RequestBranchDelete {
                name: name.to_string(),
            },
        ),
    ]
}

/// The app-bar branch dropdown: every local branch (click to switch,
/// right-click for its menu), then "+ New branch". Also requests the branch
/// list once, since this is drawn whether or not any git panel is open.
pub(crate) fn branch_menu(ui: &mut egui::Ui, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    if !state.branches_loaded && state.pending.is_none() {
        state.branches_loaded = true;
        action = Some(GitAction::RefreshBranches);
    }
    let current = state.branches.current.clone();
    let branches = state.branches.all.clone();
    let mut open_new = false;
    let title = format!(
        "{} {} {}",
        egui_phosphor::regular::GIT_BRANCH,
        current.as_deref().unwrap_or("(detached)"),
        egui_phosphor::regular::CARET_DOWN
    );
    let color = if current.is_some() {
        p.on_surface
    } else {
        p.warning
    };
    ui.menu_button(RichText::new(title).color(color), |ui| {
        for name in &branches {
            let row = ui.selectable_label(Some(name.as_str()) == current.as_deref(), name);
            if row.clicked() {
                if let Some(a) = branch_select_action(name, current.as_deref()) {
                    action = Some(a);
                }
                ui.close();
            }
            row.context_menu(|ui| {
                for (label, enabled, a) in branch_menu_items(name, current.as_deref()) {
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                        action = Some(a);
                        ui.close();
                    }
                }
            });
        }
        ui.separator();
        if ui
            .button(format!("{} New branch", egui_phosphor::regular::PLUS))
            .clicked()
        {
            open_new = true;
            ui.close();
        }
    });
    if open_new {
        state.new_branch = Some(String::new());
    }
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

/// The create-branch dialog, drawn in the shared popup layout.
pub(super) fn create_branch_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    let buf = state.new_branch.as_mut()?;
    let mut enter = false;
    let outcome = dialog(
        ctx,
        "create_branch_modal",
        "New branch",
        |ui| {
            ui.label(RichText::new("branch name").color(p.on_surface_variant));
            ui.add_space(SPACE_SMALL);
            let resp = bordered_input(ui, buf, f32::INFINITY, "");
            enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Create", ButtonKind::Primary),
        ],
    );
    if enter || outcome.clicked == Some(1) {
        let name = state.new_branch.take().unwrap_or_default();
        let name = name.trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(GitAction::CreateBranch { name })
        }
    } else if outcome.clicked == Some(0) || outcome.closed {
        state.new_branch = None;
        None
    } else {
        None
    }
}

/// Delete confirmation for the right-clicked branch. The target is fixed:
/// the dialog names it and offers no picker.
pub(super) fn delete_branch_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    let name = state.pending_branch_delete.clone()?;
    let outcome = dialog(
        ctx,
        "delete_branch_modal",
        "Delete branch",
        |ui| {
            ui.label(RichText::new("Delete branch").color(p.on_surface_variant));
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                ui.label(RichText::new(&name).color(p.on_surface).strong());
            });
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Delete", ButtonKind::Danger),
        ],
    );
    if outcome.clicked == Some(1) {
        Some(GitAction::ConfirmBranchDelete)
    } else if outcome.clicked == Some(0) || outcome.closed {
        state.pending_branch_delete = None;
        None
    } else {
        None
    }
}

/// The branches a pull or merge into `target` may bring in: remote branches
/// for a pull, every other local branch then every remote branch for a merge.
pub(super) fn branch_op_sources(
    kind: BranchOpKind,
    target: &str,
    branches: &Branches,
) -> Vec<String> {
    match kind {
        BranchOpKind::Pull => branches.remotes.clone(),
        BranchOpKind::Merge => branches
            .all
            .iter()
            .filter(|name| name.as_str() != target)
            .chain(branches.remotes.iter())
            .cloned()
            .collect(),
    }
}

/// The pull or merge popup: the right-clicked branch shown fixed, and a
/// picker for the branch to bring into it. The confirm button is disabled
/// when there is nothing to pick.
pub(super) fn branch_op_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    let op = state.pending_branch_op.as_ref()?;
    let sources = branch_op_sources(op.kind, &op.target, &state.branches);
    let (title, empty) = match op.kind {
        BranchOpKind::Pull => ("Pull", "no branches to pull from"),
        BranchOpKind::Merge => ("Merge", "no branches to merge"),
    };
    let op = state.pending_branch_op.as_mut().expect("checked above");
    if !sources.contains(&op.source) {
        op.source = sources.first().cloned().unwrap_or_default();
    }
    let target = op.target.clone();
    let outcome = dialog(
        ctx,
        "branch_op_modal",
        title,
        |ui| {
            ui.label(RichText::new("into branch").color(p.on_surface_variant));
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                ui.label(RichText::new(&target).color(p.on_surface).strong());
            });
            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new("from").color(p.on_surface_variant));
            if sources.is_empty() {
                ui.label(RichText::new(empty).color(p.on_surface_variant));
            } else {
                egui::ComboBox::from_id_salt("branch_op_combo")
                    .selected_text(op.source.clone())
                    .show_ui(ui, |ui| {
                        for name in &sources {
                            ui.selectable_value(&mut op.source, name.clone(), name);
                        }
                    });
            }
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton {
                label: title,
                kind: ButtonKind::Primary,
                enabled: !sources.is_empty(),
            },
        ],
    );
    if outcome.clicked == Some(1) {
        Some(GitAction::ConfirmBranchOp)
    } else if outcome.clicked == Some(0) || outcome.closed {
        state.pending_branch_op = None;
        None
    } else {
        None
    }
}

/// Discard confirmation modal, shown when a discard is pending, drawn in the
/// shared popup layout.
pub(super) fn discard_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    let path = state.pending_discard.clone()?;
    let target = discard_target(&state.status, &path);
    let outcome = dialog(
        ctx,
        "discard_modal",
        "Discard changes",
        |ui| match target {
            DiscardTarget::File => {
                ui.label(RichText::new("Discard changes to").color(p.on_surface_variant));
                ui.add_space(SPACE_SMALL);
                detail_box(ui, |ui| {
                    ui.label(RichText::new(&path).color(p.on_surface).strong());
                });
            }
            DiscardTarget::Folder(count) => {
                ui.label(RichText::new("Discard changes in").color(p.on_surface_variant));
                ui.add_space(SPACE_SMALL);
                detail_box(ui, |ui| {
                    ui.label(RichText::new(&path).color(p.on_surface).strong());
                    ui.label(
                        RichText::new(format!(
                            "{count} tracked file{} will be reverted",
                            if count == 1 { "" } else { "s" }
                        ))
                        .color(p.on_surface_variant),
                    );
                });
            }
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Discard", ButtonKind::Danger),
        ],
    );
    if outcome.clicked == Some(1) {
        Some(GitAction::ConfirmDiscard)
    } else if outcome.clicked == Some(0) || outcome.closed {
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

    #[test]
    fn branch_menu_offers_pull_merge_and_delete() {
        let other = branch_menu_items("feature", Some("main"));
        let labels: Vec<&str> = other.iter().map(|(l, _, _)| *l).collect();
        assert_eq!(labels, ["Pull", "Merge", "Delete"]);
        assert!(other.iter().all(|(_, enabled, _)| *enabled));
        assert!(other.iter().any(|(_, _, a)| matches!(
            a,
            GitAction::RequestBranchOp { kind: BranchOpKind::Pull, target } if target == "feature"
        )));
        assert!(other.iter().any(|(_, _, a)| matches!(
            a,
            GitAction::RequestBranchOp { kind: BranchOpKind::Merge, target } if target == "feature"
        )));
        assert!(other.iter().any(|(_, _, a)| matches!(
            a,
            GitAction::RequestBranchDelete { name } if name == "feature"
        )));
    }

    #[test]
    fn delete_is_disabled_on_the_current_branch_only() {
        let current = branch_menu_items("main", Some("main"));
        for (label, enabled, _) in current {
            assert_eq!(enabled, label != "Delete", "{label}");
        }
    }

    #[test]
    fn pull_sources_are_remote_branches() {
        let b = Branches {
            current: Some("main".into()),
            all: vec!["main".into(), "feature".into()],
            remotes: vec!["origin/main".into()],
        };
        assert_eq!(
            branch_op_sources(BranchOpKind::Pull, "main", &b),
            ["origin/main"]
        );
    }

    #[test]
    fn merge_sources_are_other_locals_then_remotes() {
        let b = Branches {
            current: Some("main".into()),
            all: vec!["main".into(), "feature".into()],
            remotes: vec!["origin/main".into()],
        };
        assert_eq!(
            branch_op_sources(BranchOpKind::Merge, "main", &b),
            ["feature", "origin/main"]
        );
    }
}
