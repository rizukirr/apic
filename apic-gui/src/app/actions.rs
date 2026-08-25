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

    /// Show the diff for this repo-relative path, staged or unstaged.
    Select { path: String, staged: bool },

    /// Stage this repo-relative path.
    Stage { path: String },

    /// Unstage this repo-relative path, leaving the working tree untouched.
    Unstage { path: String },

    /// Ask to discard this tracked path, shows a confirmation before
    /// anything is reverted.
    RequestDiscard { path: String },

    /// The pending discard was confirmed, revert it.
    ConfirmDiscard,

    /// Commit the currently staged changes with `GitState::commit_message`.
    Commit,

    /// Re-read the branch list.
    ///
    /// Not yet constructed outside tests, Task 3 wires it into the view.
    #[allow(dead_code)]
    RefreshBranches,

    /// Switch to this local branch.
    ///
    /// Not yet constructed outside tests, Task 3 wires it into the view.
    #[allow(dead_code)]
    SwitchBranch { name: String },

    /// Create this branch without switching to it.
    ///
    /// Not yet constructed outside tests, Task 3 wires it into the view.
    #[allow(dead_code)]
    CreateBranch { name: String },

    /// Ask to delete this branch, shows a confirmation before anything is
    /// removed.
    ///
    /// Not yet constructed outside tests, Task 3 wires it into the view.
    #[allow(dead_code)]
    RequestBranchDelete { name: String },

    /// The pending branch delete was confirmed, delete it.
    ///
    /// Not yet constructed outside tests, Task 3 wires it into the view.
    #[allow(dead_code)]
    ConfirmBranchDelete,
}

/// Anything a view returned this frame, from either sidebar tab.
pub(crate) enum Action {
    Sidebar(SidebarAction),
    Git(GitAction),
}
