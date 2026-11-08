//! The git panel's rendering: the sidebar body (status list, refresh) and the
//! central body (diff view). Both bodies fill a frame the app shell owns, the
//! same shape as `features::contracts::ui::view`.

mod branches;
mod diff;
mod files;
mod log;
mod resolver;

use std::path::Path;

use eframe::egui;
use egui::RichText;

pub(crate) use self::branches::branch_menu;
use self::branches::{
    branch_op_dialog, create_branch_dialog, delete_branch_dialog, discard_dialog,
};
use self::diff::{inline_diff_view, semantic_diff_view};
use self::files::{FileTree, RowMode, file_row};
pub(crate) use self::log::log_panel;
use self::log::revert_dialog;
use self::resolver::conflict_resolver;
use crate::features::git::data::jobs::DiffData;
use crate::features::git::domain::diff as domain_diff;
use crate::features::git::domain::log::short;
use crate::features::git::domain::status::{Status, split_conflicts};
use crate::features::git::ui::state::GitAction;
use crate::features::git::ui::state::GitState;
use crate::ui::components::{
    ButtonKind, DialogButton, detail_box, dialog, empty_state, text_button,
};
use crate::ui::theme::*;

/// What the Git section header should render, decided purely from a `Status`: the label
/// text, `name` plus a ` [conflict]` suffix when any entry is conflicted, and
/// the colour, `error` when anything has changed at all and `on_surface_variant` (the
/// unchanged colour) otherwise. Selection is shown through the tab's filled
/// background, so this never needs to consider which tab is selected.
pub(crate) fn tab_label(name: &str, status: &Status, p: &Palette) -> (String, egui::Color32) {
    let all = status.inside.iter().chain(status.outside.iter());
    let mut dirty = false;
    let mut conflicted = false;
    for f in all {
        dirty = true;
        if f.conflicted {
            conflicted = true;
        }
    }
    let label = if conflicted {
        format!("{name} [conflict]")
    } else {
        name.to_string()
    };
    let color = if dirty { p.error } else { p.on_surface_variant };
    (label, color)
}

/// The file list for the Git section: the error line when set, then the
/// collapsible Conflicts and Changes groups with their counts and the changes
/// outside the project. Every changed row carries a checkbox ticking it for
/// the next commit.
/// The refresh control and the dirty indicator live in the section header.
pub(crate) fn changes_body(
    ui: &mut egui::Ui,
    state: &mut GitState,
    repo_root: Option<&Path>,
) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    if !state.error.is_empty() {
        ui.label(RichText::new(&state.error).color(p.error));
        ui.add_space(SPACE_SMALL);
    }

    if repo_root.is_none() {
        ui.label(RichText::new("Not inside a git repository.").color(p.on_surface_variant));
        return action;
    }

    let selected = state.selected.clone();
    let (conflicted, changed) = split_conflicts(&state.status.inside);
    let conflict_tree = FileTree::from_files(&conflicted, |_| false);
    let changed_tree = FileTree::from_files(&changed, |f| state.is_ticked(f));
    let outside: Vec<_> = state
        .status
        .outside
        .iter()
        .map(|f| (f, state.is_ticked(f)))
        .collect();
    let mut show_outside = state.show_outside;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if !conflicted.is_empty() {
                change_group(ui, "git_conflicts", "Conflicts", conflicted.len(), |ui| {
                    conflict_tree.show(ui, "", RowMode::Conflicted, selected.as_ref(), &mut action);
                });
            }
            if !changed.is_empty() {
                change_group(ui, "git_changes", "Changes", changed.len(), |ui| {
                    changed_tree.show(ui, "", RowMode::Changed, selected.as_ref(), &mut action);
                });
            }
            if conflicted.is_empty() && changed.is_empty() && outside.is_empty() {
                ui.label(RichText::new("No changes").color(p.on_surface_variant));
            }

            if !outside.is_empty() {
                let title = format!("Outside this project  {}", outside.len());
                let header =
                    egui::CollapsingHeader::new(RichText::new(title).color(p.on_surface_variant))
                        .id_salt("git_outside")
                        .open(Some(show_outside))
                        .show(ui, |ui| {
                            for (file, ticked) in &outside {
                                let mode = if file.conflicted {
                                    RowMode::Conflicted
                                } else {
                                    RowMode::Changed
                                };
                                if let Some(a) =
                                    file_row(ui, file, mode, *ticked, selected.as_ref())
                                {
                                    action = Some(a);
                                }
                            }
                        });
                if header.header_response.clicked() {
                    show_outside = !show_outside;
                }
            }
        });
    state.show_outside = show_outside;

    action
}

/// One collapsible group of changed files with its count, open by default.
fn change_group(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    count: usize,
    body: impl FnOnce(&mut egui::Ui),
) {
    let p = palette(ui.ctx());
    egui::CollapsingHeader::new(
        RichText::new(format!("{title}  {count}")).color(p.on_surface_variant),
    )
    .id_salt(id)
    .default_open(true)
    .show(ui, body);
}

