//! Contracts feature rendering. Every body fills a frame the app shell owns
//! and returns a [`ContractsAction`](crate::features::contracts::ui::state::ContractsAction)
//! instead of writing shell state.

mod central;
mod dialogs;
mod editor;
mod live;
mod sidebar;

pub(crate) use self::central::central_body;
pub(crate) use self::dialogs::{FOCUS_NEW_REQUEST, FOCUS_NEW_TEMPLATE, dialogs};
pub(crate) use self::sidebar::{header_actions, sidebar_body, templates_body};
