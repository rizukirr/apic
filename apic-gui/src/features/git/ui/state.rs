//! The git feature's view model: what the panel remembers, and the handling of every git action and finished background job.

use std::sync::mpsc::TryRecvError;

use eframe::egui;

use crate::features::git::data::jobs::{
    self, DiffData, DiffRequest, JobResult, MutateKind, Mutation, RepoPaths,
};
use crate::features::git::domain::conflict::{self, Choice, ConflictFile, Segment};
use crate::features::git::domain::status::{Branches, Status};

/// One completed diff fetch: the raw line diff plus each side's file content,
/// A conflicted file parsed into its blocks, plus the choice made for each
/// one so far. Rebuilt from scratch whenever the selected path changes, so it
/// never carries choices belonging to a different file.
pub(crate) struct ResolveState {
    /// The repo-relative path being resolved.
    pub(crate) path: String,

    /// The parsed segments: text and conflict blocks, in order.
    pub(crate) file: ConflictFile,

    /// One entry per conflict block in `file`, in the same order. `None`
    /// until the user picks a side for that block.
    pub(crate) choices: Vec<Option<Choice>>,
}

impl ResolveState {
    /// Parses the conflicted working file `text` at `path`, with every block
    /// undecided. `None` when the markers do not parse, in which case the
    /// panel falls back to the read-only diff.
    pub(crate) fn parse(path: &str, text: &str) -> Option<ResolveState> {
        let file = conflict::parse(text)?;
        let block_count = file
            .segments
            .iter()
            .filter(|s| matches!(s, Segment::Conflict { .. }))
            .count();
        Some(ResolveState {
            path: path.to_string(),
            file,
            choices: vec![None; block_count],
        })
    }
}

/// A one-shot action requested by the Git panel this frame.
pub(crate) enum GitAction {
    /// Re-read `git status`.
    Refresh,

    /// Tick or untick Amend on the commit box.
    #[cfg_attr(not(test), expect(dead_code, reason = "no panel control sends it yet"))]
    ToggleAmend,

    /// Show the diff for this repo-relative path, staged or unstaged.
    Select { path: String, staged: bool },

    /// Stage this repo-relative path.
    Stage { path: String },

    /// Unstage this repo-relative path, leaving the working tree untouched.
    Unstage { path: String },

    /// Ask to discard this tracked path, shows a confirmation before
    /// anything is reverted.
    RequestDiscard { path: String },

    /// The pending discard was confirmed, revert it.
    ConfirmDiscard,

    /// Commit the currently staged changes with `GitState::commit_message`.
    Commit,

    /// Re-read the branch list.
    RefreshBranches,

    /// Switch to this local branch.
    SwitchBranch { name: String },

    /// Create this branch without switching to it.
    CreateBranch { name: String },

    /// Ask to delete this branch, shows a confirmation before anything is
    /// removed.
    RequestBranchDelete { name: String },

    /// The pending branch delete was confirmed, delete it.
    ConfirmBranchDelete,

    /// Write the rendered resolution `text` for this conflicted path and
    /// stage it. The text is already rendered, this action carries no
    /// markers for the job to parse.
    ResolveConflict { path: String, text: String },
}

/// Everything the git feature owns. `App` holds one of these; no other
/// feature may read or write it.
#[derive(Default)]
pub(crate) struct GitState {
    /// The last status read from the repository, scoped to the contracts
    /// working dir plus a count of changes elsewhere.
    pub(crate) status: Status,

    /// The local branches and which one is checked out, from the last
    /// listing.
    pub(crate) branches: Branches,

    /// Whether the initial branch fetch has already been requested. An empty
    /// `branches.all` is a valid result, a repository with no commits has no
    /// branches, so emptiness alone cannot tell "not fetched yet" from
    /// "fetched and there are none". Set true by the sidebar the moment it
    /// asks for that first fetch, so the request never repeats regardless of
    /// whether the fetch succeeds, fails, or comes back empty. Branch
    /// mutations (switch, create, delete) refresh the list through their own
    /// call in the app shell and do not depend on this flag.
    pub(crate) branches_loaded: bool,

    /// The path and side (staged or unstaged) whose diff is shown, when one
    /// is selected.
    pub(crate) selected: Option<(String, bool)>,

    /// The diff fetched for `selected`, keyed the same way, so switching
    /// between two files and back does not re-run git. `None` while the
    /// first fetch for the current selection is still in flight.
    pub(crate) diff: Option<((String, bool), DiffData)>,

