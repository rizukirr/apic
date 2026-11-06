//! Background git jobs and the scope arithmetic that confines the file list
//! to the active contracts root.
//!
//! A job runs on a spawned thread and delivers one [`JobResult`] over a
//! channel that the view model drains every frame.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use crate::features::git::data::cli;
use crate::features::git::domain::diff::{DiffRow, parse_unified};
use crate::features::git::domain::status::{Branches, Status};

/// fetched only for a `.json` path since that is the only shape `diff::parse`
/// can turn into a semantic comparison.
pub(crate) struct DiffData {
    /// `git diff` output for the selected path and side, `--cached` for
    /// staged.
    pub(crate) raw: String,

    /// `raw` parsed into inline diff rows, done here on the job thread so
    /// the view never parses on a frame.
    pub(crate) rows: Vec<DiffRow>,

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
    Unstage,
    Discard,
    Commit,
    SwitchBranch,
    CreateBranch,
    DeleteBranch,
    Resolve,
}

/// The result of a completed background git job, sent back over `pending`.
pub(crate) enum JobResult {
    Status(Result<Status, String>),

    /// A diff fetch for one `(path, staged)` selection. The key travels with
    /// the result so a stale reply, one for a file the user has since
    /// unselected, does not get filed under the wrong key.
    Diff((String, bool), Result<DiffData, String>),

    /// An unstage, discard, commit, or branch switch, create or
    /// delete, run then followed by a status refresh in the same background
    /// job. The refreshed status travels with the result so a successful
    /// mutation lands on screen in one poll. The message names what the
    /// mutation did, built at spawn time where the path or branch name is
    /// already in hand, and is written to `shell.status` on success.
    Mutate(MutateKind, String, Result<Status, String>),

    /// A branch listing.
    Branches(Result<Branches, String>),

    /// What amending would rewrite, read when Amend is ticked.
    Amend(Result<AmendInfo, String>),
}

/// Builds one `DiffData`: the line diff plus, for a `.json` path or a
/// conflicted path, each side's content so the caller can attempt a semantic
/// comparison or, for a conflict, parse the working file's markers.
///
/// Staged compares the index against `HEAD`; unstaged compares the working
/// file against the index. `git show :<path>` reads the index (stage 0),
/// which is what an empty revision means to `cli::blob`.
pub(crate) fn diff_job(
    root: &Path,
    path: &str,
    staged: bool,
    conflicted: bool,
) -> Result<DiffData, String> {
    let mut raw = cli::diff_text(root, path, staged)?;
    let head_blob = cli::blob(root, "HEAD", path)?;
    let index_blob = cli::blob(root, "", path)?;

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

    let (old_blob, new_blob) = if path.ends_with(".json") || conflicted {
        if staged {
            (head_blob, index_blob)
        } else {
            let new = std::fs::read_to_string(root.join(path)).ok();
            (index_blob, new)
        }
    } else {
        (None, None)
    };
    let rows = parse_unified(&raw);
    Ok(DiffData {
        raw,
        rows,
        old_blob,
        new_blob,
    })
}

/// Resolves symlinks in `path` for comparison purposes, falling back to
/// `path` unresolved when resolution fails, a project directory deleted
/// while the app is open being the case that matters. Never used for a path
/// shown to the user: `ProjectState::project_root` stays unresolved in the
/// status bar, and this is only applied to a local copy at the point it is
/// compared against `repo_root`.
fn resolve_for_comparison(path: &Path) -> Cow<'_, Path> {
    match path.canonicalize() {
        Ok(resolved) => Cow::Owned(resolved),
        Err(_) => Cow::Borrowed(path),
    }
}

