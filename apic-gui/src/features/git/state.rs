//! State owned by the git feature: the current status, the selected diff, and
//! the in-flight background job (if any).

use crate::features::git::model::Status;

/// The result of a completed background git job, sent back over `pending`.
/// Extended with more variants (diff, commit) in later tasks.
pub(crate) enum JobResult {
    Status(Result<Status, String>),
}

/// Everything the git feature owns. `App` holds one of these; no other
/// feature may read or write it.
#[derive(Default)]
pub(crate) struct GitState {
    /// The last status read from the repository, scoped to the contracts
    /// working dir plus a count of changes elsewhere.
    pub(crate) status: Status,

    /// The path and side (staged or unstaged) whose diff is shown, when one
    /// is selected.
    pub(crate) selected: Option<(String, bool)>,

    /// Nothing reads this yet, the commit box lands in a later task.
    #[allow(dead_code)]
    pub(crate) commit_message: String,

    /// The last git error, shown in the panel; empty when there is none.
    pub(crate) error: String,

    /// Whether the "outside" (out-of-scope) changes section is expanded.
    pub(crate) show_outside: bool,

    /// The receiving end of an in-flight background git job, if any. Only one
    /// git command runs at a time: concurrent commands contend for
    /// `index.lock`.
    pub(crate) pending: Option<std::sync::mpsc::Receiver<JobResult>>,
}