    /// Whether the raw git diff is shown instead of the default field list.
    /// Persists across selections: the panel keeps showing diffs until the
    /// box is unchecked, rather than snapping back on every file clicked.
    pub(crate) show_raw_diff: bool,

    /// The commit message box, bound to the commit row's text field. Cleared
    /// after a successful commit.
    pub(crate) commit_message: String,

    /// The repo-relative path of a discard awaiting confirmation, if any.
    pub(crate) pending_discard: Option<String>,

    /// The name typed into the create-branch dialog, if it is open.
    pub(crate) new_branch: Option<String>,

    /// The branch a delete would target, when the delete dialog is open.
    /// Doubles as the dialog's own picker: it starts on whichever branch the
    /// `x` button was clicked for, and changes as the user picks a different
    /// one from the dialog's dropdown before confirming. `None` when the
    /// dialog is closed.
    pub(crate) pending_branch_delete: Option<String>,

    /// The last git error, shown in the panel; empty when there is none.
    pub(crate) error: String,

    /// Whether the "outside" (out-of-scope) changes section is expanded.
    pub(crate) show_outside: bool,

    /// The receiving end of an in-flight background git job, if any. Only one
    /// git command runs at a time: concurrent commands contend for
    /// `index.lock`.
    pub(crate) pending: Option<std::sync::mpsc::Receiver<JobResult>>,

    /// The in-progress resolution of the selected conflicted file, if its
    /// text parsed. `None` both before a conflicted file is selected and
    /// when its text failed to parse, in which case the panel falls back to
    /// the read-only diff view instead.
    pub(crate) resolve: Option<ResolveState>,

    /// A status refresh is owed, a project just loaded or a contract was just
    /// saved, and starts once no other job is in flight. This is what makes
    /// the Git section's dirty indicator correct without ever opening the section.
    pub(crate) needs_refresh: bool,

    /// A branch listing is owed after a switch, create or delete, and starts
    /// once the mutation's own job has finished.
    pub(crate) needs_branches: bool,

    /// Whether the next commit amends the last one.
    pub(crate) amend: bool,

    /// What was typed before Amend was ticked, restored on untick.
    pub(crate) message_before_amend: Option<String>,

    /// The upstream already holding the commit Amend would rewrite.
    pub(crate) amend_pushed_to: Option<String>,

    /// An amend read is owed, started by `poll` once nothing is pending.
    pub(crate) needs_amend_info: bool,
}

/// Something a finished git job asks the app shell to do, because it touches
/// state the git feature does not own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GitEvent {
    /// Text for the shell status line.
    Status(String),

    /// A checkout succeeded, so the open contract must be reconciled with
    /// the new branch.
    BranchSwitched,
}

/// A repaint callback for a background job, so `data` never sees egui.
fn notifier(ctx: &egui::Context) -> impl Fn() + Send + 'static {
    let ctx = ctx.clone();
    move || ctx.request_repaint()
}

impl GitState {
    /// Queues a status refresh for [`GitState::poll`] to start once no other
    /// job is in flight. Retrying from `poll` rather than starting the job
    /// here means a refresh requested while another job runs is not dropped.
    pub(crate) fn request_refresh(&mut self) {
        self.needs_refresh = true;
    }