/// The scopes passed to `cli::status`: `root` made relative to
/// `repo_root`, plus `project_root/.apic` made relative to `repo_root` so the
/// project config and templates always count as inside even when they sit
/// outside the contract working dir. Falls back to an empty list (the whole
/// repo, unconditionally inside) when `root` is not under `repo_root`.
///
/// `repo_root` comes from `git rev-parse --show-toplevel`, which resolves
/// symlinks. `root` and `project_root` are stored unresolved, so both are
/// resolved here before the comparison rather than trusting the caller to
/// have matched `repo_root`'s form.
pub(crate) fn scopes(root: &Path, project_root: Option<&Path>, repo_root: &Path) -> Vec<String> {
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

/// The repository locations a job runs against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoPaths {
    /// Root of the git repository, as `git rev-parse --show-toplevel` reports it.
    pub(crate) repo_root: PathBuf,

    /// The contracts working dir, or `repo_root` when no project root is known.
    pub(crate) root: PathBuf,

    /// The directory containing `.apic/`, when a project is open.
    pub(crate) project_root: Option<PathBuf>,
}

impl RepoPaths {
    /// The status scopes for these paths, see [`scopes`].
    fn scopes(&self) -> Vec<String> {
        scopes(&self.root, self.project_root.as_deref(), &self.repo_root)
    }
}

/// One diff fetch: the selected path and side, and whether the path is in a
/// merge conflict, which decides whether each side's content is read.
pub(crate) struct DiffRequest {
    pub(crate) repo_root: PathBuf,
    pub(crate) path: String,
    pub(crate) staged: bool,
    pub(crate) conflicted: bool,
}

/// One mutating git command. Each is followed by a status refresh in the same
/// job, so a successful mutation lands on screen in one poll.
pub(crate) enum Mutation {
    Unstage {
        path: String,
    },

    /// Called only after the discard confirmation.
    Discard {
        path: String,
    },
    Commit {
        message: String,
        amend: bool,

        /// Ticked paths, staged first.
        stage: Vec<String>,

        /// Staged paths the user unticked, unstaged before the commit.
        unstage: Vec<String>,
    },
    SwitchBranch {
        name: String,
    },

    /// Creates a branch without switching to it.
    CreateBranch {
        name: String,
    },

    /// Called only after the delete confirmation.
    DeleteBranch {
        name: String,
    },

    /// Writes the rendered resolution `text` for `path` and stages it.
    ///
    /// The write and the stage are two halves of one operation: editing the
    /// file does not clear the conflict in git, only `git add` does. If the
    /// write succeeds and the stage then fails, that failure must still
    /// reach the user as an error rather than being swallowed, a file that
    /// looks resolved while git still calls it conflicted is worse than a
    /// resolve that failed outright.
    Resolve {
        path: String,
        text: String,
    },
}

impl Mutation {
    /// Which kind of mutation this is, carried back with the result.
    fn kind(&self) -> MutateKind {
        match self {
            Mutation::Unstage { .. } => MutateKind::Unstage,
            Mutation::Discard { .. } => MutateKind::Discard,
            Mutation::Commit { .. } => MutateKind::Commit,
            Mutation::SwitchBranch { .. } => MutateKind::SwitchBranch,
            Mutation::CreateBranch { .. } => MutateKind::CreateBranch,
            Mutation::DeleteBranch { .. } => MutateKind::DeleteBranch,
            Mutation::Resolve { .. } => MutateKind::Resolve,
        }
    }

    /// The status line text shown when the mutation succeeds.
    fn success_message(&self) -> String {
        match self {
            Mutation::Unstage { path } => format!("unstaged {path}"),
            Mutation::Discard { path } => format!("discarded {path}"),
            Mutation::Commit { amend: true, .. } => "amended the last commit".to_string(),
            Mutation::Commit { amend: false, .. } => "committed the staged changes".to_string(),
            Mutation::SwitchBranch { name } => format!("switched to branch {name}"),
            Mutation::CreateBranch { name } => format!("created branch {name}"),
            Mutation::DeleteBranch { name } => format!("deleted branch {name}"),
            Mutation::Resolve { path, .. } => format!("resolved {path}"),
        }
    }

