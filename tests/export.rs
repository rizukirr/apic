use assert_cmd::Command;
use std::fs;

/// Sets up an initialized project in a fresh temp dir with two contracts and a
/// `dev` environment whose local file holds a secret.
fn project(tag: &str) -> std::path::PathBuf {
    let work = std::env::temp_dir().join(format!("apic_it_export_{tag}"));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();
    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["init"])
        .assert()
        .success();
    fs::remove_file(work.join(".apic/template/convention.json")).unwrap();
    fs::create_dir_all(work.join("users")).unwrap();
    fs::write(
        work.join("users/get_user.json"),
        r#"{"name": "Get user", "method": "GET", "url": "$base/users/{id}",
            "headers": [{"name": "Authorization", "value": "Bearer $token", "required": true}],
            "responses": [{"code": 200, "description": "OK", "schema": {"id": 1}}]}"#,
    )
    .unwrap();
    fs::write(
        work.join("login.json"),
        r#"{"name": "Login", "method": "POST", "url": "$base/login", "headers": [],
            "request": {"user": "a"}, "responses": []}"#,
    )
    .unwrap();
    fs::create_dir_all(work.join(".apic/env")).unwrap();
    fs::write(
        work.join(".apic/env/dev.toml"),
        "base = \"https://api.example.com\"\n",
    )
    .unwrap();
    fs::write(
        work.join(".apic/env/dev.local.toml"),
        "token = \"s3cret\"\n",
    )
    .unwrap();
    work
}

/// `apic export --postman` writes a collection that keeps folders, takes
/// variable values from the shared env file only, and imports back through
/// `apic convert --postman` into contracts that validate.
#[test]
fn export_postman_round_trips_and_never_leaks_local_values() {
    let work = project("round_trip");
    let out_file = std::env::temp_dir().join("apic_it_export_round_trip.postman.json");
    let _ = fs::remove_file(&out_file);
    let out_arg = out_file.to_str().unwrap();

    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args([
            "export",
            "--postman",
            out_arg,
            "--env",
            "dev",
            "--name",
            "Demo",
        ])
        .assert()
        .success();

    let text = fs::read_to_string(&out_file).unwrap();
    assert!(
        !text.contains("s3cret"),
        "local env values must never be exported"
    );
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["info"]["name"], "Demo");
    assert_eq!(doc["variable"][0]["key"], "base");
    assert_eq!(doc["variable"][0]["value"], "https://api.example.com");
    assert_eq!(doc["variable"][1]["key"], "token");
    assert_eq!(doc["variable"][1]["value"], "");
    let users = doc["item"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "users")
        .expect("users folder");
    assert_eq!(
        users["item"][0]["request"]["url"]["raw"],
        "{{base}}/users/:id"
    );

    // A second export to the same file needs --force.
    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["export", "--postman", out_arg])
        .assert()
        .failure();

    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args([
            "convert",
            "--postman",
            out_arg,
            "--destination",
            "reimported",
        ])
        .assert()
        .success();
    assert!(work.join("reimported/users/get_user.json").is_file());
    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["validate"])
        .assert()
        .success();

    fs::remove_dir_all(&work).unwrap();
    fs::remove_file(&out_file).unwrap();
}

/// `-f folder/` exports only that folder, `--json` reports the count, and an
/// output file inside the contracts folder is warned about.
#[test]
fn export_postman_folder_scope_and_json() {
    let work = project("scope");

    let out = Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args([
            "export",
            "--postman",
            "users.json",
            "-f",
            "users/",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["exported"], 1);
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("inside the contracts folder"), "{stderr}");

    let doc: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(work.join("users.json")).unwrap()).unwrap();
    assert_eq!(doc["item"].as_array().unwrap().len(), 1);

    fs::remove_dir_all(&work).unwrap();
}

/// `apic export --openapi` writes YAML with a server variable filled from the
/// shared env file only, a bearer security scheme for the Authorization
/// header, and imports back through `apic convert --openapi` into contracts
/// that validate.
#[test]
fn export_openapi_yaml_round_trips_and_never_leaks_local_values() {
    let work = project("openapi");
    let out_file = std::env::temp_dir().join("apic_it_export_openapi.yaml");
    let _ = fs::remove_file(&out_file);
    let out_arg = out_file.to_str().unwrap();

    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args([
            "export",
            "--openapi",
            out_arg,
            "--env",
            "dev",
            "--name",
            "Demo",
        ])
        .assert()
        .success();

    let text = fs::read_to_string(&out_file).unwrap();
    assert!(
        !text.contains("s3cret"),
        "local env values must never be exported"
    );
    assert!(text.starts_with("openapi: 3.0.3"), "{text}");
    assert!(text.contains("default: https://api.example.com"), "{text}");
    assert!(text.contains("bearerAuth"), "{text}");

    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args([
            "convert",
            "--openapi",
            out_arg,
            "--destination",
            "reimported",
        ])
        .assert()
        .success();
    let back: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(work.join("reimported/users/get_user.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(back["url"], "https://api.example.com/users/{id}");
    assert_eq!(back["responses"][0]["schema"], serde_json::json!({"id": 1}));
    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["validate"])
        .assert()
        .success();

    fs::remove_dir_all(&work).unwrap();
    fs::remove_file(&out_file).unwrap();
}

/// `--openapi-version 3.1` writes OpenAPI 3.1 as JSON for a `.json` file, and
/// `--openapi-version` without `--openapi` is a usage error.
#[test]
fn export_openapi_3_1_json_and_version_needs_openapi() {
    let work = project("openapi31");
    let out_file = std::env::temp_dir().join("apic_it_export_openapi31.json");
    let _ = fs::remove_file(&out_file);
    let out_arg = out_file.to_str().unwrap();

    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["export", "--openapi", out_arg, "--openapi-version", "3.1"])
        .assert()
        .success();
    let doc: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&out_file).unwrap()).unwrap();
    assert_eq!(doc["openapi"], "3.1.0");
    // Without --env the server variable has an empty default.
    assert_eq!(doc["servers"][0]["variables"]["base"]["default"], "");

    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["export", "--postman", "x.json", "--openapi-version", "3.1"])
        .assert()
        .failure();

    fs::remove_dir_all(&work).unwrap();
    fs::remove_file(&out_file).unwrap();
}
