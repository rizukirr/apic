//! End-to-end tests that drive the actual `apic` binary.
//!
//! Each test runs in its own temporary project directory so they can execute
//! in parallel without interfering. The `EDITOR` is set to `true` (a no-op
//! that exits 0) so `open` never blocks on an interactive editor.

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
        .stdout(predicate::str::contains("created"));
    assert!(dir.join("contracts/auth/login.json").exists());

    apic(&dir)
        .args(["read", "-f", "login"])
        .assert()
        .success()
        .stdout(predicate::str::contains("api.example.com"));
}

#[test]
fn remove_deletes_a_resolved_contract() {
    let dir = init_project("remove");
    apic(&dir)
        .args(["create", "-f", "auth/login.json"])
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
        "url": "https://api.example.com/x",
        "headers": [ { "name": "device-id", "value": "{device_id}" } ],
        "responses": []
    }"#;
    fs::write(dir.join(".apic/template/convention.json"), custom).unwrap();

    apic(&dir)
        .args(["create", "-f", "foo.json"])
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
        .args(["create", "-f", "bar.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not valid JSON"));

    assert!(!dir.join("contracts/bar.json").exists());
    // The user's broken template must be left untouched on failure.
    let template = fs::read_to_string(dir.join(".apic/template/convention.json")).unwrap();
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
        .code(2)
        .stderr(predicate::str::contains("No contract found"));
}

