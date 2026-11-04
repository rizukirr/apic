//! The git sidebar's changed-file rows and folder tree: one row per file or
//! folder, grouped by directory. Rows carry their actions in a hover and
//! right-click menu, like the Contracts tree.

use std::collections::BTreeMap;
use std::path::Path;

use eframe::egui;
use egui::RichText;

use crate::features::git::domain::status::{Change, FileStatus};
use crate::features::git::ui::state::GitAction;
use crate::ui::components::row_menu_button;
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

/// The colour of a change letter: success for added or untracked, warning
/// for modified or renamed, error for deleted or conflicted.
fn change_color(change: Change, conflicted: bool, p: &Palette) -> egui::Color32 {
    if conflicted {
        return p.error;
    }
    match change {
        Change::Added | Change::Untracked => p.success,
        Change::Modified | Change::Renamed => p.warning,
        Change::Deleted => p.error,
        Change::Unmodified => p.on_surface_variant,
    }
}

/// The menu entries for a file row. A conflicted row has none: staging it
/// would write its conflict markers into the index, and clicking the row
/// opens the resolver instead.
pub(super) fn file_menu_items(file: &FileStatus, mode: RowMode) -> Vec<(&'static str, GitAction)> {
    let path = file.path.clone();
    match mode {
        RowMode::Staged => vec![("Unstage", GitAction::Unstage { path })],
        RowMode::Unstaged => {
            let mut items = vec![("Stage", GitAction::Stage { path: path.clone() })];
            if file.tracked() {
                items.push(("Discard changes", GitAction::RequestDiscard { path }));
            }
            items
        }
        RowMode::Conflicted => Vec::new(),
    }
}

/// The menu entries for a folder row, acting on its repo-relative path.
pub(super) fn folder_menu_items(
    folder_path: &str,
    mode: RowMode,
    node: &FileTree,
) -> Vec<(&'static str, GitAction)> {
    let path = folder_path.to_string();
    match mode {
        RowMode::Staged if node.any_staged() => vec![("Unstage all", GitAction::Unstage { path })],
        RowMode::Staged | RowMode::Conflicted => Vec::new(),
        RowMode::Unstaged => {
            let mut items = vec![("Stage all", GitAction::Stage { path: path.clone() })];
            if node.tracked_count() > 0 {
                items.push(("Discard changes", GitAction::RequestDiscard { path }));
            }
            items
        }
    }
}

/// Renders `items` as menu buttons, recording the clicked one in `out`.
fn menu_items_ui(
    ui: &mut egui::Ui,
    items: Vec<(&'static str, GitAction)>,
    out: &mut Option<GitAction>,
) {
    for (label, action) in items {
        if ui.button(label).clicked() {
            *out = Some(action);
            ui.close();
        }
    }
}

/// One changed-file row: the change letter, the selectable file name
/// (truncated), and a hover `⋮` and right-click menu with the row's actions.
///
/// The menu button is reserved first with a right-to-left layout, and the
/// name is nested inside a left-to-right layout marked `.truncate()`, the
/// pattern in `features::contracts::view` so a long file name truncates
/// instead of forcing the panel wider than its dragged width.
pub(super) fn file_row(
    ui: &mut egui::Ui,
    file: &FileStatus,
    mode: RowMode,
    selected: Option<&(String, bool)>,
) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    let mut menu_action = None;
    let mut context_action = None;
    let staged = mode.staged();
    let change = if staged { file.index } else { file.worktree };
    let name = Path::new(&file.path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.path.clone());
    let is_selected = selected == Some(&(file.path.clone(), staged));
    let has_menu = !file_menu_items(file, mode).is_empty();
    ui.horizontal(|ui| {
        let hovered = ui.rect_contains_pointer(ui.max_rect());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if has_menu {
                row_menu_button(ui, hovered, |ui| {
                    menu_items_ui(ui, file_menu_items(file, mode), &mut menu_action)
                });
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let letter = if file.conflicted {
                    "U"
                } else {
                    change.letter()
                };
                ui.label(
                    RichText::new(letter)
                        .color(change_color(change, file.conflicted, p))
                        .size(11.0),
                );
                let button =
                    egui::Button::selectable(is_selected, RichText::new(name).color(p.on_surface))
                        .truncate();
                let response = ui.add(button);
                if response.clicked() {
                    action = Some(GitAction::Select {
                        path: file.path.clone(),
                        staged,
                    });
                }
                if has_menu {
                    response.context_menu(|ui| {
                        menu_items_ui(ui, file_menu_items(file, mode), &mut context_action)
                    });
                }
            });
        });
    });
    menu_action.or(context_action).or(action)
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

