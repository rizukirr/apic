//! The git panel's rendering: the sidebar body (status list, refresh) and the
//! central body (diff view). Both bodies fill a frame the app shell owns, the
//! same shape as `features::contracts::view`.

use std::path::Path;

use eframe::egui;
use egui::RichText;

use crate::app::actions::GitAction;
use crate::features::git::diff::{self, FieldChange};
use crate::features::git::model::{Change, FileStatus};
use crate::features::git::state::GitState;
use crate::ui::components::text_button;
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

    egui::Panel::bottom("git_commit_bar")
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.add_space(SPACE_EXTRA_SMALL);
            ui.add(
                egui::TextEdit::multiline(&mut state.commit_message)
                    .hint_text("commit message")
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(SPACE_EXTRA_SMALL);
            let has_staged = state
                .status
                .inside
                .iter()
                .chain(state.status.outside.iter())
                .any(|f| f.index != Change::Unmodified);
            let enabled = !state.commit_message.trim().is_empty() && has_staged;
            ui.add_enabled_ui(enabled, |ui| {
                if text_button(ui, "Commit", GREEN) {
                    action = Some(GitAction::Commit);
                }
            });
            ui.add_space(SPACE_EXTRA_SMALL);
        });

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

    if let Some(a) = discard_dialog(ui.ctx(), state) {
        action = Some(a);
    }

    action
}

/// Discard confirmation modal, shown when a discard is pending. Follows the
/// shape of `delete_dialog` in `features::contracts::view`, so the codebase
/// does not grow a second confirmation style.
fn discard_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let path = state.pending_discard.clone()?;
    let mut confirm = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::new("discard_modal"))
        .frame(egui::Frame::window(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("DISCARD").color(RED).strong().size(16.0));
            });
            ui.add_space(SPACE_MEDIUM);
            ui.label(RichText::new("Discard changes to").color(DIM));
            ui.label(RichText::new(&path).color(TEXT).strong());
            ui.add_space(SPACE_LARGE);
            ui.columns(2, |cols| {
                cols[0].vertical_centered(|ui| {
                    if text_button(ui, "Discard", RED) {
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
            if staged {
                if ui
                    .small_button(RichText::new("Unstage").color(DIM))
                    .clicked()
                {
                    action = Some(GitAction::Unstage {
                        path: file.path.clone(),
                    });
                }
            } else {
                if file.tracked()
                    && ui
                        .small_button(RichText::new("Discard").color(RED))
                        .clicked()
                {
                    action = Some(GitAction::RequestDiscard {
                        path: file.path.clone(),
                    });
                }
                if ui.small_button(RichText::new("Stage").color(DIM)).clicked() {
                    action = Some(GitAction::Stage {
                        path: file.path.clone(),
                    });
                }
            }
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
pub(crate) fn central_body(ui: &mut egui::Ui, state: &GitState) {
    let Some((path, staged)) = state.selected.clone() else {
        empty_state(ui, "No file selected", "Select a changed file on the left.");
        return;
    };

    let loaded = match &state.diff {
        Some((key, data)) if *key == (path.clone(), staged) => Some(data),
        _ => None,
    };
    let Some(data) = loaded else {
        empty_state(ui, "Loading diff...", "");
        return;
    };

    let conflicted = find_file(state, &path)
        .map(|f| f.conflicted)
        .unwrap_or(false);

    ui.add_space(SPACE_SMALL);
    ui.horizontal(|ui| {
        ui.label(RichText::new(&path).color(TEXT).strong());
        let side = if staged { "staged" } else { "unstaged" };
        ui.label(RichText::new(side).color(DIM));
        if !conflicted {
            let mut raw = state.raw_view.get();
            if ui.checkbox(&mut raw, "Raw diff").changed() {
                state.raw_view.set(raw);
            }
        }
    });
    ui.separator();

    if conflicted {
        raw_diff_view(ui, &data.raw);
        ui.add_space(SPACE_SMALL);
        ui.label(RichText::new("Resolve conflicts in your editor.").color(AMBER));
        return;
    }

    let semantic = if state.raw_view.get() {
        None
    } else {
        match (
            data.old_blob.as_deref().and_then(diff::parse),
            data.new_blob.as_deref().and_then(diff::parse),
        ) {
            (Some(old), Some(new)) => Some(diff::diff_models(&old, &new)),
            _ => None,
        }
    };

    match semantic {
        Some(changes) if !changes.is_empty() => semantic_diff_view(ui, &changes),
        Some(_) => {
            ui.label(RichText::new("No semantic changes (formatting only).").color(DIM));
            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new("Check \"Raw diff\" above to see the raw text.").color(DIM));
        }
        None => raw_diff_view(ui, &data.raw),
    }
}

/// The centered placeholder shown before a selection exists and while the
/// diff for the current selection is still loading.
fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).color(DIM).size(16.0));
        if !hint.is_empty() {
            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new(hint).color(DIM));
        }
    });
}

