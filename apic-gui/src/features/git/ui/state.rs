//! The git feature's view model: what the panel remembers, and the handling of every git action and finished background job.

use std::sync::mpsc::TryRecvError;

use eframe::egui;

use crate::features::git::data::jobs::{
    self, DiffData, DiffRequest, JobResult, MutateKind, Mutation, RepoPaths,
};
use crate::features::git::domain::conflict::{self, Choice, ConflictFile, Segment};
use crate::features::git::domain::log::{Commit, LogFilter, Row, lanes};
use crate::features::git::domain::status::{Branches, FileStatus, Status};

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
    ToggleAmend,

    /// Show the diff for this repo-relative path, staged or unstaged.
    Select { path: String, staged: bool },

    /// Tick or untick these repo-relative file paths for the next commit.
    SetTicked { paths: Vec<String>, ticked: bool },

    /// Unstage this repo-relative path, leaving the working tree untouched.
    Unstage { path: String },

    /// Ask to discard this tracked path, shows a confirmation before
    /// anything is reverted.
    RequestDiscard { path: String },

    /// The pending discard was confirmed, revert it.
    ConfirmDiscard,

    /// Stage the ticked files, unstage the unticked ones, and commit with
    /// `GitState::commit_message`.
    Commit,

    /// Amend after the user confirmed rewriting a pushed commit.
    ConfirmAmend,

    /// Close the amend-pushed popup without committing.
    CancelAmend,

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

    /// Open the confirmation for pulling or merging `source` into `target`,
    /// both picked in the branch menu.
    RequestBranchOp {
        kind: BranchOpKind,
        target: String,
        source: String,
    },

    /// The pending pull or merge was confirmed, run it.
    ConfirmBranchOp,

    /// Write the rendered resolution `text` for this conflicted path and
    /// stage it. The text is already rendered, this action carries no
    /// markers for the job to parse.
    ResolveConflict { path: String, text: String },

    /// Open or close the log panel. Opening reads the log.
    ToggleLog,

    /// Reread the log from the start with this filter.
    SetLogFilter(LogFilter),

    /// Reread the log from the start with the same filter.
    RefreshLog,

    /// Append the next page of commits.
    LoadMoreLog,

    /// List the changed files of this commit.
    SelectCommit { hash: String },

    /// Show the selected commit's diff of this repo-relative path.
    SelectCommitFile { path: String },

    /// Open the revert confirmation for this commit, unless `revert_block`
    /// refuses it.
    RequestRevert { hash: String },

    /// The pending revert was confirmed, run it.
    ConfirmRevert,
}

/// Which branch operation the pull/merge popup runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchOpKind {
    /// Fetch every remote, then merge a remote branch.
    Pull,

    /// Merge a local or remote branch, no fetch.
    Merge,
}

/// A pull or merge awaiting confirmation: bring `source` into `target`, both picked in the branch menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BranchOp {
    pub(crate) kind: BranchOpKind,
    pub(crate) target: String,
    pub(crate) source: String,
}

/// Everything the log panel remembers.
#[derive(Default)]
pub(crate) struct LogState {
    /// Whether the panel is shown. Persisted by the app shell.
    pub(crate) open: bool,

    /// The filter the loaded commits were read with, or are being read with.
    pub(crate) filter: LogFilter,

    /// The search box, applied to `filter.text` once typing pauses.
    pub(crate) draft: String,

    /// The `ui.input` time of the last keystroke in the search box.
    pub(crate) edited_at: f64,

    /// The Paths menu's text field.
    pub(crate) typed_path: String,

    /// The loaded commits, newest first, hash prefix applied.
    pub(crate) commits: Vec<Commit>,

    /// `lanes(&commits)`, rebuilt whenever `commits` changes.
    pub(crate) rows: Vec<Row>,

    /// How many commits git returned so far, before the hash prefix, which
    /// is where Load more skips to.
    pub(crate) fetched: usize,

    /// The last page was full, so Load more is offered.
    pub(crate) more: bool,

    /// A read has finished, so an empty list means nothing matched.
    pub(crate) loaded: bool,

    /// The last log error, shown above the rows it leaves in place.
    pub(crate) error: String,

    /// The hash of the commit whose files are listed.
    pub(crate) selected: Option<String>,

    /// The selected commit's changed files, `None` while they load.
    pub(crate) files: Option<Vec<FileStatus>>,

    /// The commit file whose diff the central panel shows.
    pub(crate) selected_file: Option<String>,

    /// The diff for `(selected, selected_file)`, keyed the same way.
    pub(crate) diff: Option<((String, String), DiffData)>,

    /// The commit a revert would undo, while its confirmation is open.
    pub(crate) pending_revert: Option<Commit>,

    /// A read from the start is owed.
    pub(crate) needs_load: bool,

    /// A Load more is owed.
    pub(crate) needs_more: bool,

    /// The selected commit's file list is owed.
    pub(crate) needs_files: bool,

    /// The selected commit file's diff is owed.
    pub(crate) needs_diff: bool,
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

    /// Whether a changed contract shows the field list instead of the
    /// default inline diff. Persists across selections: the panel keeps
    /// showing field lists until the box is unchecked, rather than snapping
    /// back on every file clicked.
    pub(crate) show_fields: bool,

    /// The commit message box, bound to the commit row's text field. Cleared
    /// after a successful commit.
    pub(crate) commit_message: String,

