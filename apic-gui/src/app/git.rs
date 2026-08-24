//! The git feature's shell side: the background worker that keeps `git`
//! commands off the UI thread, and the scope arithmetic that confines the
//! file list to the active contracts root.
//!
//! Mirrors `app::project::{spawn_folder_dialog, poll_dialog}`: a job is
//! refused while one is already pending, run on a spawned thread, and
//! delivered back over a channel that a poll method drains every frame.

use std::path::Path;
use std::sync::mpsc::TryRecvError;

use eframe::egui;

use crate::app::App;
use crate::features::git::service;
use crate::features::git::state::{DiffData, JobResult};

impl App {
    /// Spawns `git status` on a background thread. Refuses to start a second
    /// job while one is pending: concurrent git commands contend for
    /// `index.lock`. Does nothing when the project is not inside a repo.
    pub(crate) fn spawn(&mut self, ctx: &egui::Context) {
        if self.git.pending.is_some() {
            return;
        }
        let Some(repo_root) = self.shell.repo_root.clone() else {
            return;
        };
        let root = self.shell.root.clone().unwrap_or_else(|| repo_root.clone());
        let scope = scope(&root, &repo_root);
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = service::status(&repo_root, &scope);
            let _ = tx.send(JobResult::Status(result));
            ctx.request_repaint();
        });
        self.git.pending = Some(rx);
    }

    /// Spawns the diff fetch for one `(path, staged)` selection on a
    /// background thread. Refuses to start a second job while one is
    /// pending, the same rule as `spawn`.
    fn spawn_diff(&mut self, ctx: &egui::Context, path: String, staged: bool) {
        if self.git.pending.is_some() {
            return;
        }
        let Some(repo_root) = self.shell.repo_root.clone() else {
            return;
        };
        let key = (path.clone(), staged);
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = diff_job(&repo_root, &path, staged);
            let _ = tx.send(JobResult::Diff(key, result));
            ctx.request_repaint();
        });
        self.git.pending = Some(rx);
    }

    /// Starts the diff fetch for the current selection when no job is
    /// pending and the cached diff, if any, is for a different selection.
    /// Called from `poll_git` so a click on `GitAction::Select`, which only
    /// records the selection, is picked up on the very next poll.
    fn maybe_spawn_diff(&mut self, ctx: &egui::Context) {
        if self.git.pending.is_some() {
            return;
        }
        let Some((path, staged)) = self.git.selected.clone() else {
            return;
        };
        let cached = matches!(&self.git.diff, Some((key, _)) if *key == (path.clone(), staged));
        if !cached {
            self.spawn_diff(ctx, path, staged);
        }
    }

    /// Polls the in-flight git job and applies its result. Called every frame
    /// from `App::ui`, next to `poll_dialog`.
    pub(crate) fn poll_git(&mut self, ctx: &egui::Context) {
        self.maybe_spawn_diff(ctx);
        let Some(rx) = &self.git.pending else {
            return;
        };
        match rx.try_recv() {
            Ok(JobResult::Status(Ok(status))) => {
                self.git.pending = None;
                self.git.status = status;
                self.git.error = String::new();
            }
            Ok(JobResult::Status(Err(e))) => {
                self.git.pending = None;
                self.git.error = e;
            }
            Ok(JobResult::Diff(key, Ok(data))) => {
                self.git.pending = None;
                self.git.diff = Some((key, data));
                self.git.error = String::new();
            }
            Ok(JobResult::Diff(_, Err(e))) => {
                self.git.pending = None;
                self.git.error = e;
            }
            Err(TryRecvError::Empty) => ctx.request_repaint(),
            // A panicking worker must not leave the panel disabled forever:
            // clear `pending` and surface an error instead of hanging.
            Err(TryRecvError::Disconnected) => {
                self.git.pending = None;
                self.git.error = "git worker disconnected".into();
            }
        }
    }
}

/// Builds one `DiffData`: the line diff plus, for a `.json` path, each side's
/// content so the caller can attempt a semantic comparison.
///
/// Staged compares the index against `HEAD`; unstaged compares the working
/// file against the index. `git show :<path>` reads the index (stage 0),
/// which is what an empty revision means to `service::blob`.
fn diff_job(root: &Path, path: &str, staged: bool) -> Result<DiffData, String> {
    let raw = service::diff_text(root, path, staged)?;
    let (old_blob, new_blob) = if path.ends_with(".json") {
        if staged {
            (
                service::blob(root, "HEAD", path)?,
                service::blob(root, "", path)?,
            )
        } else {
            let old = service::blob(root, "", path)?;
            let new = std::fs::read_to_string(root.join(path)).ok();
            (old, new)
        }
    } else {
        (None, None)
    };
    Ok(DiffData {
        raw,
        old_blob,
        new_blob,
    })
}

/// The scope passed to `service::status`: `root` made relative to
/// `repo_root`, falling back to an empty scope (the whole repo) when `root`
/// is not under `repo_root`.
fn scope(root: &Path, repo_root: &Path) -> String {
    match root.strip_prefix(repo_root) {
        Ok(rel) => apic_core::file::to_slash(rel),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_is_root_relative_to_repo_root() {
        let repo_root = Path::new("/home/user/proj");
        let root = Path::new("/home/user/proj/apic-gui");
        assert_eq!(scope(root, repo_root), "apic-gui");
    }

    #[test]
    fn scope_of_repo_root_itself_is_empty() {
        let repo_root = Path::new("/home/user/proj");
        assert_eq!(scope(repo_root, repo_root), "");
    }

    #[test]
    fn scope_falls_back_to_whole_repo_when_root_is_outside() {
        let repo_root = Path::new("/home/user/proj");
        let root = Path::new("/home/other/place");
        assert_eq!(scope(root, repo_root), "");
    }
}
