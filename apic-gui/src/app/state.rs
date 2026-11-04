//! Application-level state: the window chrome and the repository location.
//! Feature-specific state lives in that feature's own `state.rs`.

use std::path::PathBuf;

use crate::features::git::data::jobs::RepoPaths;
use crate::features::project::ui::state::ProjectState;

/// Application chrome and the repository location. Shared by every
/// feature; no feature owns it.
///
/// `Default` is written by hand rather than derived: `sidebar_open` starts
/// `true`, and a derived `Default` would silently start the app with the
/// sidebar collapsed.
pub(crate) struct ShellState {
    /// The status line shown in the bottom bar.
    pub(crate) status: String,

    /// Whether the left contracts sidebar is shown. Toggled from the top bar;
    /// not persisted, so it always starts `true` on launch.
    pub(crate) sidebar_open: bool,

    /// What fills the central panel, set by the last click in the sidebar.
    pub(crate) central: CentralView,

    /// Which list fills the sidebar's main area.
    pub(crate) sidebar_view: SidebarView,

    /// Whether the Commit section at the bottom of the sidebar is expanded.
    pub(crate) commit_open: bool,

    /// Whether the Templates section at the bottom of the sidebar is expanded.
    pub(crate) templates_open: bool,

    /// Root of the git repository containing `project_root`, or `None` when
    /// the project is not inside one.
    pub(crate) repo_root: Option<PathBuf>,
}

impl ShellState {
    /// The repository locations a git job runs against, or `None` when the
    /// project is not inside a repository.
    ///
    /// # Arguments
    ///
    /// * `project` - where the active project and its contracts live
    pub(crate) fn repo_paths(&self, project: &ProjectState) -> Option<RepoPaths> {
        let repo_root = self.repo_root.clone()?;
        Some(RepoPaths {
            root: project.root.clone().unwrap_or_else(|| repo_root.clone()),
            project_root: project.project_root.clone(),
            repo_root,
        })
    }
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            status: String::new(),
            sidebar_open: true,
            central: CentralView::Contracts,
            sidebar_view: SidebarView::Contracts,
            commit_open: false,
            templates_open: false,
            repo_root: None,
        }
    }
}

/// Which list fills the sidebar's main area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SidebarView {
    /// The contract tree.
    Contracts,
    /// The git changes: conflicts, staged, unstaged and outside files.
    Changes,
}

/// What fills the central panel: the last thing clicked in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CentralView {
    /// The open contract or template.
    Contracts,
    /// The selected git file's diff or conflict resolver.
    Git,
}
