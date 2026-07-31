//! Application-level state: the active project location and window chrome.
//! Feature-specific state lives in that feature's own `state.rs`.

use std::path::PathBuf;

/// Application chrome and the active project location. Shared by every
/// feature; no feature owns it.
///
/// `Default` is written by hand rather than derived: `sidebar_open` starts
/// `true`, and a derived `Default` would silently start the app with the
/// sidebar collapsed.
pub(crate) struct ShellState {
    /// The directory whose contracts are listed in the sidebar.
    pub(crate) root: Option<PathBuf>,

    /// Absolute root of the active project (the dir containing `.apic/`). `None`
    /// when no project is open. All discovery resolves against this, never cwd.
    pub(crate) project_root: Option<PathBuf>,

    /// The `.apic` directory, for locating templates.
    pub(crate) apic_dir: Option<PathBuf>,

    /// The status line shown in the bottom bar.
    pub(crate) status: String,

    /// Whether the left contracts sidebar is shown. Toggled from the top bar;
    /// not persisted, so it always starts `true` on launch.
    pub(crate) sidebar_open: bool,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            root: None,
            project_root: None,
            apic_dir: None,
            status: String::new(),
            sidebar_open: true,
        }
    }
}

/// Which action consumes the path chosen by an in-flight file dialog.
#[derive(Clone, Copy)]
pub(crate) enum DialogKind {
    OpenProject,
    NewProject,
    ImportPostman,
}
