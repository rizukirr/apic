//! The git sidebar's changed-file rows and folder tree: one row per file or
//! folder, grouped by directory. A changed row carries a checkbox that ticks
//! it for the next commit, and its other actions sit in a hover and
//! right-click menu, like the Contracts tree.

use std::collections::BTreeMap;
use std::path::Path;

use eframe::egui;
use egui::RichText;

use crate::features::git::domain::status::{Change, FileStatus};
use crate::features::git::ui::state::GitAction;
use crate::ui::components::row_menu_button;
use crate::ui::theme::*;

/// Which of the sidebar's groups a row belongs to, deciding the controls it
/// offers.
///
/// A conflicted row has no checkbox and no menu: `git add` on a conflicted
/// file marks it resolved with its conflict markers still inside it, so a
/// tick here would let one commit write a broken file into the index, which
/// is exactly what the resolver exists to prevent. Clicking the row is the
/// only action, and it opens the resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum RowMode {
    Changed,
    Conflicted,

    /// A file a commit changed, in the log panel: no checkbox, no menu, and
    /// a click shows that commit's diff of it.
    Commit,
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

/// The menu entries for a file row: Unstage when the file has a staged
/// change, Discard when a tracked file has a worktree change. A conflicted
/// row has none: clicking it opens the resolver instead.
pub(super) fn file_menu_items(file: &FileStatus, mode: RowMode) -> Vec<(&'static str, GitAction)> {
    let mut items = Vec::new();
    if mode != RowMode::Changed {
        return items;
    }
    if file.has_staged_change() {
        items.push((
            "Unstage",
            GitAction::Unstage {
                path: file.path.clone(),
            },
        ));
    }
    if file.tracked() && file.has_worktree_change() {
        items.push((
            "Discard changes",
            GitAction::RequestDiscard {
                path: file.path.clone(),
            },
        ));
    }
    items
}

