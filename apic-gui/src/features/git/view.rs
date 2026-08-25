//! The git panel's rendering: the sidebar body (status list, refresh) and the
//! central body (diff view). Both bodies fill a frame the app shell owns, the
//! same shape as `features::contracts::view`.

use std::collections::BTreeMap;
use std::path::Path;

use eframe::egui;
use egui::RichText;

use crate::app::actions::GitAction;
use crate::features::git::diff::{self, FieldChange};
use crate::features::git::model::{FileStatus, Status};
use crate::features::git::state::GitState;
use crate::ui::components::{bordered_input, text_button};
use crate::ui::theme::*;

/// What the Git tab should render, decided purely from a `Status`: the label
/// text, `name` plus a ` [conflict]` suffix when any entry is conflicted, and
/// the colour, `RED` when anything has changed at all and `DIM` (the
/// unchanged colour) otherwise. Selection is shown through the tab's filled
/// background, so this never needs to consider which tab is selected.
pub(crate) fn tab_label(name: &str, status: &Status) -> (String, egui::Color32) {
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
    let color = if dirty { RED } else { DIM };
    (label, color)
}

/// Left sidebar body for the Git tab: the error line when set, and the file
/// list split into staged and unstaged sections. The refresh control and the
/// tab's own dirty indicator live in the shell's tab row, not here.
///
/// The panel frame is owned by the app shell, not by this feature, so that a
/// second sidebar tab can render into the same frame; this only fills it.
pub(crate) fn sidebar_body(
    ui: &mut egui::Ui,
    state: &mut GitState,
    repo_root: Option<&Path>,
) -> Option<GitAction> {
    let mut action = None;
    if !state.error.is_empty() {
        ui.label(RichText::new(&state.error).color(RED));
        ui.add_space(SPACE_SMALL);
    }

    if repo_root.is_none() {
        ui.label(RichText::new("Not inside a git repository.").color(DIM));
        return action;
    }

    // Populate the branch list once, and again after every branch mutation
    // (handled in the app shell). `pending` guards against piling up a
    // second git command while one is already in flight. `branches_loaded`
    // guards against retrying forever: a repository with no commits has no
    // branches, so an empty list after a completed fetch is not itself a
    // reason to refetch. Set right here, the moment the request is made, so
    // the condition cannot fire again regardless of how the fetch resolves.
    if !state.branches_loaded && state.pending.is_none() {
        state.branches_loaded = true;
        action = Some(GitAction::RefreshBranches);
    }
    if let Some(a) = branch_row(ui, state) {
        action = Some(a);
    }
    ui.separator();

    egui::Panel::bottom("git_commit_bar")
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.add_space(SPACE_EXTRA_SMALL);
            ui.add(
                egui::TextEdit::multiline(&mut state.commit_message)
                    .hint_text("commit message")
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(SPACE_EXTRA_SMALL);
            let has_staged = state
                .status
                .inside
                .iter()
                .chain(state.status.outside.iter())
                .any(|f| f.has_staged_change());
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
                .filter(|f| f.has_staged_change())
                .collect();
            if staged.is_empty() {
                ui.label(RichText::new("(none)").color(DIM));
            }
            FileTree::from_files(&staged).show(ui, "", true, selected.as_ref(), &mut action);

            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new("UNSTAGED").color(DIM).size(11.0));
            let unstaged: Vec<&FileStatus> = state
                .status
                .inside
                .iter()
                .filter(|f| f.has_worktree_change())
                .collect();
            if unstaged.is_empty() {
                ui.label(RichText::new("(none)").color(DIM));
            }
            FileTree::from_files(&unstaged).show(ui, "", false, selected.as_ref(), &mut action);

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
                        let staged = file.has_staged_change();
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
    if let Some(a) = create_branch_dialog(ui.ctx(), state) {
        action = Some(a);
    }
    if let Some(a) = delete_branch_dialog(ui.ctx(), state) {
        action = Some(a);
    }

    action
}

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
fn branch_row(ui: &mut egui::Ui, state: &mut GitState) -> Option<GitAction> {
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
                    .small_button(RichText::new("x").color(RED))
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
                .small_button(RichText::new("+").color(GREEN))
                .on_hover_text("Create a new branch")
                .clicked()
            {
                state.new_branch = Some(String::new());
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("git_branch_combo")
                    .selected_text(branch_label(current.as_deref()))
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
fn branch_select_action(name: &str, current: Option<&str>) -> Option<GitAction> {
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
fn branch_label(current: Option<&str>) -> RichText {
    match current {
        Some(name) => RichText::new(name),
        None => RichText::new("(detached)").color(AMBER),
    }
}

/// The create-branch dialog, shaped like `new_template_dialog` in
/// `features::contracts::view` so the codebase does not grow a second
/// input-dialog style.
fn create_branch_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    state.new_branch.as_ref()?;
    let mut create = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::new("create_branch_modal"))
        .frame(egui::Frame::window(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("NEW BRANCH").color(GREEN).strong().size(16.0));
            });
            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new("branch name").color(DIM));
            ui.add_space(SPACE_MEDIUM);
            let buf = state.new_branch.as_mut().expect("dialog open");
            let resp = bordered_input(ui, buf, f32::INFINITY, "");
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                create = true;
            }
            ui.add_space(SPACE_LARGE);
            ui.columns(2, |cols| {
                cols[0].vertical_centered(|ui| {
                    if text_button(ui, "Create", GREEN) {
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
fn delete_branch_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
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
                        .color(RED)
                        .strong()
                        .size(16.0),
                );
            });
            ui.add_space(SPACE_MEDIUM);
            ui.label(RichText::new("Delete branch").color(DIM));
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
                    if text_button(ui, "Delete", RED) {
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
            match discard_target(state, &path) {
                DiscardTarget::File => {
                    ui.label(RichText::new("Discard changes to").color(DIM));
                    ui.label(RichText::new(&path).color(TEXT).strong());
                }
                DiscardTarget::Folder(count) => {
                    ui.label(RichText::new("Discard changes in").color(DIM));
                    ui.label(RichText::new(&path).color(TEXT).strong());
                    ui.label(
                        RichText::new(format!(
                            "{count} tracked file{} will be reverted",
                            if count == 1 { "" } else { "s" }
                        ))
                        .color(DIM),
                    );
                }
            }
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

/// What a pending discard path names: an exact file, or a folder holding some
/// number of tracked files. Git only ever reports files in status, so a path
/// that matches no `FileStatus::path` exactly is a folder.
enum DiscardTarget {
    File,
    Folder(usize),
}

/// Resolves a pending discard path against the current status. The count for
/// a folder is tracked files only, matching what `git checkout -- <dir>`
/// actually reverts: untracked files under the folder are left alone.
fn discard_target(state: &GitState, path: &str) -> DiscardTarget {
    let all = state
        .status
        .inside
        .iter()
        .chain(state.status.outside.iter());
    if all.clone().any(|f| f.path == path) {
        return DiscardTarget::File;
    }
    let prefix = format!("{path}/");
    let count = all
        .filter(|f| f.tracked() && f.path.starts_with(&prefix))
        .count();
    DiscardTarget::Folder(count)
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
                    .small_button(RichText::new("-").color(DIM))
                    .on_hover_text(format!("Unstage {}", file.path))
                    .clicked()
                {
                    action = Some(GitAction::Unstage {
                        path: file.path.clone(),
                    });
                }
            } else {
                if file.tracked()
                    && ui
                        .small_button(RichText::new("x").color(RED))
                        .on_hover_text(format!("Discard {}", file.path))
                        .clicked()
                {
                    action = Some(GitAction::RequestDiscard {
                        path: file.path.clone(),
                    });
                }
                if ui
                    .small_button(RichText::new("+").color(GREEN))
                    .on_hover_text(format!("Stage {}", file.path))
                    .clicked()
                {
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

/// A folder tree of a section's changed files, built from their
/// `/`-separated repo-relative paths. Local to `features/git/`: that module
/// may not import `features/contracts/`, and `contracts::view::TreeNode`
/// carries method badges and contract indices that mean nothing here.
#[derive(Default)]
struct FileTree<'a> {
    dirs: BTreeMap<String, FileTree<'a>>,
    files: Vec<&'a FileStatus>,
}

impl<'a> FileTree<'a> {
    fn from_files(files: &[&'a FileStatus]) -> Self {
        let mut tree = FileTree::default();
        for file in files {
            tree.insert(&file.path, file);
        }
        tree
    }

    fn insert(&mut self, rel: &str, file: &'a FileStatus) {
        match rel.split_once('/') {
            Some((dir, rest)) => self
                .dirs
                .entry(dir.to_string())
                .or_default()
                .insert(rest, file),
            None => self.files.push(file),
        }
    }

    /// Whether any file under this node, at any depth, has a staged change.
    fn any_staged(&self) -> bool {
        self.files.iter().any(|f| f.has_staged_change())
            || self.dirs.values().any(FileTree::any_staged)
    }

    /// The count of tracked files under this node, at any depth. Matches what
    /// `git checkout -- <dir>` reverts: untracked files are left alone.
    fn tracked_count(&self) -> usize {
        self.files.iter().filter(|f| f.tracked()).count()
            + self
                .dirs
                .values()
                .map(FileTree::tracked_count)
                .sum::<usize>()
    }

    /// Renders the tree: folders first with `CollapsingState`, then this
    /// node's own files. `prefix` is the path accumulated so far, used for
    /// both the folder's repo-relative path and its persistent id, so
    /// expansion survives a status refresh.
    fn show(
        &self,
        ui: &mut egui::Ui,
        prefix: &str,
        staged: bool,
        selected: Option<&(String, bool)>,
        action: &mut Option<GitAction>,
    ) {
        for (name, child) in &self.dirs {
            let folder_path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let id = ui.make_persistent_id(("git_tree", staged, &folder_path));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    if let Some(a) = folder_row(ui, &folder_path, name, staged, child) {
                        *action = Some(a);
                    }
                })
                .body(|ui| child.show(ui, &folder_path, staged, selected, action));
        }
        for file in &self.files {
            if let Some(a) = file_row(ui, file, staged, selected) {
                *action = Some(a);
            }
        }
    }
}

/// One folder row: the same actions as a file row, acting on the folder's
/// repo-relative path. Buttons are reserved first with a right-to-left
/// layout, and the name is `.truncate()`d, matching `file_row`.
fn folder_row(
    ui: &mut egui::Ui,
    folder_path: &str,
    name: &str,
    staged: bool,
    node: &FileTree,
) -> Option<GitAction> {
    let mut action = None;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if staged {
            if node.any_staged()
                && ui
                    .small_button(RichText::new("-").color(DIM))
                    .on_hover_text(format!("Unstage {folder_path}"))
                    .clicked()
            {
                action = Some(GitAction::Unstage {
                    path: folder_path.to_string(),
                });
            }
        } else {
            let tracked_count = node.tracked_count();
            if tracked_count > 0
                && ui
                    .small_button(RichText::new("x").color(RED))
                    .on_hover_text(format!(
                        "Discard {folder_path} ({tracked_count} tracked file{})",
                        if tracked_count == 1 { "" } else { "s" }
                    ))
                    .clicked()
            {
                action = Some(GitAction::RequestDiscard {
                    path: folder_path.to_string(),
                });
            }
            if ui
                .small_button(RichText::new("+").color(GREEN))
                .on_hover_text(format!("Stage {folder_path}"))
                .clicked()
            {
                action = Some(GitAction::Stage {
                    path: folder_path.to_string(),
                });
            }
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(name).color(DIM)).truncate());
        });
    });
    action
}

