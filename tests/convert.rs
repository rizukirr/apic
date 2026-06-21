use assert_cmd::Command;
use std::fs;

/// `apic convert` imports a v2.1 collection, mirroring folders into files that
/// then pass `apic validate`.
#[test]
fn convert_imports_v2_1_collection() {
    let work = std::env::temp_dir().join("apic_it_convert");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();

    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["init"])
        .assert()
        .success();

    // Drop the template so `validate` below checks only schema validity of the
    // imported contracts: with no template file, conformance enforces nothing
    // (template conformance is covered by template.rs unit tests).
    fs::remove_file(work.join(".apic/template/convention.json")).unwrap();

    let fixture = format!(
        "{}/tests/fixtures/convert/petstore-v2.1.0.json",
        env!("CARGO_MANIFEST_DIR")
    );

    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args([
            "convert",
            "--postman",
            &fixture,
            "--destination",
            "imported",
        ])
        .assert()
        .success();

    // Files mirror the collection's folder nesting.
    assert!(work.join("imported/pets/list_pets.json").is_file());
    assert!(work.join("imported/pets/get_pet.json").is_file());

    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["validate"])
        .assert()
        .success();

    fs::remove_dir_all(&work).unwrap();
}

/// With `--destination` omitted, `apic convert` writes into the working
/// directory itself (from `.apic/config.toml`).
#[test]
fn convert_without_destination_uses_working_dir() {
    let work = std::env::temp_dir().join("apic_it_convert_default_dest");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();

    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["init"])
        .assert()
        .success();

    let fixture = format!(
        "{}/tests/fixtures/convert/petstore-v2.1.0.json",
        env!("CARGO_MANIFEST_DIR")
    );

    // No --destination: files land under the working directory root.
    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["convert", "--postman", &fixture])
        .assert()
        .success();

    assert!(work.join("pets/list_pets.json").is_file());
    assert!(work.join("pets/get_pet.json").is_file());

    fs::remove_dir_all(&work).unwrap();
}
