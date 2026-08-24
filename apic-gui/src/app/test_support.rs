//! Scaffolding shared by the git wiring tests: a temp repository with a real
//! apic project inside it, and a bounded wait for a background git job.
//!
//! Nothing here asserts application behaviour. It builds the fixture and the
//! wait helper and proves they work, so a failure in a test that uses them is
//! a failure of the thing under test rather than of its scaffolding.

use std::path::{Path, PathBuf};
use std::process::Command;

use eframe::egui;

use crate::app::App;
use crate::app::state::ShellState;
use crate::features::contracts::state::ContractsState;
use crate::features::git::state::GitState;

/// Fresh directory per test, made unique with the process id and a counter,
/// mirroring `apic-core/src/config.rs:309-317`, so parallel test runs never
/// collide.
pub(crate) fn tempdir() -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let id = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("apic-gui-app-{}-{}", std::process::id(), id));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// True when `git` is usable on this machine. Every test that needs a real
/// repository returns early when this is false, matching
/// `features::git::service::tests`.
pub(crate) fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .expect("git spawns");
    assert!(status.success(), "git {args:?} failed");
}

/// Builds a temp directory holding a git repository and an apic project, and
/// returns the project root.
///
/// `working_dir` in `.apic/config.toml` names a subdirectory (`contracts`)
/// rather than `.`, the layout that made the original `.apic` scoping bug
/// invisible: with `working_dir` set to `.` the working dir and the repo
/// root are the same path, so a bug that confuses the two never shows up.
pub(crate) fn project_fixture() -> PathBuf {
    let root = tempdir();
    run_git(&root, &["init", "-b", "apic-test"]);
    run_git(&root, &["config", "user.email", "test@example.com"]);
    run_git(&root, &["config", "user.name", "Test User"]);

    let contracts_dir = root.join("contracts");
    std::fs::create_dir_all(&contracts_dir).unwrap();
    std::fs::write(
        contracts_dir.join("sample.json"),
        apic_core::template::DEFAULT,
    )
    .unwrap();

    let apic_dir = root.join(".apic");
    std::fs::create_dir_all(&apic_dir).unwrap();
    std::fs::write(
        apic_dir.join("config.toml"),
        "name = \"apic\"\nversion = \"0.1.0\"\n\n[root]\nworking_dir = \"contracts\"\n",
    )
    .unwrap();

    run_git(&root, &["add", "--all"]);
    run_git(&root, &["commit", "-m", "initial"]);

    root
}

/// Builds `App` with a struct literal rooted at `root`. Never calls
/// `App::new()`: that loads `Settings::load()`, which reads the developer's
/// real `~/.config/apic-gui/config.toml` and can open their last project.
///
/// Not yet called from this module: it is scaffolding for the wiring tests
/// that land in a later commit, so allow the otherwise-correct dead code
/// warning until then.
#[allow(dead_code)]
pub(crate) fn app_at(root: PathBuf) -> App {
    App {
        shell: ShellState {
            project_root: Some(root),
            ..Default::default()
        },
        contracts: ContractsState::default(),
        git: GitState::default(),
        pending_dialog: None,
        needs_git_refresh: false,
    }
}

/// Polls `app.poll_git(ctx)` until `app.git.pending` clears, with a short
/// sleep between iterations and a bounded iteration count.
///
/// The bound is the point of this helper: without it, a wiring regression
/// that never completes its job turns a red test into a hung suite, and a
/// hung suite is worse than no suite at all.
///
/// Not yet called from this module: it is scaffolding for the wiring tests
/// that land in a later commit, so allow the otherwise-correct dead code
/// warning until then.
#[allow(dead_code)]
pub(crate) fn settle(app: &mut App, ctx: &egui::Context) {
    const MAX_ITERS: u32 = 200;
    for _ in 0..MAX_ITERS {
        app.poll_git(ctx);
        if app.git.pending.is_none() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("git job did not complete within {MAX_ITERS} polls");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_builds_a_repository_with_a_readable_contract() {
        if !git_available() {
            return;
        }
        let root = project_fixture();

        let status = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["status", "--porcelain=v2"])
            .output()
            .expect("git spawns");
        assert!(status.status.success(), "git status must run cleanly");
        assert!(
            status.stdout.is_empty(),
            "the initial commit should leave nothing pending"
        );

        let contract = root.join("contracts").join("sample.json");
        assert!(contract.is_file(), "the fixture contract must be on disk");
    }
}
