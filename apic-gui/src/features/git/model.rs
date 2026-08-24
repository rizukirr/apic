//! Working-tree status as plain data, and the parser that produces it.
//!
//! Nothing here spawns a process or touches egui, so the parser is testable
//! against captured `git status --porcelain=v2 -z` bytes.
//!
//! Consumed by `service`, `state`, and `view`.

use std::path::Path;

/// How one side of a file changed. `Unmodified` is git's `.` placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    Unmodified,
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

impl Change {
    /// Maps one XY status character. An unrecognized code reads as `Modified`
    /// so a future git status letter shows the file rather than dropping it.
    fn from_code(c: u8) -> Change {
        match c {
            b'.' => Change::Unmodified,
            b'A' => Change::Added,
            b'D' => Change::Deleted,
            b'R' => Change::Renamed,
            b'?' => Change::Untracked,
            _ => Change::Modified,
        }
    }

    /// The single letter shown in the file list.
    pub(crate) fn letter(self) -> &'static str {
        match self {
            Change::Unmodified => " ",
            Change::Added => "A",
            Change::Modified => "M",
            Change::Deleted => "D",
            Change::Renamed => "R",
            Change::Untracked => "?",
        }
    }
}

/// One changed file. `path` is repo-relative, as git reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileStatus {
    pub(crate) path: String,
    pub(crate) index: Change,
    pub(crate) worktree: Change,
    pub(crate) conflicted: bool,
}

impl FileStatus {
    /// Whether git is tracking this path. Discard is offered for tracked files
    /// only: reverting a tracked file is recoverable, deleting an untracked one
    /// is not.
    pub(crate) fn tracked(&self) -> bool {
        self.index != Change::Untracked && self.worktree != Change::Untracked
    }
}

/// The working tree, split by location once at parse time so the view never
/// re-derives it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Status {
    /// Changes under the contract working dir.
    pub(crate) inside: Vec<FileStatus>,

    /// Changes elsewhere in the repo, reported as a count and expandable.
    pub(crate) outside: Vec<FileStatus>,
}

/// Parses `git status --porcelain=v2 -z` output, splitting each entry by whether
/// it falls under `scope` (a repo-relative prefix, empty for the whole repo).
///
/// Unknown record types are skipped rather than fatal, so a git version that
/// adds a line type costs a missing row instead of an empty panel.
pub(crate) fn parse_status(bytes: &[u8], scope: &str) -> Status {
    let mut status = Status::default();
    let mut records = bytes.split(|b| *b == 0).filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        let text = String::from_utf8_lossy(record);
        let Some(kind) = text.as_bytes().first() else {
            continue;
        };
        let file = match kind {
            b'1' => ordinary(&text, 8),
            b'2' => {
                // The original path follows as its own NUL-terminated record.
                let renamed = ordinary(&text, 9);
                records.next();
                renamed
            }
            b'u' => unmerged(&text),
            b'?' => field(&text, 1).map(|path| FileStatus {
                path,
                index: Change::Untracked,
                worktree: Change::Untracked,
                conflicted: false,
            }),
            _ => None,
        };
        if let Some(file) = file {
            if in_scope(&file.path, scope) {
                status.inside.push(file);
            } else {
                status.outside.push(file);
            }
        }
    }
    status
}

/// The `n`th space-separated field onward, kept whole. Paths are unquoted under
/// `-z` and may contain spaces, so the split is bounded rather than greedy.
fn field(text: &str, n: usize) -> Option<String> {
    let mut parts = text.splitn(n + 1, ' ');
    for _ in 0..n {
        parts.next()?;
    }
    parts.next().map(|s| s.to_string())
}

/// An ordinary or rename record: XY is field 1, the path is at `path_field`.
fn ordinary(text: &str, path_field: usize) -> Option<FileStatus> {
    let xy = text.split(' ').nth(1)?.as_bytes().to_vec();
    Some(FileStatus {
        path: field(text, path_field)?,
        index: Change::from_code(*xy.first()?),
        worktree: Change::from_code(*xy.get(1)?),
        conflicted: false,
    })
}

