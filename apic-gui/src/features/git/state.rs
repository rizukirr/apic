//! State owned by the git feature: the current status, the selected diff, and
//! the in-flight background job (if any).

use crate::features::git::model::{Branches, Status};

/// One completed diff fetch: the raw line diff plus each side's file content,
/// fetched only for a `.json` path since that is the only shape `diff::parse`
/// can turn into a semantic comparison.
pub(crate) struct DiffData {
    /// `git diff` output for the selected path and side, `--cached` for
    /// staged.
    pub(crate) raw: String,

    /// The old side's content: `HEAD` for staged, the index for unstaged.
    /// `None` when the path did not exist at that revision, the normal case
    /// for a newly added file.
    pub(crate) old_blob: Option<String>,

    /// The new side's content: the index for staged, the working file for
    /// unstaged. `None` when the path does not exist there, the normal case
    /// for a deleted file.
    pub(crate) new_blob: Option<String>,
}

/// Which mutating git command a `JobResult::Mutate` reports on. Carried
/// alongside the result so the caller knows, for a successful commit, to
/// clear `GitState::commit_message`.
pub(crate) enum MutateKind {
    Stage,
    Unstage,
    Discard,
    Commit,
    SwitchBranch,
    CreateBranch,
    DeleteBranch,
}

/// The result of a completed background git job, sent back over `pending`.
pub(crate) enum JobResult {
    Status(Result<Status, String>),

    /// A diff fetch for one `(path, staged)` selection. The key travels with
    /// the result so a stale reply, one for a file the user has since
    /// unselected, does not get filed under the wrong key.
    Diff((String, bool), Result<DiffData, String>),

    /// A stage, unstage, discard, commit, or branch switch, create or
    /// delete, run then followed by a status refresh in the same background
    /// job. The refreshed status travels with the result so a successful
    /// mutation lands on screen in one poll.
    Mutate(MutateKind, Result<Status, String>),

    /// A branch listing.
    Branches(Result<Branches, String>),
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

    /// The path and side (staged or unstaged) whose diff is shown, when one
    /// is selected.
    pub(crate) selected: Option<(String, bool)>,

    /// The diff fetched for `selected`, keyed the same way, so switching
    /// between two files and back does not re-run git. `None` while the
    /// first fetch for the current selection is still in flight.
    pub(crate) diff: Option<((String, bool), DiffData)>,

    /// Forces the line diff for the current file even when a semantic view
    /// is available. Reset to `false` when the selection changes, so the
    /// toggle applies to the file it was set on rather than persisting
    /// across selections.
    pub(crate) raw_view: bool,

    /// The commit message box, bound to the commit row's text field. Cleared
    /// after a successful commit.
    pub(crate) commit_message: String,

    /// The repo-relative path of a discard awaiting confirmation, if any.
    pub(crate) pending_discard: Option<String>,

    /// The name of a branch delete awaiting confirmation, if any.
    pub(crate) pending_branch_delete: Option<String>,

    /// The last git error, shown in the panel; empty when there is none.
    pub(crate) error: String,

    /// Whether the "outside" (out-of-scope) changes section is expanded.
    pub(crate) show_outside: bool,

    /// The receiving end of an in-flight background git job, if any. Only one
    /// git command runs at a time: concurrent commands contend for
    /// `index.lock`.
    pub(crate) pending: Option<std::sync::mpsc::Receiver<JobResult>>,
}
