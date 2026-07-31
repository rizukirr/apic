//! Reusable egui primitives. Each consumes tokens from [`crate::ui::theme`] and
//! knows nothing about contracts, git, or any other feature.
//!
//! The dependency direction is one-way: components import theme, never the
//! reverse, and nothing here may import from `crate::features`.

pub(crate) mod button;
pub(crate) mod chip;
pub(crate) mod input;
pub(crate) mod table;
pub(crate) mod text;

pub(crate) use button::*;
pub(crate) use chip::*;
pub(crate) use input::*;
pub(crate) use table::*;
pub(crate) use text::*;