    /// The repo-relative path of a discard awaiting confirmation, if any.
    pub(crate) pending_discard: Option<String>,

    /// The name typed into the create-branch dialog, if it is open.
    pub(crate) new_branch: Option<String>,

    /// The right-clicked branch a delete would remove, while its confirmation is open.
    pub(crate) pending_branch_delete: Option<String>,

    /// The pull or merge awaiting confirmation, when its popup is open.
    pub(crate) pending_branch_op: Option<BranchOp>,

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

    /// Mutations asked for while another job held the slot, started in
    /// order by `poll` once nothing is pending.
    pub(crate) queued_mutations: std::collections::VecDeque<Mutation>,

    /// A diff of the selected Changes file is owed, asked for while another
    /// job held the slot.
    pub(crate) needs_diff: bool,

    /// Whether the next commit amends the last one.
    pub(crate) amend: bool,

    /// What was typed before Amend was ticked, restored on untick.
    pub(crate) message_before_amend: Option<String>,

    /// The upstream already holding the commit Amend would rewrite.
    pub(crate) amend_pushed_to: Option<String>,

    /// The amend-pushed popup is open.
    pub(crate) confirm_amend: bool,

    /// An amend read is owed, started by `poll` once nothing is pending.
    pub(crate) needs_amend_info: bool,

    /// The user's tick or untick per path, overriding the index-derived
    /// default until the path leaves the status.
    pub(crate) ticks: std::collections::HashMap<String, bool>,

    /// Every conflicted path, set when a status brings a conflicted path the
    /// previous status did not have. Drives the "Merge conflict" popup, which
    /// clears it on OK. Cleared too by a status with no conflicts.
    pub(crate) conflict_notice: Option<Vec<String>>,

    /// Repository the in-flight status or mutation job runs against.
    pub(crate) job_repo: Option<std::path::PathBuf>,
    /// Repository `status` was read from.
    pub(crate) status_repo: Option<std::path::PathBuf>,

    /// The log panel.
    pub(crate) log: LogState,
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

    /// A resolve or discard rewrote files in the working tree, so anything
    /// read from those files must be read again.
    WorkingTreeChanged,
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

    /// Queues a log read from the start, when the panel is open.
    pub(crate) fn request_log(&mut self) {
        if self.log.open {
            self.log.needs_load = true;
        }
    }