/// The commit box for the Git section: the Amend toggle, the Commit button
/// and the message editor, which fills the space the controls leave.
pub(crate) fn commit_body(ui: &mut egui::Ui, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    egui::Panel::bottom("git_commit_controls")
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.add_space(SPACE_EXTRA_SMALL);
            if let Some(upstream) = &state.amend_pushed_to {
                ui.label(
                    RichText::new(format!(
                        "This commit is already on {upstream}. Amending it will need a force push."
                    ))
                    .color(p.warning),
                );
            }
            ui.horizontal(|ui| {
                let mut amend = state.amend;
                if ui.checkbox(&mut amend, "Amend").changed() {
                    action = Some(GitAction::ToggleAmend);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let enabled = !state.commit_message.trim().is_empty()
                        && (state.any_ticked() || state.amend);
                    ui.add_enabled_ui(enabled, |ui| {
                        if text_button(ui, "Commit", p.primary) {
                            action = Some(GitAction::Commit);
                        }
                    });
                });
            });
            ui.add_space(SPACE_EXTRA_SMALL);
        });
    ui.add_sized(
        ui.available_size(),
        egui::TextEdit::multiline(&mut state.commit_message).hint_text("commit message"),
    );
    action
}

/// The git modal dialogs (discard, create branch, delete branch, amend
/// pushed). Returns the last action any of them produced.
pub(crate) fn dialogs(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let mut action = None;
    if let Some(a) = amend_pushed_dialog(ctx, state) {
        action = Some(a);
    }
    if let Some(a) = discard_dialog(ctx, state) {
        action = Some(a);
    }
    if let Some(a) = create_branch_dialog(ctx, state) {
        action = Some(a);
    }
    if let Some(a) = delete_branch_dialog(ctx, state) {
        action = Some(a);
    }
    if let Some(a) = branch_op_dialog(ctx, state) {
        action = Some(a);
    }
    if let Some(a) = revert_dialog(ctx, state) {
        action = Some(a);
    }
    conflict_notice_dialog(ctx, state);
    action
}

/// The "Merge conflict" popup, shown while `conflict_notice` is set: it
/// lists every conflicted path. OK or closing dismisses it until a new path
/// conflicts.
fn conflict_notice_dialog(ctx: &egui::Context, state: &mut GitState) {
    let Some(paths) = state.conflict_notice.clone() else {
        return;
    };
    let p = palette(ctx);
    let outcome = dialog(
        ctx,
        "conflict_notice_modal",
        "Merge conflict",
        |ui| {
            ui.label(
                RichText::new("These files have conflicts. Resolve them in Changes.")
                    .color(p.on_surface_variant),
            );
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                for path in &paths {
                    ui.label(RichText::new(path).color(p.on_surface).monospace());
                }
            });
        },
        &[DialogButton::new("OK", ButtonKind::Primary)],
    );
    if outcome.clicked.is_some() || outcome.closed {
        state.conflict_notice = None;
    }
}

/// Amend confirmation modal, shown while `confirm_amend` is set: the commit
/// Amend would rewrite is already on an upstream, so amending it means a
/// force push.
fn amend_pushed_dialog(ctx: &egui::Context, state: &GitState) -> Option<GitAction> {
    if !state.confirm_amend {
        return None;
    }
    let p = palette(ctx);
    let upstream = state.amend_pushed_to.as_deref().unwrap_or("the remote");
    let outcome = dialog(
        ctx,
        "amend_pushed_modal",
        "Amend pushed commit",
        |ui| {
            ui.label(
                RichText::new(format!(
                    "This commit is already on {upstream}. Amending it rewrites history, so you will need to force push."
                ))
                .color(p.on_surface_variant),
            );
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Amend", ButtonKind::Danger),
        ],
    );
    if outcome.clicked == Some(1) {
        Some(GitAction::ConfirmAmend)
    } else if outcome.clicked == Some(0) || outcome.closed {
        Some(GitAction::CancelAmend)
    } else {
        None
    }
}

/// The central body for the Git section: a commit file's diff when one was
/// picked in the log, otherwise the selected working-tree file's diff, an
/// empty state until then, or the conflict resolver for a conflicted file.
///
/// The panel frame is owned by the app shell. This only sees the git slice.
/// Returns the resolve action once the user clicks Resolve, for the app
/// shell to apply after this closure ends.
pub(crate) fn central_body(ui: &mut egui::Ui, state: &mut GitState) -> Option<GitAction> {
    if let Some(path) = state.log.selected_file.clone() {
        let hash = state.log.selected.clone().unwrap_or_default();
        match &state.log.diff {
            Some((key, data)) if *key == (hash.clone(), path.clone()) => {
                diff_body(ui, &mut state.show_fields, &path, short(&hash), data)
            }
            _ => empty_state(ui, "Loading diff...", ""),
        }
        return None;
    }
    let Some((path, staged)) = state.selected.clone() else {
        empty_state(ui, "No file selected", "Select a changed file on the left.");
        return None;
    };

    let loaded = match &state.diff {
        Some((key, data)) if *key == (path.clone(), staged) => Some(data),
        _ => None,
    };
    let Some(data) = loaded else {
        empty_state(ui, "Loading diff...", "");
        return None;
    };

    if state.status.find(&path).is_some_and(|f| f.conflicted) {
        // Cloned out of `data` before it is dropped, so `state` can be
        // borrowed mutably below without fighting the borrow checker over a
        // reference into `state.diff`.
        let raw = data.raw.clone();
        let new_blob = data.new_blob.clone();
        ui.add_space(SPACE_SMALL);
        return conflict_resolver(ui, state, &path, staged, &raw, new_blob.as_deref());
    }

    let side = if staged { "staged" } else { "unstaged" };
    diff_body(ui, &mut state.show_fields, &path, side, data);
    None
}