/// The changed file's `FileStatus`, searched across both scopes so a
/// conflict indicator is found regardless of whether the file sits inside or
/// outside the contracts root.
fn find_file<'a>(state: &'a GitState, path: &str) -> Option<&'a FileStatus> {
    state
        .status
        .inside
        .iter()
        .chain(state.status.outside.iter())
        .find(|f| f.path == path)
}

/// Renders `diff::diff_models` as one row per change: the field name in
/// `DIM`, the removed value in `RED`, the added value in `GREEN`. Either
/// value is absent when the field was purely added or purely removed.
fn semantic_diff_view(ui: &mut egui::Ui, changes: &[FieldChange]) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for change in changes {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&change.what).color(DIM));
                    if !change.from.is_empty() {
                        ui.label(RichText::new(&change.from).color(RED));
                    }
                    if !change.from.is_empty() && !change.to.is_empty() {
                        ui.label(RichText::new("->").color(DIM));
                    }
                    if !change.to.is_empty() {
                        ui.label(RichText::new(&change.to).color(GREEN));
                    }
                });
            }
        });
}

/// Renders raw `git diff` text line by line: `+` lines `GREEN`, `-` lines
/// `RED`, everything else `TEXT`. Scrolls inside its own area so a large diff
/// does not force the panel to content height.
fn raw_diff_view(ui: &mut egui::Ui, raw: &str) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for line in raw.lines() {
                let color = if line.starts_with('+') && !line.starts_with("+++") {
                    GREEN
                } else if line.starts_with('-') && !line.starts_with("---") {
                    RED
                } else {
                    TEXT
                };
                ui.label(RichText::new(line).color(color).monospace());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::model::Status;
    use crate::features::git::state::DiffData;

    const CONTRACT_GET: &str =
        r#"{"name":"x","method":"GET","url":"http://a","headers":[],"responses":[]}"#;
    const CONTRACT_POST: &str =
        r#"{"name":"x","method":"POST","url":"http://a","headers":[],"responses":[]}"#;
    const CONTRACT_GET_REFORMATTED: &str = "{\n  \"name\": \"x\",\n  \"method\": \"GET\",\n  \"url\": \"http://a\",\n  \"headers\": [],\n  \"responses\": []\n}\n";

    fn state_with_diff(path: &str, staged: bool, data: DiffData) -> GitState {
        GitState {
            selected: Some((path.to_string(), staged)),
            diff: Some(((path.to_string(), staged), data)),
            ..GitState::default()
        }
    }

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

    #[test]
    fn sidebar_renders_discard_confirmation_without_panicking() {
        let mut state = GitState {
            pending_discard: Some("src/unstaged.rs".into()),
            ..GitState::default()
        };
        eframe::egui::__run_test_ui(|ui| {
            sidebar_body(ui, &mut state, Some(Path::new("/repo")));
        });
        // The dialog stays open until the user confirms or cancels: a bare
        // render must not clear it.
        assert!(state.pending_discard.is_some());
    }

    #[test]
    fn sidebar_renders_an_error_without_panicking() {
        let mut state = GitState {
            error: "git commit failed".into(),
            ..GitState::default()
        };
        eframe::egui::__run_test_ui(|ui| {
            sidebar_body(ui, &mut state, Some(Path::new("/repo")));
        });
    }

    #[test]
    fn central_renders_semantic_diff_for_a_changed_contract() {
        let old = diff::parse(CONTRACT_GET).expect("valid contract");
        let new = diff::parse(CONTRACT_POST).expect("valid contract");
        assert_eq!(diff::diff_models(&old, &new).len(), 1);
        let state = state_with_diff(
            "contracts/login.json",
            true,
            DiffData {
                raw: "diff --git a/contracts/login.json b/contracts/login.json".into(),
                old_blob: Some(CONTRACT_GET.into()),
                new_blob: Some(CONTRACT_POST.into()),
            },
        );
        eframe::egui::__run_test_ui(|ui| {
            central_body(ui, &state);
        });
    }

    #[test]
    fn central_renders_raw_diff_for_a_non_contract_file() {
        let state = state_with_diff(
            "src/main.rs",
            false,
            DiffData {
                raw: "@@ -1 +1 @@\n-old line\n+new line\n".into(),
                old_blob: None,
                new_blob: None,
            },
        );
        eframe::egui::__run_test_ui(|ui| {
            central_body(ui, &state);
        });
    }

    #[test]
    fn central_reports_a_reformatting_only_change_and_offers_raw_view() {
        let old = diff::parse(CONTRACT_GET).expect("valid contract");
        let new = diff::parse(CONTRACT_GET_REFORMATTED).expect("valid contract");
        assert!(diff::diff_models(&old, &new).is_empty());
        let state = state_with_diff(
            "contracts/login.json",
            false,
            DiffData {
                raw: "@@ -1,7 +1 @@\n-{\"name\":\"x\",\"method\":\"GET\",\"url\":\"http://a\",\"headers\":[],\"responses\":[]}\n+..."
                    .into(),
                old_blob: Some(CONTRACT_GET.into()),
                new_blob: Some(CONTRACT_GET_REFORMATTED.into()),
            },
        );
        eframe::egui::__run_test_ui(|ui| {
            central_body(ui, &state);
        });
    }
}
