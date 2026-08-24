//! The git panel's rendering: the sidebar body (status list, refresh) and the
//! central body (diff view). Both bodies fill a frame the app shell owns, the
//! same shape as `features::contracts::view`.

use std::path::Path;

use eframe::egui;
use egui::RichText;

use crate::app::actions::GitAction;
use crate::features::git::model::{Change, FileStatus};
use crate::features::git::state::GitState;
use crate::ui::theme::*;

/// Left sidebar body for the Git tab: a heading, a refresh button, the error
/// line when set, and the file list split into staged and unstaged sections.
///
/// The panel frame is owned by the app shell, not by this feature, so that a
/// second sidebar tab can render into the same frame; this only fills it.
pub(crate) fn sidebar_body(
    ui: &mut egui::Ui,
    state: &mut GitState,
    repo_root: Option<&Path>,
) -> Option<GitAction> {
    let mut action = None;
    ui.add_space(SPACE_MEDIUM);
    ui.horizontal(|ui| {
        ui.label(RichText::new("GIT").color(GREEN).strong().size(16.0));
        if ui
            .small_button(RichText::new("⟳").color(GREEN))
            .on_hover_text("Refresh status")
            .clicked()
        {
            action = Some(GitAction::Refresh);
        }
    });
    ui.separator();

    if !state.error.is_empty() {
        ui.label(RichText::new(&state.error).color(RED));
        ui.add_space(SPACE_SMALL);
    }

    if repo_root.is_none() {
        ui.label(RichText::new("Not inside a git repository.").color(DIM));
        return action;
    }

    let selected = state.selected.clone();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(RichText::new("STAGED").color(DIM).size(11.0));
            let staged: Vec<&FileStatus> = state
                .status
                .inside
                .iter()
                .filter(|f| f.index != Change::Unmodified)
                .collect();
            if staged.is_empty() {
                ui.label(RichText::new("(none)").color(DIM));
            }
            for file in &staged {
                if let Some(a) = file_row(ui, file, true, selected.as_ref()) {
                    action = Some(a);
                }
            }

            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new("UNSTAGED").color(DIM).size(11.0));
            let unstaged: Vec<&FileStatus> = state
                .status
                .inside
                .iter()
                .filter(|f| f.worktree != Change::Unmodified)
                .collect();
            if unstaged.is_empty() {
                ui.label(RichText::new("(none)").color(DIM));
            }
            for file in &unstaged {
                if let Some(a) = file_row(ui, file, false, selected.as_ref()) {
                    action = Some(a);
                }
            }

            if !state.status.outside.is_empty() {
                ui.add_space(SPACE_SMALL);
                ui.separator();
                let arrow = if state.show_outside { "▼" } else { "▶" };
                let label = format!(
                    "{} {} change(s) outside this project",
                    arrow,
                    state.status.outside.len()
                );
                if ui
                    .selectable_label(false, RichText::new(label).color(DIM))
                    .clicked()
                {
                    state.show_outside = !state.show_outside;
                }
                if state.show_outside {
                    for file in &state.status.outside {
                        let staged = file.index != Change::Unmodified;
                        if let Some(a) = file_row(ui, file, staged, selected.as_ref()) {
                            action = Some(a);
                        }
                    }
                }
            }
        });

    action
}

/// One changed-file row: the change letter, the file name (truncated), and a
/// red conflict indicator when the file is in a merge conflict.
///
/// The conflict indicator is reserved first with a right-to-left layout, and
/// the label is nested inside a left-to-right layout marked `.truncate()`,
/// the pattern in `features::contracts::view` so a long file name truncates
/// instead of forcing the panel wider than its dragged width.
fn file_row(
    ui: &mut egui::Ui,
    file: &FileStatus,
    staged: bool,
    selected: Option<&(String, bool)>,
) -> Option<GitAction> {
    let mut action = None;
    let change = if staged { file.index } else { file.worktree };
    let name = Path::new(&file.path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.path.clone());
    let is_selected = selected == Some(&(file.path.clone(), staged));
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if file.conflicted {
                ui.label(RichText::new("●").color(RED))
                    .on_hover_text("Conflicted");
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(RichText::new(change.letter()).color(DIM));
                let button = egui::Button::selectable(is_selected, RichText::new(name).color(TEXT))
                    .truncate();
                if ui.add(button).clicked() {
                    action = Some(GitAction::Select {
                        path: file.path.clone(),
                        staged,
                    });
                }
            });
        });
    });
    action
}

/// The central body for the Git tab: the diff view once selected, an empty
/// state until then.
///
/// The panel frame is owned by the app shell. This only sees the git slice.
pub(crate) fn central_body(ui: &mut egui::Ui, _state: &GitState) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("No file selected").color(DIM).size(16.0));
        ui.add_space(SPACE_SMALL);
        ui.label(RichText::new("Select a changed file on the left.").color(DIM));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::model::Status;

    #[test]
    fn sidebar_and_central_render_without_panicking() {
        eframe::egui::__run_test_ui(|ui| {
            let mut state = GitState::default();
            sidebar_body(ui, &mut state, None);
            central_body(ui, &state);
        });
    }

    #[test]
    fn sidebar_renders_file_list_without_panicking() {
        let status = Status {
            inside: vec![
                FileStatus {
                    path: "src/staged.rs".into(),
                    index: Change::Added,
                    worktree: Change::Unmodified,
                    conflicted: false,
                },
                FileStatus {
                    path: "src/unstaged.rs".into(),
                    index: Change::Unmodified,
                    worktree: Change::Modified,
                    conflicted: false,
                },
                FileStatus {
                    path: "src/untracked.rs".into(),
                    index: Change::Untracked,
                    worktree: Change::Untracked,
                    conflicted: false,
                },
                FileStatus {
                    path: "src/conflicted.rs".into(),
                    index: Change::Modified,
                    worktree: Change::Modified,
                    conflicted: true,
                },
            ],
            outside: vec![FileStatus {
                path: "apic-core/src/lib.rs".into(),
                index: Change::Modified,
                worktree: Change::Unmodified,
                conflicted: false,
            }],
        };
        eframe::egui::__run_test_ui(|ui| {
            let mut state = GitState {
                status: status.clone(),
                show_outside: true,
                ..GitState::default()
            };
            sidebar_body(ui, &mut state, Some(Path::new("/repo")));
            central_body(ui, &state);
        });
    }
}
