//! Actions that views return to the app shell.
//!
//! Views never mutate [`crate::app::App`] directly: they return one of these and
//! `App::ui` applies it. A new feature adds its own action enum here rather than
//! reaching into another feature's state.

use crate::app::state::SidebarView;
use crate::features::contracts::domain::entry::DeleteTarget;
use crate::features::git::ui::state::GitAction;

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

    /// Expand or collapse the Commit section.
    ToggleCommitSection,

    /// Show this list in the sidebar's main area.
    SwitchView(SidebarView),

    /// Expand or collapse the Templates section at the bottom of the sidebar.
    ToggleTemplatesSection,
}

/// Anything a view returned this frame: from the app bar, the sidebar, or the
/// Git view in the central panel.
pub(crate) enum Action {
    Sidebar(SidebarAction),
    Git(GitAction),
}