/// The central body for the Git tab: the diff view once selected, an empty
/// state until then.
///
/// The panel frame is owned by the app shell. This only sees the git slice.
pub(crate) fn central_body(ui: &mut egui::Ui, state: &mut GitState) {
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
            ui.checkbox(&mut state.raw_view, "Raw diff");
        }
    });
    ui.separator();

    if conflicted {
        raw_diff_view(ui, &data.raw);
        ui.add_space(SPACE_SMALL);
        ui.label(RichText::new("Resolve conflicts in your editor.").color(AMBER));
        return;
    }

    let semantic = if state.raw_view {
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

/// Renders `diff::diff_models` as one block per change. A single-line change
/// keeps the field name in `DIM` followed by `from` in `RED`, an arrow, and
/// `to` in `GREEN`. A change whose `from` or `to` spans multiple lines (a
/// body, mainly) instead renders as a line-level diff under the field name,
/// so a one-line edit inside a many-line body shows as one changed line
/// rather than the whole body twice.
fn semantic_diff_view(ui: &mut egui::Ui, changes: &[FieldChange]) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for change in changes {
                if change.from.contains('\n') || change.to.contains('\n') {
                    multiline_change_view(ui, change);
                } else {
                    single_line_change_view(ui, change);
                }
                ui.add_space(SPACE_SMALL);
            }
        });
}

