//! Every `git` invocation, and nothing else. No state, no egui, no threads: the threading lives in `jobs`, which is what keeps this testable against a real temp-dir repository.
//!
//! Consumed by `jobs` and the app shell's project discovery.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::features::git::domain::status::{Branches, Status, parse_status};

/// Runs `git -C <root> <args>` and returns stdout bytes.
///
/// A spawn failure and a non-zero exit are distinguished: the first means git
/// is not installed, the second means git ran and refused, and the two have
/// different fixes.
///
/// GIT_TERMINAL_PROMPT=0 makes a command that needs credentials fail instead of waiting on a terminal the GUI does not have.
fn run(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "git not found on PATH".to_string(),
            _ => format!("could not run git: {e}"),
        })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let err = if err.is_empty() {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .rev()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or_default()
                .to_string()
        } else {
            err
        };
        return Err(if err.is_empty() {
            format!("git {} failed", args.join(" "))
        } else {
            err
        });
    }
    Ok(out.stdout)
}

/// The repo root containing `project_root`, or `None` when it is not in a repo.
/// An `Err` is a reportable problem (git missing), `None` is the ordinary case
/// of a project outside version control.
pub(crate) fn discover(project_root: &Path) -> Result<Option<PathBuf>, String> {
    match run(project_root, &["rev-parse", "--show-toplevel"]) {
        Ok(out) => Ok(Some(PathBuf::from(
            String::from_utf8_lossy(&out).trim().to_string(),
        ))),
        Err(e) if e == "git not found on PATH" => Err(e),
        Err(_) => Ok(None),
    }
}

/// The working tree, split against `scopes` (repo-relative prefixes, empty
/// for the whole repo).
pub(crate) fn status(root: &Path, scopes: &[&str]) -> Result<Status, String> {
    let out = run(
        root,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ],
    )?;
    Ok(parse_status(&out, scopes))
}

