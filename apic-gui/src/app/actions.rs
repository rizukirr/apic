//! Actions that views return to the app shell.
//!
//! Views never mutate [`crate::app::App`] directly: they return one of these and
//! `App::ui` applies it. A new feature adds its own action enum here rather than
//! reaching into another feature's state.

use crate::app::state::SidebarView;
use crate::features::contracts::ui::state::ContractsAction;
use crate::features::git::ui::state::GitAction;
use crate::features::project::ui::state::ProjectAction;

/// A one-shot action on the window chrome requested by the header or sidebar
/// this frame.
pub(crate) enum ShellAction {
    /// Toggle the left contracts sidebar between fully hidden and shown.
    ToggleSidebar,

    /// Expand or collapse the Commit section.
    ToggleCommitSection,

    /// Show this list in the sidebar's main area.
    SwitchView(SidebarView),

    /// Expand or collapse the Templates section at the bottom of the sidebar.
    ToggleTemplatesSection,
}

/// Anything a view returned this frame: from the app bar, the sidebar, the
/// contracts views, or the Git view in the central panel.
pub(crate) enum Action {
    Shell(ShellAction),
    Project(ProjectAction),
    Contracts(ContractsAction),
    Git(GitAction),
}
