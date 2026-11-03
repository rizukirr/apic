//! State owned by the git feature: the current status, the selected diff, and
//! the in-flight background job (if any).

use crate::features::git::data::jobs::{DiffData, JobResult};
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
}