/// One read-only diff: `path` and `side` with the Show field changes toggle,
/// then the field list when asked for and available, the inline diff
/// otherwise. A non-contract file, an untracked file or a formatting-only
/// change always shows the inline diff, so no selection renders an empty pane.
fn diff_body(ui: &mut egui::Ui, show_fields: &mut bool, path: &str, side: &str, data: &DiffData) {
    let p = palette(ui.ctx());
    let field_changes =
        domain_diff::semantic_changes(data.old_blob.as_deref(), data.new_blob.as_deref());
    ui.add_space(SPACE_SMALL);
    ui.horizontal(|ui| {
        ui.label(RichText::new(path).color(p.on_surface).strong());
        ui.label(RichText::new(side).color(p.on_surface_variant));
        ui.add_enabled_ui(field_changes.is_some(), |ui| {
            ui.checkbox(show_fields, "Show field changes")
        })
        .response
        .on_disabled_hover_text("No field-level view for this change");
    });
    ui.separator();
    match field_changes.filter(|_| *show_fields) {
        Some(changes) => semantic_diff_view(ui, &changes),
        None => inline_diff_view(ui, &data.rows),
    }
}

#[cfg(test)]
mod tests {
    use super::resolver::resolve_label;
    use super::*;
    use crate::features::git::domain::conflict::{self, Choice};
    use crate::features::git::domain::status::fixtures::{file, untracked};
    use crate::features::git::domain::status::{Branches, Change, FileStatus};
    use crate::features::git::ui::state::{BranchOp, BranchOpKind};

    const CONTRACT_GET: &str =
        r#"{"name":"x","method":"GET","url":"http://a","headers":[],"responses":[]}"#;
    const CONTRACT_POST: &str =
        r#"{"name":"x","method":"POST","url":"http://a","headers":[],"responses":[]}"#;
    const CONTRACT_GET_REFORMATTED: &str = "{\n  \"name\": \"x\",\n  \"method\": \"GET\",\n  \"url\": \"http://a\",\n  \"headers\": [],\n  \"responses\": []\n}\n";

    fn diff_data(raw: &str, old_blob: Option<&str>, new_blob: Option<&str>) -> DiffData {
        DiffData {
            raw: raw.to_string(),
            rows: domain_diff::parse_unified(raw),
            old_blob: old_blob.map(str::to_string),
            new_blob: new_blob.map(str::to_string),
        }
    }

    fn state_with_diff(path: &str, staged: bool, data: DiffData) -> GitState {
        GitState {
            selected: Some((path.to_string(), staged)),
            diff: Some(((path.to_string(), staged), data)),
            ..GitState::default()
        }
    }

    #[test]
    fn tab_label_is_the_plain_name_and_dim_when_clean() {
        assert_eq!(
            tab_label("Git", &Status::default(), &TOKYO_NIGHT_STORM),
            ("Git".to_string(), TOKYO_NIGHT_STORM.on_surface_variant)
        );
    }

    #[test]
    fn tab_label_is_the_plain_name_and_red_for_plain_changes() {
        let status = Status {
            inside: vec![file("a.json", false)],
            outside: vec![],
        };
        assert_eq!(
            tab_label("Git", &status, &TOKYO_NIGHT_STORM),
            ("Git".to_string(), TOKYO_NIGHT_STORM.error)
        );
    }

    #[test]
    fn tab_label_gets_the_conflict_suffix_and_red_for_a_conflict() {
        let status = Status {
            inside: vec![file("a.json", true)],
            outside: vec![],
        };
        assert_eq!(
            tab_label("Git", &status, &TOKYO_NIGHT_STORM),
            ("Git [conflict]".to_string(), TOKYO_NIGHT_STORM.error)
        );
    }

    #[test]
    fn tab_label_keeps_the_conflict_suffix_alongside_several_plain_changes() {
        let status = Status {
            inside: vec![file("a.json", false), file("b.json", true)],
            outside: vec![file("c.json", false)],
        };
        assert_eq!(
            tab_label("Git", &status, &TOKYO_NIGHT_STORM),
            ("Git [conflict]".to_string(), TOKYO_NIGHT_STORM.error)
        );
    }

