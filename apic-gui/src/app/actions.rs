//! Actions that views return to the app shell.
//!
//! Views never mutate [`crate::app::App`] directly: they return one of these and
//! `App::ui` applies it. A new feature adds its own action enum here rather than
//! reaching into another feature's state.

use crate::app::state::SidebarTab;
use crate::features::contracts::state::DeleteTarget;

/// A one-shot action requested by the header or sidebar this frame.
pub(crate) enum SidebarAction {
    LoadContract(usize),
    LoadTemplate(usize),
    OpenProject,
    NewProject,
    ImportPostman,
    NewTemplate,

    /// Open the new-request dialog, pre-filled with this path prefix (e.g.
    /// `auth/` when `+` is clicked on the `auth` folder, empty for the root
    /// button).
    NewRequest(String),

    /// Ask to delete something; shows a confirmation before anything is removed.
    RequestDelete(DeleteTarget),

    /// Toggle the left contracts sidebar between fully hidden and shown.
    ToggleSidebar,

    /// Switch which panel fills the sidebar frame.
    SwitchTab(SidebarTab),
}

/// A one-shot action requested by the Git panel this frame.
pub(crate) enum GitAction {
    /// Re-read `git status`.
    Refresh,

    /// Show the diff for this repo-relative path, staged or unstaged. Nothing
    /// constructs this yet, the file list lands in Task 5.
    #[allow(dead_code)]
    Select { path: String, staged: bool },
}

/// Anything a view returned this frame, from either sidebar tab.
pub(crate) enum Action {
    Sidebar(SidebarAction),
    Git(GitAction),
}
