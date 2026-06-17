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
    let template = dir.join(".apic/template/convention.json");
    assert!(
        template.exists(),
        "init should seed .apic/template/convention.json"
    );
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
        .args(["create", "--editor", "true", "-f", "auth/login.json"])
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
fn remove_deletes_a_resolved_contract() {
    let dir = init_project("remove");
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "auth/login.json"])
        .assert()
        .success();
    assert!(dir.join("contracts/auth/login.json").exists());

    // Non-interactive (no TTY) proceeds without prompting.
    apic(&dir)
        .args(["remove", "-f", "login"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));
    assert!(!dir.join("contracts/auth/login.json").exists());
}

#[test]
fn remove_reports_when_nothing_matches() {
    let dir = init_project("remove_missing");
    apic(&dir)
        .args(["remove", "-f", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No contract found"));
}

#[test]
fn create_refuses_to_overwrite() {
    let dir = init_project("overwrite");
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "x.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "x.json"])
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
    fs::write(dir.join(".apic/template/convention.json"), custom).unwrap();

    apic(&dir)
        .args(["create", "--editor", "true", "-f", "foo.json"])
        .assert()
        .success();

    let created = fs::read_to_string(dir.join("contracts/foo.json")).unwrap();
    assert!(
        created.contains("device-id"),
        "create should use the custom template"
    );
}

#[test]
fn create_fails_when_template_malformed() {
    let dir = init_project("malformed_template");
    let broken = "{ not valid json";
    fs::write(dir.join(".apic/template/convention.json"), broken).unwrap();

    apic(&dir)
        .args(["create", "--editor", "true", "-f", "bar.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not valid JSON"));

    // No contract is created when the template is invalid...
    assert!(!dir.join("contracts/bar.json").exists());
    // ...and the user's (broken) template is left untouched.
    let template = fs::read_to_string(dir.join(".apic/template/convention.json")).unwrap();
    assert_eq!(template, broken);
}

#[test]
fn create_rejects_path_traversal() {
    let dir = init_project("traversal");
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "../../escape.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside the working directory"));
    assert!(!dir.join("escape.json").exists());
}