    #[test]
    fn sidebar_and_central_render_without_panicking() {
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                let mut state = GitState::default();
                changes_body(ui, &mut state, None);
                central_body(ui, &mut state);
            });
        }
    }

    #[test]
    fn sidebar_renders_file_list_without_panicking() {
        for preference in BOTH_THEMES {
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
            run_in_theme(preference, |ui| {
                let mut state = GitState {
                    status: status.clone(),
                    show_outside: true,
                    ..GitState::default()
                };
                changes_body(ui, &mut state, Some(Path::new("/repo")));
                central_body(ui, &mut state);
            });
        }
    }

    #[test]
    fn sidebar_renders_discard_confirmation_without_panicking() {
        for preference in BOTH_THEMES {
            let mut state = GitState {
                pending_discard: Some("src/unstaged.rs".into()),
                ..GitState::default()
            };
            run_in_theme(preference, |ui| {
                dialogs(ui.ctx(), &mut state);
            });
            // The dialog stays open until the user confirms or cancels: a bare
            // render must not clear it.
            assert!(state.pending_discard.is_some());
        }
    }

    #[test]
    fn sidebar_renders_an_error_without_panicking() {
        for preference in BOTH_THEMES {
            let mut state = GitState {
                error: "git commit failed".into(),
                ..GitState::default()
            };
            run_in_theme(preference, |ui| {
                changes_body(ui, &mut state, Some(Path::new("/repo")));
            });
        }
    }

    #[test]
    fn central_renders_the_field_list_when_show_fields_is_set() {
        for preference in BOTH_THEMES {
            let old = domain_diff::parse(CONTRACT_GET).expect("valid contract");
            let new = domain_diff::parse(CONTRACT_POST).expect("valid contract");
            assert_eq!(domain_diff::diff_models(&old, &new).len(), 1);
            let mut state = state_with_diff(
                "contracts/login.json",
                true,
                diff_data(
                    "diff --git a/contracts/login.json b/contracts/login.json",
                    Some(CONTRACT_GET),
                    Some(CONTRACT_POST),
                ),
            );
            state.show_fields = true;
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
        }
    }

    #[test]
    fn central_renders_the_inline_diff_by_default_for_a_changed_contract() {
        for preference in BOTH_THEMES {
            let mut state = state_with_diff(
                "contracts/login.json",
                true,
                diff_data(
                    "diff --git a/contracts/login.json b/contracts/login.json\nindex 2a4e428..5d3ef15 100644\n--- a/contracts/login.json\n+++ b/contracts/login.json\n@@ -1,3 +1,3 @@\n {\n-    \"name\": \"user-login\",\n+    \"name\": \"login-v2\",\n }\n",
                    Some(CONTRACT_GET),
                    Some(CONTRACT_POST),
                ),
            );
            assert!(!state.show_fields);
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
        }
    }

    #[test]
    fn inline_diff_view_renders_every_row_kind_in_both_themes() {
        let rows = domain_diff::parse_unified(
            "@@ -1,3 +1,3 @@\n a\n-\"method\": \"GET\",\n+\"method\": \"POST\",\n c\n\\ No newline at end of file\n",
        );
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                inline_diff_view(ui, &rows);
            });
        }
    }

    #[test]
    fn central_renders_the_inline_diff_for_a_non_contract_file() {
        for preference in BOTH_THEMES {
            let mut state = state_with_diff(
                "src/main.rs",
                false,
                diff_data("@@ -1 +1 @@\n-old line\n+new line\n", None, None),
            );
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
        }
    }

    #[test]
    fn central_falls_back_to_the_diff_for_a_reformatting_only_change() {
        for preference in BOTH_THEMES {
            let old = domain_diff::parse(CONTRACT_GET).expect("valid contract");
            let new = domain_diff::parse(CONTRACT_GET_REFORMATTED).expect("valid contract");
            assert!(domain_diff::diff_models(&old, &new).is_empty());
            let mut state = state_with_diff(
                "contracts/login.json",
                false,
                diff_data(
                    "@@ -1,7 +1 @@\n-{\"name\":\"x\",\"method\":\"GET\",\"url\":\"http://a\",\"headers\":[],\"responses\":[]}\n+...",
                    Some(CONTRACT_GET),
                    Some(CONTRACT_GET_REFORMATTED),
                ),
            );
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
        }
    }

    const CONFLICT_TEXT: &str = "{\n<<<<<<< HEAD\n  \"name\": \"main\",\n=======\n  \"name\": \"side\",\n>>>>>>> side\n  \"method\": \"GET\",\n<<<<<<< HEAD\n  \"url\": \"http://a\"\n=======\n  \"url\": \"http://b\"\n>>>>>>> side\n}\n";

    /// A conflicted `GitState` selecting `path`, its working file (with
    /// markers) as `new_blob`, and a matching conflicted `FileStatus` so
    /// `central_body` takes the resolver branch rather than the plain diff
    /// one.
    fn conflict_state(path: &str, text: &str) -> GitState {
        let mut state = state_with_diff(
            path,
            false,
            diff_data("@@ -1,3 +1,7 @@\n conflict", None, Some(text)),
        );
        state.status = Status {
            inside: vec![file(path, true)],
            outside: vec![],
        };
        state
    }

    #[test]
    fn central_renders_the_conflict_resolver_with_nothing_decided_without_panicking() {
        for preference in BOTH_THEMES {
            let mut state = conflict_state("contracts/sample.json", CONFLICT_TEXT);
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
            let resolve = state.resolve.as_ref().expect("markers must parse");
            assert_eq!(resolve.choices, vec![None, None]);
        }
    }

    #[test]
    fn central_renders_the_conflict_resolver_with_everything_decided_without_panicking() {
        for preference in BOTH_THEMES {
            let mut state = conflict_state("contracts/sample.json", CONFLICT_TEXT);
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
            state
                .resolve
                .as_mut()
                .expect("markers must parse")
                .choices
                .fill(Some(Choice::Ours));
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
        }
    }

    #[test]
    fn central_renders_a_warning_when_the_resolution_would_not_be_valid_json() {
        for preference in BOTH_THEMES {
            let text =
                "{\n<<<<<<< HEAD\n  invalid,,\n=======\n  \"name\": \"side\"\n>>>>>>> side\n}\n";
            let mut state = conflict_state("contracts/sample.json", text);
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
            state
                .resolve
                .as_mut()
                .expect("markers must parse")
                .choices
                .fill(Some(Choice::Ours));
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
            let resolve = state.resolve.as_ref().expect("markers must parse");
            let rendered = conflict::render(&resolve.file, &resolve.choices);
            assert!(apic_core::json::validate(&rendered).is_err());
        }
    }

    #[test]
    fn central_renders_the_conflict_resolver_with_a_mix_of_decided_and_undecided_blocks() {
        for preference in BOTH_THEMES {
            let mut state = conflict_state("contracts/sample.json", CONFLICT_TEXT);
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
            let resolve = state.resolve.as_mut().expect("markers must parse");
            resolve.choices[0] = Some(Choice::Both);
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
            let resolve = state.resolve.as_ref().expect("markers must parse");
            assert_eq!(resolve.choices, vec![Some(Choice::Both), None]);
        }
    }

    #[test]
    fn central_renders_the_conflict_resolver_for_a_non_json_file() {
        for preference in BOTH_THEMES {
            let mut state = conflict_state("notes.txt", CONFLICT_TEXT);
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
            assert!(state.resolve.is_some());
        }
    }

    /// Picking choices, including via take-all-ours, must never itself write
    /// to disk. Resolve is the single path to disk, so a fixture file
    /// carrying markers must still carry them after rendering through both
    /// undecided and fully-decided states.
    #[test]
    fn resolving_choices_in_the_view_never_writes_the_file_to_disk() {
        for preference in BOTH_THEMES {
            let dir = std::env::temp_dir().join(format!(
                "apic-gui-conflict-resolve-test-{:?}",
                std::thread::current().id()
            ));
            std::fs::create_dir_all(&dir).expect("temp dir creates");
            let path = dir.join("sample.json");
            std::fs::write(&path, CONFLICT_TEXT).expect("fixture file writes");

            let mut state = conflict_state("sample.json", CONFLICT_TEXT);
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
            state
                .resolve
                .as_mut()
                .expect("markers must parse")
                .choices
                .fill(Some(Choice::Ours));
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });

            let on_disk = std::fs::read_to_string(&path).expect("fixture file reads");
            assert!(
                on_disk.contains("<<<<<<<"),
                "the file on disk must still carry conflict markers: {on_disk:?}"
            );
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// A path long enough to need truncation must not push the header's
    /// buttons out of reach or squeeze the side label to nothing. Nothing
    /// here caught that before: every other resolver test uses a short
    /// path, so a regression that hides the buttons behind a long path
    /// would pass unnoticed. `run_in_theme` sends no synthetic clicks, so
    /// this checks what a click would produce (`resolve_label`, and the
    /// text a Resolve click writes) rather than a click itself, the same
    /// way `resolving_choices_in_the_view_never_writes_the_file_to_disk`
    /// exercises Resolve's output without simulating the click.
    #[test]
    fn central_renders_the_conflict_resolver_with_a_long_path_without_panicking() {
        for preference in BOTH_THEMES {
            let long_path = format!(
                "contracts/{}/logout.json",
                "deeply/nested/folder/".repeat(10)
            );
            let mut state = conflict_state(&long_path, CONFLICT_TEXT);
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
            let resolve = state.resolve.as_mut().expect("markers must parse");
            assert_eq!(resolve_label(&resolve.choices), "Resolve (0/2)");

            resolve.choices.fill(Some(Choice::Ours));
            run_in_theme(preference, |ui| {
                central_body(ui, &mut state);
            });
            let resolve = state.resolve.as_ref().expect("markers must parse");
            assert_eq!(resolve_label(&resolve.choices), "Resolve");
            let rendered = conflict::render(&resolve.file, &resolve.choices);
            assert!(
                !rendered.contains("<<<<<<<"),
                "a fully decided resolution must not still carry markers: {rendered:?}"
            );
        }
    }

    #[test]
    fn refresh_branches_does_not_refire_once_a_fetch_has_completed_with_no_branches() {
        for preference in BOTH_THEMES {
            let mut state = GitState::default();
            let mut first = None;
            run_in_theme(preference, |ui| {
                first = branch_menu(ui, &mut state);
            });
            assert!(matches!(first, Some(GitAction::RefreshBranches)));

            // Stand in for a completed fetch that found no branches, the state a
            // commitless repository leaves behind for good.
            state.branches_loaded = true;
            let mut second = None;
            run_in_theme(preference, |ui| {
                second = branch_menu(ui, &mut state);
            });
            assert!(!matches!(second, Some(GitAction::RefreshBranches)));
        }
    }

    #[test]
    fn sidebar_renders_a_normal_two_branch_state_without_panicking() {
        for preference in BOTH_THEMES {
            let mut state = GitState {
                branches: Branches {
                    current: Some("main".into()),
                    all: vec!["main".into(), "feature".into()],
                    remotes: Vec::new(),
                },
                ..GitState::default()
            };
            run_in_theme(preference, |ui| {
                branch_menu(ui, &mut state);
            });
        }
    }

    #[test]
    fn sidebar_renders_detached_head_without_panicking() {
        for preference in BOTH_THEMES {
            let mut state = GitState {
                branches: Branches {
                    current: None,
                    all: vec!["main".into()],
                    remotes: Vec::new(),
                },
                ..GitState::default()
            };
            run_in_theme(preference, |ui| {
                branch_menu(ui, &mut state);
            });
        }
    }

    #[test]
    fn sidebar_renders_the_create_branch_input_without_panicking() {
        for preference in BOTH_THEMES {
            let mut state = GitState {
                branches: Branches {
                    current: Some("main".into()),
                    all: vec!["main".into()],
                    remotes: Vec::new(),
                },
                new_branch: Some("feature".into()),
                ..GitState::default()
            };
            run_in_theme(preference, |ui| {
                branch_menu(ui, &mut state);
                dialogs(ui.ctx(), &mut state);
            });
            // The dialog stays open until the user confirms or cancels: a bare
            // render must not clear it.
            assert!(state.new_branch.is_some());
        }
    }

    #[test]
    fn sidebar_renders_the_delete_branch_confirmation_without_panicking() {
        for preference in BOTH_THEMES {
            let mut state = GitState {
                branches: Branches {
                    current: Some("main".into()),
                    all: vec!["main".into(), "feature".into()],
                    remotes: Vec::new(),
                },
                pending_branch_delete: Some("feature".into()),
                ..GitState::default()
            };
            run_in_theme(preference, |ui| {
                branch_menu(ui, &mut state);
                dialogs(ui.ctx(), &mut state);
            });
            // The dialog stays open until the user confirms or cancels: a bare
            // render must not clear it.
            assert!(state.pending_branch_delete.is_some());
        }
    }

    #[test]
    fn branch_op_popup_renders_in_both_themes() {
        for preference in BOTH_THEMES {
            for kind in [BranchOpKind::Pull, BranchOpKind::Merge] {
                let mut state = GitState {
                    pending_branch_op: Some(BranchOp {
                        kind,
                        target: "main".into(),
                        source: "origin/main".into(),
                    }),
                    ..GitState::default()
                };
                run_in_theme(preference, |ui| {
                    dialogs(ui.ctx(), &mut state);
                });
                // The popup stays open until the user confirms or cancels.
                assert!(state.pending_branch_op.is_some());
            }
        }
    }

    /// Every AccessKit node in `out` that has a label (egui puts a `Label`
    /// widget's text in the node's value), with its bounds.
    fn labelled_nodes(out: egui::FullOutput) -> Vec<(String, egui::Rect)> {
        out.platform_output
            .accesskit_update
            .map(|update| {
                update
                    .nodes
                    .into_iter()
                    .filter_map(|(_, node)| {
                        let b = node.bounds()?;
                        Some((
                            node.label().or_else(|| node.value())?.to_string(),
                            egui::Rect::from_min_max(
                                egui::pos2(b.x0 as f32, b.y0 as f32),
                                egui::pos2(b.x1 as f32, b.y1 as f32),
                            ),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// One headless frame of `dialogs` fed `events`, returning labelled nodes.
    fn dialogs_frame(
        ctx: &egui::Context,
        state: &mut GitState,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        labelled_nodes(ctx.run_ui(input, |ui| {
            dialogs(ui.ctx(), state);
        }))
    }

    #[test]
    fn conflict_notice_popup_lists_paths_and_ok_dismisses_it() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = GitState {
            conflict_notice: Some(vec!["contracts/a.json".into()]),
            ..GitState::default()
        };
        dialogs_frame(&ctx, &mut state, vec![]);
        let nodes = dialogs_frame(&ctx, &mut state, vec![]);
        node(&nodes, "Merge conflict");
        node(&nodes, "contracts/a.json");
        let pos = node(&nodes, "OK").center();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            dialogs_frame(&ctx, &mut state, events);
        }
        assert_eq!(state.conflict_notice, None);
    }

    #[test]
    fn conflict_notice_popup_stays_open_until_dismissed() {
        for preference in BOTH_THEMES {
            let mut state = GitState {
                conflict_notice: Some(vec!["contracts/a.json".into()]),
                ..GitState::default()
            };
            run_in_theme(preference, |ui| {
                dialogs(ui.ctx(), &mut state);
            });
            assert!(state.conflict_notice.is_some());
        }
    }

    /// One headless frame of `branch_menu` fed `events`. Stores any action
    /// it returns into `action` and returns every AccessKit node that has a
    /// label, with its bounds.
    fn menu_frame(
        ctx: &egui::Context,
        state: &mut GitState,
        events: Vec<egui::Event>,
        action: &mut Option<GitAction>,
    ) -> Vec<(String, egui::Rect)> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| {
            if let Some(a) = branch_menu(ui, state) {
                *action = Some(a);
            }
        });
        labelled_nodes(out)
    }

    /// Moves the pointer to the centre of `rect` and clicks there, then runs
    /// two idle frames. Returns the nodes from the last frame.
    fn click_at(
        ctx: &egui::Context,
        state: &mut GitState,
        rect: egui::Rect,
        action: &mut Option<GitAction>,
    ) -> Vec<(String, egui::Rect)> {
        let pos = rect.center();
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut nodes = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![button(true)],
            vec![button(false)],
            vec![],
            vec![],
        ] {
            nodes = menu_frame(ctx, state, events, action);
        }
        nodes
    }

    /// The bounds of the first node whose label starts with `label`. Sub
    /// menu buttons carry a trailing arrow, hence `starts_with`.
    fn node(nodes: &[(String, egui::Rect)], label: &str) -> egui::Rect {
        nodes
            .iter()
            .find(|(l, _)| l.starts_with(label))
            .map(|(_, r)| *r)
            .unwrap_or_else(|| {
                let labels: Vec<&String> = nodes.iter().map(|(l, _)| l).collect();
                panic!("no node {label:?} in {labels:?}")
            })
    }

    /// Drives the real dropdown with pointer events: open it, click ⋮ on a
    /// non-current row, open Pull, click a remote branch. Guards against the
    /// right-click menu bug, where the dropdown closed under the menu.
    #[test]
    fn kebab_menu_picks_a_pull_source_through_nested_menus() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = GitState {
            branches: Branches {
                current: Some("main".into()),
                all: vec!["main".into(), "feature".into()],
                remotes: vec!["origin/main".into()],
            },
            branches_loaded: true,
            ..GitState::default()
        };
        let mut action = None;

        let nodes = menu_frame(&ctx, &mut state, vec![], &mut action);
        let title = node(&nodes, egui_phosphor::regular::GIT_BRANCH);
        let nodes = click_at(&ctx, &mut state, title, &mut action);
        let row = node(&nodes, "feature");
        let kebab = nodes
            .iter()
            .find(|(l, r)| {
                l.starts_with(egui_phosphor::regular::DOTS_THREE_VERTICAL)
                    && (r.center().y - row.center().y).abs() < 2.0
            })
            .map(|(_, r)| *r)
            .expect("the feature row has a ⋮ button");
        let nodes = click_at(&ctx, &mut state, kebab, &mut action);
        let pull = node(&nodes, "Pull");
        let nodes = click_at(&ctx, &mut state, pull, &mut action);
        let source = node(&nodes, "origin/main");
        click_at(&ctx, &mut state, source, &mut action);

        state.handle(
            action.expect("picking a source returns an action"),
            None,
            &ctx,
        );
        assert_eq!(
            state.pending_branch_op,
            Some(BranchOp {
                kind: BranchOpKind::Pull,
                target: "feature".into(),
                source: "origin/main".into(),
            })
        );
    }

    /// The ⋮ buttons line up in one column at the right edge of the
    /// dropdown, whatever the branch name lengths.
    #[test]
    fn kebab_buttons_share_one_column() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = GitState {
            branches: Branches {
                current: Some("main".into()),
                all: vec!["main".into(), "feature/a-much-longer-branch-name".into()],
                remotes: Vec::new(),
            },
            branches_loaded: true,
            ..GitState::default()
        };
        let mut action = None;

        let nodes = menu_frame(&ctx, &mut state, vec![], &mut action);
        let title = node(&nodes, egui_phosphor::regular::GIT_BRANCH);
        let nodes = click_at(&ctx, &mut state, title, &mut action);

        let kebabs: Vec<egui::Rect> = nodes
            .iter()
            .filter(|(l, _)| l.starts_with(egui_phosphor::regular::DOTS_THREE_VERTICAL))
            .map(|(_, r)| *r)
            .collect();
        assert!(kebabs.len() >= 2, "one ⋮ per branch: {kebabs:?}");
        let new_branch = node(&nodes, egui_phosphor::regular::PLUS);
        for r in &kebabs {
            assert!((r.min.x - kebabs[0].min.x).abs() < 0.5, "{kebabs:?}");
            assert!(r.max.x <= new_branch.max.x + 0.5, "{r:?} vs {new_branch:?}");
        }
    }

    #[test]
    fn amend_popup_renders_in_both_themes() {
        for preference in BOTH_THEMES {
            let mut state = GitState {
                amend: true,
                amend_pushed_to: Some("origin/main".into()),
                confirm_amend: true,
                ..GitState::default()
            };
            run_in_theme(preference, |ui| {
                assert!(dialogs(ui.ctx(), &mut state).is_none());
            });
            // The popup stays open until the user amends or cancels: a bare
            // render must not clear it.
            assert!(state.confirm_amend);
        }
    }

    #[test]
    fn commit_body_renders_in_every_amend_state() {
        for preference in BOTH_THEMES {
            for (amend, pushed) in [(false, None), (true, None), (true, Some("origin/main"))] {
                let mut state = GitState {
                    amend,
                    amend_pushed_to: pushed.map(String::from),
                    ..GitState::default()
                };
                run_in_theme(preference, |ui| {
                    commit_body(ui, &mut state);
                });
            }
        }
    }

    #[test]
    fn changes_body_renders_each_tick_state_in_both_themes() {
        let folder = Status {
            inside: vec![untracked("dir/a.json"), untracked("dir/b.json")],
            outside: vec![],
        };
        let all_ticked: std::collections::HashMap<String, bool> = [
            ("dir/a.json".to_string(), true),
            ("dir/b.json".to_string(), true),
        ]
        .into_iter()
        .collect();
        let one_ticked: std::collections::HashMap<String, bool> =
            [("dir/a.json".to_string(), true)].into_iter().collect();
        let cases = [
            (Status::default(), Default::default()),
            (
                Status {
                    inside: vec![file("a.json", false), untracked("b.json")],
                    outside: vec![file("c.json", false)],
                },
                Default::default(),
            ),
            (
                Status {
                    inside: vec![file("d.json", true)],
                    outside: vec![],
                },
                Default::default(),
            ),
            (folder.clone(), all_ticked),
            (folder, one_ticked),
        ];
        for preference in BOTH_THEMES {
            for (status, ticks) in &cases {
                let mut state = GitState {
                    status: status.clone(),
                    ticks: ticks.clone(),
                    show_outside: true,
                    ..GitState::default()
                };
                run_in_theme(preference, |ui| {
                    let _ = changes_body(ui, &mut state, Some(std::path::Path::new("/repo")));
                });
            }
        }
    }

    use crate::features::git::domain::log::{Commit, lanes};
    use crate::features::git::ui::state::LogState;

    /// A merge on top of a root commit, both loaded, on branch `main`.
    fn log_view_state() -> GitState {
        let merge = Commit {
            hash: "b".repeat(40),
            parents: vec!["a".repeat(40), "c".repeat(40)],
            author: "Ann".into(),
            date: "2026-11-08 10:00".into(),
            refs: vec!["HEAD -> main".into()],
            subject: "Merge branch 'side'".into(),
        };
        let root = Commit {
            hash: "a".repeat(40),
            parents: vec![],
            author: "Bob".into(),
            date: "2026-11-07 09:00".into(),
            refs: vec![],
            subject: "first commit".into(),
        };
        let commits = vec![merge, root];
        GitState {
            branches: Branches {
                current: Some("main".into()),
                all: vec!["main".into()],
                remotes: vec![],
            },
            log: LogState {
                open: true,
                rows: lanes(&commits),
                commits,
                loaded: true,
                ..LogState::default()
            },
            ..GitState::default()
        }
    }

    /// One headless frame of `log_panel` fed `events`. Stores any action it
    /// returns into `action` and returns every labelled AccessKit node.
    fn log_frame(
        ctx: &egui::Context,
        state: &mut GitState,
        events: Vec<egui::Event>,
        action: &mut Option<GitAction>,
    ) -> Vec<(String, egui::Rect)> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        labelled_nodes(ctx.run_ui(input, |ui| {
            if let Some(a) = log_panel(ui, state) {
                *action = Some(a);
            }
        }))
    }

    /// Moves to the centre of `rect` and clicks `button` there, then runs two
    /// idle frames. Returns the nodes from the last frame.
    fn log_click(
        ctx: &egui::Context,
        state: &mut GitState,
        rect: egui::Rect,
        button: egui::PointerButton,
        action: &mut Option<GitAction>,
    ) -> Vec<(String, egui::Rect)> {
        let pos = rect.center();
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut nodes = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![press(true)],
            vec![press(false)],
            vec![],
            vec![],
        ] {
            nodes = log_frame(ctx, state, events, action);
        }
        nodes
    }

    #[test]
    fn log_view_panel_renders_its_toolbar_labels_and_rows() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = log_view_state();
        let mut action = None;
        log_frame(&ctx, &mut state, vec![], &mut action);
        let nodes = log_frame(&ctx, &mut state, vec![], &mut action);
        for label in [
            "Branch: main",
            "User: All",
            "Date: All time",
            "Paths: All",
            ".*",
            "Cc",
            "Merge branch 'side'",
            "first commit",
        ] {
            node(&nodes, label);
        }
    }

    #[test]
    fn log_view_right_click_revert_opens_the_popup() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = log_view_state();
        let mut action = None;
        log_frame(&ctx, &mut state, vec![], &mut action);
        let nodes = log_frame(&ctx, &mut state, vec![], &mut action);
        let row = node(&nodes, "first commit");
        let nodes = log_click(
            &ctx,
            &mut state,
            row,
            egui::PointerButton::Secondary,
            &mut action,
        );
        let revert = node(&nodes, "Revert");
        log_click(
            &ctx,
            &mut state,
            revert,
            egui::PointerButton::Primary,
            &mut action,
        );

        state.handle(action.expect("Revert returns an action"), None, &ctx);
        assert_eq!(
            state.log.pending_revert.as_ref().map(|c| c.hash.clone()),
            Some("a".repeat(40))
        );
        dialogs_frame(&ctx, &mut state, vec![]);
        let nodes = dialogs_frame(&ctx, &mut state, vec![]);
        node(&nodes, "Revert commit");
        node(&nodes, "first commit");
    }

    /// Every state the panel draws: loading, nothing matched, a failed
    /// read over rows, a selected commit with files and Load more, and dots
    /// only under a text filter.
    fn log_view_variants() -> Vec<GitState> {
        let loading = GitState::default();
        let empty = GitState {
            log: LogState {
                loaded: true,
                ..LogState::default()
            },
            ..GitState::default()
        };
        let mut failed = log_view_state();
        failed.log.error = "fatal: bad revision".into();
        let mut busy = log_view_state();
        busy.log.more = true;
        busy.log.selected = Some("a".repeat(40));
        busy.log.files = Some(vec![FileStatus {
            path: "contracts/a.json".into(),
            index: Change::Added,
            worktree: Change::Unmodified,
            conflicted: false,
        }]);
        busy.log.selected_file = Some("contracts/a.json".into());
        let mut dots = log_view_state();
        dots.log.filter.text = "first".into();
        vec![loading, empty, failed, busy, dots]
    }

    #[test]
    fn log_view_renders_each_state_in_both_themes() {
        for preference in BOTH_THEMES {
            for mut state in log_view_variants() {
                run_in_theme(preference, |ui| {
                    let _ = log_panel(ui, &mut state);
                });
            }
        }
    }

    #[test]
    fn log_view_central_shows_a_commit_file_diff() {
        for preference in BOTH_THEMES {
            for show_fields in [false, true] {
                let hash = "a".repeat(40);
                let mut state = GitState {
                    show_fields,
                    log: LogState {
                        selected: Some(hash.clone()),
                        selected_file: Some("contracts/a.json".into()),
                        diff: Some((
                            (hash, "contracts/a.json".into()),
                            diff_data(
                                "@@ -1 +1 @@\n-GET\n+POST\n",
                                Some(CONTRACT_GET),
                                Some(CONTRACT_POST),
                            ),
                        )),
                        ..LogState::default()
                    },
                    ..GitState::default()
                };
                run_in_theme(preference, |ui| {
                    assert!(central_body(ui, &mut state).is_none());
                });
            }
        }
    }
}