    /// Why `commit` cannot be reverted from the log, or `None` when it can.
    pub(crate) fn revert_block(&self, commit: &Commit) -> Option<&'static str> {
        if commit.is_merge() {
            Some("Reverting a merge commit is not supported")
        } else if self
            .log
            .filter
            .branch
            .as_ref()
            .is_some_and(|b| Some(b) != self.branches.current.as_ref())
        {
            Some("Revert works on the current branch only")
        } else {
            None
        }
    }

    /// Whether `file` is ticked for the next commit: the user's choice when
    /// one was made, otherwise whether it has a staged change. Never true
    /// for a conflicted file.
    pub(crate) fn is_ticked(&self, file: &FileStatus) -> bool {
        !file.conflicted
            && self
                .ticks
                .get(&file.path)
                .copied()
                .unwrap_or_else(|| file.has_staged_change())
    }

    /// The commit's two path lists: ticked files to stage, and staged files
    /// the user unticked to unstage. Conflicted files are in neither.
    fn commit_paths(&self) -> (Vec<String>, Vec<String>) {
        let mut stage = Vec::new();
        let mut unstage = Vec::new();
        for f in self.status.inside.iter().chain(self.status.outside.iter()) {
            if f.conflicted {
                continue;
            }
            if self.is_ticked(f) {
                stage.push(f.path.clone());
            } else if f.has_staged_change() {
                unstage.push(f.path.clone());
            }
        }
        (stage, unstage)
    }

    /// Whether any file is ticked, which with a message enables Commit.
    pub(crate) fn any_ticked(&self) -> bool {
        self.status
            .inside
            .iter()
            .chain(self.status.outside.iter())
            .any(|f| self.is_ticked(f))
    }

    /// Stores a fresh status and forgets ticks for paths no longer in it.
    /// Opens the conflict notice when a path is newly conflicted, and clears
    /// it when nothing is conflicted any more.
    fn set_status(&mut self, status: Status) {
        let before = conflicted_paths(&self.status);
        let now = conflicted_paths(&status);
        if now.is_empty() {
            self.conflict_notice = None;
        } else if now.iter().any(|p| !before.contains(p)) {
            self.conflict_notice = Some(now);
        }
        self.ticks.retain(|path, _| status.find(path).is_some());
        self.status = status;
    }

    /// Files a status from the job's repository. A status from another repo
    /// than the held one is compared against no previous conflicts.
    fn set_job_status(&mut self, status: Status) {
        if self.job_repo != self.status_repo {
            self.status = Status::default();
            self.status_repo = self.job_repo.clone();
        }
        self.set_status(status);
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
                self.log.selected_file = None;
                self.selected = Some((path.clone(), staged));
                let cached = matches!(&self.diff, Some((key, _)) if *key == (path.clone(), staged));
                if !cached {
                    self.start_diff(paths, ctx, path, staged);
                }
            }
            GitAction::SetTicked {
                paths: ticked_paths,
                ticked,
            } => {
                for path in ticked_paths {
                    self.ticks.insert(path, ticked);
                }
            }
            GitAction::Unstage { path } => {
                let prefix = format!("{path}/");
                let unticked: Vec<String> = self
                    .status
                    .inside
                    .iter()
                    .chain(self.status.outside.iter())
                    .filter(|f| f.path == path || f.path.starts_with(&prefix))
                    .map(|f| f.path.clone())
                    .collect();
                for p in unticked {
                    self.ticks.insert(p, false);
                }
                self.start_mutation(paths, ctx, Mutation::Unstage { path })
            }
            GitAction::RequestDiscard { path } => self.pending_discard = Some(path),
            GitAction::ConfirmDiscard => {
                if let Some(path) = self.pending_discard.take() {
                    self.start_mutation(paths, ctx, Mutation::Discard { path });
                }
            }
            GitAction::Commit => {
                if self.amend && self.amend_pushed_to.is_some() {
                    self.confirm_amend = true;
                } else {
                    self.start_commit(paths, ctx);
                }
            }
            GitAction::ConfirmAmend => {
                self.confirm_amend = false;
                self.start_commit(paths, ctx);
            }
            GitAction::CancelAmend => self.confirm_amend = false,
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
            GitAction::RequestBranchOp {
                kind,
                target,
                source,
            } => {
                self.pending_branch_op = Some(BranchOp {
                    kind,
                    target,
                    source,
                })
            }
            GitAction::ConfirmBranchOp => {
                if let Some(op) = self.pending_branch_op.take()
                    && !op.source.is_empty()
                {
                    let switch = self.branches.current.as_deref() != Some(op.target.as_str());
                    self.start_mutation(
                        paths,
                        ctx,
                        Mutation::Merge {
                            target: op.target,
                            source: op.source,
                            fetch: op.kind == BranchOpKind::Pull,
                            switch,
                        },
                    );
                }
            }
            GitAction::ResolveConflict { path, text } => {
                self.start_mutation(paths, ctx, Mutation::Resolve { path, text })
            }
            GitAction::ToggleLog => {
                self.log.open = !self.log.open;
                self.log.needs_load = self.log.open;
            }
            GitAction::SetLogFilter(filter) => {
                self.log.filter = filter;
                self.log.needs_load = true;
            }
            GitAction::RefreshLog => self.log.needs_load = true,
            GitAction::LoadMoreLog => self.log.needs_more = true,
            GitAction::SelectCommit { hash } => {
                self.log.selected = Some(hash);
                self.log.files = None;
                self.log.selected_file = None;
                self.log.needs_files = true;
            }
            GitAction::SelectCommitFile { path } => {
                self.selected = None;
                self.log.selected_file = Some(path);
                self.log.needs_diff = true;
            }
            GitAction::RequestRevert { hash } => {
                if let Some(commit) = self.log.commits.iter().find(|c| c.hash == hash)
                    && self.revert_block(commit).is_none()
                {
                    self.log.pending_revert = Some(commit.clone());
                }
            }
            GitAction::ConfirmRevert => {
                if let Some(commit) = self.log.pending_revert.take() {
                    self.start_mutation(paths, ctx, Mutation::Revert { hash: commit.hash });
                }
            }
        }
    }

    /// Collects the in-flight job's result if it has finished, then starts
    /// queued work when nothing is pending: a queued mutation first, then a
    /// branch listing, an amend read, a status refresh, a Changes diff, and
    /// last the log's work. Called every frame. Returns what the app shell must
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
            if let Some(mutation) = self.queued_mutations.pop_front() {
                self.start_mutation(paths, ctx, mutation);
            } else if self.needs_branches {
                self.needs_branches = false;
                self.start_branches(paths, ctx);
            } else if self.needs_amend_info {
                self.needs_amend_info = false;
                self.start_amend_info(paths, ctx);
            } else if self.needs_refresh {
                self.needs_refresh = false;
                self.start_status(paths, ctx);
            } else if self.needs_diff {
                self.needs_diff = false;
                if let Some((path, staged)) = self.selected.clone() {
                    self.start_diff(paths, ctx, path, staged);
                }
            } else {
                self.start_log_work(paths, ctx);
            }
        }
        events
    }

    /// Files one finished job's result into this state.
    fn apply_result(&mut self, result: JobResult, events: &mut Vec<GitEvent>) {
        match result {
            JobResult::Status(Ok(status)) => {
                self.set_job_status(status);
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
                    let typed = std::mem::replace(&mut self.commit_message, info.message);
                    self.message_before_amend.get_or_insert(typed);
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
            JobResult::Log(append, Ok(page)) => {
                if !append {
                    self.log.commits.clear();
                    self.log.fetched = 0;
                }
                self.log.fetched += page.len();
                self.log.more = page.len() == jobs::LOG_PAGE;
                let filter = &self.log.filter;
                self.log
                    .commits
                    .extend(page.into_iter().filter(|c| filter.keeps(c)));
                self.log.rows = lanes(&self.log.commits);
                self.log.loaded = true;
                self.log.error.clear();
            }
            JobResult::Log(_, Err(e)) => self.log.error = e,
            JobResult::CommitFiles(hash, Ok(files)) => {
                if self.log.selected.as_ref() == Some(&hash) {
                    self.log.files = Some(files);
                }
            }
            JobResult::CommitFiles(_, Err(e)) => {
                self.log.files = Some(Vec::new());
                self.log.error = e;
            }
            JobResult::CommitDiff(key, Ok(data)) => self.log.diff = Some((key, data)),
            JobResult::CommitDiff(_, Err(e)) => self.log.error = e,
            JobResult::Mutate(kind, message, Ok(status)) => {
                self.set_job_status(status);
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
                        self.request_log();
                    }
                    MutateKind::SwitchBranch | MutateKind::Merge => {
                        self.needs_branches = true;
                        self.log.filter.branch = None;
                        self.request_log();
                        events.push(GitEvent::BranchSwitched);
                    }
                    MutateKind::Revert => {
                        self.request_log();
                        events.push(GitEvent::WorkingTreeChanged);
                    }
                    MutateKind::CreateBranch | MutateKind::DeleteBranch => {
                        self.needs_branches = true;
                    }
                    MutateKind::Discard | MutateKind::Resolve => {
                        events.push(GitEvent::WorkingTreeChanged);
                    }
                    MutateKind::Unstage => {}
                }
            }
            JobResult::Mutate(kind, _, Err(e)) => {
                self.error = e.clone();
                events.push(GitEvent::Status(e));
                match kind {
                    // A failed merge may already have switched branch or
                    // left conflicts behind: reread both so the resolver
                    // shows them, and reconcile the open contract.
                    MutateKind::Merge => {
                        self.needs_branches = true;
                        self.needs_refresh = true;
                        self.log.filter.branch = None;
                        self.request_log();
                        events.push(GitEvent::BranchSwitched);
                    }
                    // A conflicting revert writes markers into the working
                    // tree: the status reread raises the conflict notice.
                    MutateKind::Revert => {
                        self.needs_refresh = true;
                        events.push(GitEvent::WorkingTreeChanged);
                    }
                    _ => {}
                }
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
        self.job_repo = Some(paths.repo_root.clone());
        self.pending = Some(jobs::status(paths.clone(), notifier(ctx)));
    }

    /// Starts the diff fetch for `(path, staged)`. While a job is pending it
    /// marks the diff owed, and `poll` fetches the selected file's later.
    fn start_diff(
        &mut self,
        paths: Option<&RepoPaths>,
        ctx: &egui::Context,
        path: String,
        staged: bool,
    ) {
        if self.pending.is_some() {
            self.needs_diff = true;
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

    /// Starts `mutation`, or queues it for `poll` while a job is pending.
    fn start_mutation(
        &mut self,
        paths: Option<&RepoPaths>,
        ctx: &egui::Context,
        mutation: Mutation,
    ) {
        if self.pending.is_some() {
            self.queued_mutations.push_back(mutation);
            return;
        }
        let Some(paths) = paths else {
            return;
        };
        self.job_repo = Some(paths.repo_root.clone());
        self.pending = Some(jobs::mutate(paths.clone(), mutation, notifier(ctx)));
    }

    /// Commits the ticked files with `commit_message`, amending when Amend is
    /// ticked.
    fn start_commit(&mut self, paths: Option<&RepoPaths>, ctx: &egui::Context) {
        let message = self.commit_message.clone();
        let (stage, unstage) = self.commit_paths();
        self.start_mutation(
            paths,
            ctx,
            Mutation::Commit {
                message,
                amend: self.amend,
                stage,
                unstage,
            },
        );
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

    /// Starts the first owed log job, under the one-job rule: a read from
    /// the start, then Load more, then the selected commit's files, then
    /// its file diff.
    fn start_log_work(&mut self, paths: Option<&RepoPaths>, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let Some(paths) = paths else {
            return;
        };
        let log = &mut self.log;
        let job = if log.needs_load || log.needs_more {
            let append = !log.needs_load;
            log.needs_load = false;
            log.needs_more = false;
            let skip = if append { log.fetched } else { 0 };
            jobs::log(
                paths.clone(),
                log.filter.clone(),
                skip,
                append,
                notifier(ctx),
            )
        } else if log.needs_files {
            log.needs_files = false;
            let Some(hash) = log.selected.clone() else {
                return;
            };
            jobs::commit_files(paths.repo_root.clone(), hash, notifier(ctx))
        } else if log.needs_diff {
            log.needs_diff = false;
            let (Some(hash), Some(path)) = (log.selected.clone(), log.selected_file.clone()) else {
                return;
            };
            jobs::commit_diff(paths.repo_root.clone(), hash, path, notifier(ctx))
        } else {
            return;
        };
        self.pending = Some(job);
    }
}

/// Every conflicted path in `status`, inside the working dir first.
fn conflicted_paths(status: &Status) -> Vec<String> {
    status
        .inside
        .iter()
        .chain(status.outside.iter())
        .filter(|f| f.conflicted)
        .map(|f| f.path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::features::git::domain::status::Change;
    use crate::features::git::domain::status::fixtures::file;

    use crate::app::test_support::{
        conflict_fixture, current_branch, dirty, git_at, git_available, git_output,
        project_fixture, settle_git, tempdir,
    };

    fn status_of(files: &[(&str, bool)]) -> Status {
        Status {
            inside: files.iter().map(|(p, c)| file(p, *c)).collect(),
            outside: vec![],
        }
    }

    #[test]
    fn conflict_notice_opens_when_a_conflict_appears() {
        let mut git = GitState::default();
        git.set_status(status_of(&[("a.json", true), ("b.json", false)]));
        assert_eq!(git.conflict_notice, Some(vec!["a.json".to_string()]));
    }

    #[test]
    fn conflict_notice_stays_dismissed_while_no_new_path_conflicts() {
        let mut git = GitState::default();
        git.set_status(status_of(&[("a.json", true)]));
        git.conflict_notice = None;
        git.set_status(status_of(&[("a.json", true)]));
        assert_eq!(git.conflict_notice, None);
    }

    #[test]
    fn conflict_notice_reopens_with_every_path_on_a_new_conflict() {
        let mut git = GitState::default();
        git.set_status(status_of(&[("a.json", true)]));
        git.conflict_notice = None;
        git.set_status(status_of(&[("a.json", true), ("b.json", true)]));
        assert_eq!(
            git.conflict_notice,
            Some(vec!["a.json".to_string(), "b.json".to_string()])
        );
    }

    #[test]
    fn conflict_notice_clears_when_no_conflict_is_left() {
        let mut git = GitState::default();
        git.set_status(status_of(&[("a.json", true)]));
        git.set_status(status_of(&[("a.json", false)]));
        assert_eq!(git.conflict_notice, None);
    }

    /// Spec goal 4: a project already mid-merge opens the notice on its
    /// first status.
    #[test]
    fn conflict_notice_opens_on_the_first_status_of_a_mid_merge_repo() {
        if !git_available() {
            return;
        }
        let root = conflict_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.handle(GitAction::Refresh, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(
            git.conflict_notice,
            Some(vec!["contracts/sample.json".to_string()])
        );
    }

    /// Spec goal 3: a merge from the branch menu that conflicts opens the
    /// notice through the status reread after the failed merge.
    #[test]
    fn conflict_notice_opens_after_a_conflicting_merge_from_the_menu() {
        if !git_available() {
            return;
        }
        let root = conflict_fixture();
        git_output(&root, &["merge", "--abort"]);
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.branches.current = Some(current_branch(&root));
        git.handle(
            GitAction::RequestBranchOp {
                kind: BranchOpKind::Merge,
                target: current_branch(&root),
                source: "conflict-branch".into(),
            },
            Some(&paths),
            &ctx,
        );
        git.handle(GitAction::ConfirmBranchOp, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(
            git.conflict_notice,
            Some(vec!["contracts/sample.json".to_string()])
        );
    }

    #[test]
    fn conflict_notice_opens_for_a_second_mid_merge_repo() {
        if !git_available() {
            return;
        }
        let a = conflict_fixture();
        let b = conflict_fixture();
        let (mut git, pa) = git_at(&a);
        let (_, pb) = git_at(&b);
        let ctx = egui::Context::default();
        git.handle(GitAction::Refresh, Some(&pa), &ctx);
        settle_git(&mut git, &pa, &ctx);
        assert!(git.conflict_notice.is_some());
        git.conflict_notice = None;
        git.handle(GitAction::Refresh, Some(&pb), &ctx);
        settle_git(&mut git, &pb, &ctx);
        assert_eq!(
            git.conflict_notice,
            Some(vec!["contracts/sample.json".to_string()])
        );
    }

    /// The text of the last `GitEvent::Status`, the value the app shell would
    /// leave in its status line.
    fn last_status(events: &[GitEvent]) -> String {
        events
            .iter()
            .rev()
            .find_map(|e| match e {
                GitEvent::Status(s) => Some(s.clone()),
                GitEvent::BranchSwitched | GitEvent::WorkingTreeChanged => None,
            })
            .unwrap_or_default()
    }

    /// A commit stages what is ticked and unstages what is not, read back
    /// from `git` directly rather than from `git.status`, so this cannot
    /// pass on the app merely agreeing with itself.
    #[test]
    fn commit_stages_ticked_and_leaves_out_unticked() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join("contracts").join("sample.json"));
        std::fs::write(root.join("contracts").join("extra.json"), "{}\n")
            .expect("extra file writes");
        git_output(&root, &["add", "contracts/extra.json"]);

        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.handle(GitAction::Refresh, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);

        let sample = git
            .status
            .find("contracts/sample.json")
            .expect("sample is changed")
            .clone();
        let extra = git
            .status
            .find("contracts/extra.json")
            .expect("extra is changed")
            .clone();
        assert!(!git.is_ticked(&sample), "an unstaged file starts unticked");
        assert!(git.is_ticked(&extra), "a staged file starts ticked");

        git.handle(
            GitAction::SetTicked {
                paths: vec!["contracts/sample.json".to_string()],
                ticked: true,
            },
            Some(&paths),
            &ctx,
        );
        git.handle(
            GitAction::SetTicked {
                paths: vec!["contracts/extra.json".to_string()],
                ticked: false,
            },
            Some(&paths),
            &ctx,
        );
        git.commit_message = "commit the ticked file".to_string();
        git.handle(GitAction::Commit, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);

        let committed = git_output(&root, &["show", "--name-only", "--format=", "HEAD"]);
        assert!(
            committed.lines().any(|l| l == "contracts/sample.json"),
            "the ticked file must be committed: {committed:?}"
        );
        assert!(
            !committed.lines().any(|l| l == "contracts/extra.json"),
            "the unticked file must be left out: {committed:?}"
        );
        let porcelain = git_output(&root, &["status", "--porcelain=v2"]);
        assert!(
            porcelain.lines().any(|l| l == "? contracts/extra.json"),
            "the unticked file must be untracked again: {porcelain:?}"
        );
    }

    /// A ticked deletion lands in the commit, which `git add -A` makes
    /// possible for a path no longer on disk.
    #[test]
    fn commit_includes_a_ticked_deletion() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        std::fs::remove_file(root.join("contracts").join("sample.json"))
            .expect("fixture file deletes");

        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.handle(GitAction::Refresh, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);

        git.handle(
            GitAction::SetTicked {
                paths: vec!["contracts/sample.json".to_string()],
                ticked: true,
            },
            Some(&paths),
            &ctx,
        );
        git.commit_message = "delete the sample".to_string();
        git.handle(GitAction::Commit, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);

        let committed = git_output(&root, &["show", "--name-status", "--format=", "HEAD"]);
        assert!(
            committed.lines().any(|l| l == "D\tcontracts/sample.json"),
            "the deletion must be committed: {committed:?}"
        );
    }

    /// One changed file with the given sides, not conflicted.
    fn changed(path: &str, index: Change, worktree: Change) -> FileStatus {
        FileStatus {
            path: path.to_string(),
            index,
            worktree,
            conflicted: false,
        }
    }

    #[test]
    fn ticks_follow_the_index_until_the_user_changes_them() {
        let staged = changed("staged.json", Change::Added, Change::Unmodified);
        let unstaged = changed("unstaged.json", Change::Unmodified, Change::Modified);
        let status = Status {
            inside: vec![staged.clone(), unstaged.clone()],
            ..Status::default()
        };
        let mut git = GitState::default();
        git.set_status(status.clone());
        assert!(git.is_ticked(&staged));
        assert!(!git.is_ticked(&unstaged));

        let ctx = egui::Context::default();
        git.handle(
            GitAction::SetTicked {
                paths: vec!["unstaged.json".to_string()],
                ticked: true,
            },
            None,
            &ctx,
        );
        assert!(git.is_ticked(&unstaged));

        git.set_status(status.clone());
        assert!(git.is_ticked(&unstaged), "a tick survives a refresh");

        git.set_status(Status::default());
        assert!(
            git.ticks.is_empty(),
            "a tick is forgotten once the path leaves"
        );
    }

    #[test]
    fn unstage_unticks() {
        let file = changed("dir/a.json", Change::Modified, Change::Unmodified);
        let mut git = GitState::default();
        git.set_status(Status {
            inside: vec![file.clone()],
            ..Status::default()
        });
        assert!(git.is_ticked(&file));

        let ctx = egui::Context::default();
        git.handle(
            GitAction::Unstage {
                path: "dir".to_string(),
            },
            None,
            &ctx,
        );
        assert!(!git.is_ticked(&file));
    }

    #[test]
    fn selecting_another_file_keeps_the_field_list_toggle() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();

        git.show_fields = true;
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
        assert!(git.show_fields);
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
            GitAction::SetTicked {
                paths: vec!["contracts/sample.json".to_string()],
                ticked: true,
            },
            Some(&paths),
            &ctx,
        );

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

    /// A repo whose HEAD is pushed to a bare remote, with Amend ticked and
    /// the pushed warning read.
    fn pushed_amend_fixture() -> (std::path::PathBuf, GitState, RepoPaths, egui::Context) {
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
        assert!(git.amend_pushed_to.is_some());
        (root, git, paths, ctx)
    }

    #[test]
    fn committing_a_pushed_amend_asks_first() {
        if !git_available() {
            return;
        }
        let (root, mut git, paths, ctx) = pushed_amend_fixture();
        let head = git_output(&root, &["rev-parse", "HEAD"]);
        git.commit_message = "reworded initial".to_string();
        git.handle(GitAction::Commit, Some(&paths), &ctx);
        assert!(git.confirm_amend);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(git_output(&root, &["rev-parse", "HEAD"]), head);
        assert!(git.confirm_amend);
    }

    #[test]
    fn confirm_amend_amends() {
        if !git_available() {
            return;
        }
        let (root, mut git, paths, ctx) = pushed_amend_fixture();
        let count = git_output(&root, &["rev-list", "--count", "HEAD"]);
        git.commit_message = "reworded initial".to_string();
        git.handle(GitAction::Commit, Some(&paths), &ctx);
        git.handle(GitAction::ConfirmAmend, Some(&paths), &ctx);
        assert!(!git.confirm_amend);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(
            git_output(&root, &["log", "-1", "--format=%s"]).trim(),
            "reworded initial"
        );
        assert_eq!(git_output(&root, &["rev-list", "--count", "HEAD"]), count);
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

    #[test]
    fn retick_while_the_first_read_is_in_flight_keeps_the_draft() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.commit_message = "draft".to_string();
        git.handle(GitAction::ToggleAmend, Some(&paths), &ctx);
        let _ = git.poll(Some(&paths), &ctx);
        git.handle(GitAction::ToggleAmend, Some(&paths), &ctx);
        git.handle(GitAction::ToggleAmend, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        git.handle(GitAction::ToggleAmend, Some(&paths), &ctx);
        assert_eq!(git.commit_message, "draft");
    }

    #[test]
    fn confirm_merge_into_another_branch_switches_to_it() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.branches.current = Some("apic-test".into());

        git.handle(
            GitAction::RequestBranchOp {
                kind: BranchOpKind::Merge,
                target: "apic-second".into(),
                source: "apic-test".into(),
            },
            Some(&paths),
            &ctx,
        );
        git.handle(GitAction::ConfirmBranchOp, Some(&paths), &ctx);
        let events = settle_git(&mut git, &paths, &ctx);

        assert_eq!(current_branch(&root), "apic-second");
        assert!(events.contains(&GitEvent::BranchSwitched), "{events:?}");
        assert!(git.pending_branch_op.is_none());
    }

    #[test]
    fn confirm_with_no_source_runs_nothing() {
        let mut git = GitState::default();
        let ctx = egui::Context::default();
        git.pending_branch_op = Some(BranchOp {
            kind: BranchOpKind::Pull,
            target: "main".into(),
            source: String::new(),
        });
        git.handle(GitAction::ConfirmBranchOp, None, &ctx);
        assert!(git.pending.is_none());
        assert!(git.pending_branch_op.is_none());
    }

    #[test]
    fn failed_merge_rereads_status_and_branches() {
        let mut git = GitState::default();
        let mut events = Vec::new();
        git.apply_result(
            JobResult::Mutate(MutateKind::Merge, String::new(), Err("conflict".into())),
            &mut events,
        );
        assert!(git.needs_branches && git.needs_refresh);
        assert_eq!(git.error, "conflict");
        assert!(events.contains(&GitEvent::BranchSwitched));
    }

    /// A git state on `root` with the log opened and read.
    fn open_log(root: &std::path::Path) -> (GitState, RepoPaths, egui::Context) {
        let (mut git, paths) = git_at(root);
        let ctx = egui::Context::default();
        git.handle(GitAction::ToggleLog, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        (git, paths, ctx)
    }

    fn subjects(git: &GitState) -> Vec<&str> {
        git.log.commits.iter().map(|c| c.subject.as_str()).collect()
    }

    fn log_commit(hash: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: hash.to_string(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            author: "Ann".into(),
            date: String::new(),
            refs: Vec::new(),
            subject: hash.to_string(),
        }
    }

    #[test]
    fn log_state_toggle_opens_reads_and_closes() {
        if !git_available() {
            return;
        }
        let (mut git, paths, ctx) = open_log(&project_fixture());
        assert!(git.log.open);
        assert!(git.log.loaded);
        assert_eq!(subjects(&git), ["initial"]);
        git.handle(GitAction::ToggleLog, Some(&paths), &ctx);
        assert!(!git.log.open);
        assert!(!git.log.needs_load);
    }

    #[test]
    fn log_state_filter_change_rereads_from_the_start() {
        if !git_available() {
            return;
        }
        let (mut git, paths, ctx) = open_log(&project_fixture());
        let second = LogFilter {
            branch: Some("apic-second".into()),
            ..LogFilter::default()
        };
        git.handle(GitAction::SetLogFilter(second), Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(subjects(&git), ["second branch contracts", "initial"]);

        let nothing = LogFilter {
            text: "no commit says this".into(),
            ..LogFilter::default()
        };
        git.handle(GitAction::SetLogFilter(nothing), Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert!(git.log.commits.is_empty());
        assert!(git.log.loaded);
        assert!(git.log.error.is_empty(), "{}", git.log.error);
    }

    #[test]
    fn log_state_load_more_appends_the_next_page() {
        if !git_available() {
            return;
        }
        let (mut git, paths, ctx) = open_log(&project_fixture());
        let second = LogFilter {
            branch: Some("apic-second".into()),
            ..LogFilter::default()
        };
        git.handle(GitAction::SetLogFilter(second), Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        git.log.commits.truncate(1);
        git.log.fetched = 1;

        git.handle(GitAction::LoadMoreLog, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);

        assert_eq!(subjects(&git), ["second branch contracts", "initial"]);
        assert_eq!(git.log.rows.len(), git.log.commits.len());
        assert!(!git.log.more);
    }

    #[test]
    fn log_state_a_failed_read_keeps_the_rows() {
        let mut git = GitState::default();
        git.log.commits = vec![log_commit("a", &[])];
        let mut events = Vec::new();
        git.apply_result(
            JobResult::Log(false, Err("bad revision".into())),
            &mut events,
        );
        assert_eq!(git.log.commits.len(), 1);
        assert_eq!(git.log.error, "bad revision");
    }

    #[test]
    fn log_state_select_commit_then_file_reads_the_commit_diff() {
        if !git_available() {
            return;
        }
        let (mut git, paths, ctx) = open_log(&project_fixture());
        let hash = git.log.commits[0].hash.clone();
        git.handle(
            GitAction::SelectCommit { hash: hash.clone() },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);
        let files: Vec<&str> = git
            .log
            .files
            .as_ref()
            .expect("the files load")
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert!(files.contains(&"contracts/sample.json"), "{files:?}");

        git.selected = Some(("contracts/sample.json".into(), false));
        git.handle(
            GitAction::SelectCommitFile {
                path: "contracts/sample.json".into(),
            },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);
        let ((key_hash, key_path), data) = git.log.diff.as_ref().expect("the diff loads");
        assert_eq!(key_hash, &hash);
        assert_eq!(key_path, "contracts/sample.json");
        assert!(data.old_blob.is_none(), "the root commit has no old side");
        assert!(data.new_blob.is_some());
        assert!(!data.rows.is_empty());
        assert!(
            git.selected.is_none(),
            "a commit file replaces the working-tree selection"
        );
    }

    #[test]
    fn log_state_confirm_revert_adds_a_revert_commit_on_top() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join("contracts").join("sample.json"));
        git_output(&root, &["commit", "-am", "change"]);
        let (mut git, paths, ctx) = open_log(&root);
        let hash = git.log.commits[0].hash.clone();
        git.handle(GitAction::RequestRevert { hash }, Some(&paths), &ctx);
        assert!(git.log.pending_revert.is_some());
        git.handle(GitAction::ConfirmRevert, Some(&paths), &ctx);
        let events = settle_git(&mut git, &paths, &ctx);
        assert_eq!(
            git_output(&root, &["log", "-1", "--format=%s"]).trim(),
            "Revert \"change\""
        );
        assert_eq!(subjects(&git)[0], "Revert \"change\"");
        assert!(events.contains(&GitEvent::WorkingTreeChanged), "{events:?}");
    }

    #[test]
    fn log_state_conflicting_revert_sets_the_conflict_notice() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let path = root.join("contracts").join("sample.json");
        let named = |prefix: &str| {
            apic_core::template::DEFAULT.replace("\"name\": \"", &format!("\"name\": \"{prefix}"))
        };
        std::fs::write(&path, named("a-")).unwrap();
        git_output(&root, &["commit", "-am", "a"]);
        std::fs::write(&path, named("b-")).unwrap();
        git_output(&root, &["commit", "-am", "b"]);
        let (mut git, paths, ctx) = open_log(&root);
        let a = git
            .log
            .commits
            .iter()
            .find(|c| c.subject == "a")
            .expect("commit a is listed")
            .hash
            .clone();
        git.handle(GitAction::RequestRevert { hash: a }, Some(&paths), &ctx);
        git.handle(GitAction::ConfirmRevert, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(
            git.conflict_notice,
            Some(vec!["contracts/sample.json".to_string()])
        );
    }

    #[test]
    fn log_state_revert_is_refused_for_a_merge_and_another_branch() {
        let ctx = egui::Context::default();
        let mut git = GitState::default();
        git.log.commits = vec![log_commit("m", &["a", "b"]), log_commit("p", &["a"])];
        git.handle(GitAction::RequestRevert { hash: "m".into() }, None, &ctx);
        assert!(
            git.log.pending_revert.is_none(),
            "a merge commit is refused"
        );

        git.branches.current = Some("main".into());
        git.log.filter.branch = Some("other".into());
        git.handle(GitAction::RequestRevert { hash: "p".into() }, None, &ctx);
        assert!(
            git.log.pending_revert.is_none(),
            "another branch is refused"
        );

        git.log.filter.branch = Some("main".into());
        git.handle(GitAction::RequestRevert { hash: "p".into() }, None, &ctx);
        assert_eq!(
            git.log.pending_revert.map(|c| c.hash),
            Some("p".to_string())
        );
    }

    #[test]
    fn log_state_a_commit_reloads_the_log() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join("contracts").join("sample.json"));
        let (mut git, paths, ctx) = open_log(&root);
        git.handle(GitAction::Refresh, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        git.handle(
            GitAction::SetTicked {
                paths: vec!["contracts/sample.json".to_string()],
                ticked: true,
            },
            Some(&paths),
            &ctx,
        );
        git.commit_message = "log me".to_string();
        git.handle(GitAction::Commit, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(subjects(&git)[0], "log me");
    }

    #[test]
    fn log_state_a_branch_switch_resets_the_branch_filter() {
        if !git_available() {
            return;
        }
        let (mut git, paths, ctx) = open_log(&project_fixture());
        let second = LogFilter {
            branch: Some("apic-second".into()),
            ..LogFilter::default()
        };
        git.handle(GitAction::SetLogFilter(second), Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        git.handle(
            GitAction::SwitchBranch {
                name: "apic-second".into(),
            },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(git.log.filter.branch, None);
        assert_eq!(subjects(&git)[0], "second branch contracts");
    }

    #[test]
    fn log_state_revert_confirmed_during_a_log_read_still_runs() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join("contracts").join("sample.json"));
        git_output(&root, &["commit", "-am", "change"]);
        let (mut git, paths, ctx) = open_log(&root);
        let hash = git.log.commits[0].hash.clone();
        git.handle(GitAction::RequestRevert { hash }, Some(&paths), &ctx);
        git.handle(GitAction::RefreshLog, Some(&paths), &ctx);
        git.poll(Some(&paths), &ctx);
        assert!(git.pending.is_some(), "the log read holds the slot");
        git.handle(GitAction::ConfirmRevert, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(
            git_output(&root, &["log", "-1", "--format=%s"]).trim(),
            "Revert \"change\""
        );
    }

    #[test]
    fn log_state_select_during_a_log_read_still_loads_the_diff() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join("contracts").join("sample.json"));
        let (mut git, paths, ctx) = open_log(&root);
        git.handle(GitAction::RefreshLog, Some(&paths), &ctx);
        git.poll(Some(&paths), &ctx);
        assert!(git.pending.is_some(), "the log read holds the slot");
        git.handle(
            GitAction::Select {
                path: "contracts/sample.json".into(),
                staged: false,
            },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);
        let (key, _) = git.diff.as_ref().expect("the diff loads");
        assert_eq!(key, &("contracts/sample.json".to_string(), false));
    }
}
