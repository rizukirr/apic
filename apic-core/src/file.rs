//! Filesystem helpers: locating files by name or extension, and reading them.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

/// Maximum size of a contract file `apic` will read into memory (5 MiB).
///
/// Contracts are small JSON documents; this cap turns a hostile or accidental
/// multi-gigabyte file into a clean error instead of exhausting memory.
pub const MAX_CONTRACT_BYTES: u64 = 5 * 1024 * 1024;

/// Renders a path with `/` separators on every platform.
///
/// Contract paths are meant to be portable and git-reviewable, so they are
/// stored and displayed with forward slashes regardless of the host OS. Rust
/// accepts `/` as a separator on all platforms (including Windows), so the
/// normalized form is also valid for filesystem access. Windows filenames
/// cannot contain `\`, so swapping the platform separator never corrupts a
/// name; on Unix `MAIN_SEPARATOR` is already `/`, making this a no-op.
pub fn to_slash(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

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

/// Reads a contract file at `path` into a `String`, rejecting oversized files.
///
/// Files larger than [`MAX_CONTRACT_BYTES`] are refused before any bytes are
/// read, so an untrusted or accidental huge file cannot exhaust memory.
///
/// # Errors
///
/// Returns an [`io::Error`] if the file cannot be opened or read, exceeds the
/// size cap, or is not valid UTF-8.
pub fn read_file(path: &Path) -> Result<String, io::Error> {
    let meta = fs::metadata(path)?;
    if meta.len() > MAX_CONTRACT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "file is {} bytes, larger than the {} byte limit",
                meta.len(),
                MAX_CONTRACT_BYTES
            ),
        ));
    }
    fs::read_to_string(path)
}

/// Lexically resolves `.` and `..` components in `path` without touching the
/// filesystem. Used to detect path-traversal escapes before any IO.
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolves `target` against `base` and confirms it stays inside `base`.
///
/// `base` must be an absolute, normalized directory (e.g. a canonicalized
/// working directory). A relative `target` is joined onto `base`; an absolute
/// `target` is taken as-is. The result is lexically normalized and rejected if
/// it escapes `base` via `..` or an absolute path elsewhere.
///
/// The check is also symlink-aware: lexical normalization works purely on the
/// path string and cannot see that a component is a symlink pointing outside
/// `base`, so `fs::write` would still escape (issue #22). Every component of the
/// path below `base` that exists on disk is therefore checked, and the path is
/// rejected if any of them is a symlink. Components that do not exist yet (the
/// usual `apic create` case, where the final filename is new) are not symlinks
/// and pass. Pure-lexical callers and tests, whose `base` does not exist on
/// disk, see no symlinks and keep the lexical result.
///
/// # Errors
///
/// Returns `Err` with a human-readable message if `target` resolves outside
/// `base`.
pub fn confine_to_dir(base: &Path, target: &Path) -> Result<PathBuf, String> {
    let joined = if target.is_absolute() {
        target.to_path_buf()
    } else {
        base.join(target)
    };
    let normalized = normalize_lexical(&joined);
    let base_norm = normalize_lexical(base);
    if !normalized.starts_with(&base_norm) {
        return Err(escape_error(target));
    }

    // Reject a symlink anywhere below `base`: such a component could redirect the
    // operation outside the working directory even though the path is lexically
    // contained. Only the components under `base` are user-influenced, so the
    // trusted `base` itself is not probed.
    if let Ok(relative) = normalized.strip_prefix(&base_norm) {
        let mut probe = base_norm.clone();
        for component in relative.components() {
            probe.push(component);
            if probe.is_symlink() {
                return Err(escape_error(target));
            }
        }
    }

    Ok(normalized)
}

/// The "outside the working directory" rejection message for `target`.
fn escape_error(target: &Path) -> String {
    format!(
        "refusing to use a path outside the working directory: {}",
        target.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_slash_renders_components_with_forward_slashes() {
        // Built from components so the path uses the platform separator, which
        // to_slash must normalize to `/` (this is the case that differs on
        // Windows). A single relative join is enough to exercise it.
        let p: PathBuf = ["user", "profile", "user.json"].iter().collect();
        assert_eq!(to_slash(&p), "user/profile/user.json");
    }

    #[test]
    fn to_slash_leaves_a_plain_name_untouched() {
        assert_eq!(to_slash(Path::new("login.json")), "login.json");
    }

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
    fn read_file_rejects_oversized_files() {
        let root = temp_dir("oversize");
        let path = root.join("big.json");
        // One byte past the cap, written sparsely so the test stays cheap.
        let f = fs::File::create(&path).unwrap();
        f.set_len(MAX_CONTRACT_BYTES + 1).unwrap();
        assert!(read_file(&path).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn confine_accepts_paths_inside_base() {
        let base = Path::new("/home/u/project");
        assert_eq!(
            confine_to_dir(base, Path::new("auth/login.json")).unwrap(),
            PathBuf::from("/home/u/project/auth/login.json")
        );
    }

    #[test]
    fn confine_rejects_parent_dir_escape() {
        let base = Path::new("/home/u/project");
        assert!(confine_to_dir(base, Path::new("../../etc/passwd")).is_err());
    }

    #[test]
    fn confine_rejects_absolute_path_outside_base() {
        let base = Path::new("/home/u/project");
        assert!(confine_to_dir(base, Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn confine_normalizes_interior_dotdot() {
        let base = Path::new("/home/u/project");
        // Escapes then returns inside base — still allowed, fully normalized.
        assert_eq!(
            confine_to_dir(base, Path::new("auth/../user/x.json")).unwrap(),
            PathBuf::from("/home/u/project/user/x.json")
        );
    }

    #[test]
    #[cfg(unix)]
    fn confine_rejects_symlinked_dir_component_escaping_base() {
        // A path that is lexically inside base but whose intermediate component
        // is a symlink to an outside directory must be rejected (issue #22).
        let base = temp_dir("confine_symlink_dir").canonicalize().unwrap();
        let outside = temp_dir("confine_symlink_dir_out");
        std::os::unix::fs::symlink(&outside, base.join("link")).unwrap();

        // The target file does not exist yet, but its parent `link` is a symlink.
        assert!(confine_to_dir(&base, Path::new("link/escaped.json")).is_err());

        fs::remove_dir_all(&base).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn confine_rejects_symlinked_final_component() {
        // A symlinked file as the final component is also rejected, not followed.
        let base = temp_dir("confine_symlink_file").canonicalize().unwrap();
        let outside = temp_dir("confine_symlink_file_out");
        let target_outside = outside.join("real.json");
        fs::write(&target_outside, "{}").unwrap();
        std::os::unix::fs::symlink(&target_outside, base.join("alias.json")).unwrap();

        assert!(confine_to_dir(&base, Path::new("alias.json")).is_err());

        fs::remove_dir_all(&base).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn confine_allows_real_nonexistent_target_inside_base() {
        // The normal `apic create` case: a genuinely-inside path whose final
        // components do not exist yet is accepted.
        let base = temp_dir("confine_real_inside").canonicalize().unwrap();
        fs::create_dir_all(base.join("auth")).unwrap();

        assert_eq!(
            confine_to_dir(&base, Path::new("auth/new.json")).unwrap(),
            base.join("auth/new.json")
        );

        fs::remove_dir_all(&base).unwrap();
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
