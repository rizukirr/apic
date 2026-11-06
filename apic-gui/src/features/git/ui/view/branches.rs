//! The git sidebar's branch controls: the branch dropdown row and the create,
//! delete and discard dialogs.

use eframe::egui;
use egui::RichText;
use egui::containers::menu::SubMenuButton;

use crate::features::git::domain::status::{Branches, DiscardTarget, discard_target};
use crate::features::git::ui::state::{BranchOpKind, GitAction, GitState};
use crate::ui::components::{ButtonKind, DialogButton, bordered_input, detail_box, dialog};
use crate::ui::theme::*;

/// Whether the menu may offer deleting `name`: not the current branch, which
/// git refuses to delete.
fn can_delete(name: &str, current: Option<&str>) -> bool {
    Some(name) != current
}

/// The menu and popup title for a branch operation.
fn op_label(kind: BranchOpKind) -> &'static str {
    match kind {
        BranchOpKind::Pull => "Pull",
        BranchOpKind::Merge => "Merge",
    }
}

/// The app-bar branch dropdown: every local branch (click the name to switch,
/// ⋮ for pull, merge and delete), then "+ New branch". Also requests the branch
/// list once, since this is drawn whether or not any git panel is open.
pub(crate) fn branch_menu(ui: &mut egui::Ui, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    if !state.branches_loaded && state.pending.is_none() {
        state.branches_loaded = true;
        action = Some(GitAction::RefreshBranches);
    }
    let current = state.branches.current.clone();
    let branches = state.branches.clone();
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
        // Every name gets the widest name's width, so the ⋮ buttons that
        // follow line up in one column.
        let font = egui::TextStyle::Button.resolve(ui.style());
        let name_width = branches
            .all
            .iter()
            .map(|name| {
                ui.fonts_mut(|f| {
                    f.layout_no_wrap(name.clone(), font.clone(), egui::Color32::PLACEHOLDER)
                        .size()
                        .x
                })
            })
            .fold(0.0, f32::max)
            + ui.spacing().button_padding.x * 2.0;
        for name in &branches.all {
            ui.horizontal(|ui| {
                let row = ui.add(
                    egui::Button::selectable(Some(name.as_str()) == current.as_deref(), name)
                        .min_size(egui::vec2(name_width, 0.0)),
                );
                if row.clicked() {
                    if let Some(a) = branch_select_action(name, current.as_deref()) {
                        action = Some(a);
                    }
                    ui.close();
                }
                SubMenuButton::from_button(egui::Button::new(
                    egui_phosphor::regular::DOTS_THREE_VERTICAL,
                ))
                .ui(ui, |ui| {
                    for kind in [BranchOpKind::Pull, BranchOpKind::Merge] {
                        ui.menu_button(op_label(kind), |ui| {
                            let sources = branch_op_sources(kind, name, &branches);
                            if sources.is_empty() {
                                ui.add_enabled(false, egui::Button::new("no branches"));
                            }
                            for source in sources {
                                if ui.button(&source).clicked() {
                                    action = Some(GitAction::RequestBranchOp {
                                        kind,
                                        target: name.clone(),
                                        source,
                                    });
                                    ui.close();
                                }
                            }
                        });
                    }
                    if ui
                        .add_enabled(
                            can_delete(name, current.as_deref()),
                            egui::Button::new("Delete"),
                        )
                        .clicked()
                    {
                        action = Some(GitAction::RequestBranchDelete { name: name.clone() });
                        ui.close();
                    }
                });
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

/// The pull or merge confirmation: names the source and the target branch,
/// both picked in the branch menu.
pub(super) fn branch_op_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    let op = state.pending_branch_op.clone()?;
    let title = op_label(op.kind);
    let outcome = dialog(
        ctx,
        "branch_op_modal",
        title,
        |ui| {
            ui.label(RichText::new("from").color(p.on_surface_variant));
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                ui.label(RichText::new(&op.source).color(p.on_surface).strong());
            });
            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new("into branch").color(p.on_surface_variant));
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                ui.label(RichText::new(&op.target).color(p.on_surface).strong());
            });
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new(title, ButtonKind::Primary),
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
    fn delete_is_offered_off_the_current_branch_only() {
        assert!(can_delete("feature", Some("main")));
        assert!(!can_delete("main", Some("main")));
        assert!(can_delete("main", None));
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