    /// Runs the git command against the repository at `root`.
    fn run(&self, root: &Path) -> Result<(), String> {
        match self {
            Mutation::Unstage { path } => cli::unstage(root, path),
            Mutation::Discard { path } => cli::discard(root, path),
            Mutation::Commit {
                message,
                amend,
                stage,
                unstage,
            } => {
                if !stage.is_empty() {
                    cli::stage_paths(root, stage)?;
                }
                if !unstage.is_empty() {
                    cli::unstage_paths(root, unstage)?;
                }
                cli::commit(root, message, *amend)
            }
            Mutation::SwitchBranch { name } => cli::switch_branch(root, name),
            Mutation::CreateBranch { name } => cli::create_branch(root, name),
            Mutation::DeleteBranch { name } => cli::delete_branch(root, name),
            Mutation::Resolve { path, text } => {
                std::fs::write(root.join(path), text).map_err(|e| e.to_string())?;
                cli::stage(root, path)
            }
        }
    }
}

/// What amending would rewrite: the last commit's message, and the upstream
/// that already holds that commit, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AmendInfo {
    pub(crate) message: String,
    pub(crate) pushed_to: Option<String>,
}

/// Starts reading what amending would rewrite.
pub(crate) fn amend_info(
    repo_root: PathBuf,
    notify: impl Fn() + Send + 'static,
) -> Receiver<JobResult> {
    spawn(
        move || {
            JobResult::Amend(
                cli::last_commit_message(&repo_root).map(|message| AmendInfo {
                    message,
                    pushed_to: cli::upstream(&repo_root)
                        .filter(|_| cli::head_is_on_upstream(&repo_root)),
                }),
            )
        },
        notify,
    )
}

/// Runs `job` on a new thread, sends its one result, then calls `notify` so
/// the caller's event loop wakes to collect it.
fn spawn(
    job: impl FnOnce() -> JobResult + Send + 'static,
    notify: impl Fn() + Send + 'static,
) -> Receiver<JobResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(job());
        notify();
    });
    rx
}

/// Starts `git status`, scoped to the contracts root.
pub(crate) fn status(paths: RepoPaths, notify: impl Fn() + Send + 'static) -> Receiver<JobResult> {
    spawn(
        move || {
            let scopes = paths.scopes();
            let refs: Vec<&str> = scopes.iter().map(String::as_str).collect();
            JobResult::Status(cli::status(&paths.repo_root, &refs))
        },
        notify,
    )
}

/// Starts the diff fetch for one `(path, staged)` selection.
pub(crate) fn diff(
    request: DiffRequest,
    notify: impl Fn() + Send + 'static,
) -> Receiver<JobResult> {
    spawn(
        move || {
            let result = diff_job(
                &request.repo_root,
                &request.path,
                request.staged,
                request.conflicted,
            );
            JobResult::Diff((request.path, request.staged), result)
        },
        notify,
    )
}

/// Starts one mutation followed by a status refresh in the same job.
pub(crate) fn mutate(
    paths: RepoPaths,
    mutation: Mutation,
    notify: impl Fn() + Send + 'static,
) -> Receiver<JobResult> {
    spawn(
        move || {
            let scopes = paths.scopes();
            let refs: Vec<&str> = scopes.iter().map(String::as_str).collect();
            let result = mutation
                .run(&paths.repo_root)
                .and_then(|()| cli::status(&paths.repo_root, &refs));
            JobResult::Mutate(mutation.kind(), mutation.success_message(), result)
        },
        notify,
    )
}

/// Starts `git branch`.
pub(crate) fn branches(
    repo_root: PathBuf,
    notify: impl Fn() + Send + 'static,
) -> Receiver<JobResult> {
    spawn(
        move || JobResult::Branches(cli::branches(&repo_root)),
        notify,
    )
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

        let data = diff_job(&root, "contracts/untracked.json", false, false).unwrap();

        assert!(
            data.raw.contains("hello"),
            "an untracked file's diff must carry its own content, got: {:?}",
            data.raw
        );
        assert!(!data.rows.is_empty());
        assert!(data.rows.iter().all(|row| matches!(
            row,
            DiffRow::Line(line) if line.kind == crate::features::git::domain::diff::LineKind::Added
        )));
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