#[test]
fn validate_passes_for_valid_and_fails_for_broken() {
    let dir = init_project("validate");
    apic(&dir)
        .args(["create", "-f", "good.json"])
        .assert()
        .success();

    apic(&dir)
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("ok"))
        .stdout(predicate::str::contains("0 failed"));

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

    apic(&dir)
        .args(["validate", "--template"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok"));

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
        .args(["create", "-f", "auth/login.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "auth/sub/refresh.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "user/user.json"])
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
        .args(["create", "-f", "user/user.json"])
        .assert()
        .success();

    // Every form resolves to the same contract.
    for form in ["user/user.json", "user/user", "user.json", "user"] {
        apic(&dir)
            .args(["read", "-f", form])
            .assert()
            .success()
            .stdout(predicate::str::contains("api.example.com"));
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
        .args(["create", "-f", "a.json"])
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
        .args(["create", "-f", "auth/login.json"])
        .assert()
        .success();
    apic(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("auth/login.json"))
        .stdout(predicate::str::contains(dir.to_string_lossy().to_string()).not());
}

#[test]
fn read_shows_raw_json_body_examples() {
    let dir = init_project("example_view");
    let contract = r#"{
        "name": "login",
        "method": "POST",
        "url": "https://api.example.com/auth/login",
        "headers": [],
        "request": { "username": "rizukirr", "password": "123qweA@" },
        "responses": [
            { "code": 200, "description": "ok",
              "schema": { "status": 200, "message": "welcome" } },
            { "code": 401, "description": "denied" }
        ]
    }"#;
    fs::write(dir.join("contracts/login.json"), contract).unwrap();

    // A body renders as its raw JSON example; a response without one says so.
    apic(&dir)
        .args(["read", "-f", "login"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"password\": \"123qweA@\""))
        .stdout(predicate::str::contains("\"message\": \"welcome\""))
        .stdout(predicate::str::contains("(no example provided)"));
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
        .stdout(predicate::str::contains("api.example.com"));
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
        .args(["open", "--editor", "true", "-f", "user"])
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

#[test]
fn create_template_authors_named_template() {
    let dir = init_project("author_template");
    apic(&dir)
        .args(["create", "--template", "billing"])
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
        "url": "https://api.example.com/graphql",
        "headers": [ { "name": "x-gql", "value": "1" } ],
        "responses": []
    }"#;
    fs::write(dir.join(".apic/template/graphql.json"), graphql).unwrap();

    apic(&dir)
        .args(["create", "-f", "q.json", "--use-template", "graphql"])
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
        "url": "https://api.example.com/x",
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
        .args(["create", "--template", "dup"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "--template", "dup"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn create_template_rejects_path_traversal() {
    let dir = init_project("template_traversal");
    apic(&dir)
        .args(["create", "--template", "../escape"])
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
        "url": "https://api.example.com/c",
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
        .args(["create", "-f", "q.json"])
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
        .args(["create", "-f", "q.json", "--use-template", "zzz_no_match"])
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
        .args(["create", "--template", "billing"])
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

#[test]
fn validate_template_checks_all_templates_and_fails_on_any_invalid() {
    let dir = init_project("validate_all_templates");
    // convention.json is valid (seeded); add a malformed second template.
    fs::write(dir.join(".apic/template/broken.json"), "{ not json").unwrap();
    apic(&dir)
        .args(["validate", "--template"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("FAIL"))
        .stdout(predicate::str::contains("broken.json"));
}

#[test]
fn validate_template_passes_when_all_templates_valid() {
    let dir = init_project("validate_all_valid");
    // A second, valid template alongside the seeded convention.json.
    apic(&dir)
        .args(["create", "--template", "mobile"])
        .assert()
        .success();
    apic(&dir)
        .args(["validate", "--template"])
        .assert()
        .success()
        .stdout(predicate::str::contains("convention.json"))
        .stdout(predicate::str::contains("mobile.json"));
}

#[test]
fn validate_template_reports_none_when_dir_empty() {
    let dir = init_project("validate_no_templates");
    fs::remove_file(dir.join(".apic/template/convention.json")).unwrap();
    apic(&dir)
        .args(["validate", "--template"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No project template found"));
}

#[test]
fn create_with_curl_flags_writes_a_readable_contract() {
    let dir = init_project("create_curl");
    fs::write(dir.join("body.json"), r#"{"username":"rizukirr"}"#).unwrap();
    fs::write(dir.join("ok.json"), r#"{"token":"abc"}"#).unwrap();
    apic(&dir)
        .args([
            "create",
            "-f",
            "auth/login",
            "-X",
            "POST",
            "https://api/auth/login?remember=true",
            "-H",
            "Content-Type: application/json",
            "-d",
            "@body.json",
            "--response",
            "200:OK",
            "--response-body",
            "200",
            "@ok.json",
            "--response-header",
            "200",
            "X-Rate: 10",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("created"));
    assert!(dir.join("contracts/auth/login.json").exists());

    apic(&dir)
        .args(["read", "-f", "auth/login"])
        .assert()
        .success()
        .stdout(predicate::str::contains("POST"))
        .stdout(predicate::str::contains("https://api/auth/login"))
        .stdout(predicate::str::contains("remember"))
        .stdout(predicate::str::contains("Content-Type"))
        .stdout(predicate::str::contains("rizukirr"))
        .stdout(predicate::str::contains("\"token\": \"abc\""));

    // `read` does not render response headers, so check the file for those.
    let written = fs::read_to_string(dir.join("contracts/auth/login.json")).unwrap();
    assert!(written.contains("\"name\": \"login\""), "{written}");
    assert!(written.contains("X-Rate"), "{written}");
}

#[test]
fn edit_changes_only_the_passed_fields() {
    let dir = init_project("edit_only");
    apic(&dir)
        .args([
            "create",
            "-f",
            "auth/login.json",
            "-H",
            "Accept: */*",
            "-H",
            "DeviceID: 42",
            "-d",
            r#"{"user":"a"}"#,
        ])
        .assert()
        .success();
    let before = fs::read_to_string(dir.join("contracts/auth/login.json")).unwrap();

    apic(&dir)
        .args(["edit", "-f", "login", "-H", "X-Trace: 1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated"));

    let after = fs::read_to_string(dir.join("contracts/auth/login.json")).unwrap();
    let before: serde_json::Value = serde_json::from_str(&before).unwrap();
    let mut after: serde_json::Value = serde_json::from_str(&after).unwrap();
    let headers = after["headers"].as_array_mut().unwrap();
    let added = headers.pop().unwrap();
    assert_eq!(added["name"], "X-Trace");
    assert_eq!(added["value"], "1");
    assert_eq!(after, before);

    apic(&dir)
        .args(["edit", "-f", "login", "-H", "DeviceID:"])
        .assert()
        .success();
    let removed = fs::read_to_string(dir.join("contracts/auth/login.json")).unwrap();
    assert!(!removed.contains("DeviceID"));
    assert!(removed.contains("X-Trace"));
    assert!(removed.contains("\"user\": \"a\""));
}

/// Runs a failing `edit` and asserts the contract file is byte-for-byte
/// unchanged.
fn assert_edit_fails_unchanged(dir: &Path, rel: &str, args: &[&str], message: &str) {
    let path = dir.join("contracts").join(rel);
    let before = fs::read(&path).unwrap();
    apic(dir)
        .args(["edit", "-f", rel])
        .args(args)
        .assert()
        .failure()
        .stderr(predicate::str::contains(message));
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn edit_data_on_a_multipart_body_fails_and_writes_nothing() {
    let dir = init_project("edit_multipart");
    apic(&dir)
        .args([
            "create",
            "-f",
            "upload.json",
            "-F",
            "avatar=@me.png;type=image/png",
        ])
        .assert()
        .success();
    assert_edit_fails_unchanged(&dir, "upload.json", &["-d", "{}"], "apic open");
}

#[test]
fn edit_unknown_response_code_fails_and_writes_nothing() {
    let dir = init_project("edit_unknown_code");
    apic(&dir)
        .args(["create", "-f", "a.json", "--response", "200:OK"])
        .assert()
        .success();
    assert_edit_fails_unchanged(
        &dir,
        "a.json",
        &["--response-body", "500", "{}"],
        "--response 500",
    );
    assert_edit_fails_unchanged(
        &dir,
        "a.json",
        &["--response-header", "404", "X-A: b"],
        "--response 404",
    );
}

#[test]
fn edit_invalid_result_writes_nothing() {
    let dir = init_project("edit_invalid");
    // Two request bodies: the flags apply, but the result cannot validate.
    let broken = r#"{
        "name": "x",
        "method": "POST",
        "url": "https://api.example.com/x",
        "headers": [],
        "request": { "a": 1 },
        "multipart": [ { "name": "f", "filename": "f.png" } ],
        "responses": []
    }"#;
    fs::write(dir.join("contracts/x.json"), broken).unwrap();
    assert_edit_fails_unchanged(&dir, "x.json", &["-H", "A: b"], "one body");
}

#[test]
fn ambiguous_name_lists_candidates() {
    let dir = init_project("ambiguous_edit");
    for f in ["user/user.json", "auth/user.json"] {
        apic(&dir).args(["create", "-f", f]).assert().success();
    }
    apic(&dir)
        .args(["edit", "-f", "user", "-H", "A: b"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is ambiguous"))
        .stderr(predicate::str::contains("user/user.json"))
        .stderr(predicate::str::contains("auth/user.json"));
}

#[test]
fn remove_yes_deletes_without_asking() {
    let dir = init_project("remove_yes");
    apic(&dir)
        .args(["create", "-f", "a.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["remove", "-y", "-f", "a"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));
    assert!(!dir.join("contracts/a.json").exists());
}

/// Serves one request on 127.0.0.1 with the raw `reply`, handing back the raw
/// request bytes.
fn serve_once(reply: &'static str) -> (String, std::sync::mpsc::Receiver<Vec<u8>>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
                let len = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .map_or(0, |v| v.trim().parse::<usize>().unwrap());
                if buf.len() >= end + 4 + len {
                    break;
                }
            }
        }
        stream.write_all(reply.as_bytes()).unwrap();
        tx.send(buf).unwrap();
    });
    (format!("http://{addr}"), rx)
}

const SEND_OK: &str = "HTTP/1.1 201 Created\r\nX-Test: yes\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";

fn send_project(tag: &str) -> PathBuf {
    let dir = init_project(tag);
    fs::write(
        dir.join("contracts/get_user.json"),
        r#"{"name":"get user","method":"GET","url":"$base/users/{id}",
            "headers":[{"name":"Authorization","value":"Bearer $token"}],"responses":[]}"#,
    )
    .unwrap();
    fs::create_dir_all(dir.join(".apic/env")).unwrap();
    dir
}

#[test]
fn send_fills_env_with_local_override_and_var_flag() {
    let dir = send_project("send_env");
    let (base, rx) = serve_once(SEND_OK);
    fs::write(
        dir.join(".apic/env/dev.toml"),
        format!("base = \"{base}\"\ntoken = \"shared\"\nid = \"1\"\n"),
    )
    .unwrap();
    fs::write(dir.join(".apic/env/dev.local.toml"), "token = \"secret\"\n").unwrap();
    apic(&dir)
        .args([
            "send", "-f", "get_user", "--env", "dev", "--var", "id=5", "-i",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("HTTP 201 Created"))
        .stdout(predicate::str::contains("x-test: yes"))
        .stdout(predicate::str::contains("{\"ok\":true}"))
        .stderr(predicate::str::contains("201"));
    let raw = String::from_utf8(rx.recv().unwrap()).unwrap();
    assert!(raw.starts_with("GET /users/5 HTTP/1.1\r\n"), "{raw}");
    assert!(
        raw.to_ascii_lowercase()
            .contains("authorization: bearer secret\r\n"),
        "{raw}"
    );
    let ignore = fs::read_to_string(dir.join(".apic/.gitignore")).unwrap();
    assert!(ignore.lines().any(|l| l == "env/*.local.toml"));
}

#[test]
fn send_refuses_unfilled_path_param_and_warns_on_undefined() {
    let dir = send_project("send_refuse");
    fs::write(
        dir.join(".apic/env/dev.toml"),
        "base = \"http://127.0.0.1:9\"\n",
    )
    .unwrap();
    apic(&dir)
        .args(["send", "-f", "get_user", "--env", "dev"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--var id="))
        .stderr(predicate::str::contains("$token"));
}

#[test]
fn send_unknown_env_lists_available() {
    let dir = send_project("send_unknown_env");
    fs::write(dir.join(".apic/env/dev.toml"), "").unwrap();
    apic(&dir)
        .args(["send", "-f", "get_user", "--env", "prod"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("dev"));
}

#[test]
fn env_lists_names_with_local_flag_and_no_values() {
    let dir = init_project("env_list");
    fs::create_dir_all(dir.join(".apic/env")).unwrap();
    fs::write(dir.join(".apic/env/dev.toml"), "token = \"shh\"\n").unwrap();
    fs::write(dir.join(".apic/env/dev.local.toml"), "token = \"shh2\"\n").unwrap();
    fs::write(dir.join(".apic/env/prod.toml"), "").unwrap();
    apic(&dir)
        .args(["env"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dev (+local)"))
        .stdout(predicate::str::contains("prod"))
        .stdout(predicate::str::contains("shh").not());
}

#[test]
fn vars_convert_is_dry_until_write() {
    let dir = init_project("vars_convert");
    let file = dir.join("contracts/a.json");
    let original = r#"{"name":"a","method":"GET","url":"{{baseUrl}}/x","headers":[{"name":"A","value":"Bearer {{tok}}"}],"responses":[]}"#;
    fs::write(&file, original).unwrap();
    apic(&dir)
        .args(["vars", "--convert"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a.json"));
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    apic(&dir)
        .args(["vars", "--convert", "--write"])
        .assert()
        .success();
    let converted = fs::read_to_string(&file).unwrap();
    assert!(converted.contains("\"$baseUrl/x\""), "{converted}");
    assert!(converted.contains("Bearer $tok"), "{converted}");
}

/// Parses stdout as exactly one JSON document.
fn json_out(out: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).expect("stdout is one JSON document")
}

#[test]
fn unknown_flag_is_a_usage_error_with_exit_1() {
    let dir = fresh_dir("json_usage");
    apic(&dir).args(["list", "--nope"]).assert().code(1);
    let out = apic(&dir)
        .args(["list", "--nope", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(json_out(&out)["error"]["kind"], "usage");
}

#[test]
fn outside_a_project_is_not_initialized_with_exit_1() {
    let dir = fresh_dir("json_noproject");
    apic(&dir).arg("list").assert().code(1);
    let out = apic(&dir).args(["list", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(json_out(&out)["error"]["kind"], "not_initialized");
}

#[test]
fn read_with_no_match_is_not_found_with_exit_2() {
    let dir = init_project("json_read_missing");
    let out = apic(&dir)
        .args(["read", "-f", "zzz_no_match_zzz", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json_out(&out)["error"]["kind"], "not_found");
}

#[test]
fn ambiguous_query_exits_2() {
    let dir = init_project("json_ambiguous");
    for f in ["user/user.json", "auth/user.json"] {
        apic(&dir).args(["create", "-f", f]).assert().success();
    }
    apic(&dir).args(["read", "-f", "user"]).assert().code(2);
    let out = apic(&dir)
        .args(["read", "-f", "user", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json_out(&out)["error"]["kind"], "ambiguous");
}

#[test]
fn validate_failure_exits_3_with_results() {
    let dir = init_project("json_validate_fail");
    apic(&dir)
        .args(["create", "-f", "good.json"])
        .assert()
        .success();
    fs::write(dir.join("contracts/broken.json"), "{ not json").unwrap();
    apic(&dir).arg("validate").assert().code(3);
    let out = apic(&dir).args(["validate", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    let doc = json_out(&out);
    assert_eq!(doc["failed"], 1);
    assert_eq!(doc["passed"], 1);
    let broken = doc["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["path"] == "broken.json")
        .expect("broken.json is reported");
    assert_eq!(broken["ok"], false);
    assert!(broken["error"].is_string());
}

#[test]
fn send_to_a_closed_port_is_network_with_exit_4() {
    let dir = init_project("json_send_closed");
    fs::write(
        dir.join("contracts/down.json"),
        r#"{"name":"down","method":"GET","url":"http://127.0.0.1:1/","headers":[],"responses":[]}"#,
    )
    .unwrap();
    apic(&dir)
        .args(["send", "-f", "down", "--timeout", "5"])
        .assert()
        .code(4);
    let out = apic(&dir)
        .args(["send", "-f", "down", "--timeout", "5", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(json_out(&out)["error"]["kind"], "network");
}

#[test]
fn remove_in_json_mode_needs_yes() {
    let dir = init_project("json_remove_no_yes");
    apic(&dir)
        .args(["create", "-f", "a.json"])
        .assert()
        .success();
    let out = apic(&dir)
        .args(["remove", "-f", "a", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(json_out(&out)["error"]["kind"], "usage");
    assert!(dir.join("contracts/a.json").exists());
}

#[test]
fn help_exits_0_with_text() {
    let dir = fresh_dir("json_help");
    apic(&dir)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
    apic(&dir)
        .args(["--help", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn list_json_prints_contract_paths() {
    let dir = init_project("json_list");
    apic(&dir)
        .args(["create", "-f", "auth/login.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "user.json"])
        .assert()
        .success();
    let out = apic(&dir).args(["list", "--json"]).output().unwrap();
    assert!(out.status.success());
    let doc = json_out(&out);
    let paths: Vec<&str> = doc["contracts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert!(
        paths.iter().any(|p| p.ends_with("auth/login.json")),
        "{paths:?}"
    );
    assert!(paths.iter().any(|p| p.ends_with("user.json")), "{paths:?}");

    // The flag is global: before the subcommand gives the same document.
    let before = apic(&dir).args(["--json", "list"]).output().unwrap();
    assert_eq!(json_out(&before), doc);

    let out = apic(&dir)
        .args(["list", "--filter", "zzz_no_match_zzz", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(json_out(&out)["contracts"], serde_json::json!([]));
}

#[test]
fn read_json_prints_the_stored_contract_filtered_by_status() {
    let dir = init_project("json_read");
    apic(&dir)
        .args([
            "create",
            "-f",
            "a.json",
            "--response",
            "200:OK",
            "--response",
            "404:Missing",
        ])
        .assert()
        .success();
    let out = apic(&dir)
        .args(["read", "-f", "a", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let doc = json_out(&out);
    assert_eq!(doc["name"], "a");
    assert!(doc["responses"].as_array().unwrap().len() >= 2);

    let out = apic(&dir)
        .args(["read", "-f", "a", "--status", "404", "--json"])
        .output()
        .unwrap();
    let responses = json_out(&out)["responses"].clone();
    let codes: Vec<u64> = responses
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["code"].as_u64().unwrap())
        .collect();
    assert!(!codes.is_empty());
    assert!(codes.iter().all(|c| *c == 404), "{codes:?}");
}

#[test]
fn validate_json_reports_every_contract() {
    let dir = init_project("json_validate_ok");
    apic(&dir)
        .args(["create", "-f", "good.json"])
        .assert()
        .success();
    let out = apic(&dir).args(["validate", "--json"]).output().unwrap();
    assert!(out.status.success());
    let doc = json_out(&out);
    assert_eq!(doc["failed"], 0);
    assert_eq!(doc["results"][0]["path"], "good.json");
    assert_eq!(doc["results"][0]["ok"], true);

    let out = apic(&dir)
        .args(["validate", "--template", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(json_out(&out)["failed"], 0);
}

#[test]
fn config_and_init_json() {
    let dir = fresh_dir("json_init");
    let out = apic(&dir).args(["init", "--json"]).output().unwrap();
    assert!(out.status.success());
    assert_eq!(json_out(&out)["status"], "initialized");

    let out = apic(&dir).args(["config", "--json"]).output().unwrap();
    assert!(out.status.success());
    assert!(json_out(&out)["working_dir"].is_string());
}

#[test]
fn env_json_lists_environments_without_values() {
    let dir = init_project("json_env");
    fs::create_dir_all(dir.join(".apic/env")).unwrap();
    fs::write(dir.join(".apic/env/staging.toml"), "token = \"secret\"\n").unwrap();
    let out = apic(&dir).args(["env", "--json"]).output().unwrap();
    assert!(out.status.success());
    let doc = json_out(&out);
    assert_eq!(doc["environments"][0]["name"], "staging");
    assert_eq!(doc["environments"][0]["has_local"], false);
    assert!(!String::from_utf8_lossy(&out.stdout).contains("secret"));
}

#[test]
fn vars_convert_json_reports_files() {
    let dir = init_project("json_vars");
    fs::write(
        dir.join("contracts/a.json"),
        r#"{"name":"a","method":"GET","url":"{{baseUrl}}/x","headers":[],"responses":[]}"#,
    )
    .unwrap();
    let out = apic(&dir)
        .args(["vars", "--convert", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let doc = json_out(&out);
    assert_eq!(doc["written"], false);
    assert!(doc["total"].as_u64().unwrap() >= 1);
    assert!(
        doc["files"][0]["path"]
            .as_str()
            .unwrap()
            .ends_with("a.json")
    );
}

#[test]
fn create_edit_remove_json_report_actions() {
    let dir = init_project("json_actions");
    let out = apic(&dir)
        .args(["create", "-f", "a.json", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let doc = json_out(&out);
    assert_eq!(doc["action"], "created");
    assert_eq!(doc["path"], "a.json");

    let out = apic(&dir)
        .args(["edit", "-f", "a", "-H", "X-Trace: 1", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(json_out(&out)["action"], "updated");

    let out = apic(&dir)
        .args(["remove", "-f", "a", "--yes", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(json_out(&out)["action"], "removed");
    assert!(!dir.join("contracts/a.json").exists());
}

#[test]
fn create_template_json_reports_a_project_relative_path() {
    let dir = init_project("json_create_template");
    let out = apic(&dir)
        .args(["create", "--template", "extra", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(json_out(&out)["path"], ".apic/template/extra.json");
}

#[test]
fn open_json_reports_the_path() {
    let dir = init_project("json_open");
    apic(&dir)
        .args(["create", "-f", "a.json"])
        .assert()
        .success();
    let out = apic(&dir)
        .args(["open", "-f", "a", "--editor", "true", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(json_out(&out)["path"], "a.json");
}

#[test]
fn convert_json_reports_written_count() {
    let dir = init_project("json_convert");
    let fixture = format!(
        "{}/tests/fixtures/convert/petstore-v2.1.0.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let out = apic(&dir)
        .args(["convert", "--postman", &fixture, "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let doc = json_out(&out);
    assert!(doc["written"].as_u64().unwrap() >= 1);
    assert!(doc["destination"].is_string());
}

/// A project with one GET contract pointed at `url`.
fn json_send_project(tag: &str, url: &str) -> PathBuf {
    let dir = init_project(tag);
    fs::write(
        dir.join("contracts/ping.json"),
        format!(
            r#"{{"name":"ping","method":"GET","url":"{url}/ping","headers":[],"responses":[]}}"#
        ),
    )
    .unwrap();
    dir
}

#[test]
fn send_json_embeds_a_json_body() {
    let (url, _rx) = serve_once(SEND_OK);
    let dir = json_send_project("json_send_json", &url);
    let out = apic(&dir)
        .args(["send", "-f", "ping", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let doc = json_out(&out);
    assert_eq!(doc["status"], 201);
    assert_eq!(doc["body_encoding"], "json");
    assert_eq!(doc["body"]["ok"], true);
    assert!(doc["headers"].as_array().unwrap().iter().any(|h| {
        h["name"].as_str().unwrap().eq_ignore_ascii_case("x-test") && h["value"] == "yes"
    }));
    assert!(doc["elapsed_ms"].is_u64());
}

#[test]
fn send_json_keeps_a_text_body_as_a_string() {
    let (url, _rx) =
        serve_once("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello");
    let dir = json_send_project("json_send_text", &url);
    let out = apic(&dir)
        .args(["send", "-f", "ping", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let doc = json_out(&out);
    assert_eq!(doc["body_encoding"], "text");
    assert_eq!(doc["body"], "hello");
}

/// Serves one request on 127.0.0.1 with the raw `reply` bytes, for bodies
/// that are not valid UTF-8.
fn serve_once_bytes(reply: &'static [u8]) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        stream.write_all(reply).unwrap();
    });
    format!("http://{addr}")
}

#[test]
fn send_json_encodes_a_binary_body_as_base64() {
    let url = serve_once_bytes(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n\xff\xfe",
    );
    let dir = json_send_project("json_send_binary", &url);
    let out = apic(&dir)
        .args(["send", "-f", "ping", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let doc = json_out(&out);
    assert_eq!(doc["body_encoding"], "base64");
    assert_eq!(doc["body"], "//4=");
}

#[test]
fn send_json_exits_0_on_a_404() {
    let (url, _rx) =
        serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let dir = json_send_project("json_send_404", &url);
    let out = apic(&dir)
        .args(["send", "-f", "ping", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(json_out(&out)["status"], 404);
}

#[test]
fn template_queries_use_not_found_and_ambiguous() {
    let dir = init_project("json_template_kinds");
    let out = apic(&dir)
        .args(["remove", "--template", "zzz_no_match_zzz", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json_out(&out)["error"]["kind"], "not_found");

    apic(&dir)
        .args([
            "create",
            "-f",
            "a.json",
            "--use-template",
            "zzz_no_match_zzz",
        ])
        .assert()
        .code(2);

    let template = dir.join(".apic/template/convention.json");
    fs::copy(&template, dir.join(".apic/template/svc-one.json")).unwrap();
    fs::copy(&template, dir.join(".apic/template/svc-two.json")).unwrap();
    let out = apic(&dir)
        .args(["remove", "--template", "svc", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json_out(&out)["error"]["kind"], "ambiguous");
    assert!(dir.join(".apic/template/svc-one.json").exists());
}
