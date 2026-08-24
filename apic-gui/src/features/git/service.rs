//! Every `git` invocation, and nothing else. No state, no egui, no threads:
//! the threading lives in the app shell, which is what keeps this testable
//! against a real temp-dir repository.
//!
//! Nothing outside `tests` calls into this yet, wired in by `state` and
//! `view` in later tasks.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::features::git::model::{Status, parse_status};

/// Runs `git -C <root> <args>` and returns stdout bytes.
///
/// A spawn failure and a non-zero exit are distinguished: the first means git
/// is not installed, the second means git ran and refused, and the two have
/// different fixes.
fn run(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "git not found on PATH".to_string(),
            _ => format!("could not run git: {e}"),
        })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
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

/// The working tree, split against `scope` (a repo-relative prefix).
pub(crate) fn status(root: &Path, scope: &str) -> Result<Status, String> {
    let out = run(
        root,
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
    )?;
    Ok(parse_status(&out, scope))
}

/// The line diff for one path, staged or unstaged.
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

/// Reverts one tracked path to its staged content. Untracked files are not
/// offered, deleting bytes git has never seen has no recovery path.
pub(crate) fn discard(root: &Path, path: &str) -> Result<(), String> {
    run(root, &["checkout", "--", path]).map(|_| ())
}

/// Commits the index with `message`.
pub(crate) fn commit(root: &Path, message: &str) -> Result<(), String> {
    run(root, &["commit", "-m", message]).map(|_| ())
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
        fs::write(dir.join("file.txt"), "original\n").unwrap();
        assert!(run(dir, &["add", "--", "file.txt"]).is_ok());
        assert!(commit(dir, "initial").is_ok());
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
        let st = status(&root, "").unwrap();
        assert_eq!(st.inside.len(), 1);
        assert_eq!(st.inside[0].path, "file.txt");

        stage(&root, "file.txt").unwrap();
        let st = status(&root, "").unwrap();
        assert_eq!(
            st.inside[0].index,
            crate::features::git::model::Change::Modified
        );
        assert_eq!(
            st.inside[0].worktree,
            crate::features::git::model::Change::Unmodified
        );

        unstage(&root, "file.txt").unwrap();
        let st = status(&root, "").unwrap();
        assert_eq!(
            st.inside[0].index,
            crate::features::git::model::Change::Unmodified
        );
        assert_eq!(
            st.inside[0].worktree,
            crate::features::git::model::Change::Modified
        );

        assert_eq!(
            blob(&root, "HEAD", "file.txt").unwrap(),
            Some("original\n".to_string())
        );
        assert_eq!(blob(&root, "HEAD", "never-existed.txt").unwrap(), None);

        stage(&root, "file.txt").unwrap();
        commit(&root, "second").unwrap();

        let st = status(&root, "").unwrap();
        assert!(st.inside.is_empty());
    }

    #[test]
    fn discover_outside_a_repo_is_none() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        assert_eq!(discover(&dir).unwrap(), None);
    }
}
