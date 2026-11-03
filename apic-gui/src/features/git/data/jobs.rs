//! Background git jobs and the scope arithmetic that confines the file list
//! to the active contracts root.
//!
//! A job runs on a spawned thread and delivers one [`JobResult`] over a
//! channel that the view model drains every frame.

use std::borrow::Cow;
use std::path::Path;

use crate::features::git::data::cli;
use crate::features::git::domain::status::{Branches, Status};

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
    Resolve,
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
    /// mutation lands on screen in one poll. The message names what the
    /// mutation did, built at spawn time where the path or branch name is
    /// already in hand, and is written to `shell.status` on success.
    Mutate(MutateKind, String, Result<Status, String>),

    /// A branch listing.
    Branches(Result<Branches, String>),
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