    /// Applies one action from the git panel. `paths` is `None` when the
    /// project is not inside a repository, in which case nothing is started.
    pub(crate) fn handle(
        &mut self,
        action: GitAction,
        paths: Option<&RepoPaths>,
        ctx: &egui::Context,
    ) {
        match action {
            GitAction::Refresh => self.start_status(paths, ctx),
            GitAction::Select { path, staged } => {
                self.selected = Some((path.clone(), staged));
                let cached = matches!(&self.diff, Some((key, _)) if *key == (path.clone(), staged));
                if !cached {
                    self.start_diff(paths, ctx, path, staged);
                }
            }
            GitAction::Stage { path } => self.start_mutation(paths, ctx, Mutation::Stage { path }),
            GitAction::Unstage { path } => {
                self.start_mutation(paths, ctx, Mutation::Unstage { path })
            }
            GitAction::RequestDiscard { path } => self.pending_discard = Some(path),
            GitAction::ConfirmDiscard => {
                if let Some(path) = self.pending_discard.take() {
                    self.start_mutation(paths, ctx, Mutation::Discard { path });
                }
            }
            GitAction::Commit => {
                let message = self.commit_message.clone();
                self.start_mutation(
                    paths,
                    ctx,
                    Mutation::Commit {
                        message,
                        amend: self.amend,
                    },
                );
            }
            GitAction::ToggleAmend => {
                if self.amend {
                    self.amend = false;
                    self.needs_amend_info = false;
                    self.amend_pushed_to = None;
                    if let Some(message) = self.message_before_amend.take() {
                        self.commit_message = message;
                    }
                } else {
                    self.amend = true;
                    self.needs_amend_info = true;
                }
            }
            GitAction::RefreshBranches => self.start_branches(paths, ctx),
            GitAction::SwitchBranch { name } => {
                self.start_mutation(paths, ctx, Mutation::SwitchBranch { name })
            }
            GitAction::CreateBranch { name } => {
                self.start_mutation(paths, ctx, Mutation::CreateBranch { name })
            }
            GitAction::RequestBranchDelete { name } => self.pending_branch_delete = Some(name),
            GitAction::ConfirmBranchDelete => {
                if let Some(name) = self.pending_branch_delete.take() {
                    self.start_mutation(paths, ctx, Mutation::DeleteBranch { name });
                }
            }
            GitAction::ResolveConflict { path, text } => {
                self.start_mutation(paths, ctx, Mutation::Resolve { path, text })
            }
        }
    }

