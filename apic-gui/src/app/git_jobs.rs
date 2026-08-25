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
use crate::features::git::state::{DiffData, JobResult, MutateKind};

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
            let result = service::status(&repo_root, &refs);
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

    /// Runs one mutating git command (stage, unstage, discard or commit) on a
    /// background thread, followed by a status refresh in the same job so a
    /// successful mutation lands on screen in one poll. Refuses to start a
    /// second job while one is pending, the same rule as `spawn`.
    fn spawn_mutation(
        &mut self,
        ctx: &egui::Context,
        kind: MutateKind,
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
            let result = op(&repo_root).and_then(|()| service::status(&repo_root, &refs));
            let _ = tx.send(JobResult::Mutate(kind, result));
            ctx.request_repaint();
        });
        self.git.pending = Some(rx);
    }

    /// Stages `path`.
    pub(crate) fn spawn_stage(&mut self, ctx: &egui::Context, path: String) {
        self.spawn_mutation(ctx, MutateKind::Stage, move |root| {
            service::stage(root, &path)
        });
    }

    /// Unstages `path`.
    pub(crate) fn spawn_unstage(&mut self, ctx: &egui::Context, path: String) {
        self.spawn_mutation(ctx, MutateKind::Unstage, move |root| {
            service::unstage(root, &path)
        });
    }

    /// Discards `path`. Called only after the discard confirmation.
    pub(crate) fn spawn_discard(&mut self, ctx: &egui::Context, path: String) {
        self.spawn_mutation(ctx, MutateKind::Discard, move |root| {
            service::discard(root, &path)
        });
    }

    /// Commits the index with `message`.
    pub(crate) fn spawn_commit(&mut self, ctx: &egui::Context, message: String) {
        self.spawn_mutation(ctx, MutateKind::Commit, move |root| {
            service::commit(root, &message)
        });
    }

    /// Checks out an existing branch.
    pub(crate) fn spawn_switch_branch(&mut self, ctx: &egui::Context, name: String) {
        self.spawn_mutation(ctx, MutateKind::SwitchBranch, move |root| {
            service::switch_branch(root, &name)
        });
    }

    /// Creates a branch without switching to it.
    pub(crate) fn spawn_create_branch(&mut self, ctx: &egui::Context, name: String) {
        self.spawn_mutation(ctx, MutateKind::CreateBranch, move |root| {
            service::create_branch(root, &name)
        });
    }

    /// Deletes a branch. Called only after the delete confirmation.
    pub(crate) fn spawn_delete_branch(&mut self, ctx: &egui::Context, name: String) {
        self.spawn_mutation(ctx, MutateKind::DeleteBranch, move |root| {
            service::delete_branch(root, &name)
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
            let result = service::branches(&repo_root);
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
            Ok(JobResult::Mutate(kind, Ok(status))) => {
                self.git.pending = None;
                self.git.status = status;
                self.git.error = String::new();
                if let Some((path, _)) = &self.git.selected
                    && !self
                        .git
                        .status
                        .inside
                        .iter()
                        .chain(self.git.status.outside.iter())
                        .any(|f| &f.path == path)
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
            Ok(JobResult::Mutate(_, Err(e))) => {
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

/// Builds one `DiffData`: the line diff plus, for a `.json` path, each side's
/// content so the caller can attempt a semantic comparison.
///
/// Staged compares the index against `HEAD`; unstaged compares the working
/// file against the index. `git show :<path>` reads the index (stage 0),
/// which is what an empty revision means to `service::blob`.
fn diff_job(root: &Path, path: &str, staged: bool) -> Result<DiffData, String> {
    let mut raw = service::diff_text(root, path, staged)?;
    let head_blob = service::blob(root, "HEAD", path)?;
    let index_blob = service::blob(root, "", path)?;

    // Untracked: absent from both the last commit and the index. `git diff`
    // has nothing to say about a file it has never seen, so build the raw
    // text ourselves from the working file, every line marked added. No
    // hunk header is invented: the renderer only reads the `+` prefix, and
    // a line number nobody computed would be a number nobody should trust.
    if head_blob.is_none() && index_blob.is_none() {
        raw = std::fs::read_to_string(root.join(path))
            .map(|content| {
                content
                    .lines()
                    .map(|line| format!("+{line}\n"))
                    .collect::<String>()
            })
            .unwrap_or_default();
    }

    let (old_blob, new_blob) = if path.ends_with(".json") {
        if staged {
            (head_blob, index_blob)
        } else {
            let new = std::fs::read_to_string(root.join(path)).ok();
            (index_blob, new)
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

/// Resolves symlinks in `path` for comparison purposes, falling back to
/// `path` unresolved when resolution fails, a project directory deleted
/// while the app is open being the case that matters. Never used for a path
/// shown to the user: `ShellState::project_root` stays unresolved in the
/// status bar, and this is only applied to a local copy at the point it is
/// compared against `repo_root`.
fn resolve_for_comparison(path: &Path) -> std::borrow::Cow<'_, Path> {
    match path.canonicalize() {
        Ok(resolved) => std::borrow::Cow::Owned(resolved),
        Err(_) => std::borrow::Cow::Borrowed(path),
    }
}

/// The scopes passed to `service::status`: `root` made relative to
/// `repo_root`, plus `project_root/.apic` made relative to `repo_root` so the
/// project config and templates always count as inside even when they sit
/// outside the contract working dir. Falls back to an empty list (the whole
/// repo, unconditionally inside) when `root` is not under `repo_root`.
///
/// `repo_root` comes from `git rev-parse --show-toplevel`, which resolves
/// symlinks. `root` and `project_root` are stored unresolved, so both are
/// resolved here before the comparison rather than trusting the caller to
/// have matched `repo_root`'s form.
fn scopes(root: &Path, project_root: Option<&Path>, repo_root: &Path) -> Vec<String> {
    let repo_root = resolve_for_comparison(repo_root);
    let root_scope = match resolve_for_comparison(root).strip_prefix(&repo_root) {
        Ok(rel) => apic_core::file::to_slash(rel),
        Err(_) => return Vec::new(),
    };
    let mut scopes = vec![root_scope];
    if let Some(project_root) = project_root
        && let Ok(rel) =
            resolve_for_comparison(&project_root.join(".apic")).strip_prefix(&repo_root)
    {
        scopes.push(apic_core::file::to_slash(rel));
    }
    scopes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An untracked file has no committed and no staged side, so `diff_text`
    /// and both `blob` calls return nothing for it. The diff must still show
    /// the file's own content rather than an empty body.
    #[test]
    fn diff_job_shows_content_for_an_untracked_file() {
        if !crate::app::test_support::git_available() {
            return;
        }
        let root = crate::app::test_support::project_fixture();
        std::fs::write(root.join("contracts").join("untracked.json"), "hello\n").unwrap();

        let data = diff_job(&root, "contracts/untracked.json", false).unwrap();

        assert!(
            data.raw.contains("hello"),
            "an untracked file's diff must carry its own content, got: {:?}",
            data.raw
        );
    }

    /// `project_root` reaches the fixture through a symlink while
    /// `repo_root` is already resolved, the same shape `git rev-parse
    /// --show-toplevel` produces on any machine where the project sits
    /// behind a symlinked temp dir, `/var/folders` on macOS being the
    /// common case. The comparison must not depend on the caller having
    /// already resolved `project_root`.
    #[test]
    #[cfg(unix)]
    fn scopes_resolves_a_symlinked_project_root_before_comparing_to_repo_root() {
        let base = crate::app::test_support::tempdir();
        let real = base.join("real");
        std::fs::create_dir_all(real.join(".apic")).unwrap();
        std::fs::create_dir_all(real.join("contracts")).unwrap();
        let link = base.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let repo_root = real.canonicalize().unwrap();
        let root = repo_root.clone();
        let project_root = link;

        let result = scopes(&root, Some(&project_root), &repo_root);
        assert!(
            result.contains(&".apic".to_string()),
            "a project root reached through a symlink must still resolve to the .apic scope: {result:?}"
        );
    }

    #[test]
    fn scopes_root_is_root_relative_to_repo_root() {
        let repo_root = Path::new("/home/user/proj");
        let root = Path::new("/home/user/proj/apic-gui");
        assert_eq!(scopes(root, None, repo_root), vec!["apic-gui".to_string()]);
    }

    #[test]
    fn scopes_of_repo_root_itself_is_empty_string() {
        let repo_root = Path::new("/home/user/proj");
        assert_eq!(scopes(repo_root, None, repo_root), vec![String::new()]);
    }

    #[test]
    fn scopes_falls_back_to_whole_repo_when_root_is_outside() {
        let repo_root = Path::new("/home/user/proj");
        let root = Path::new("/home/other/place");
        assert_eq!(scopes(root, None, repo_root), Vec::<String>::new());
    }

    #[test]
    fn scopes_includes_dot_apic_under_project_root_when_it_sits_outside_root() {
        let repo_root = Path::new("/home/user/proj");
        let project_root = Path::new("/home/user/proj");
        let root = Path::new("/home/user/proj/contracts");
        assert_eq!(
            scopes(root, Some(project_root), repo_root),
            vec!["contracts".to_string(), ".apic".to_string()]
        );
    }

    #[test]
    fn scopes_omits_dot_apic_when_project_root_is_outside_the_repo() {
        let repo_root = Path::new("/home/user/proj");
        let project_root = Path::new("/home/other/place");
        let root = Path::new("/home/user/proj/contracts");
        assert_eq!(
            scopes(root, Some(project_root), repo_root),
            vec!["contracts".to_string()]
        );
    }
}