/// One folder row: the name, with the same actions as a file row in a hover
/// and right-click menu, acting on the folder's repo-relative path. The menu
/// button is reserved first with a right-to-left layout, and the name is
/// `.truncate()`d, matching `file_row`.
pub(super) fn folder_row(
    ui: &mut egui::Ui,
    folder_path: &str,
    name: &str,
    mode: RowMode,
    node: &FileTree,
) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let mut menu_action = None;
    let mut context_action = None;
    let has_menu = !folder_menu_items(folder_path, mode, node).is_empty();
    let hovered = ui.rect_contains_pointer(ui.max_rect());
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if has_menu {
            row_menu_button(ui, hovered, |ui| {
                menu_items_ui(
                    ui,
                    folder_menu_items(folder_path, mode, node),
                    &mut menu_action,
                )
            });
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let label = egui::Label::new(RichText::new(name).color(p.on_surface_variant))
                .truncate()
                .sense(egui::Sense::click());
            let response = ui.add(label);
            if has_menu {
                response.context_menu(|ui| {
                    menu_items_ui(
                        ui,
                        folder_menu_items(folder_path, mode, node),
                        &mut context_action,
                    )
                });
            }
        });
    });
    menu_action.or(context_action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::domain::status::fixtures::{file, untracked};

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
    fn file_menu_offers_stage_discard_unstage_by_row() {
        let tracked = file("a.json", false);
        let unstaged = file_menu_items(&tracked, RowMode::Unstaged);
        assert!(
            unstaged
                .iter()
                .any(|(_, a)| matches!(a, GitAction::Stage { path } if path == "a.json"))
        );
        assert!(
            unstaged
                .iter()
                .any(|(_, a)| matches!(a, GitAction::RequestDiscard { path } if path == "a.json"))
        );
        let new = untracked("b.json");
        let untracked_items = file_menu_items(&new, RowMode::Unstaged);
        assert!(
            untracked_items
                .iter()
                .any(|(_, a)| matches!(a, GitAction::Stage { .. }))
        );
        assert!(
            !untracked_items
                .iter()
                .any(|(_, a)| matches!(a, GitAction::RequestDiscard { .. }))
        );
        let staged = file_menu_items(&tracked, RowMode::Staged);
        assert!(
            staged
                .iter()
                .any(|(_, a)| matches!(a, GitAction::Unstage { path } if path == "a.json"))
        );
        assert!(file_menu_items(&file("c.json", true), RowMode::Conflicted).is_empty());
    }

    #[test]
    fn folder_menu_offers_stage_all_discard_unstage_all_by_row() {
        let only_new = [untracked("dir/a.txt")];
        let refs: Vec<&FileStatus> = only_new.iter().collect();
        let tree = FileTree::from_files(&refs);
        let dir = tree.dirs.get("dir").expect("dir node");
        let items = folder_menu_items("dir", RowMode::Unstaged, dir);
        assert!(
            items
                .iter()
                .any(|(_, a)| matches!(a, GitAction::Stage { path } if path == "dir"))
        );
        assert!(
            !items
                .iter()
                .any(|(_, a)| matches!(a, GitAction::RequestDiscard { .. }))
        );

        let mixed = [file("dir/a.rs", false)];
        let refs: Vec<&FileStatus> = mixed.iter().collect();
        let tree = FileTree::from_files(&refs);
        let dir = tree.dirs.get("dir").expect("dir node");
        let unstaged = folder_menu_items("dir", RowMode::Unstaged, dir);
        assert!(
            unstaged
                .iter()
                .any(|(_, a)| matches!(a, GitAction::RequestDiscard { path } if path == "dir"))
        );
        let staged = folder_menu_items("dir", RowMode::Staged, dir);
        assert!(
            staged
                .iter()
                .any(|(_, a)| matches!(a, GitAction::Unstage { path } if path == "dir"))
        );
    }
}
