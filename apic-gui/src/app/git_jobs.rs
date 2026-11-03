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
use crate::features::git::data::cli;
use crate::features::git::data::jobs::{JobResult, MutateKind, diff_job, scopes};

impl App {
    /// Queues a status refresh for [`App::maybe_refresh_status`] to start once
    /// no other git job is in flight. Called after a project loads and after
    /// the app saves a contract: the two moments the tree can change without
    /// the user ever opening the Git tab, so the tab's dirty indicator would
    /// otherwise stay stale until first activation.
    pub(crate) fn request_status_refresh(&mut self) {
        self.needs_git_refresh = true;
    }

    /// Starts the queued status refresh once no other job is in flight.
    /// Called every frame from `App::ui`, next to `poll_git`. Retrying every
    /// frame rather than starting the job the moment it is requested means a
    /// refresh requested while another job is in flight is not silently
    /// dropped by that job's `pending` guard.
    pub(crate) fn maybe_refresh_status(&mut self, ctx: &egui::Context) {
        if self.needs_git_refresh && self.git.pending.is_none() {
            self.needs_git_refresh = false;
            self.spawn(ctx);
        }
    }

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
        let scopes = scopes(&root, self.shell.project_root.as_deref(), &repo_root);
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let refs: Vec<&str> = scopes.iter().map(String::as_str).collect();
            let result = cli::status(&repo_root, &refs);
            let _ = tx.send(JobResult::Status(result));
            ctx.request_repaint();
        });
        self.git.pending = Some(rx);
    }

    /// Spawns the diff fetch for one `(path, staged)` selection on a
    /// background thread. Refuses to start a second job while one is
    /// pending, the same rule as `spawn`. Called from `GitAction::Select`.
    pub(crate) fn spawn_diff(&mut self, ctx: &egui::Context, path: String, staged: bool) {
        if self.git.pending.is_some() {
            return;
        }
        let Some(repo_root) = self.shell.repo_root.clone() else {
            return;
        };
        let conflicted = self.git.status.find(&path).is_some_and(|f| f.conflicted);
        let key = (path.clone(), staged);
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = diff_job(&repo_root, &path, staged, conflicted);
            let _ = tx.send(JobResult::Diff(key, result));
            ctx.request_repaint();
        });
        self.git.pending = Some(rx);
    }

    /// Runs one mutating git command (stage, unstage, discard or commit) on a
    /// background thread, followed by a status refresh in the same job so a
    /// successful mutation lands on screen in one poll. Refuses to start a
    /// second job while one is pending, the same rule as `spawn`.
    ///
    /// `message` names what the mutation did, for `shell.status` on success.
    /// It is built by the caller rather than derived here or in the poll,
    /// since the caller already has the path or branch name in scope and
    /// `MutateKind` would otherwise have no use for one.
    fn spawn_mutation(
        &mut self,
        ctx: &egui::Context,
        kind: MutateKind,
        message: String,
        op: impl FnOnce(&Path) -> Result<(), String> + Send + 'static,
    ) {
        if self.git.pending.is_some() {
            return;
        }
        let Some(repo_root) = self.shell.repo_root.clone() else {
            return;
        };
        let root = self.shell.root.clone().unwrap_or_else(|| repo_root.clone());
        let scopes = scopes(&root, self.shell.project_root.as_deref(), &repo_root);
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let refs: Vec<&str> = scopes.iter().map(String::as_str).collect();
            let result = op(&repo_root).and_then(|()| cli::status(&repo_root, &refs));
            let _ = tx.send(JobResult::Mutate(kind, message, result));
            ctx.request_repaint();
        });
        self.git.pending = Some(rx);
    }

    /// Stages `path`.
    pub(crate) fn spawn_stage(&mut self, ctx: &egui::Context, path: String) {
        let message = format!("staged {path}");
        self.spawn_mutation(ctx, MutateKind::Stage, message, move |root| {
            cli::stage(root, &path)
        });
    }

    /// Unstages `path`.
    pub(crate) fn spawn_unstage(&mut self, ctx: &egui::Context, path: String) {
        let message = format!("unstaged {path}");
        self.spawn_mutation(ctx, MutateKind::Unstage, message, move |root| {
            cli::unstage(root, &path)
        });
    }

    /// Discards `path`. Called only after the discard confirmation.
    pub(crate) fn spawn_discard(&mut self, ctx: &egui::Context, path: String) {
        let message = format!("discarded {path}");
        self.spawn_mutation(ctx, MutateKind::Discard, message, move |root| {
            cli::discard(root, &path)
        });
    }

    /// Commits the index with `message`.
    pub(crate) fn spawn_commit(&mut self, ctx: &egui::Context, message: String) {
        let status_message = "committed the staged changes".to_string();
        self.spawn_mutation(ctx, MutateKind::Commit, status_message, move |root| {
            cli::commit(root, &message)
        });
    }

    /// Checks out an existing branch.
    pub(crate) fn spawn_switch_branch(&mut self, ctx: &egui::Context, name: String) {
        let message = format!("switched to branch {name}");
        self.spawn_mutation(ctx, MutateKind::SwitchBranch, message, move |root| {
            cli::switch_branch(root, &name)
        });
    }

    /// Creates a branch without switching to it.
    pub(crate) fn spawn_create_branch(&mut self, ctx: &egui::Context, name: String) {
        let message = format!("created branch {name}");
        self.spawn_mutation(ctx, MutateKind::CreateBranch, message, move |root| {
            cli::create_branch(root, &name)
        });
    }

    /// Deletes a branch. Called only after the delete confirmation.
    pub(crate) fn spawn_delete_branch(&mut self, ctx: &egui::Context, name: String) {
        let message = format!("deleted branch {name}");
        self.spawn_mutation(ctx, MutateKind::DeleteBranch, message, move |root| {
            cli::delete_branch(root, &name)
        });
    }

    /// Writes the rendered resolution `text` for `path` and stages it.
    ///
    /// The write and the stage are two halves of one operation: editing the
    /// file does not clear the conflict in git, only `git add` does. If the
    /// write succeeds and the stage then fails, that failure must still
    /// reach the user as an error rather than being swallowed, a file that
    /// looks resolved while git still calls it conflicted is worse than a
    /// resolve that failed outright.
    pub(crate) fn spawn_resolve(&mut self, ctx: &egui::Context, path: String, text: String) {
        let message = format!("resolved {path}");
        self.spawn_mutation(ctx, MutateKind::Resolve, message, move |root| {
            std::fs::write(root.join(&path), text).map_err(|e| e.to_string())?;
            cli::stage(root, &path)
        });
    }

    /// Spawns `git branch` on a background thread. Refuses to start a second
    /// job while one is pending, the same rule as `spawn`. Follows `spawn`
    /// rather than `spawn_mutation` since listing branches mutates nothing.
    pub(crate) fn spawn_branches(&mut self, ctx: &egui::Context) {
        if self.git.pending.is_some() {
            return;
        }
        let Some(repo_root) = self.shell.repo_root.clone() else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = cli::branches(&repo_root);
            let _ = tx.send(JobResult::Branches(result));
            ctx.request_repaint();
        });
        self.git.pending = Some(rx);
    }

    /// Reconciles the open contract with the freshly checked-out branch.
    /// A checkout can change or remove the tree behind every piece of
    /// derived contract state at once, and `reload_project` only rebuilds
    /// `contracts.entries`. Does not touch `shell.project_root` or persisted
    /// settings, unlike `activate_project`: a checkout changes neither.
    fn reconcile_after_checkout(&mut self) {
        self.reload_project();
        let path = self.contracts.path.clone();
        match path.and_then(|p| self.contracts.entries.iter().position(|e| e.path == p)) {
            Some(i) => self.load(i),
            None => {
                self.contracts.model = None;
                self.contracts.path = None;
                self.contracts.selected = None;
            }
        }
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
            Ok(JobResult::Diff(key, Ok(data))) => {
                self.git.pending = None;
                self.git.diff = Some((key, data));
                self.git.error = String::new();
            }
            Ok(JobResult::Diff(_, Err(e))) => {
                self.git.pending = None;
                self.git.error = e;
            }
            Ok(JobResult::Mutate(kind, message, Ok(status))) => {
                self.git.pending = None;
                self.git.status = status;
                self.git.error = String::new();
                self.shell.status = message;
                if let Some((path, _)) = &self.git.selected
                    && self.git.status.find(path).is_none()
                {
                    self.git.selected = None;
                    self.git.diff = None;
                }
                if matches!(kind, MutateKind::Commit) {
                    self.git.commit_message.clear();
                }
                if matches!(kind, MutateKind::SwitchBranch) {
                    self.reconcile_after_checkout();
                }
                if matches!(
                    kind,
                    MutateKind::SwitchBranch | MutateKind::CreateBranch | MutateKind::DeleteBranch
                ) {
                    self.spawn_branches(ctx);
                }
            }
            Ok(JobResult::Mutate(_, _, Err(e))) => {
                self.git.pending = None;
                self.git.error = e.clone();
                self.shell.status = e;
            }
            Ok(JobResult::Branches(Ok(branches))) => {
                self.git.pending = None;
                self.git.branches = branches;
                self.git.error = String::new();
            }
            Ok(JobResult::Branches(Err(e))) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::app::actions::{Action, GitAction};
    use crate::app::test_support::{
        app_at, conflict_fixture, git_available, project_fixture, settle,
    };

    /// Runs `git -C root <args>` and returns stdout, for assertions that read
    /// the repository directly rather than the app's own cache.
    fn git_output(root: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("git spawns");
        assert!(out.status.success(), "git {args:?} failed: {out:?}");
        String::from_utf8(out.stdout).expect("git output is utf8")
    }

    /// A successful resolve must leave `shell.status` non-empty and naming
    /// the resolved path, so the user learns the resolve happened instead of
    /// noticing it only by a row moving between sections. This is the
    /// regression the whole task exists to fix.
    #[test]
    fn resolve_conflict_reports_success_in_shell_status() {
        if !git_available() {
            return;
        }
        let root = conflict_fixture();
        let mut app = app_at(root);
        let ctx = egui::Context::default();
        app.reload_project();

        app.apply(
            Action::Git(GitAction::ResolveConflict {
                path: "contracts/sample.json".to_string(),
                text: apic_core::template::DEFAULT.to_string(),
            }),
            &ctx,
        );
        settle(&mut app, &ctx);

        assert!(
            !app.shell.status.is_empty(),
            "a successful resolve must report itself in shell.status"
        );
        assert!(
            app.shell.status.contains("contracts/sample.json"),
            "the status must name the resolved path, got: {:?}",
            app.shell.status
        );
    }

    /// A failing mutation must still put git's own error text in
    /// `shell.status`, so a success message can never quietly replace a real
    /// error. Deleting a branch with unmerged commits is refused by git with
    /// a message on stderr, which is what `spawn_delete_branch` surfaces.
    #[test]
    fn failing_mutation_still_reports_git_error_in_shell_status() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let mut app = app_at(root.clone());
        let ctx = egui::Context::default();
        app.reload_project();

        app.apply(
            Action::Git(GitAction::RequestBranchDelete {
                name: "apic-second".to_string(),
            }),
            &ctx,
        );
        app.apply(Action::Git(GitAction::ConfirmBranchDelete), &ctx);
        settle(&mut app, &ctx);

        let listed = git_output(&root, &["branch", "--list", "apic-second"]);
        assert!(
            listed.contains("apic-second"),
            "the delete must actually have been refused: {listed:?}"
        );
        assert!(
            !app.shell.status.is_empty(),
            "a failing mutation must still report something in shell.status"
        );
        assert!(
            !app.shell.status.contains("deleted branch"),
            "a success message must never replace git's own error, got: {:?}",
            app.shell.status
        );
        assert_eq!(
            app.shell.status, app.git.error,
            "shell.status must carry git's own stderr, the same text as git.error"
        );
    }
}