/// The menu entries for a folder row, acting on its repo-relative path:
/// Unstage all when anything under it is staged, Discard when anything
/// under it can be discarded.
pub(super) fn folder_menu_items(
    folder_path: &str,
    mode: RowMode,
    node: &FileTree,
) -> Vec<(&'static str, GitAction)> {
    let mut items = Vec::new();
    if mode != RowMode::Changed {
        return items;
    }
    if node.any_staged() {
        items.push((
            "Unstage all",
            GitAction::Unstage {
                path: folder_path.to_string(),
            },
        ));
    }
    if node.discardable_count() > 0 {
        items.push((
            "Discard changes",
            GitAction::RequestDiscard {
                path: folder_path.to_string(),
            },
        ));
    }
    items
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

/// One changed-file row: the tick checkbox (changed rows only), the change
/// letter, the selectable file name (truncated), and a hover `⋮` and
/// right-click menu with the row's actions. `ticked` is whether the file is
/// ticked for the next commit.
///
/// The menu button is reserved first with a right-to-left layout, and the
/// name is nested inside a left-to-right layout marked `.truncate()`, the
/// pattern in `features::contracts::ui::view` so a long file name truncates
/// instead of forcing the panel wider than its dragged width.
pub(super) fn file_row(
    ui: &mut egui::Ui,
    file: &FileStatus,
    mode: RowMode,
    ticked: bool,
    selected: Option<&(String, bool)>,
) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    let mut menu_action = None;
    let mut context_action = None;
    // The worktree side wins when it has a change, so a conflicted row (both
    // sides `Modified`) reads the working file with its markers, the only
    // side `diff_job` reads its content from.
    let staged = !file.has_worktree_change();
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
                if mode == RowMode::Changed {
                    let mut t = ticked;
                    if ui.checkbox(&mut t, "").changed() {
                        action = Some(GitAction::SetTicked {
                            paths: vec![file.path.clone()],
                            ticked: t,
                        });
                    }
                }
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
                    action = Some(if mode == RowMode::Commit {
                        GitAction::SelectCommitFile {
                            path: file.path.clone(),
                        }
                    } else {
                        GitAction::Select {
                            path: file.path.clone(),
                            staged,
                        }
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

/// A folder tree of a group's changed files, each with its tick, built from their
/// `/`-separated repo-relative paths. Local to `features/git/`: that module
/// may not import `features/contracts/`, and `contracts::domain::tree::TreeNode`
/// carries method badges and contract indices that mean nothing here.
#[derive(Default)]
pub(super) struct FileTree<'a> {
    dirs: BTreeMap<String, FileTree<'a>>,
    files: Vec<(&'a FileStatus, bool)>,
}

impl<'a> FileTree<'a> {
    /// Builds the tree from `files`, storing `ticked(file)` with each.
    pub(super) fn from_files(
        files: &[&'a FileStatus],
        ticked: impl Fn(&FileStatus) -> bool,
    ) -> Self {
        let mut tree = FileTree::default();
        for file in files {
            tree.insert(&file.path, file, ticked(file));
        }
        tree
    }

    fn insert(&mut self, rel: &str, file: &'a FileStatus, ticked: bool) {
        match rel.split_once('/') {
            Some((dir, rest)) => self
                .dirs
                .entry(dir.to_string())
                .or_default()
                .insert(rest, file, ticked),
            None => self.files.push((file, ticked)),
        }
    }

    /// Every repo-relative file path under this node, at any depth.
    fn paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.files.iter().map(|(f, _)| f.path.clone()).collect();
        for child in self.dirs.values() {
            paths.extend(child.paths());
        }
        paths
    }

    /// The ticked and total file counts under this node, at any depth.
    fn tick_counts(&self) -> (usize, usize) {
        let ticked = self.files.iter().filter(|(_, t)| *t).count();
        self.dirs
            .values()
            .map(FileTree::tick_counts)
            .fold((ticked, self.files.len()), |(t, n), (ct, cn)| {
                (t + ct, n + cn)
            })
    }

    /// Whether any file under this node, at any depth, has a staged change.
    fn any_staged(&self) -> bool {
        self.files.iter().any(|(f, _)| f.has_staged_change())
            || self.dirs.values().any(FileTree::any_staged)
    }

    /// The count of tracked files with a worktree change under this node, at
    /// any depth. Matches what `git checkout -- <dir>` reverts: untracked
    /// files and staged-only changes are left alone.
    fn discardable_count(&self) -> usize {
        self.files
            .iter()
            .filter(|(f, _)| f.tracked() && f.has_worktree_change())
            .count()
            + self
                .dirs
                .values()
                .map(FileTree::discardable_count)
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
        for (file, ticked) in &self.files {
            if let Some(a) = file_row(ui, file, mode, *ticked, selected) {
                *action = Some(a);
            }
        }
    }
}

/// One folder row: a checkbox ticking every file under it (changed rows
/// only, indeterminate when some are ticked), then the name, with the same
/// actions as a file row in a hover and right-click menu, acting on the
/// folder's repo-relative path. The menu
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
    let mut tick_action = None;
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
            if mode == RowMode::Changed {
                let (ticked, total) = node.tick_counts();
                let mut all = ticked == total;
                let checkbox =
                    egui::Checkbox::new(&mut all, "").indeterminate(ticked > 0 && ticked < total);
                if ui.add(checkbox).changed() {
                    tick_action = Some(GitAction::SetTicked {
                        paths: node.paths(),
                        ticked: ticked < total,
                    });
                }
            }
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
    menu_action.or(context_action).or(tick_action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::domain::status::fixtures::{file, untracked};

    use crate::features::git::domain::status::Change;

    fn side(path: &str, index: Change, worktree: Change) -> FileStatus {
        FileStatus {
            path: path.to_string(),
            index,
            worktree,
            conflicted: false,
        }
    }

    fn labels(items: &[(&'static str, GitAction)]) -> Vec<&'static str> {
        items.iter().map(|(l, _)| *l).collect()
    }

    #[test]
    fn folder_with_only_untracked_files_offers_no_discard() {
        let files = [untracked("dir/a.txt"), untracked("dir/b.txt")];
        let refs: Vec<&FileStatus> = files.iter().collect();
        let tree = FileTree::from_files(&refs, |_| false);
        let dir = tree.dirs.get("dir").expect("dir node");
        assert_eq!(dir.discardable_count(), 0);
    }

    #[test]
    fn folder_with_a_mix_offers_discard_while_untracked_files_do_not() {
        let files = [file("dir/a.rs", false), untracked("dir/b.txt")];
        let refs: Vec<&FileStatus> = files.iter().collect();
        let tree = FileTree::from_files(&refs, |_| false);
        let dir = tree.dirs.get("dir").expect("dir node");
        assert_eq!(dir.discardable_count(), 1);
        assert!(!files[1].tracked());
    }

    #[test]
    fn file_menu_offers_unstage_and_discard_by_side() {
        let staged_only = side("a.json", Change::Modified, Change::Unmodified);
        assert_eq!(
            labels(&file_menu_items(&staged_only, RowMode::Changed)),
            ["Unstage"]
        );
        let unstaged = side("b.json", Change::Unmodified, Change::Modified);
        assert_eq!(
            labels(&file_menu_items(&unstaged, RowMode::Changed)),
            ["Discard changes"]
        );
        let both = file("c.json", false);
        let items = file_menu_items(&both, RowMode::Changed);
        assert!(
            items
                .iter()
                .any(|(_, a)| matches!(a, GitAction::Unstage { path } if path == "c.json"))
        );
        assert!(
            items
                .iter()
                .any(|(_, a)| matches!(a, GitAction::RequestDiscard { path } if path == "c.json"))
        );
        assert!(file_menu_items(&untracked("d.json"), RowMode::Changed).is_empty());
        assert!(file_menu_items(&file("e.json", true), RowMode::Conflicted).is_empty());
    }

    #[test]
    fn folder_menu_offers_unstage_all_and_discard_by_side() {
        let only_new = [untracked("dir/a.txt")];
        let refs: Vec<&FileStatus> = only_new.iter().collect();
        let tree = FileTree::from_files(&refs, |_| false);
        let dir = tree.dirs.get("dir").expect("dir node");
        assert!(folder_menu_items("dir", RowMode::Changed, dir).is_empty());

        let mixed = [file("dir/a.rs", false)];
        let refs: Vec<&FileStatus> = mixed.iter().collect();
        let tree = FileTree::from_files(&refs, |_| false);
        let dir = tree.dirs.get("dir").expect("dir node");
        let items = folder_menu_items("dir", RowMode::Changed, dir);
        assert!(
            items
                .iter()
                .any(|(_, a)| matches!(a, GitAction::Unstage { path } if path == "dir"))
        );
        assert!(
            items
                .iter()
                .any(|(_, a)| matches!(a, GitAction::RequestDiscard { path } if path == "dir"))
        );
    }

    #[test]
    fn tick_counts_and_paths_cover_nested_files() {
        let files = [
            untracked("dir/a.json"),
            untracked("dir/sub/b.json"),
            untracked("dir/sub/c.json"),
        ];
        let refs: Vec<&FileStatus> = files.iter().collect();
        let tree = FileTree::from_files(&refs, |f| f.path != "dir/sub/c.json");
        let dir = tree.dirs.get("dir").expect("dir node");
        assert_eq!(dir.tick_counts(), (2, 3));
        let mut paths = dir.paths();
        paths.sort();
        assert_eq!(paths, ["dir/a.json", "dir/sub/b.json", "dir/sub/c.json"]);
        let sub = dir.dirs.get("sub").expect("sub node");
        assert_eq!(sub.tick_counts(), (1, 2));
    }

    #[test]
    fn log_view_commit_rows_offer_no_menu() {
        let added = side("dir/a.json", Change::Added, Change::Unmodified);
        assert!(file_menu_items(&added, RowMode::Commit).is_empty());
        let refs = [&added];
        let tree = FileTree::from_files(&refs, |_| false);
        let dir = tree.dirs.get("dir").expect("dir node");
        assert!(folder_menu_items("dir", RowMode::Commit, dir).is_empty());
    }
}