/// An unmerged record. Both sides read as modified and `conflicted` is set: the
/// panel shows an indicator and a read-only view, resolution is out of scope.
fn unmerged(text: &str) -> Option<FileStatus> {
    Some(FileStatus {
        path: field(text, 10)?,
        index: Change::Modified,
        worktree: Change::Modified,
        conflicted: true,
    })
}

/// Whether a repo-relative path sits under `scope`, itself repo-relative. An
/// empty scope matches everything.
fn in_scope(path: &str, scope: &str) -> bool {
    scope.is_empty() || Path::new(path).starts_with(scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Joins porcelain v2 records with NUL, the way `-z` output is delimited.
    fn joined(records: &[&str]) -> Vec<u8> {
        records.join("\0").into_bytes()
    }

    #[test]
    fn ordinary_unstaged_modification() {
        let bytes = joined(&["1 .M N... 100644 100644 100644 abc123 def456 src/main.rs"]);
        let status = parse_status(&bytes, "");
        assert_eq!(status.inside.len(), 1);
        let file = &status.inside[0];
        assert_eq!(file.path, "src/main.rs");
        assert_eq!(file.index, Change::Unmodified);
        assert_eq!(file.worktree, Change::Modified);
        assert!(!file.conflicted);
    }

    #[test]
    fn staged_addition() {
        let bytes = joined(&["1 A. N... 000000 100644 100644 0000000 def456 src/new.rs"]);
        let status = parse_status(&bytes, "");
        assert_eq!(status.inside.len(), 1);
        let file = &status.inside[0];
        assert_eq!(file.path, "src/new.rs");
        assert_eq!(file.index, Change::Added);
        assert_eq!(file.worktree, Change::Unmodified);
    }

    #[test]
    fn rename_consumes_original_path_record() {
        let bytes = joined(&[
            "2 R. N... 100644 100644 100644 abc123 abc123 R100 src/renamed.rs",
            "src/old.rs",
        ]);
        let status = parse_status(&bytes, "");
        assert_eq!(status.inside.len(), 1);
        let file = &status.inside[0];
        assert_eq!(file.path, "src/renamed.rs");
        assert_eq!(file.index, Change::Renamed);
    }

    #[test]
    fn untracked_file() {
        let bytes = joined(&["? src/scratch.rs"]);
        let status = parse_status(&bytes, "");
        assert_eq!(status.inside.len(), 1);
        let file = &status.inside[0];
        assert_eq!(file.path, "src/scratch.rs");
        assert_eq!(file.index, Change::Untracked);
        assert_eq!(file.worktree, Change::Untracked);
        assert!(!file.tracked());
    }

    #[test]
    fn unmerged_file_is_conflicted() {
        let bytes =
            joined(&["u UU N... 100644 100644 100644 100644 abc123 def456 111111 src/conflict.rs"]);
        let status = parse_status(&bytes, "");
        assert_eq!(status.inside.len(), 1);
        let file = &status.inside[0];
        assert_eq!(file.path, "src/conflict.rs");
        assert_eq!(file.index, Change::Modified);
        assert_eq!(file.worktree, Change::Modified);
        assert!(file.conflicted);
    }

    #[test]
    fn path_with_space() {
        let bytes = joined(&["1 .M N... 100644 100644 100644 abc123 def456 src/my file.rs"]);
        let status = parse_status(&bytes, "");
        assert_eq!(status.inside.len(), 1);
        assert_eq!(status.inside[0].path, "src/my file.rs");
    }

    #[test]
    fn unknown_record_type_is_skipped() {
        let bytes = joined(&["! ignored entry"]);
        let status = parse_status(&bytes, "");
        assert!(status.inside.is_empty());
        assert!(status.outside.is_empty());
    }

    #[test]
    fn scope_splits_outside_paths() {
        let bytes = joined(&[
            "1 .M N... 100644 100644 100644 abc123 def456 apic-gui/src/main.rs",
            "1 .M N... 100644 100644 100644 abc123 def456 apic-core/src/lib.rs",
        ]);
        let status = parse_status(&bytes, "apic-gui");
        assert_eq!(status.inside.len(), 1);
        assert_eq!(status.inside[0].path, "apic-gui/src/main.rs");
        assert_eq!(status.outside.len(), 1);
        assert_eq!(status.outside[0].path, "apic-core/src/lib.rs");
    }
}
