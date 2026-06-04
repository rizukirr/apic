//! Filesystem helpers: locating files by name or extension, and reading them.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Outcome of a file search: either the matching paths or nothing found.
pub enum FindFileResult {
    Found(Vec<PathBuf>),
    NotFound,
}

/// Recursively collects every file under `start` whose extension is in
/// `extensions`.
///
/// Symlinks are not followed. Returns [`FindFileResult::NotFound`] if no file
/// matches.
pub fn find_file_by_ext_downward(start: PathBuf, extensions: &[&str]) -> FindFileResult {
    let pwd = start.to_path_buf();
    let mut files = Vec::new();

    for entry in WalkDir::new(&pwd)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        for ext in extensions {
            if entry.path().extension() == Some(ext.as_ref()) {
                files.push(entry.path().to_path_buf());
            }
        }
    }

    if !files.is_empty() {
        return FindFileResult::Found(files);
    }

    FindFileResult::NotFound
}

/// Recursively searches under `start` for entries matching any of `names`.
///
/// For each directory walked (files are skipped), each name is joined and
/// checked for existence. Symlinks are not followed. Returns
/// [`FindFileResult::NotFound`] if nothing matches.
pub fn find_file_downward(start: PathBuf, names: &[PathBuf]) -> FindFileResult {
    let mut files = Vec::new();

    for entry in WalkDir::new(&start)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
    {
        for name in names {
            let candidate = entry.path().join(name);
            if candidate.exists() {
                files.push(candidate);
            }
        }
    }

    if !files.is_empty() {
        return FindFileResult::Found(files);
    }

    FindFileResult::NotFound
}

/// Walks upward from `start` toward the filesystem root looking for `names`.
///
/// Each name resolves to its nearest occurrence: once a name is found it is
/// not searched again in higher ancestors. The walk stops when every name has
/// been found or the root is reached. Returns [`FindFileResult::NotFound`] if
/// nothing matches.
pub fn find_file_upward(start: PathBuf, names: &[PathBuf]) -> FindFileResult {
    let mut pwd = start;
    let mut files = Vec::new();
    let mut remaining: Vec<&PathBuf> = names.iter().collect();

    loop {
        remaining.retain(|name| {
            let candidate = pwd.join(name);
            if candidate.exists() {
                files.push(candidate);
                false
            } else {
                true
            }
        });

        if remaining.is_empty() || !pwd.pop() {
            break;
        }
    }

    if !files.is_empty() {
        return FindFileResult::Found(files);
    }

    FindFileResult::NotFound
}

/// Reads the entire file at `path` into a `String`.
///
/// # Errors
///
/// Returns an [`io::Error`] if the file cannot be opened, read, or is not
/// valid UTF-8.
pub fn read_file(path: &Path) -> Result<String, io::Error> {
    fs::read_to_string(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a unique, empty temp directory for a single test.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apic_test_file_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_file_preserves_multibyte_across_old_chunk_boundary() {
        // Regression: the old chunked reader corrupted any multibyte char that
        // straddled a 1 KiB boundary. Place 'é' so its bytes cross offset 1024.
        let root = temp_dir("utf8");
        let path = root.join("c.txt");
        let content = format!("{}é tail", "x".repeat(1023));
        fs::write(&path, &content).unwrap();

        assert_eq!(read_file(&path).unwrap(), content);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn read_file_errors_instead_of_panicking_on_missing_file() {
        let missing = std::env::temp_dir().join("apic_test_file_does_not_exist.txt");
        let _ = fs::remove_file(&missing);
        assert!(read_file(&missing).is_err());
    }

    #[test]
    fn find_by_ext_downward_finds_json_and_reports_not_found() {
        let root = temp_dir("byext");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub/a.json"), "{}").unwrap();
        fs::write(root.join("b.txt"), "x").unwrap();

        match find_file_by_ext_downward(root.clone(), &["json"]) {
            FindFileResult::Found(files) => {
                assert_eq!(files.len(), 1);
                assert!(files[0].ends_with("a.json"));
            }
            FindFileResult::NotFound => panic!("expected to find a.json"),
        }

        // A directory with no matching extension reports NotFound.
        let empty = temp_dir("byext_empty");
        assert!(matches!(
            find_file_by_ext_downward(empty.clone(), &["json"]),
            FindFileResult::NotFound
        ));

        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&empty).unwrap();
    }

    #[test]
    fn find_upward_locates_marker_in_an_ancestor() {
        let root = temp_dir("upward");
        let nested = root.join("a/b/c");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("marker"), "x").unwrap();

        match find_file_upward(nested, &[PathBuf::from("marker")]) {
            FindFileResult::Found(files) => {
                assert_eq!(files.len(), 1);
                assert!(files[0].ends_with("marker"));
            }
            FindFileResult::NotFound => panic!("expected to find marker upward"),
        }

        fs::remove_dir_all(&root).unwrap();
    }
}
