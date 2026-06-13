use assert_cmd::Command;
use std::fs;

/// `apic convert` imports a v2.1 collection, mirroring folders into files that
/// then pass `apic validate`.
#[test]
fn convert_imports_v2_1_collection() {
    let work = std::env::temp_dir().join("apic_it_convert");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();

    // Initialize an apic project rooted at `work`.
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

    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["convert", "--postman", &fixture, "--destination", "imported"])
        .assert()
        .success();

    // Files mirror the collection's folder nesting.
    assert!(work.join("imported/pets/list_pets.json").is_file());
    assert!(work.join("imported/pets/get_pet.json").is_file());

    // The generated contracts validate.
    Command::cargo_bin("apic")
        .unwrap()
        .current_dir(&work)
        .args(["validate"])
        .assert()
        .success();

    fs::remove_dir_all(&work).unwrap();
}
