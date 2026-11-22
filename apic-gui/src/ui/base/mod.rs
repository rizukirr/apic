//! Base components: single widgets that hold no other component, such as
//! text, buttons and text inputs. Each takes its sizes and colors from
//! [`crate::ui::theme`] and nothing else.
//!
//! Base components may import theme, never `crate::ui::components` or
//! `crate::features`. The guard test in `crate::ui` checks this.

pub(crate) mod button;
pub(crate) mod chip;
pub(crate) mod input;
pub(crate) mod menu;
pub(crate) mod text;

pub(crate) use button::*;
pub(crate) use chip::*;
pub(crate) use input::*;
pub(crate) use menu::*;
pub(crate) use text::*;