/// The line diff for one path, staged or unstaged. Git's default context is
/// what the panel renders, so what comes back is a real diff with hunk
/// headers rather than the whole file with the change tinted. The field list
/// answers "what changed" for a contract; this is the view for everything
/// else, and for anyone who wants the diff itself.
pub(crate) fn diff_text(root: &Path, path: &str, staged: bool) -> Result<String, String> {
    let mut args = vec!["diff", "--no-color"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(path);
    let out = run(root, &args)?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// One file's content at `rev`, e.g. `HEAD` for the last commit or an empty
/// string for the index. `Ok(None)` means the path does not exist there, which
/// is the normal case for a newly added file.
pub(crate) fn blob(root: &Path, rev: &str, path: &str) -> Result<Option<String>, String> {
    match run(root, &["show", &format!("{rev}:{path}")]) {
        Ok(out) => Ok(Some(String::from_utf8_lossy(&out).into_owned())),
        Err(e) if e == "git not found on PATH" => Err(e),
        Err(_) => Ok(None),
    }
}

/// Stages one path.
pub(crate) fn stage(root: &Path, path: &str) -> Result<(), String> {
    run(root, &["add", "--", path]).map(|_| ())
}

/// Unstages one path, leaving the working tree untouched.
pub(crate) fn unstage(root: &Path, path: &str) -> Result<(), String> {
    run(root, &["restore", "--staged", "--", path]).map(|_| ())
}

/// Stages every path in `paths`, including deletions and untracked files.
pub(crate) fn stage_paths(root: &Path, paths: &[String]) -> Result<(), String> {
    let mut args = vec!["add", "-A", "--"];
    args.extend(paths.iter().map(String::as_str));
    run(root, &args).map(|_| ())
}

/// Unstages every path in `paths`, leaving the working tree untouched.
pub(crate) fn unstage_paths(root: &Path, paths: &[String]) -> Result<(), String> {
    let mut args = vec!["restore", "--staged", "--"];
    args.extend(paths.iter().map(String::as_str));
    run(root, &args).map(|_| ())
}

/// Reverts one tracked path to its staged content. Untracked files are not
/// offered, deleting bytes git has never seen has no recovery path.
pub(crate) fn discard(root: &Path, path: &str) -> Result<(), String> {
    run(root, &["checkout", "--", path]).map(|_| ())
}

/// Commits the index with `message`, amending the last commit instead of
/// adding one when `amend` is set.
pub(crate) fn commit(root: &Path, message: &str, amend: bool) -> Result<(), String> {
    let mut args = vec!["commit"];
    if amend {
        args.push("--amend");
    }
    args.extend(["-m", message]);
    run(root, &args).map(|_| ())
}

/// The full message of the last commit, without its trailing newline.
/// Fails in a repository with no commits.
pub(crate) fn last_commit_message(root: &Path) -> Result<String, String> {
    let out = run(root, &["log", "-1", "--format=%B"])?;
    Ok(String::from_utf8_lossy(&out).trim_end().to_string())
}

/// The current branch's upstream, such as `origin/main`, or `None` when
/// none is configured.
pub(crate) fn upstream(root: &Path) -> Option<String> {
    run(
        root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .ok()
    .map(|out| String::from_utf8_lossy(&out).trim().to_string())
    .filter(|name| !name.is_empty())
}

/// Whether `HEAD` is already contained in the upstream branch.
pub(crate) fn head_is_on_upstream(root: &Path) -> bool {
    run(root, &["merge-base", "--is-ancestor", "HEAD", "@{u}"]).is_ok()
}

/// The local branches and the current one. `current` is `None` in detached
/// HEAD, where `git branch --show-current` prints nothing.
pub(crate) fn branches(root: &Path) -> Result<Branches, String> {
    let names = run(
        root,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
    )?;
    let all = String::from_utf8_lossy(&names)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    let current = run(root, &["branch", "--show-current"])?;
    let current = String::from_utf8_lossy(&current).trim().to_string();
    let current = if current.is_empty() {
        None
    } else {
        Some(current)
    };

    Ok(Branches {
        current,
        all,
        remotes: remote_branches(root)?,
    })
}

/// Checks out an existing branch. `git checkout`, not `git switch`, which
/// needs git 2.23 or newer.
pub(crate) fn switch_branch(root: &Path, name: &str) -> Result<(), String> {
    run(root, &["checkout", name]).map(|_| ())
}

/// Creates a branch without switching to it. Creating and switching are
/// deliberately separate operations here.
pub(crate) fn create_branch(root: &Path, name: &str) -> Result<(), String> {
    run(root, &["branch", name]).map(|_| ())
}

/// Deletes a branch. Git itself refuses a merge-unsafe delete with a
/// non-zero exit, which surfaces here as `Err` and leaves the branch in
/// place, the panel never forces this past the refusal.
pub(crate) fn delete_branch(root: &Path, name: &str) -> Result<(), String> {
    run(root, &["branch", "-d", name]).map(|_| ())
}

/// Remote-tracking branches as `remote/branch`, such as `origin/main`. The
/// symbolic `refs/remotes/<remote>/HEAD` is left out: git prints it as the
/// bare remote name, or as `<remote>/HEAD` on older versions.
pub(crate) fn remote_branches(root: &Path) -> Result<Vec<String>, String> {
    let out = run(
        root,
        &["for-each-ref", "--format=%(refname:short)", "refs/remotes/"],
    )?;
    Ok(String::from_utf8_lossy(&out)
        .lines()
        .map(str::trim)
        .filter(|l| l.contains('/') && !l.ends_with("/HEAD"))
        .map(str::to_string)
        .collect())
}

/// Fetches every configured remote.
pub(crate) fn fetch_all(root: &Path) -> Result<(), String> {
    run(root, &["fetch", "--all"]).map(|_| ())
}

/// Merges `source`, a local branch or a `remote/branch`, into the checked-out
/// branch. `--no-edit` keeps git from opening an editor for a merge commit. A
/// conflict exits non-zero and leaves the repository mid-merge for the
/// resolver.
pub(crate) fn merge(root: &Path, source: &str) -> Result<(), String> {
    run(root, &["merge", "--no-edit", source]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Mirrors the `tempdir()` helper in `apic-core/src/config.rs`, a fresh
    /// directory per test, made unique with the process id and a counter so
    /// parallel test runs never collide.
    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("apic-gui-git-{}-{}", std::process::id(), id));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// True when git is not usable on this machine, in which case the tests
    /// that need a real repository return early instead of failing.
    fn git_missing() -> bool {
        Command::new("git").arg("--version").output().is_err()
    }

    /// Sets up a repository with one committed file and local identity, not
    /// depending on the developer's global config or `init.defaultBranch`.
    fn init_repo(dir: &Path) {
        assert!(run(dir, &["init"]).is_ok());
        assert!(run(dir, &["config", "user.email", "test@example.com"]).is_ok());
        assert!(run(dir, &["config", "user.name", "Test User"]).is_ok());
        // Windows runners set core.autocrlf, which would check files out
        // with CRLF and break the content assertions.
        assert!(run(dir, &["config", "core.autocrlf", "false"]).is_ok());
        fs::write(dir.join("file.txt"), "original\n").unwrap();
        assert!(run(dir, &["add", "--", "file.txt"]).is_ok());
        assert!(commit(dir, "initial", false).is_ok());
    }

    #[test]
    fn full_workflow_against_a_real_repo() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);

        let root = discover(&dir).unwrap().unwrap();
        assert_eq!(
            fs::canonicalize(&root).unwrap(),
            fs::canonicalize(&dir).unwrap()
        );

        fs::write(dir.join("file.txt"), "changed\n").unwrap();
        let st = status(&root, &[]).unwrap();
        assert_eq!(st.inside.len(), 1);
        assert_eq!(st.inside[0].path, "file.txt");

        stage(&root, "file.txt").unwrap();
        let st = status(&root, &[]).unwrap();
        assert_eq!(
            st.inside[0].index,
            crate::features::git::domain::status::Change::Modified
        );
        assert_eq!(
            st.inside[0].worktree,
            crate::features::git::domain::status::Change::Unmodified
        );

        unstage(&root, "file.txt").unwrap();
        let st = status(&root, &[]).unwrap();
        assert_eq!(
            st.inside[0].index,
            crate::features::git::domain::status::Change::Unmodified
        );
        assert_eq!(
            st.inside[0].worktree,
            crate::features::git::domain::status::Change::Modified
        );

        assert_eq!(
            blob(&root, "HEAD", "file.txt").unwrap(),
            Some("original\n".to_string())
        );
        assert_eq!(blob(&root, "HEAD", "never-existed.txt").unwrap(), None);

        stage(&root, "file.txt").unwrap();
        commit(&root, "second", false).unwrap();

        let st = status(&root, &[]).unwrap();
        assert!(st.inside.is_empty());
    }

    #[test]
    fn diff_text_leaves_out_lines_far_from_the_change() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        assert!(run(&dir, &["init"]).is_ok());
        assert!(run(&dir, &["config", "user.email", "test@example.com"]).is_ok());
        assert!(run(&dir, &["config", "user.name", "Test User"]).is_ok());
        let before: String = (0..40).map(|i| format!("line{i}\n")).collect();
        fs::write(dir.join("file.txt"), &before).unwrap();
        assert!(run(&dir, &["add", "--", "file.txt"]).is_ok());
        assert!(commit(&dir, "initial", false).is_ok());

        let after = before.replace("line20\n", "CHANGED\n");
        fs::write(dir.join("file.txt"), &after).unwrap();

        let diff = diff_text(&dir, "file.txt", false).unwrap();
        // Git's default context is a few lines, so both ends of a 40-line
        // file fall outside the hunk. A future reader who puts `-U100000`
        // back sees this fail rather than a silently whole-file diff.
        assert!(diff.contains("@@"));
        assert!(diff.contains("-line20"));
        assert!(diff.contains("+CHANGED"));
        assert!(!diff.contains("line0\n"));
        assert!(!diff.contains("line39"));
    }

    #[test]
    fn discover_outside_a_repo_is_none() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        assert_eq!(discover(&dir).unwrap(), None);
    }

    #[test]
    fn branches_reports_the_initial_branch_as_current() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);

        let initial = String::from_utf8_lossy(&run(&dir, &["branch", "--show-current"]).unwrap())
            .trim()
            .to_string();

        let b = branches(&dir).unwrap();
        assert_eq!(b.current, Some(initial.clone()));
        assert!(b.all.contains(&initial));
    }

    #[test]
    fn create_branch_lists_it_without_switching() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let initial = String::from_utf8_lossy(&run(&dir, &["branch", "--show-current"]).unwrap())
            .trim()
            .to_string();

        create_branch(&dir, "feature").unwrap();

        let b = branches(&dir).unwrap();
        assert!(b.all.contains(&"feature".to_string()));
        assert_eq!(b.current, Some(initial));
    }

    #[test]
    fn switch_branch_changes_current() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        create_branch(&dir, "feature").unwrap();

        switch_branch(&dir, "feature").unwrap();

        let b = branches(&dir).unwrap();
        assert_eq!(b.current, Some("feature".to_string()));
    }

    #[test]
    fn delete_branch_removes_a_merged_branch() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        create_branch(&dir, "feature").unwrap();

        delete_branch(&dir, "feature").unwrap();

        let b = branches(&dir).unwrap();
        assert!(!b.all.contains(&"feature".to_string()));
    }

    #[test]
    fn branches_in_detached_head_reports_no_current() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);

        assert!(run(&dir, &["checkout", "--detach", "HEAD"]).is_ok());

        let b = branches(&dir).unwrap();
        assert_eq!(b.current, None);
    }

    #[test]
    fn delete_branch_with_unmerged_commits_is_refused() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let initial = String::from_utf8_lossy(&run(&dir, &["branch", "--show-current"]).unwrap())
            .trim()
            .to_string();

        create_branch(&dir, "feature").unwrap();
        switch_branch(&dir, "feature").unwrap();
        fs::write(dir.join("file.txt"), "unmerged change\n").unwrap();
        assert!(run(&dir, &["add", "--", "file.txt"]).is_ok());
        assert!(commit(&dir, "unmerged commit", false).is_ok());
        switch_branch(&dir, &initial).unwrap();

        assert!(delete_branch(&dir, "feature").is_err());

        let b = branches(&dir).unwrap();
        assert!(b.all.contains(&"feature".to_string()));
    }

    #[test]
    fn amend_replaces_the_last_commit_message_without_adding_a_commit() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let count = |dir: &Path| {
            String::from_utf8(run(dir, &["rev-list", "--count", "HEAD"]).unwrap()).unwrap()
        };
        let before = count(&dir);
        commit(&dir, "reworded", true).unwrap();
        assert_eq!(count(&dir), before);
        assert_eq!(last_commit_message(&dir).unwrap(), "reworded");
    }

    #[test]
    fn upstream_and_head_is_on_upstream_follow_a_bare_remote() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        assert_eq!(upstream(&dir), None);
        let remote = tempdir();
        assert!(run(&remote, &["init", "--bare"]).is_ok());
        let remote_path = remote.to_string_lossy().into_owned();
        assert!(run(&dir, &["remote", "add", "origin", &remote_path]).is_ok());
        assert!(run(&dir, &["push", "-u", "origin", "HEAD"]).is_ok());
        assert!(upstream(&dir).is_some_and(|u| u.starts_with("origin/")));
        assert!(head_is_on_upstream(&dir));
        fs::write(dir.join("file.txt"), "changed\n").unwrap();
        assert!(run(&dir, &["add", "--", "file.txt"]).is_ok());
        commit(&dir, "local only", false).unwrap();
        assert!(!head_is_on_upstream(&dir));
    }

    /// Commits `content` to `file.txt` on the checked-out branch.
    fn commit_file(dir: &Path, content: &str, message: &str) {
        fs::write(dir.join("file.txt"), content).unwrap();
        assert!(run(dir, &["add", "--", "file.txt"]).is_ok());
        assert!(commit(dir, message, false).is_ok());
    }

    #[test]
    fn merge_fast_forwards_a_local_branch_into_the_current_one() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let base = branches(&dir).unwrap().current.unwrap();
        create_branch(&dir, "feature").unwrap();
        switch_branch(&dir, "feature").unwrap();
        commit_file(&dir, "feature\n", "feature work");
        switch_branch(&dir, &base).unwrap();

        merge(&dir, "feature").unwrap();

        assert_eq!(
            fs::read_to_string(dir.join("file.txt")).unwrap(),
            "feature\n"
        );
    }

    #[test]
    fn checkout_then_merge_lands_on_the_target() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let base = branches(&dir).unwrap().current.unwrap();
        create_branch(&dir, "target").unwrap();
        commit_file(&dir, "base moved\n", "base work");

        switch_branch(&dir, "target").unwrap();
        merge(&dir, &base).unwrap();

        assert_eq!(branches(&dir).unwrap().current.as_deref(), Some("target"));
        assert_eq!(
            fs::read_to_string(dir.join("file.txt")).unwrap(),
            "base moved\n"
        );
    }

    #[test]
    fn checkout_blocked_by_uncommitted_changes_is_refused() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let base = branches(&dir).unwrap().current.unwrap();
        create_branch(&dir, "other").unwrap();
        switch_branch(&dir, "other").unwrap();
        commit_file(&dir, "other\n", "other work");
        switch_branch(&dir, &base).unwrap();
        fs::write(dir.join("file.txt"), "uncommitted\n").unwrap();

        assert!(switch_branch(&dir, "other").is_err());
        assert_eq!(branches(&dir).unwrap().current, Some(base));
    }

    #[test]
    fn conflicting_merge_fails_with_gits_message_and_leaves_the_conflict() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let base = branches(&dir).unwrap().current.unwrap();
        create_branch(&dir, "theirs").unwrap();
        commit_file(&dir, "ours\n", "ours");
        switch_branch(&dir, "theirs").unwrap();
        commit_file(&dir, "theirs\n", "theirs");
        switch_branch(&dir, &base).unwrap();

        let err = merge(&dir, "theirs").unwrap_err();

        assert!(err.to_lowercase().contains("conflict"), "got: {err}");
        let st = status(&dir, &[]).unwrap();
        assert!(st.find("file.txt").is_some());
    }

    #[test]
    fn remote_branches_lists_origin_branches_without_head() {
        if git_missing() {
            return;
        }
        let src = tempdir();
        init_repo(&src);
        let remote = tempdir();
        assert!(run(&remote, &["init", "--bare"]).is_ok());
        let remote_path = remote.to_string_lossy().into_owned();
        assert!(run(&src, &["remote", "add", "origin", &remote_path]).is_ok());
        assert!(run(&src, &["push", "origin", "HEAD"]).is_ok());
        let base = branches(&src).unwrap().current.unwrap();
        let clone = tempdir();
        assert!(run(&clone, &["clone", &remote_path, "."]).is_ok());

        let remotes = remote_branches(&clone).unwrap();

        assert!(
            remotes.contains(&format!("origin/{base}")),
            "got: {remotes:?}"
        );
        assert!(
            remotes
                .iter()
                .all(|r| r.contains('/') && !r.ends_with("/HEAD")),
            "got: {remotes:?}"
        );
        assert_eq!(branches(&clone).unwrap().remotes, remotes);
    }
}
