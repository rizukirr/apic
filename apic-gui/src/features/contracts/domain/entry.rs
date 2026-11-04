//! The contract list entry, the delete target, and how a location is shown.

use std::path::{Path, PathBuf};

/// A discovered contract plus the lightweight summary shown in the sidebar.
pub(crate) struct Entry {
    pub(crate) path: PathBuf,
    pub(crate) rel: String,
    pub(crate) method: String,

    /// Validation error when this contract is invalid; `None` when it is valid.
    pub(crate) error: Option<String>,
}

/// A thing the user asked to delete (pending confirmation).
#[derive(Clone)]
pub(crate) enum DeleteTarget {
    /// A contract or folder, by path relative to the contracts root.
    Contract { rel: String, is_dir: bool },

    /// A template file in `.apic/template/`, by display name and absolute path.
    Template { name: String, path: PathBuf },
}

/// Renders `path` with the home directory collapsed to `~` (forward-slashed),
/// reusing `apic_core::file::{to_slash, home_relative}` so the footer matches the
/// CLI and no logic is duplicated.
pub(crate) fn display_location(path: &Path) -> String {
    apic_core::file::home_relative(&apic_core::file::to_slash(path))
}
