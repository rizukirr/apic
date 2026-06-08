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
fn init_seeds_template_file() {
    let dir = init_project("seed_template");
    let template = dir.join(".apic/template.json");
    assert!(template.exists(), "init should seed .apic/template.json");
    let content = fs::read_to_string(&template).unwrap();
    // The built-in default's endpoint name.
    assert!(content.contains("endpoint-name"));
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
fn create_uses_customized_template() {
    let dir = init_project("custom_template");
    // Overwrite the seeded template with a valid contract that adds a header.
    let custom = r#"{
        "name": "custom",
        "method": "GET",
        "url": { "protocol": "https", "host": "api.example.com", "path": ["x"] },
        "headers": [ { "name": "device-id", "value": "{device_id}" } ],
        "responses": []
    }"#;
    fs::write(dir.join(".apic/template.json"), custom).unwrap();

    apic(&dir)
        .args(["create", "-f", "foo.json"])
        .assert()
        .success();

    let created = fs::read_to_string(dir.join("contracts/foo.json")).unwrap();
    assert!(created.contains("device-id"), "create should use the custom template");
}

#[test]
fn create_falls_back_when_template_malformed() {
    let dir = init_project("malformed_template");
    let broken = "{ not valid json";
    fs::write(dir.join(".apic/template.json"), broken).unwrap();

    apic(&dir)
        .args(["create", "-f", "bar.json"])
        .assert()
        .success()
        .stderr(predicate::str::contains("built-in template"));

    // Falls back to the built-in default for the new contract...
    let created = fs::read_to_string(dir.join("contracts/bar.json")).unwrap();
    assert!(created.contains("endpoint-name"));
    // ...and leaves the user's (broken) template untouched.
    let template = fs::read_to_string(dir.join(".apic/template.json")).unwrap();
    assert_eq!(template, broken);
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
fn read_resolves_path_extensionless_and_fuzzy_forms() {
    let dir = init_project("resolve");
    apic(&dir)
        .args(["create", "-f", "user/user.json"])
        .assert()
        .success();

    // Every form resolves to the same contract.
    for form in ["user/user.json", "user/user", "user.json", "user"] {
        apic(&dir)
            .args(["read", "-f", form])
            .assert()
            .success()
            .stdout(predicate::str::contains("/resource/{id}/action"));
    }
}

#[test]
fn open_resolves_and_succeeds() {
    let dir = init_project("open");
    apic(&dir)
        .args(["create", "-f", "user/user.json"])
        .assert()
        .success();

    for form in ["user/user.json", "user/user", "user"] {
        apic(&dir).args(["open", "-f", form]).assert().success();
    }
}

#[test]
fn open_missing_contract_fails() {
    let dir = init_project("open_missing");
    apic(&dir)
        .args(["create", "-f", "a.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["open", "-f", "zzz_no_match_zzz"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No contract found"));
}

#[test]
fn list_defaults_to_relative_paths() {
    let dir = init_project("list_rel");
    apic(&dir)
        .args(["create", "-f", "auth/login.json"])
        .assert()
        .success();
    // Default output is relative: the contract path, not the absolute prefix.
    apic(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("auth/login.json"))
        .stdout(predicate::str::contains(dir.to_string_lossy().to_string()).not());
}

#[test]
fn read_renders_accept_column_for_multipart_file_fields() {
    let dir = init_project("multipart");
    let contract = r#"{
        "name": "upload-avatar",
        "method": "POST",
        "url": { "protocol": "https", "host": "api.example.com", "path": ["user", "avatar"] },
        "headers": [
            { "name": "Content-Type", "value": "multipart/form-data" }
        ],
        "request": {
            "schema": [
                { "name": "avatar", "type": "file", "default": null,
                  "description": "Avatar image", "required": true,
                  "accept": "image/png, image/jpeg" },
                { "name": "caption", "type": "string", "default": null,
                  "description": "Optional caption", "required": false }
            ]
        },
        "responses": []
    }"#;
    fs::write(dir.join("contracts/upload.json"), contract).unwrap();

    apic(&dir)
        .args(["read", "-f", "upload"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ACCEPT"))
        .stdout(predicate::str::contains("image/png, image/jpeg"));

    // Contracts without accept fields keep the four-column table.
    apic(&dir)
        .args(["create", "-f", "plain.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["read", "-f", "plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ACCEPT").not());
}

#[test]
fn read_example_shows_raw_json_payloads() {
    let dir = init_project("example_view");
    let contract = r#"{
        "name": "login",
        "method": "POST",
        "url": { "protocol": "https", "host": "api.example.com", "path": ["auth", "login"] },
        "headers": [],
        "request": {
            "schema": [
                { "name": "username", "type": "string", "default": null,
                  "description": "Username", "required": true }
            ],
            "example": { "username": "rizukirr", "password": "123qweA@" }
        },
        "responses": [
            { "code": 200, "description": "ok",
              "example": { "status": 200, "message": "welcome" } },
            { "code": 401, "description": "denied",
              "schema": [
                  { "name": "status", "type": "int", "default": null,
                    "description": "Status", "required": true, "properties": null }
              ] }
        ]
    }"#;
    fs::write(dir.join("contracts/login.json"), contract).unwrap();

    // Default view renders the schema table with the example shown beneath
    // it, labeled, so structure and payload stay adjacent.
    apic(&dir)
        .args(["read", "-f", "login"])
        .assert()
        .success()
        .stdout(predicate::str::contains("NAME")) // schema table header
        .stdout(predicate::str::contains("Example:"))
        .stdout(predicate::str::contains("\"password\": \"123qweA@\""))
        // Sections without an example get no note in the default view.
        .stdout(predicate::str::contains("(no example provided)").not());

    // --example shows raw JSON payloads only; a response without one says so.
    apic(&dir)
        .args(["read", "-f", "login", "--example"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"username\": \"rizukirr\""))
        .stdout(predicate::str::contains("\"message\": \"welcome\""))
        .stdout(predicate::str::contains("(no example provided)"));
}

#[test]
fn read_example_only_contract_renders_example_by_default() {
    let dir = init_project("example_only");
    let contract = r#"{
        "name": "ping",
        "method": "GET",
        "url": { "protocol": "https", "host": "api.example.com", "path": ["ping"] },
        "headers": [],
        "request": { "example": { "probe": true } },
        "responses": [
            { "code": 200, "description": "pong", "example": { "pong": true } }
        ]
    }"#;
    fs::write(dir.join("contracts/ping.json"), contract).unwrap();

    // With no schema at all, the default view falls back to the examples.
    apic(&dir)
        .args(["read", "-f", "ping"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"probe\": true"))
        .stdout(predicate::str::contains("\"pong\": true"));
}

#[test]
fn list_filter_fuzzy_matches_contracts() {
    let dir = init_project("list_filter");
    apic(&dir)
        .args(["create", "-f", "user/user.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "auth/login.json"])
        .assert()
        .success();

    // Matching contracts are shown, non-matching ones are not.
    apic(&dir)
        .args(["list", "--filter", "user"])
        .assert()
        .success()
        .stdout(predicate::str::contains("user/user.json"))
        .stdout(predicate::str::contains("login").not());

    // A filter matching nothing prints nothing and still exits 0.
    apic(&dir)
        .args(["list", "--filter", "zzz_no_match"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
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

#[test]
fn read_ambiguous_basename_errors_when_not_a_tty() {
    let dir = init_project("ambiguous_read");
    apic(&dir)
        .args(["create", "-f", "user/user.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "auth/user.json"])
        .assert()
        .success();

    // Test stdin/stdout are pipes, not TTYs, so the picker must not run:
    // the command exits non-zero and lists every candidate.
    apic(&dir)
        .args(["read", "-f", "user"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is ambiguous"))
        .stderr(predicate::str::contains("user/user.json"))
        .stderr(predicate::str::contains("auth/user.json"))
        .stderr(predicate::str::contains("Specify the path"));

    // A precise path still resolves without any prompt.
    apic(&dir)
        .args(["read", "-f", "auth/user.json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("/resource/{id}/action"));
}

#[test]
fn open_ambiguous_basename_errors_when_not_a_tty() {
    let dir = init_project("ambiguous_open");
    apic(&dir)
        .args(["create", "-f", "user/user.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "auth/user.json"])
        .assert()
        .success();

    apic(&dir)
        .args(["open", "-f", "user"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is ambiguous"));
}

#[test]
fn validate_ambiguous_basename_errors_when_not_a_tty() {
    let dir = init_project("ambiguous_validate");
    apic(&dir)
        .args(["create", "-f", "user/user.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "auth/user.json"])
        .assert()
        .success();

    apic(&dir)
        .args(["validate", "-f", "user"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is ambiguous"));
}

#[test]
fn read_fuzzy_score_tie_errors_when_not_a_tty() {
    let dir = init_project("fuzzy_tie");
    // Different basenames, identical structure: "usr" is not a basename
    // match for either, and both fuzzy-score identically -> ambiguous.
    apic(&dir)
        .args(["create", "-f", "a/user-a.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "b/user-b.json"])
        .assert()
        .success();

    apic(&dir)
        .args(["read", "-f", "usr"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is ambiguous"))
        .stderr(predicate::str::contains("user-a.json"))
        .stderr(predicate::str::contains("user-b.json"));
}

#[test]
fn list_piped_output_stays_flat_without_tree_chars() {
    let dir = init_project("list_piped_flat");
    apic(&dir)
        .args(["create", "-f", "user/profile/user.json"])
        .assert()
        .success();

    // assert_cmd captures stdout through a pipe (not a TTY), so this pins the
    // scriptable contract: one path per line, no box-drawing characters.
    apic(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("user/profile/user.json"))
        .stdout(predicate::str::contains("├──").not())
        .stdout(predicate::str::contains("└──").not());

    // Same for --absolute: flat absolute paths, no tree.
    apic(&dir)
        .args(["list", "--absolute", "true"])
        .assert()
        .success()
        .stdout(predicate::str::contains(dir.to_string_lossy().to_string()))
        .stdout(predicate::str::contains("├──").not());
}

#[test]
fn list_filter_does_not_match_across_path_components() {
    let dir = init_project("list_filter_component");
    for f in ["user/user.json", "user/upload.json", "auth/user.json"] {
        apic(&dir).args(["create", "-f", f]).assert().success();
    }

    // "user.json" must not match user/upload.json by borrowing "user" from
    // the directory and ".json" from the extension.
    apic(&dir)
        .args(["list", "--filter", "user.json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("user/user.json"))
        .stdout(predicate::str::contains("auth/user.json"))
        .stdout(predicate::str::contains("upload.json").not());
}
