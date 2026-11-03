//! The git sidebar's changed-file rows and folder tree: one row per file or
//! folder with its stage, unstage and discard controls, grouped by directory.

use std::collections::BTreeMap;
use std::path::Path;

use eframe::egui;
use egui::RichText;

use crate::features::git::domain::status::FileStatus;
use crate::features::git::ui::state::GitAction;
use crate::ui::theme::*;

/// Which of the sidebar's three groups a row belongs to, deciding both the
/// buttons it offers and which side (`index` or `worktree`) it reads.
///
/// A conflicted row offers no stage, unstage or discard control: `git add`
/// on a conflicted file marks it resolved with its conflict markers still
/// inside it, so a stage button here would let one click write a broken file
/// into the index, which is exactly what the resolver exists to prevent.
/// Clicking the row is the only action, and it opens the resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum RowMode {
    Staged,
    Unstaged,
    Conflicted,
}

impl RowMode {
    /// Whether this row reads `index` (true) or `worktree` (false), and the
    /// `staged` half of the `(path, staged)` selection key. Conflicted rows
    /// read `worktree`: `diff_job` only populates `new_blob`, the working
    /// file with its markers, when `staged` is false.
    fn staged(self) -> bool {
        matches!(self, RowMode::Staged)
    }
}

/// One changed-file row: the change letter, the file name (truncated), and a
/// red conflict indicator when the file is in a merge conflict.
///
/// The conflict indicator is reserved first with a right-to-left layout, and
/// the label is nested inside a left-to-right layout marked `.truncate()`,
/// the pattern in `features::contracts::view` so a long file name truncates
/// instead of forcing the panel wider than its dragged width.
pub(super) fn file_row(
    ui: &mut egui::Ui,
    file: &FileStatus,
    mode: RowMode,
    selected: Option<&(String, bool)>,
) -> Option<GitAction> {
    let mut action = None;
    let staged = mode.staged();
    let change = if staged { file.index } else { file.worktree };
    let name = Path::new(&file.path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.path.clone());
    let is_selected = selected == Some(&(file.path.clone(), staged));
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            match mode {
                RowMode::Staged => {
                    if ui
                        .small_button(RichText::new("-").color(DIM))
                        .on_hover_text(format!("Unstage {}", file.path))
                        .clicked()
                    {
                        action = Some(GitAction::Unstage {
                            path: file.path.clone(),
                        });
                    }
                }
                RowMode::Unstaged => {
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
                RowMode::Conflicted => {}
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
pub(super) struct FileTree<'a> {
    dirs: BTreeMap<String, FileTree<'a>>,
    files: Vec<&'a FileStatus>,
}

impl<'a> FileTree<'a> {
    pub(super) fn from_files(files: &[&'a FileStatus]) -> Self {
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
    pub(super) fn show(
        &self,
        ui: &mut egui::Ui,
        prefix: &str,
        mode: RowMode,
        selected: Option<&(String, bool)>,
        action: &mut Option<GitAction>,
    ) {
        for (name, child) in &self.dirs {
            let folder_path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let id = ui.make_persistent_id(("git_tree", mode, &folder_path));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    if let Some(a) = folder_row(ui, &folder_path, name, mode, child) {
                        *action = Some(a);
                    }
                })
                .body(|ui| child.show(ui, &folder_path, mode, selected, action));
        }
        for file in &self.files {
            if let Some(a) = file_row(ui, file, mode, selected) {
                *action = Some(a);
            }
        }
    }
}

/// One folder row: the same actions as a file row, acting on the folder's
/// repo-relative path. Buttons are reserved first with a right-to-left
/// layout, and the name is `.truncate()`d, matching `file_row`.
pub(super) fn folder_row(
    ui: &mut egui::Ui,
    folder_path: &str,
    name: &str,
    mode: RowMode,
    node: &FileTree,
) -> Option<GitAction> {
    let mut action = None;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        match mode {
            RowMode::Staged => {
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
            }
            RowMode::Unstaged => {
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
            RowMode::Conflicted => {}
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(name).color(DIM)).truncate());
        });
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::domain::status::Change;

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
}
