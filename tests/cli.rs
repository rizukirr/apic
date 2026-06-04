//! End-to-end tests that drive the actual `apic` binary.
//!
//! Each test runs in its own temporary project directory so they can execute
//! in parallel without interfering. The `EDITOR` is set to `true` (a no-op
//! that exits 0) so `create` never blocks on an interactive editor.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

/// A throwaway project directory, removed when the test starts.
fn fresh_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("apic_e2e_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn apic(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("apic").unwrap();
    cmd.current_dir(dir)
        .env("EDITOR", "true")
        .env_remove("VISUAL");
    cmd
}

/// Initializes a project with a `contracts/` working directory.
fn init_project(tag: &str) -> PathBuf {
    let dir = fresh_dir(tag);
    fs::create_dir_all(dir.join("contracts")).unwrap();
    apic(&dir)
        .args(["init", "--set-dir", "contracts"])
        .assert()
        .success();
    dir
}

#[test]
fn init_creates_config_and_refuses_second_init() {
    let dir = init_project("init");
    assert!(dir.join(".apic/config.toml").exists());
    apic(&dir)
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Already initialized"));
}

#[test]
fn commands_outside_a_project_report_not_initialized() {
    let dir = fresh_dir("noproject");
    apic(&dir)
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not initialized"));
}

#[test]
fn create_scaffolds_then_read_renders_it() {
    let dir = init_project("create_read");
    apic(&dir)
        .args(["create", "-f", "auth/login.json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created"));
    assert!(dir.join("contracts/auth/login.json").exists());

    apic(&dir)
        .args(["read", "-f", "login"])
        .assert()
        .success()
        .stdout(predicate::str::contains("/resource/{id}/action"));
}

#[test]
fn create_refuses_to_overwrite() {
    let dir = init_project("overwrite");
    apic(&dir)
        .args(["create", "-f", "x.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "x.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn create_rejects_path_traversal() {
    let dir = init_project("traversal");
    apic(&dir)
        .args(["create", "-f", "../../escape.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside the working directory"));
    assert!(!dir.join("escape.json").exists());
}

#[test]
fn read_unknown_contract_reports_not_found() {
    let dir = init_project("read_missing");
    apic(&dir)
        .args(["create", "-f", "a.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["read", "-f", "zzz_no_match_zzz"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No contract found"));
}

#[test]
fn validate_passes_for_valid_and_fails_for_broken() {
    let dir = init_project("validate");
    apic(&dir)
        .args(["create", "-f", "good.json"])
        .assert()
        .success();

    // A valid contract validates and exits 0.
    apic(&dir)
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("ok"))
        .stdout(predicate::str::contains("0 failed"));

    // A malformed contract makes validate exit non-zero.
    fs::write(dir.join("contracts/broken.json"), "{ not json").unwrap();
    apic(&dir)
        .arg("validate")
        .assert()
        .failure()
        .stdout(predicate::str::contains("FAIL"));
}

#[test]
fn config_set_dir_rejects_missing_directory() {
    let dir = init_project("setdir");
    apic(&dir)
        .args(["config", "--set-dir", "does-not-exist"])
        .assert()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn version_matches_package_version() {
    let dir = fresh_dir("version");
    apic(&dir)
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}
