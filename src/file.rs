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
