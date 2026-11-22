//! Components: global UI pieces built from more than one base component,
//! such as dialogs, tabs, tables, the URL bar and nested menus. They know
//! nothing about contracts, git, or any other feature.
//!
//! Components may import [`crate::ui::base`] and theme, never
//! `crate::features`. The guard test in `crate::ui` checks this.

pub(crate) mod dialog;
pub(crate) mod layout;
pub(crate) mod menu;
pub(crate) mod sidebar;
pub(crate) mod table;
pub(crate) mod tabs;

pub(crate) use dialog::*;
pub(crate) use layout::*;
pub(crate) use menu::*;
pub(crate) use sidebar::*;
pub(crate) use table::*;
pub(crate) use tabs::*;