    /// Collects the in-flight job's result if it has finished, then starts
    /// queued work when nothing is pending: a branch listing first, otherwise
    /// a status refresh. Called every frame. Returns what the app shell must
    /// apply, in order.
    pub(crate) fn poll(&mut self, paths: Option<&RepoPaths>, ctx: &egui::Context) -> Vec<GitEvent> {
        let mut events = Vec::new();
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    self.apply_result(result, &mut events);
                }
                Err(TryRecvError::Empty) => ctx.request_repaint(),
                // A panicking worker must not leave the panel disabled
                // forever: clear `pending` and surface an error instead.
                Err(TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.error = "git worker disconnected".into();
                }
            }
        }
        if self.pending.is_none() {
            if self.needs_branches {
                self.needs_branches = false;
                self.start_branches(paths, ctx);
            } else if self.needs_amend_info {
                self.needs_amend_info = false;
                self.start_amend_info(paths, ctx);
            } else if self.needs_refresh {
                self.needs_refresh = false;
                self.start_status(paths, ctx);
            }
        }
        events
    }

    /// Files one finished job's result into this state.
    fn apply_result(&mut self, result: JobResult, events: &mut Vec<GitEvent>) {
        match result {
            JobResult::Status(Ok(status)) => {
                self.status = status;
                self.error.clear();
            }
            JobResult::Diff(key, Ok(data)) => {
                self.diff = Some((key, data));
                self.error.clear();
            }
            JobResult::Branches(Ok(branches)) => {
                self.branches = branches;
                self.error.clear();
            }
            JobResult::Amend(Ok(info)) => {
                if self.amend {
                    self.message_before_amend =
                        Some(std::mem::replace(&mut self.commit_message, info.message));
                    self.amend_pushed_to = info.pushed_to;
                }
                self.error.clear();
            }
            JobResult::Amend(Err(e)) => {
                self.amend = false;
                self.error = e;
            }
            JobResult::Status(Err(e))
            | JobResult::Diff(_, Err(e))
            | JobResult::Branches(Err(e)) => {
                self.error = e;
            }
            JobResult::Mutate(kind, message, Ok(status)) => {
                self.status = status;
                self.error.clear();
                events.push(GitEvent::Status(message));
                if let Some((path, _)) = &self.selected
                    && self.status.find(path).is_none()
                {
                    self.selected = None;
                    self.diff = None;
                }
                match kind {
                    MutateKind::Commit => {
                        self.commit_message.clear();
                        self.amend = false;
                        self.message_before_amend = None;
                        self.amend_pushed_to = None;
                    }
                    MutateKind::SwitchBranch => {
                        self.needs_branches = true;
                        events.push(GitEvent::BranchSwitched);
                    }
                    MutateKind::CreateBranch | MutateKind::DeleteBranch => {
                        self.needs_branches = true;
                    }
                    MutateKind::Stage
                    | MutateKind::Unstage
                    | MutateKind::Discard
                    | MutateKind::Resolve => {}
                }
            }
            JobResult::Mutate(_, _, Err(e)) => {
                self.error = e.clone();
                events.push(GitEvent::Status(e));
            }
        }
    }

    /// Starts `git status`. Refuses while a job is pending: concurrent git
    /// commands contend for `index.lock`.
    fn start_status(&mut self, paths: Option<&RepoPaths>, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let Some(paths) = paths else {
            return;
        };
        self.pending = Some(jobs::status(paths.clone(), notifier(ctx)));
    }

    /// Starts the diff fetch for `(path, staged)`, under the same one-job rule.
    fn start_diff(
        &mut self,
        paths: Option<&RepoPaths>,
        ctx: &egui::Context,
        path: String,
        staged: bool,
    ) {
        if self.pending.is_some() {
            return;
        }
        let Some(paths) = paths else {
            return;
        };
        let conflicted = self.status.find(&path).is_some_and(|f| f.conflicted);
        let request = DiffRequest {
            repo_root: paths.repo_root.clone(),
            path,
            staged,
            conflicted,
        };
        self.pending = Some(jobs::diff(request, notifier(ctx)));
    }

    /// Starts `mutation`, under the same one-job rule.
    fn start_mutation(
        &mut self,
        paths: Option<&RepoPaths>,
        ctx: &egui::Context,
        mutation: Mutation,
    ) {
        if self.pending.is_some() {
            return;
        }
        let Some(paths) = paths else {
            return;
        };
        self.pending = Some(jobs::mutate(paths.clone(), mutation, notifier(ctx)));
    }

    /// Starts reading what amending would rewrite, under the same one-job rule.
    fn start_amend_info(&mut self, paths: Option<&RepoPaths>, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let Some(paths) = paths else {
            return;
        };
        self.pending = Some(jobs::amend_info(paths.repo_root.clone(), notifier(ctx)));
    }

    /// Starts `git branch`, under the same one-job rule.
    fn start_branches(&mut self, paths: Option<&RepoPaths>, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let Some(paths) = paths else {
            return;
        };
        self.pending = Some(jobs::branches(paths.repo_root.clone(), notifier(ctx)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::app::test_support::{
        conflict_fixture, current_branch, dirty, git_at, git_available, git_output,
        project_fixture, settle_git, tempdir,
    };

    /// The text of the last `GitEvent::Status`, the value the app shell would
    /// leave in its status line.
    fn last_status(events: &[GitEvent]) -> String {
        events
            .iter()
            .rev()
            .find_map(|e| match e {
                GitEvent::Status(s) => Some(s.clone()),
                GitEvent::BranchSwitched => None,
            })
            .unwrap_or_default()
    }

    /// Pins the second shipped defect: routing `GitAction::Stage` through
    /// `apply` must actually stage the file in the repository. Read the
    /// result from `git` directly rather than from `git.status`, so this
    /// cannot pass on the app merely agreeing with itself.
    #[test]
    fn stage_action_stages_in_the_repository() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join("contracts").join("sample.json"));

        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.handle(GitAction::Refresh, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);

        git.handle(
            GitAction::Stage {
                path: "contracts/sample.json".to_string(),
            },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);

        let porcelain = git_output(&root, &["status", "--porcelain=v2"]);
        let line = porcelain
            .lines()
            .find(|l| l.ends_with("contracts/sample.json"))
            .unwrap_or_else(|| panic!("git must report the file: {porcelain:?}"));
        let xy = line.split(' ').nth(1).expect("an XY field");
        let index_char = xy.as_bytes()[0];
        assert_ne!(
            index_char, b'.',
            "the index column must show a staged change, got {line:?}"
        );
    }

    #[test]
    fn selecting_another_file_keeps_the_raw_diff_toggle() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();

        git.show_raw_diff = true;
        git.handle(
            GitAction::Select {
                path: "contracts/sample.json".to_string(),
                staged: false,
            },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);

        // The toggle is a panel mode now, not a per-file one. This is the
        // deleted reset in `apply` pinned, and it lives here rather than in
        // the view because the flag's lifetime is a view-model concern.
        assert!(git.show_raw_diff);
    }

    /// Drives a commit through the dispatch and confirms it against the
    /// repository's own log, not the app's cache.
    #[test]
    fn commit_action_commits_the_staged_change() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join("contracts").join("sample.json"));

        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.handle(GitAction::Refresh, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);

        git.handle(
            GitAction::Stage {
                path: "contracts/sample.json".to_string(),
            },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);

        git.commit_message = "pin the stage wiring".to_string();
        git.handle(GitAction::Commit, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);

        let subject = git_output(&root, &["log", "-1", "--format=%s"]);
        assert_eq!(subject.trim(), "pin the stage wiring");
        assert!(
            git.commit_message.is_empty(),
            "a successful commit must clear the message box"
        );
    }

    /// A create must add the branch without switching the repository to it.
    #[test]
    fn create_branch_adds_without_switching() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();

        git.handle(
            GitAction::CreateBranch {
                name: "apic-third".to_string(),
            },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);

        assert_eq!(
            current_branch(&root),
            "apic-test",
            "a create must not change the checked-out branch"
        );
        let listed = git_output(&root, &["branch", "--list", "apic-third"]);
        assert!(
            listed.contains("apic-third"),
            "the new branch must be listed: {listed:?}"
        );
    }

    /// A delete git refuses (unmerged commits) must leave the branch in
    /// place and surface git's own message in `git.error`.
    #[test]
    fn refused_delete_leaves_the_branch_and_reports_git_error() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();

        git.handle(
            GitAction::RequestBranchDelete {
                name: "apic-second".to_string(),
            },
            Some(&paths),
            &ctx,
        );
        git.handle(GitAction::ConfirmBranchDelete, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);

        let listed = git_output(&root, &["branch", "--list", "apic-second"]);
        assert!(
            listed.contains("apic-second"),
            "a refused delete must leave the branch present: {listed:?}"
        );
        assert!(
            !git.error.is_empty(),
            "git's refusal must land in git.error"
        );
    }

    /// Resolving a real conflict through `GitState::handle` must both write the
    /// resolved text and stage it, since a written file that is not staged
    /// still reads as unmerged. Read the outcome from `git status
    /// --porcelain=v2` against the temp repo, not from `app.git`, so this
    /// cannot pass on the app agreeing with itself. `u`-prefixed lines are
    /// git's unmerged entries, so their absence for this path is what proves
    /// the conflict actually cleared.
    #[test]
    fn resolve_conflict_stages_the_file_and_clears_the_unmerged_entry() {
        if !git_available() {
            return;
        }
        let root = conflict_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();

        git.handle(
            GitAction::ResolveConflict {
                path: "contracts/sample.json".to_string(),
                text: apic_core::template::DEFAULT.to_string(),
            },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);

        assert!(
            git.error.is_empty(),
            "resolve must not report an error: {}",
            git.error
        );

        let porcelain = git_output(&root, &["status", "--porcelain=v2"]);
        assert!(
            !porcelain
                .lines()
                .any(|l| l.starts_with("u ") && l.ends_with("contracts/sample.json")),
            "the path must no longer show as unmerged: {porcelain:?}"
        );
        let line = porcelain
            .lines()
            .find(|l| l.ends_with("contracts/sample.json"))
            .unwrap_or_else(|| panic!("git must still report the file: {porcelain:?}"));
        let xy = line.split(' ').nth(1).expect("an XY field");
        assert_ne!(
            xy.as_bytes()[0],
            b'.',
            "the index column must show a staged change, got {line:?}"
        );
    }

    /// After a resolve, the file on disk must carry none of the conflict
    /// markers: a leftover marker means the write half of the resolve did
    /// not actually apply the rendered text.
    #[test]
    fn resolve_conflict_removes_the_markers_from_disk() {
        if !git_available() {
            return;
        }
        let root = conflict_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();

        git.handle(
            GitAction::ResolveConflict {
                path: "contracts/sample.json".to_string(),
                text: apic_core::template::DEFAULT.to_string(),
            },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);

        let content = std::fs::read_to_string(root.join("contracts").join("sample.json")).unwrap();
        assert!(!content.contains("<<<<<<<"), "got: {content:?}");
        assert!(!content.contains("======="), "got: {content:?}");
        assert!(!content.contains(">>>>>>>"), "got: {content:?}");
    }

    /// A successful resolve must leave `shell.status` non-empty and naming
    /// the resolved path, so the user learns the resolve happened instead of
    /// noticing it only by a row moving between sections. This is the
    /// regression the whole task exists to fix.
    #[test]
    fn resolve_conflict_reports_success_as_a_status_event() {
        if !git_available() {
            return;
        }
        let root = conflict_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();

        git.handle(
            GitAction::ResolveConflict {
                path: "contracts/sample.json".to_string(),
                text: apic_core::template::DEFAULT.to_string(),
            },
            Some(&paths),
            &ctx,
        );
        let events = settle_git(&mut git, &paths, &ctx);

        assert!(
            !last_status(&events).is_empty(),
            "a successful resolve must report itself in the status event"
        );
        assert!(
            last_status(&events).contains("contracts/sample.json"),
            "the status must name the resolved path, got: {:?}",
            last_status(&events)
        );
    }

    /// A failing mutation must still put git's own error text in
    /// the status event, so a success message can never quietly replace a real
    /// error. Deleting a branch with unmerged commits is refused by git with
    /// a message on stderr, which is what `Mutation::DeleteBranch` surfaces.
    #[test]
    fn failing_mutation_reports_git_error_as_a_status_event() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();

        git.handle(
            GitAction::RequestBranchDelete {
                name: "apic-second".to_string(),
            },
            Some(&paths),
            &ctx,
        );
        git.handle(GitAction::ConfirmBranchDelete, Some(&paths), &ctx);
        let events = settle_git(&mut git, &paths, &ctx);

        let listed = git_output(&root, &["branch", "--list", "apic-second"]);
        assert!(
            listed.contains("apic-second"),
            "the delete must actually have been refused: {listed:?}"
        );
        assert!(
            !last_status(&events).is_empty(),
            "a failing mutation must still report something in the status event"
        );
        assert!(
            !last_status(&events).contains("deleted branch"),
            "a success message must never replace git's own error, got: {:?}",
            last_status(&events)
        );
        assert_eq!(
            last_status(&events),
            git.error,
            "the status event must carry git's own stderr, the same text as git.error"
        );
    }

    #[test]
    fn switch_branch_mutation_emits_status_then_branch_switched() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();

        git.handle(
            GitAction::SwitchBranch {
                name: "apic-second".to_string(),
            },
            Some(&paths),
            &ctx,
        );
        let events = settle_git(&mut git, &paths, &ctx);

        let status_at = events
            .iter()
            .position(|e| matches!(e, GitEvent::Status(s) if s.contains("apic-second")))
            .expect("a successful switch reports itself");
        let switched_at = events
            .iter()
            .position(|e| *e == GitEvent::BranchSwitched)
            .expect("a successful switch asks for a reconcile");
        assert!(
            status_at < switched_at,
            "the reconcile must run after the message: {events:?}"
        );
        assert_eq!(current_branch(&root), "apic-second");
    }

    #[test]
    fn poll_reports_a_disconnected_worker() {
        let mut git = GitState::default();
        let ctx = egui::Context::default();
        let (tx, rx) = std::sync::mpsc::channel();
        drop(tx);
        git.pending = Some(rx);

        let events = git.poll(None, &ctx);

        assert!(git.pending.is_none());
        assert_eq!(git.error, "git worker disconnected");
        assert!(events.is_empty());
    }

    #[test]
    #[should_panic(expected = "did not complete")]
    fn settle_git_panics_rather_than_hangs_on_a_job_that_never_resolves() {
        let mut git = GitState::default();
        let ctx = egui::Context::default();
        let paths = RepoPaths {
            repo_root: tempdir(),
            root: tempdir(),
            project_root: None,
        };
        let (_tx, rx) = std::sync::mpsc::channel();
        git.pending = Some(rx);

        settle_git(&mut git, &paths, &ctx);
    }

    #[test]
    fn ticking_amend_prefills_and_unticking_restores() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.commit_message = "draft".to_string();
        git.handle(GitAction::ToggleAmend, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        let last = git_output(&root, &["log", "-1", "--format=%B"]);
        assert_eq!(git.commit_message, last.trim_end());
        assert!(git.amend);
        git.handle(GitAction::ToggleAmend, Some(&paths), &ctx);
        assert_eq!(git.commit_message, "draft");
        assert!(!git.amend);
    }

    #[test]
    fn amending_a_pushed_commit_warns_with_the_upstream() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let remote = tempdir();
        git_output(&remote, &["init", "--bare"]);
        git_output(
            &root,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        );
        git_output(&root, &["push", "-u", "origin", "HEAD"]);
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.handle(GitAction::ToggleAmend, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert!(
            git.amend_pushed_to
                .as_deref()
                .is_some_and(|u| u.starts_with("origin/"))
        );
    }

    #[test]
    fn a_successful_amend_clears_amend_and_the_message() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.handle(GitAction::ToggleAmend, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        git.commit_message = "reworded initial".to_string();
        git.handle(GitAction::Commit, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(
            git_output(&root, &["log", "-1", "--format=%s"]).trim(),
            "reworded initial"
        );
        assert!(!git.amend);
        assert!(git.commit_message.is_empty());
    }
}
