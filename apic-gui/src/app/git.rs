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
use crate::features::git::state::JobResult;

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

    /// Polls the in-flight git job and applies its result. Called every frame
    /// from `App::ui`, next to `poll_dialog`.
    pub(crate) fn poll_git(&mut self, ctx: &egui::Context) {
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
