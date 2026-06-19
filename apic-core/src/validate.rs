//! Project-wide contract validation shared by the CLI (`apic validate`) and the
//! GUI (the `[ OPEN ]` flow).

use crate::json;
use std::path::{Path, PathBuf};

/// Validates every `.json` contract under `root` (recursively) as a stand-alone
/// apic contract, returning one `(path, error)` per file that fails to parse or
/// validate. An all-valid (or empty) tree yields an empty vec.
///
/// Anything inside an `.apic/` directory is skipped: project templates are
/// partials, not stand-alone contracts, and have their own check.
pub fn validate_dir(root: &Path) -> Vec<(PathBuf, String)> {
    let mut failures = Vec::new();
    let Some(paths) = json::scan_json_file(root, true) else {
        return failures;
    };
    for path in paths {
        if path.components().any(|c| c.as_os_str() == ".apic") {
            continue;
        }
        match crate::file::read_file(&path) {
            Ok(content) => {
                if let Err(err) = json::validate(&content) {
                    failures.push((path, err.to_string()));
                }
            }
            Err(err) => failures.push((path, err.to_string())),
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, name: &str, body: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    // A minimal contract that passes `json::validate`.
    const VALID: &str = r#"{"name":"x","method":"GET","url":{"protocol":"https","host":"h"},"headers":[],"responses":[]}"#;

    #[test]
    fn all_valid_yields_no_failures() {
        let tmp = tempdir();
        write(&tmp, "a.json", VALID);
        write(&tmp, "sub/b.json", VALID);
        assert!(validate_dir(&tmp).is_empty());
    }

    #[test]
    fn broken_file_is_reported_with_its_path() {
        let tmp = tempdir();
        write(&tmp, "good.json", VALID);
        write(&tmp, "bad.json", "{ not json");
        let failures = validate_dir(&tmp);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].0.ends_with("bad.json"));
        assert!(!failures[0].1.is_empty());
    }

    #[test]
    fn apic_dir_contents_are_skipped() {
        let tmp = tempdir();
        write(&tmp, "good.json", VALID);
        write(&tmp, ".apic/template/convention.json", "{ partial not a contract");
        assert!(validate_dir(&tmp).is_empty());
    }

    // Unique temp dir without external crates: use std::env::temp_dir + pid +
    // a counter so parallel tests don't collide.
    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "apic-validate-{}-{}",
            std::process::id(),
            id
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