#[test]
fn read_unknown_contract_reports_not_found() {
    let dir = init_project("read_missing");
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "a.json"])
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
        .args(["create", "--editor", "true", "-f", "good.json"])
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
fn validate_template_reports_ok_fail_and_rejects_filename() {
    let dir = init_project("validate_template");

    // The seeded default template is valid: exits 0 with `ok`.
    apic(&dir)
        .args(["validate", "--template"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok"));

    // A malformed template makes `validate --template` exit non-zero with `FAIL`.
    fs::write(dir.join(".apic/template/convention.json"), "{ not json").unwrap();
    apic(&dir)
        .args(["validate", "--template"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("FAIL"));

    // `--template` and `--find` are mutually exclusive.
    apic(&dir)
        .args(["validate", "--template", "--find", "foo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn validate_folder_query_checks_every_contract_recursively() {
    let dir = init_project("validate_folder");
    // Two contracts under auth/ (one nested deeper) and one outside it.
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "auth/login.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "auth/sub/refresh.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "user/user.json"])
        .assert()
        .success();

    // A trailing-slash query validates only the contracts under that folder,
    // at any depth — the user/ contract is not counted.
    apic(&dir)
        .args(["validate", "-f", "auth/"])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth/sub/refresh.json"))
        .stdout(predicate::str::contains("2 passed, 0 failed"));

    // A non-existent folder query fails clearly.
    apic(&dir)
        .args(["validate", "-f", "nope/"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No such folder"));
}

#[test]
fn read_resolves_path_extensionless_and_fuzzy_forms() {
    let dir = init_project("resolve");
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "user/user.json"])
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
        .args(["create", "--editor", "true", "-f", "user/user.json"])
        .assert()
        .success();

    for form in ["user/user.json", "user/user", "user"] {
        apic(&dir)
            .args(["open", "--editor", "true", "-f", form])
            .assert()
            .success();
    }
}

#[test]
fn open_missing_contract_fails() {
    let dir = init_project("open_missing");
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "a.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["open", "--editor", "true", "-f", "zzz_no_match_zzz"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No contract found"));
}

#[test]
fn list_defaults_to_relative_paths() {
    let dir = init_project("list_rel");
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "auth/login.json"])
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
        .args(["create", "--editor", "true", "-f", "plain.json"])
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
fn read_example_only_contract_shows_none_by_default() {
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

    // With no schema, the default view shows `(none)` and does not fall back to
    // the examples — those are reachable via --example.
    apic(&dir)
        .args(["read", "-f", "ping"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(none)"))
        .stdout(predicate::str::contains("\"probe\": true").not())
        .stdout(predicate::str::contains("\"pong\": true").not());

    // --example still renders the raw payloads.
    apic(&dir)
        .args(["read", "-f", "ping", "--example"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"probe\": true"))
        .stdout(predicate::str::contains("\"pong\": true"));
}

#[test]
fn list_filter_fuzzy_matches_contracts() {
    let dir = init_project("list_filter");
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "user/user.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "auth/login.json"])
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
        .args(["create", "--editor", "true", "-f", "user/user.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "auth/user.json"])
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
        .args(["create", "--editor", "true", "-f", "user/user.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "auth/user.json"])
        .assert()
        .success();

    apic(&dir)
        .args(["open", "--editor", "true", "-f", "user"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is ambiguous"));
}

#[test]
fn validate_ambiguous_basename_errors_when_not_a_tty() {
    let dir = init_project("ambiguous_validate");
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "user/user.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "auth/user.json"])
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
        .args(["create", "--editor", "true", "-f", "a/user-a.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "--editor", "true", "-f", "b/user-b.json"])
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
        .args(["create", "--editor", "true", "-f", "user/profile/user.json"])
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

    // Same for --absolute: flat absolute paths, no tree. The tool emits
    // canonicalized, forward-slashed paths; on Windows that long-name/verbatim
    // form differs from `dir`'s raw string, so assert on the stable project-dir
    // leaf and the contract tail rather than the full temp-dir prefix.
    apic(&dir)
        .args(["list", "--absolute", "true"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            dir.file_name().unwrap().to_string_lossy().to_string(),
        ))
        .stdout(predicate::str::contains("user/profile/user.json"))
        .stdout(predicate::str::contains("├──").not());
}

#[test]
fn list_filter_does_not_match_across_path_components() {
    let dir = init_project("list_filter_component");
    for f in ["user/user.json", "user/upload.json", "auth/user.json"] {
        apic(&dir)
            .args(["create", "--editor", "true", "-f", f])
            .assert()
            .success();
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

#[test]
fn create_template_authors_named_template() {
    let dir = init_project("author_template");
    apic(&dir)
        .args(["create", "--template", "billing", "--editor", "true"])
        .assert()
        .success();
    assert!(dir.join(".apic/template/billing.json").exists());
}

#[test]
fn create_contract_uses_named_template() {
    let dir = init_project("use_named_template");
    // A second template with a distinctive header; convention.json stays seeded.
    let graphql = r#"{
        "name": "gql",
        "method": "POST",
        "url": { "protocol": "https", "host": "api.example.com", "path": ["graphql"] },
        "headers": [ { "name": "x-gql", "value": "1" } ],
        "responses": []
    }"#;
    fs::write(dir.join(".apic/template/graphql.json"), graphql).unwrap();

    apic(&dir)
        .args([
            "create",
            "-f",
            "q.json",
            "--use-template",
            "graphql",
            "--editor",
            "true",
        ])
        .assert()
        .success();

    let created = fs::read_to_string(dir.join("contracts/q.json")).unwrap();
    assert!(
        created.contains("x-gql"),
        "contract should be seeded from the graphql template"
    );
}

#[test]
fn create_template_seeds_from_existing() {
    let dir = init_project("author_from_existing");
    // Customize convention.json with a distinctive header.
    let custom = r#"{
        "name": "custom",
        "method": "GET",
        "url": { "protocol": "https", "host": "api.example.com", "path": ["x"] },
        "headers": [ { "name": "device-id", "value": "{device_id}" } ],
        "responses": []
    }"#;
    fs::write(dir.join(".apic/template/convention.json"), custom).unwrap();

    apic(&dir)
        .args([
            "create",
            "--template",
            "billing",
            "--use-template",
            "convention",
            "--editor",
            "true",
        ])
        .assert()
        .success();

    let authored = fs::read_to_string(dir.join(".apic/template/billing.json")).unwrap();
    assert!(
        authored.contains("device-id"),
        "new template should be seeded from convention"
    );
}

#[test]
fn create_template_refuses_overwrite() {
    let dir = init_project("template_overwrite");
    apic(&dir)
        .args(["create", "--template", "dup", "--editor", "true"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "--template", "dup", "--editor", "true"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn create_template_rejects_path_traversal() {
    let dir = init_project("template_traversal");
    apic(&dir)
        .args(["create", "--template", "../escape", "--editor", "true"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid template name"));
    assert!(!dir.join(".apic/escape.json").exists());
}

#[test]
fn create_contract_falls_back_to_convention_when_ambiguous_non_tty() {
    let dir = init_project("fallback_convention");
    // Mark convention.json so we can detect it was the seed.
    let conv = r#"{
        "name": "conv",
        "method": "GET",
        "url": { "protocol": "https", "host": "api.example.com", "path": ["c"] },
        "headers": [ { "name": "x-conv", "value": "1" } ],
        "responses": []
    }"#;
    fs::write(dir.join(".apic/template/convention.json"), conv).unwrap();
    fs::write(
        dir.join(".apic/template/graphql.json"),
        r#"{ "headers": [ { "name": "x-gql", "value": "1" } ] }"#,
    )
    .unwrap();

    // No --use-template + multiple templates + non-interactive -> convention.json.
    apic(&dir)
        .args(["create", "-f", "q.json", "--editor", "true"])
        .assert()
        .success();

    let created = fs::read_to_string(dir.join("contracts/q.json")).unwrap();
    assert!(created.contains("x-conv"));
    assert!(!created.contains("x-gql"));
}

#[test]
fn create_unknown_use_template_errors_with_available() {
    let dir = init_project("bad_use_template");
    fs::write(dir.join(".apic/template/graphql.json"), "{}").unwrap();
    apic(&dir)
        .args([
            "create",
            "-f",
            "q.json",
            "--use-template",
            "zzz_no_match",
            "--editor",
            "true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no template matching"))
        .stderr(predicate::str::contains("convention.json"));
}

#[test]
fn remove_template_deletes_named_template() {
    let dir = init_project("remove_template");
    // Author a second template, then remove it by (fuzzy) name.
    apic(&dir)
        .args(["create", "--template", "billing", "--editor", "true"])
        .assert()
        .success();
    assert!(dir.join(".apic/template/billing.json").exists());

    apic(&dir)
        .args(["remove", "--template", "billing"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));
    assert!(!dir.join(".apic/template/billing.json").exists());
}

#[test]
fn remove_template_can_remove_convention_default() {
    let dir = init_project("remove_template_default");
    assert!(dir.join(".apic/template/convention.json").exists());
    apic(&dir)
        .args(["remove", "--template", "convention"])
        .assert()
        .success();
    assert!(!dir.join(".apic/template/convention.json").exists());
}

#[test]
fn remove_unknown_template_errors_with_available() {
    let dir = init_project("remove_template_missing");
    apic(&dir)
        .args(["remove", "--template", "zzz_no_match"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no template matching"))
        .stderr(predicate::str::contains("convention.json"));
}