/// One row for a change whose old and new values are each a single line.
fn single_line_change_view(ui: &mut egui::Ui, change: &FieldChange) {
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

/// The field name as a heading, followed by a line-level diff: lines present
/// only in `from` in `RED`, lines present only in `to` in `GREEN`, unchanged
/// lines in `DIM`.
fn multiline_change_view(ui: &mut egui::Ui, change: &FieldChange) {
    ui.label(RichText::new(&change.what).color(DIM));
    for row in diff::line_diff(&change.from, &change.to) {
        let color = match row.kind {
            diff::LineDiffKind::Removed => RED,
            diff::LineDiffKind::Added => GREEN,
            diff::LineDiffKind::Unchanged => DIM,
        };
        ui.label(RichText::new(&row.text).color(color).monospace());
    }
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
    use crate::features::git::model::{Branches, Change};
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

    fn file(path: &str, conflicted: bool) -> FileStatus {
        FileStatus {
            path: path.into(),
            index: Change::Modified,
            worktree: Change::Modified,
            conflicted,
        }
    }

    fn untracked(path: &str) -> FileStatus {
        FileStatus {
            path: path.into(),
            index: Change::Untracked,
            worktree: Change::Untracked,
            conflicted: false,
        }
    }

    #[test]
    fn folder_with_only_untracked_files_offers_no_discard() {
        let files = [untracked("dir/a.txt"), untracked("dir/b.txt")];
        let refs: Vec<&FileStatus> = files.iter().collect();
        let tree = FileTree::from_files(&refs);
        let dir = tree.dirs.get("dir").expect("dir node");
        assert_eq!(dir.tracked_count(), 0);
    }

    #[test]
    fn folder_with_a_mix_offers_discard_while_untracked_files_do_not() {
        let files = [file("dir/a.rs", false), untracked("dir/b.txt")];
        let refs: Vec<&FileStatus> = files.iter().collect();
        let tree = FileTree::from_files(&refs);
        let dir = tree.dirs.get("dir").expect("dir node");
        assert_eq!(dir.tracked_count(), 1);
        assert!(!files[1].tracked());
    }

    #[test]
    fn tab_label_is_the_plain_name_and_dim_when_clean() {
        assert_eq!(
            tab_label("Git", &Status::default()),
            ("Git".to_string(), DIM)
        );
    }

    #[test]
    fn tab_label_is_the_plain_name_and_red_for_plain_changes() {
        let status = Status {
            inside: vec![file("a.json", false)],
            outside: vec![],
        };
        assert_eq!(tab_label("Git", &status), ("Git".to_string(), RED));
    }

    #[test]
    fn tab_label_gets_the_conflict_suffix_and_red_for_a_conflict() {
        let status = Status {
            inside: vec![file("a.json", true)],
            outside: vec![],
        };
        assert_eq!(
            tab_label("Git", &status),
            ("Git [conflict]".to_string(), RED)
        );
    }

    #[test]
    fn tab_label_keeps_the_conflict_suffix_alongside_several_plain_changes() {
        let status = Status {
            inside: vec![file("a.json", false), file("b.json", true)],
            outside: vec![file("c.json", false)],
        };
        assert_eq!(
            tab_label("Git", &status),
            ("Git [conflict]".to_string(), RED)
        );
    }

    #[test]
    fn sidebar_and_central_render_without_panicking() {
        eframe::egui::__run_test_ui(|ui| {
            let mut state = GitState::default();
            sidebar_body(ui, &mut state, None);
            central_body(ui, &mut state);
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
            central_body(ui, &mut state);
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
        let mut state = state_with_diff(
            "contracts/login.json",
            true,
            DiffData {
                raw: "diff --git a/contracts/login.json b/contracts/login.json".into(),
                old_blob: Some(CONTRACT_GET.into()),
                new_blob: Some(CONTRACT_POST.into()),
            },
        );
        eframe::egui::__run_test_ui(|ui| {
            central_body(ui, &mut state);
        });
    }

    #[test]
    fn central_renders_raw_diff_for_a_non_contract_file() {
        let mut state = state_with_diff(
            "src/main.rs",
            false,
            DiffData {
                raw: "@@ -1 +1 @@\n-old line\n+new line\n".into(),
                old_blob: None,
                new_blob: None,
            },
        );
        eframe::egui::__run_test_ui(|ui| {
            central_body(ui, &mut state);
        });
    }

    #[test]
    fn central_reports_a_reformatting_only_change_and_offers_raw_view() {
        let old = diff::parse(CONTRACT_GET).expect("valid contract");
        let new = diff::parse(CONTRACT_GET_REFORMATTED).expect("valid contract");
        assert!(diff::diff_models(&old, &new).is_empty());
        let mut state = state_with_diff(
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
            central_body(ui, &mut state);
        });
    }

    #[test]
    fn refresh_branches_does_not_refire_once_a_fetch_has_completed_with_no_branches() {
        let mut state = GitState::default();
        let mut first = None;
        eframe::egui::__run_test_ui(|ui| {
            first = sidebar_body(ui, &mut state, Some(Path::new("/repo")));
        });
        assert!(matches!(first, Some(GitAction::RefreshBranches)));

        // Stand in for a completed fetch that found no branches, the state a
        // commitless repository leaves behind for good.
        state.branches_loaded = true;
        let mut second = None;
        eframe::egui::__run_test_ui(|ui| {
            second = sidebar_body(ui, &mut state, Some(Path::new("/repo")));
        });
        assert!(!matches!(second, Some(GitAction::RefreshBranches)));
    }

    #[test]
    fn sidebar_renders_a_normal_two_branch_state_without_panicking() {
        let mut state = GitState {
            branches: Branches {
                current: Some("main".into()),
                all: vec!["main".into(), "feature".into()],
            },
            ..GitState::default()
        };
        eframe::egui::__run_test_ui(|ui| {
            sidebar_body(ui, &mut state, Some(Path::new("/repo")));
        });
    }

    #[test]
    fn sidebar_renders_detached_head_without_panicking() {
        let mut state = GitState {
            branches: Branches {
                current: None,
                all: vec!["main".into()],
            },
            ..GitState::default()
        };
        eframe::egui::__run_test_ui(|ui| {
            sidebar_body(ui, &mut state, Some(Path::new("/repo")));
        });
    }

    #[test]
    fn sidebar_renders_the_create_branch_input_without_panicking() {
        let mut state = GitState {
            branches: Branches {
                current: Some("main".into()),
                all: vec!["main".into()],
            },
            new_branch: Some("feature".into()),
            ..GitState::default()
        };
        eframe::egui::__run_test_ui(|ui| {
            sidebar_body(ui, &mut state, Some(Path::new("/repo")));
        });
        // The dialog stays open until the user confirms or cancels: a bare
        // render must not clear it.
        assert!(state.new_branch.is_some());
    }

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
    fn sidebar_renders_the_delete_branch_confirmation_without_panicking() {
        let mut state = GitState {
            branches: Branches {
                current: Some("main".into()),
                all: vec!["main".into(), "feature".into()],
            },
            pending_branch_delete: Some("feature".into()),
            ..GitState::default()
        };
        eframe::egui::__run_test_ui(|ui| {
            sidebar_body(ui, &mut state, Some(Path::new("/repo")));
        });
        // The dialog stays open until the user confirms or cancels: a bare
        // render must not clear it.
        assert!(state.pending_branch_delete.is_some());
    }
}
